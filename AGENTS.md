# Flashcap - Project Guide

> **Wherever users can see the app name, write it as "Flashcap"** (CYBERNEURA-DEV-1126. This covers `productName` = the macOS `.app` name = the name shown in the Dock, the app menu, the dmg / installer file names, window titles, and the wording in notifications, the tray and Preferences). The `c` is lowercase; `FlashCap` / `flashcap` are wrong. The identifier (`com.cyberneura.flashcap`), the crate / executable name (`flashcap`), the config directory (`~/.config/flashcap`) and the URL scheme (`flashcap://`) stay lowercase. If you change `productName`, update the Homebrew cask (`url` / `app` in `Casks/flashcap.rb` of `cyberneura/homebrew-tap`) to match, **but only after artifacts with the new name have been published**.

Screenshot capture & annotation app for macOS and Windows (CYBERNEURA-DEV-841 added Windows).

## Commands

- `pnpm install` - Install dependencies
- `pnpm tauri dev` - Start development server
- `pnpm tauri build` - Production build
- `pnpm check` - TypeScript type check
- `pnpm test` - Unit tests for edge-snap / crop-aspect / text-hit / region-select / home-path / shell-commands / thumbnail-browser / capabilities / steps / sensitive-text / edit-history / highlight / spotlight / duplicate / tool-shortcuts (`node --experimental-strip-types`, no test runner dependency)
- `pnpm release [patch|minor|major]` - Bump version and push to main (the push starts the GitHub Actions release build)

The same operations are also listed in `j-menu.yaml` (pick them with `j`).

## Architecture

- **Frontend** (`src/`): SvelteKit 2 + Svelte 5 (runes syntax), TypeScript
- **Backend** (`src-tauri/`): Rust, Tauri 2.x
- **Pages**: `src/routes/+page.svelte` - Main capture UI
- **Components**:
  - `src/lib/ArrowOverlay.svelte` - Arrow annotation overlay
  - `src/lib/MaskOverlay.svelte` - Mask (mosaic/blur/fill) overlay
  - `src/lib/CropOverlay.svelte` - Crop selection overlay
  - `src/lib/edgeSnap.ts` - Edge detection for snapping the crop frame to lines in the image
  - `src/lib/cropAspect.ts` - Aspect-ratio geometry for the crop frame (pure functions)
  - `src/lib/regionSelect.ts` - Geometry for the Windows capture-area overlay (`src/routes/capture-region/+page.svelte`)
  - `src/lib/ThumbnailSidebar.svelte` + `src/lib/thumbnailBrowser.ts` - Saved-images sidebar (see Thumbnail browser)
  - `src/lib/StepOverlay.svelte` + `src/lib/steps.ts` - Numbered step markers (see Step annotations)
- **Types**: `src/lib/types.ts`
- **Preferences**: `src/routes/preferences/+page.svelte`
- **Third-Party Licenses**: `src/routes/licenses/+page.svelte` (see Third-party license notices)

## Key Details

- Screenshots are saved to `$TMPDIR/flashcap/` (configurable in Preferences).
  **Not `/tmp`** — that is mode 1777 and readable by every account on the Mac, and
  what this app writes is whatever was on screen. `flashcap_temp_dir()` /
  `create_private_dir()` in `src-tauri/src/lib.rs` are the only way to build and
  create these directories; `create_dir_all` alone is umask-dependent (0755).
- ESC key exits the app
- Arrow tool for annotation (with white stroke, drop shadow options)
- Mask tool: mosaic, blur, fill modes with 8-direction resize handles
- Timer capture: `screencapture -i -T <delay>` via async Rust command (delay configurable in Preferences)
- Clipboard copy support (image-png feature enabled)
- Settings stored in `~/.config/flashcap/config.json` (see Config file)
- The theme is fixed to dark. `src/app.css` sets the html / body background and `color-scheme: dark`
  (so white does not show through when scrolling bounces), and the windows use `theme: Dark` + `backgroundColor`
  (main in tauri.conf.json, Preferences / Licenses in WebviewWindowBuilder) so the frame and the backdrop before the first paint are dark too.
  When you add a window, give it the same two settings.

## Config file (src-tauri/src/config.rs + src/lib/config.ts)

All settings live in a single file, `~/.config/flashcap/config.json` (shared by macOS / Windows),
as a flat JSON object of keys and values. It holds the Preferences items, auto-copy,
the tool settings (`arrow_settings` / `mask_settings` / `shape_settings` / `text_settings` /
`crop_snap`) and the shell commands (`shell_commands`). tauri-plugin-store and
localStorage are not used.

- **Do not write the real path of the home folder** (the file is shared between machines). The frontend
  folds paths to `~` with `collapseHome()` (`src/lib/homePath.ts`) before saving, and Rust's
  `config::expand_home()` expands them. If you add a setting that holds a path, route it through both.
- The Rust reader (`config::get`) does not cache; it reads the file every time. This is so that hand edits and
  content synced from another machine are picked up as they are.
- Writes go through `config_set`, one key at a time, and inside a Rust-side lock it does "read → replace →
  write to a temporary file and rename". **If the JSON is broken, it does not write** (overwriting would wipe
  all the other settings). So as not to break the link when config.json is a symlink (dotfiles management),
  it renames next to the real file the link points to.
- After writing, it emits `config-changed` (the payload is the key) to all windows. The main window
  only reloads the Preferences keys (`PREFERENCE_KEYS`).
- Tool settings are written in batches with `setConfigDebounced` on every change. **Do not write until loading
  has finished (`configLoaded`)** — otherwise the defaults present right after startup overwrite the saved values.

## Shell commands (src-tauri/src/shell_command.rs + src/lib/ShellCommandMenu.svelte)

In Preferences you register several "name, shell, command" entries, and while an image is open you run them from the
terminal button at the right end of the toolbar. macOS only (on Windows neither the button nor the settings field is shown).

- **Pass the image path in the environment variable `IMAGE_PATH`; do not do string substitution.** `${IMAGE_PATH}`
  in the command is expanded by the shell itself. With substitution, a file name opened from Finder such as
  `a;rm -rf ~.png` would be executed as a command.
- The shell is started with `-l -c` (login shell) with the home folder as cwd. A GUI app's
  PATH does not include Homebrew, so this makes it read `~/.zprofile` and the like.
- The frontend passes only the id and the image path; Rust looks the contents up again from the config file.
- Before running, `saveCompositeToFile()` burns in the annotations and cropping (same as copying the path).
- Logs go to `$TMPDIR/flashcap/shell-logs/`, one file per run (stdout + stderr, with the
  command and IMAGE_PATH at the top and the exit status at the end). Even when the shell cannot be started, this is written to the log and
  returned as a failure. Completion / failure is shown with a svelte-sonner toast, and "View log" opens the log.
  `open_shell_log` only opens files directly inside the log directory.

## Frontend Ready Handshake (src-tauri/src/lib.rs)

On a cold start (WebView not yet loaded), `app.emit()` has no recipient and events are silently
dropped. `FrontendHandshake` (`Mutex<{frontend_ready, capture_pending, pending_files}>`) prevents this in one
place by waiting for frontend-ready before emitting. It holds two kinds of work:

- **Start capture** — three routes (`--capture` cold start / single-instance relaunch /
  the macOS `flashcap://capture` URL scheme). Goes through `request_capture()`.
- **Open images** — Finder's "Open With" / dropping onto the Dock
  (both are `RunEvent::Opened`), the single-instance argv, and the cold-start argv.
  Goes through `request_open_files()`.

Points to watch:

- **The routes do not call `show()`**. `request_capture()` only emits `do-capture`.
  The window is shown by the frontend's `captureScreen()` with `show()` + `setFocus()` after the capture finishes.
  → Adding `show()` on the route side causes a show→hide flicker, so do not add it.
  `request_open_files()` also does not show while not ready (you would only see a white window.
  The reason here is showing before the first paint, not flicker, so the situation differs from the capture side).
  **The 2-second failsafe also does not show the window while a capture is reserved or images are being held.**
  Showing it there would break "do not show while not ready" through the back door.
- **The frontend emits `frontend-ready` exactly once, after every listener for held work has been
  registered** (`Promise.allSettled([unlistenDoCapture,
  unlistenOpenFile])` in `+page.svelte`). **If you add an event to be held, add it to this array too.**
  If you forget, you are back to a hard-to-reproduce bug where events are lost only on a cold start.
- Finder's "Open With" arrives as `RunEvent::Opened` whether the app is running or cold-starting
  (no second process starts for a running app). Only `flashcap foo.png` from a terminal
  arrives via argv.
- **When argv contains both `--capture` and images, capture wins**
  (`image_args_for_startup`). The single-instance route returns after `request_capture()` and
  does not look at file arguments, so if only a cold start processed both, the result of the same command would
  depend on the startup state (`loadImageFile()` and `captureScreen()` would run in parallel and the last one would win).
- In the `finally` of `captureScreen()`, the guard flag `isCapturing` is set back to false **after all** the
  `show()`/`setFocus()` awaits have finished. Resetting it earlier lets a `do-capture` arriving during restoration start a new capture
  and interleave.

## Crop Tool (src/lib/CropOverlay.svelte + src/routes/+page.svelte)

- On apply, `imageBase64` is replaced with the cropped PNG, and the annotations are not burned in but translated
  by the crop origin. Annotation coordinates are in natural-resolution pixels, so this translation alone keeps
  `renderComposite()` and each overlay in the same coordinate system.
- **Only masks are clamped to the cropped area**. `getImageData` in `renderComposite()` returns
  transparent black outside the canvas, so if a mask sticking out is left as is, blur burns the alpha in too via
  `putImageData`, and mosaic makes the edge blocks semi-transparent, so **the original image you meant to hide shows through**.
  arrow / shape / text are only clipped by the canvas, so their coordinates can be kept as they are.
- At the same time, set `imageModified`. `saveCompositeToFile()` decides whether to write with
  `needsFileWrite` (= there are annotations or `imageModified`); if this is not included, after cropping,
  ⌘C (copy path) / drag hands over **the uncropped original file on disk**.
  Do not remove the check and always write back (it causes pointless re-encoding degradation in lossy formats such as JPEG).
- The undo snapshot (`EditSnapshot`) also holds the image base64 and dimensions. However, `imageModified` is
  not reverted by undo — if the file has already been written back, rolling back leaves the disk changed, so
  "memory = file" no longer holds.
- After replacing the image, call `bumpImageRevision()`. MaskOverlay's mosaic samples from the displayed `<img>`,
  so unless the revision is bumped after decoding finishes, it freezes with a picture taken from the old image.
- While the frame covers the whole image, dragging inside it is treated as "redraw the area". There is no outside of the frame,
  so if it fell back to move there would be no way to redraw.
- If `undo()` replaces the base image, close the crop tool. `Cmd+Z` goes through even while crop is shown,
  so if it stays open, a frame in the old image's coordinates is left sticking out of the new image.

### Snapping to edges (src/lib/edgeSnap.ts)

- Pixel differences are taken over **4 RGBA channels**, but the normalization divisor **stays 3**. Opaque images
  (screenshots = the main use) always have an alpha difference of 0, so as long as we divide by 3 the scores are
  exactly the same as before alpha was introduced. Dividing by 4 would lower the main use's scores by 25% across the board and change what the
  thresholds mean. Without looking at alpha, a boundary where RGB is identical and only alpha differs, such as
  opaque black on a transparent background, would never be picked up.
- Lines are detected with projection profiles (pixel differences summed per column/row). **An absolute threshold alone is not enough** —
  photos and textures produce differences wherever you cut, so every column becomes a candidate and the snap target is effectively random.
  It additionally requires prominence relative to the neighborhood average (`PROMINENCE_RATIO`) to tell them apart.
- The detection result is a list of "lines" (`EdgeSnapRun`), and **each line has two boundaries, start / end**. A 1px border
  creates two boundaries, left and right, and both "cut including the border" and "cut excluding it" are valid intentions,
  so do not round to one of them.
- **The snap tolerance is in screen px (`6 / scale`); detection is in image px**. The two only mesh at
  100% display; when a Retina screenshot is shown scaled down, 6 screen px spread to 25+ image px,
  candidates crowd together, and you can no longer place the frame's edges anywhere other than on lines. So `snapPositions()`
  **thins them out every time based on the display scale** (of lines closer together than 3x the tolerance, only the stronger one is kept).
  Do not bake the thinning into the cached detection result — the scale changes when the window is resized.
- While drawing, if snapping would make the result smaller than `MIN_SIZE`, that axis is not snapped. mouseup
  discards redraws smaller than the minimum size, so snapping would make the whole selection disappear.
  **This is needed for both the end point (`snapEndpoint`) and the start point (`drawAnchor`).** The start point is snapped
  at mousedown, but at that moment the drag direction is not yet known. When you drag from just before a line toward it,
  the span is reduced by however far the start point moved (start 96 snaps to line 100 → dragging to 104 still gives a width of 4).
- Detection runs once per image and is tied to `imageRevision`. It is **synchronous on the main thread**
  (not offloaded to a worker), so it runs after the opened crop tool has been painted.
- **When the image is replaced, also invalidate the cache generation (`cropSnapLinesRevision`) synchronously.**
  `imageRevision` is updated only after waiting for the decode, so if crop is reopened in that gap,
  "generations match (both old)" holds, detection returns early with nothing, and even after the revision
  goes up it is not re-detected (snapping silently stops working).
- ON/OFF is the magnet toggle in the crop toolbar. It is stored as `crop_snap` in the config file.

### Locking the aspect ratio (src/lib/cropAspect.ts)

- The `1:1` / `16:9` toggles in the crop toolbar. They are mutually exclusive, and pressing again releases. **Not remembered
  across sessions** — it would be surprising if cropping were locked on its own at the next launch (handled differently from snapping).
- All geometry is pure functions in `cropAspect.ts` (tests in `tests/crop-aspect.test.mts`). Corner handles and redrawing can be expressed with the same
  `aspectRectFromCorner()` (a redraw is "a corner with the drag start point fixed").
- Fit to the larger axis (cover). Fitting to the smaller one makes the frame shrink and stop following when the
  pointer moves off the diagonal.
- With edge handles, the derived side **keeps the frame's center only while there is room, and slides once it hits the image edge**.
  When dragging the right edge there is no way to decide whether the height should grow up or down, so the center is kept by default.
  However, **do not make keeping the center a constraint (upper bound)** — if "the range you can grow while keeping the center"
  is the upper bound, then when the frame touches an edge it equals the current size and **the handle is completely dead**
  (however far you drag, it is rounded back). Moving the frame to an edge always triggers this.
- **Snapping loses to the aspect ratio**. Only the axis being moved is snapped; the other is derived from the ratio.
  Snapping both axes breaks the ratio.
- **While locked, "is the redraw too small?" cannot be judged from the resulting size**. To keep the ratio,
  the frame automatically grows to the minimum size, so even a mere click produces a MIN_SIZE frame.
  Judge it with `drawMoved` (whether the raw pointer moved by `CLICK_SLOP` or more).
  **Do not reuse `MIN_SIZE` as the threshold here** — even after dragging only 3px, a valid frame is shown
  on screen, so it would vanish the moment you release. The judgement uses coordinates **before** snapping
  (if snapping pulls the end point back to the start point, you would be "not moving" while dragging).

## Undo / Redo (src/lib/editHistory.ts + src/routes/+page.svelte)

The history of annotations and cropping uses snapshots (CYBERNEURA-DEV-1263 added Redo).
⌘Z = Undo, ⌘⇧Z = Redo (on Windows Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y).

- Pushing and popping (`recordEdit` / `stepUndo` / `stepRedo`), key detection (`historyActionForKey`) and
  text-field detection (`isTextEntryTarget`) are pure functions in `editHistory.ts` (`tests/edit-history.test.mts`).
  `+page.svelte` only has `captureSnapshot()` (a copy of the current state) and `applySnapshot()` (applies it; shared by Undo / Redo).
- **Call `pushUndo()` right before an edit**, as before. `pushUndo()` discards the Redo history.
  Undo pushes the current state onto the Redo side before popping, and Redo does the reverse. In the two places where the image is replaced by a different one
  (capture / paste / open file, and switching to video), `clearEditHistory` empties both.
- **While a text-entry field (TEXTAREA / contentEditable / an INPUT of a type you type text into) has focus, do not intercept.**
  ⌘Z / ⌘⇧Z while editing a text annotation act as the field's Undo / Redo (on macOS the native Edit menu's
  `PredefinedMenuItem::undo` / `redo`, on Windows WebView2), and the annotation history does not move.
  **Color pickers and slider INPUTs are not counted as text fields** — focus remains on them after use, so counting them would
  make ⌘Z right after changing a color do nothing.
- The native Edit > Undo / Redo only affects text entry in the WebView (it knows nothing about the annotation history), so
  outside text fields only the JS side acts, and nothing runs twice.
- If **an edit that is not pushed onto Undo** (`updateActiveAttribute`, which changes e.g. the color of the text being typed from the toolbar)
  comes after an Undo / Redo, the Redo history is discarded before the next Undo / Redo (`dropStaleRedo`).
  `undo()` / `redo()` record the annotations' signature (`annotationSignature()`) right after applying via `markApplied`, and compare it with the current signature next time.
  Without discarding, Redo would silently erase that edit. If you add another non-pushed edit, it is caught here as long as it rewrites the annotation arrays.
- Do not make the history `$state` (it is not read from the template; do not wrap the snapshots in proxies).
  If you add Undo / Redo buttons, keep only the counts in a separate `$state`.

## Changing the look of the selected annotation (restyle in +page.svelte, CYBERNEURA-DEV-1320)

Select an annotation after placing it, and change its color, thickness, white outline and shadow from the toolbar (for text: font size, bold, italic; for masks: mode and fill color).
While something is selected, **only that annotation is rewritten, and the settings (the settings for the next annotation drawn = config) are not changed**. With no selection, the settings change as before.

- Each overlay's `selectedId` (and `editingId` for TextOverlay) is `$bindable`, and `+page.svelte` binds it.
  **There is only one selection across all overlays**: when one is selected, an `$effect` calls `deselect()` on the others
  (so that Delete and toolbar changes do not apply to two). Switching tools (`deactivateAllTools`) clears all of them as before.
- The toolbar is given `arrowPanel` / `shapePanel` / `maskPanel` / `textPanel` / `stepPanel` (the selected annotation's values or the settings),
  and if `selectionKind` is set it shows that kind's settings even if the tool is not active. For shapes it shows the fields for the selected type (`selectedShapeType`)
  (select a highlighter while the rectangle tool is active and you get the highlighter's fields). **Toolbar does not rewrite the settings directly with `bind:value`** —
  it passes changes to +page.svelte via `onUpdate…Setting(key, value, continuous)`, which routes them depending on whether something is selected.
- **Undo is one step per change**. Dragging the color picker (a stream of `oninput`) is not pushed while `styleEditKey` (annotation kind:id:attribute) stays the same,
  and is split by the picker's `change`, a different attribute / annotation, a different edit (`pushUndo`), or Undo / Redo. Changes that do not change the value are not pushed.
- Exceptions: **text being typed** (new, or being re-edited after a double-click) affects both the settings and the text being typed, as before
  (`updateActiveAttribute`; text in the middle of being written is a continuation of "the next annotation drawn"). **Spotlight darkness** is
  a single value for all spotlights, so it changes the setting even while something is selected. Whether to switch a mask's mode away from fill is up to the user
  (see the section on automatic redaction of secrets). The highlighter color goes into the shape's `color` (the highlightColor field in the toolbar).
- An annotation just drawn is selected, so touching the toolbar right afterwards changes only that annotation (the settings for the next annotation do not change).
  Steps used to affect "both the selected badge and the settings", but this was aligned with the other annotations.

## Duplicating annotations (src/lib/duplicate.ts, CYBERNEURA-DEV-1321)

- **⌘D / Ctrl+D** (`handleKeydown` → `duplicateSelection` in `+page.svelte`): appends a copy of the selected annotation (`selectedArrow` etc. from
  "Changing the look of the selected annotation") shifted down-right by `DUPLICATE_OFFSET` (16 image px x `annotationScale`), and selects the copy.
  One `pushUndo`. The check `isDuplicateKey` accepts only the OS primary modifier alone (⌘⇧D etc. are different), and it does not react on form controls,
  during IME composition, or on key auto-repeat (the same `isFormControlTarget` as `toolForKey`). ⌘C / ⌘⇧C remain copy path / copy image; there is no copy & paste of annotations.
- **Option / Alt+drag** (at the start of each overlay's `handleMouseDown`): pressing an annotation while holding Option waits with `dragging = "duplicate"`,
  and on the first movement beyond `exceedsDragSlop` it calls `onBeforeMutate` → adds a copy and selects it; from then on the copy moves as a normal move
  (duplicate plus move is one Undo step). Releasing without moving does not duplicate and selects the original. It does not matter whether it was already selected (the pressed annotation is the source).
  **Arrows move as a whole only when duplicating** (`move-all`; a normal arrow drag moves only an end point). Text is excluded while being typed.
- Duplicating a step uses `stepsWithDuplicate` (appends and calls `renumber`). The copy gets the next number, and the original's number does not change.
- Tests are in `tests/duplicate.test.mts` (key detection, offset, step numbers: duplicate → delete → Undo).

## Single-letter tool keys (src/lib/toolShortcuts.ts)

A single letter with no modifier switches the annotation tool (CYBERNEURA-DEV-1265). `A` arrow / `R` rectangle / `E` ellipse /
`H` highlighter / `S` spotlight / `N` step / `T` text / `M` mask / `C` crop. Same keys on macOS and Windows.
These are not app-wide global hotkeys, only keydown while the window has focus.

- The table `TOOL_SHORTCUTS` and the check `toolForKey` are pure functions (`tests/tool-shortcuts.test.mts`). The last branch of
  `handleKeydown` in `+page.svelte` calls it, and `toggleToolByShortcut` calls **the same `toggle…Tool` as the toolbar button**
  (pressing again turns it off). The key labels in the tooltips come from the same table (`toolShortcutLabel`).
- **Conditions where it does not react**: with a modifier (⌘ / Ctrl / Alt / Shift) (to let ⌘C and friends through), when focus is on
  INPUT / TEXTAREA / SELECT / contentEditable (while typing a text annotation, the path field, the thickness select — a select jumps
  to options on letter keys, so reacting would affect both), during IME composition (`isComposing` / `key === "Process"`), on key auto-repeat,
  when there is no image or in video mode, and while capturing or recording.
- **If you add a tool, add a key too.** Even the same letter as an existing ⌘ shortcut (C / V / S / Z / Y) does not
  conflict without the modifier (`C` is used for crop). Esc / Enter / Delete are used by other branches and by each overlay, so do not assign them.
- The check differs from Undo / Redo's `isTextEntryTarget` (`editHistory.ts`): that one only looks at text-entry fields because it should still work
  when focus remains on a color picker or slider. This one stops on all form controls.

## Text annotations (src/lib/TextOverlay.svelte + src/lib/textHit.ts)

- **A single click "selects" / "moves"; re-editing is a double-click** (CYBERNEURA-DEV-695).
  Pressing an already-selected one starts a move, so a branch that enters editing on mousedown would
  never be reached even if written (that used to be the case: the branch remained, but the move start before it
  returned). The only entry point to editing is `ondblclick`.
- **Call `onBeforeMutate` (= push undo) on the first actual movement, not on press.**
  Calling it on press pushes a "nothing changed" state onto undo every time you re-select or double-click.
  Whether it moved is decided by `exceedsDragSlop`, 2px, the same as `CLICK_SLOP` in
  `CropOverlay`. Exact equality (dx === 0) is not enough —
  the second press of a double-click also starts a move, so if the finger wobbles 1px the text shifts.
- **Undo for rewriting text is also pushed only once, on the first input.** `handleEditInput` calls
  `onTextsChange` on every keystroke, so pushing there each time would make undo roll back keystroke by keystroke.
  Conversely, if nothing is ever pushed, ⌘Z right after rewriting rolls back **a different operation**. Newly created ones
  are already pushed in `handleMouseDown`, so only rewrites of existing annotations are pushed here.
- Hit testing and boxes are in `textHit.ts`. Which one to select, which one to edit and which cursor to show are
  all decided by these boxes, so they are written in a form that needs neither the DOM nor a mouse, and can be called directly from the tests (`tests/text-hit.test.mts`).
  **Do not measure the actually rendered glyphs** — `getBBox()` only answers after rendering, and calling it on every mouse move would trigger a layout recalculation every frame.

## Toolbar icons (src/lib/Toolbar.svelte)

Icons are unified on Bootstrap Icons (the `bootstrap-icons` package, with the CSS font imported in `app.css`)
(CYBERNEURA-DEV-1215 replaced the ones that were "not intuitive". The policy is to use an existing library rather than drawing our own).
The mapping is as follows, and **do not reuse a similar glyph for a different meaning** (eye-slash = hide, incognito = automatically hide secrets,
images = saved images, image = this image).

| Button | Icon | Meaning |
|---|---|---|
| Thumbnail browser | `bi-images` | List of saved images |
| Arrow / rectangle / ellipse / step / text | `bi-arrow-up-right` / `bi-square` / `bi-circle` / `bi-1-circle-fill` / `bi-fonts` | The shape of the thing being drawn |
| Highlighter | `bi-highlighter` | Semi-transparent fill that makes text stand out (a type of the shape tool, like rectangle / ellipse) |
| Spotlight | `bi-brightness-high` | Darkens outside the area (a type of the shape tool). The darkness picker is `bi-moon` |
| White outline / shadow | `bi-border-outer` / `bi-shadows` | Outline / shadow (shared by the four tools) |
| Bold / italic | `bi-type-bold` / `bi-type-italic` | |
| Mask | `bi-eye-slash` | Hide. Modes are `bi-grid-3x3-gap-fill` (mosaic) / `bi-droplet-half` (blur) / `bi-paint-bucket` (fill) |
| Redact sensitive text | `bi-incognito` | Automatically hide secrets (macOS) |
| Crop / snap | `bi-crop` / `bi-magnet` | |
| Open save location | `bi-folder2-open` | |
| Drag to hand over | `bi-box-arrow-up` | Share (the same arrow as macOS sharing) |
| Copy path / copy image | `bi-link-45deg` / `bi-clipboard` | Path (reference) / image to the clipboard (`bi-image` is not used because its picture is easily confused with "this image". CYBERNEURA-DEV-1309) |
| OCR: whole image / select an area / capture the screen | `bi-body-text` / `bi-textarea-t` / `bi-bounding-box` | Text / an area within the image / an area on screen |
| Timer capture / capture / record | `bi-stopwatch` / `bi-camera` / `bi-record-circle` | |

The temporary success indicator is `bi-check-lg`, and in-progress is `bi-arrow-repeat` (spinning).

## Highlighter (src/lib/highlight.ts + src/lib/ShapeOverlay.svelte)

A semi-transparent filled rectangle that makes text or lines stand out (CYBERNEURA-DEV-1264). It is the third `ShapeType`, `highlight`, and
placing, moving, resizing, Delete and Undo reuse the rectangle's behavior as is (it goes into `shapes`). In the toolbar it is
`bi-highlighter`, next to the ellipse. No smart highlighting that relies on text recognition has been added.

- **The color is `shape_settings.highlightColor`** (default `#ffe600`). It is separate from the stroke color (`color`) so that
  the highlighter does not turn red while the frame setting is red. Thickness, white outline and shadow are not shown for the highlighter, and `whiteStroke` / `dropShadow` are set to false when placing.
- **How it is composited is decided per area by `highlightPaint()`**: on a light background, multiply (the background gets colored, black text stays as is);
  on a dark background whose average luminance is below `DARK_LUMA_THRESHOLD` (dark-mode screens), normal compositing at opacity 0.4
  (with multiply the background just gets darker and no color shows). The average luminance is taken from a `LumaMap`, the image shrunk to 256 cells on its long side.
  **The screen (`lumaMap` prop of `ShapeOverlay`) and the export (`renderComposite`) both go through the same function and a map built the same way
  (`buildLumaMap`).** Changing the decision on only one side makes what you see and the saved image disagree.
  The map is only built while there is a highlighter or the highlighter tool is active.
- **On screen it is drawn not in the svg but as a `<div>` with `mix-blend-mode`** (`src/lib/HighlightLayer.svelte`. Display only;
  interaction is received by ShapeOverlay's svg, which draws only the selection frame and handles). Whether mix-blend-mode applied to an element inside an svg
  blends with an `<img>` outside the svg depends on the rendering engine, so we do not rely on it (it blended in Chrome; WKWebView is unverified).
  For the div approach, the structure "a div layered over an img inside a container with transform + overflow-hidden" was rendered in Chrome and confirmed to match the same picture
  drawn on a canvas with `multiply` / `globalAlpha` (multiply on a light background, 0.4 on a dark background,
  and also that multiply on a dark background shows no color).
- **The stacking order is just above the image and below masks. The highlighter is not shown in mask areas.** `HighlightLayer` sits between `<img>` and
  `MaskOverlay` (rectangle, ellipse, text, arrow and step are higher still), and cuts out the mask rectangles with `clip-path`
  (`highlightClipPath` → `subtractRects`). Placing it underneath is not enough: fill and mosaic cover from above, but **the blur
  preview uses `backdrop-filter`**, which blurs the picture below as is, so a remaining highlighter would mix its color in.
  **The export cuts out the same rectangles (`roundedMaskRect`) too**: right after `drawImage`, each highlighter is painted onto a small layer,
  the mask rectangles are `clearRect`ed, and then it is composited. Drawing without cutting out would let the later mosaic / blur pick up the highlighter's color.
  Only the one being drawn is drawn by ShapeOverlay itself (it appears above masks only until it is committed).
- Hit testing is anywhere inside (`hitsHighlight`). Unlike the rectangle, which only reacts near its stroke.
- The `type` in the settings is checked with `parseShapeType`, and unknown values fall back to `rect` (`pick` only checks typeof).
- Tests are in `tests/highlight.test.mts` (type validation, the map, average luminance, switching the compositing, hit testing).

## Spotlight (src/lib/spotlight.ts + src/lib/SpotlightLayer.svelte)

Darkens outside the enclosed area (CYBERNEURA-DEV-1322). It is the fourth `ShapeType`, `spotlight`, and placing, moving,
resizing, Delete and Undo reuse the rectangle's behavior as is (it goes into `shapes`). In the toolbar it is
`bi-brightness-high`, next to the highlighter, and the single-letter key is `S` (⌘S / Ctrl+S with the modifier remain save).

- **The darkness is a single value, `shape_settings.spotlightDarkness`** (default 0.5; 30 / 50 / 70 / 85% in the toolbar). It is not
  stored per shape (the dark area is one layer for all spotlights). Changing it also affects spotlights already placed, and it is not pushed onto Undo.
  The value from the config file is checked to 0 < x <= 1 with `parseSpotlightDarkness`. Color, thickness, white outline and shadow are not used (false when placing).
- **The area to darken is returned by `spotlightDarkRects()` as "the whole image − (union of spotlights ∪ masks)", as non-overlapping
  integer rectangles** (reusing `subtractRects`). The screen (`SpotlightLayer`'s clip-path) and the export (`renderComposite`'s
  fillRect) both go through this function. Do not switch to punching holes with clip-path evenodd — where two holes
  overlap, the overlap goes dark again. Rounding to integers avoids seams between adjacent rectangles (thin anti-aliased lines).
- **The stacking order is above the highlighter and below masks. Arrows, frames, text and steps are not darkened** (`SpotlightLayer` sits between
  `HighlightLayer` and `MaskOverlay`; in the export it comes right after the highlighter). Mask areas are not darkened (like the highlighter,
  masks take priority. The blur preview uses backdrop-filter, so a remaining dark area would disagree with the export).
- The one being drawn is passed to `shapeDraft` in `+page.svelte` via `ShapeOverlay`'s `onDraftChange` and
  added to `SpotlightLayer` (the dark area is computed from all spotlights together, so ShapeOverlay cannot draw it alone).
- Hit testing is anywhere inside (reusing `hitsHighlight`). Coordinates are natural-resolution px with no thickness, so Retina's
  `annotationScale` is irrelevant.
- **Inside the `.ts` modules in `src/lib`**, when importing values from another `.ts` module, write the extension, as in `./highlight.ts`
  (tests load them directly with `node --experimental-strip-types`, and node does not add extensions; see tsconfig's
  `allowImportingTsExtensions`). Imports from `.svelte` files are resolved by vite and never read by node, so they can stay extensionless as before
  (`./spotlight` / `$lib/spotlight`). Tests are in `tests/spotlight.test.mts`.

## Automatic redaction of secrets (src/lib/sensitiveText.ts + src-tauri/src/ocr_lines.rs)

"Redact sensitive text" next to the mask tool in the toolbar (`bi-incognito`; macOS only, like OCR.
CYBERNEURA-DEV-1210). The `ocr_image_boxes` command (`ocr.rs`) returns, per line recognized by `ocr.swift --json`,
`{ text, box: [x, y, w, h] }` (image pixels, top-left origin), and the frontend's `findSensitive` picks lines containing
emails / IPv4 / prefixed tokens (`sk-` `ghp_` `gho_` `xox[abp]-` `AKIA`) / JWT-like strings / alphanumeric strings of 16+ characters
(containing both digits and letters; ordinary long words and digit-only IDs are not picked up) / card numbers passing Luhn,
and adds a `MaskRect` with that box expanded by 2px on every side to `masks` (subject to `pushUndo`). If there are none,
a toast says so.

- **The mode of automatic masks is always `fill`, regardless of the mask settings** (`redactMaskStyle`, CYBERNEURA-DEV-1253).
  Mosaic does not apply annotationScale to masks, so on Retina it gets as fine as 7px blocks = 3.5 logical px, and
  monospaced tokens could potentially be recovered with Depix-style techniques. Blur can also be readable depending on the radius. If a feature that
  "automatically hides secrets" placed reversible masks, it would betray people who share thinking they were hidden. The color is the mask setting's color, but if it is not an opaque
  `#rgb` / `#rrggbb`, black is used (hand-editing config.json could put `transparent` etc. in there). Changing the mode of
  individual masks by hand after they are placed is left to the user.

- `ocr.swift --json` is not combined with `--region` (with a region, the boundingBox is relative to the region). The Rust side always
  calls it on the whole image. The output without `--json` (newline-separated strings) is unchanged.
- The output format is pinned by the tests of `parse_ocr_lines` in `ocr_lines.rs` (Swift cannot run on this host).
  When changing the format, fix the Swift and the Rust tests together.
- The OCR input is the base image without annotations (`imageBase64`). It also picks up text already hidden by masks, so pressing twice stacks
  masks in the same place (harmless as long as it stays hidden).
- The IPC command `ocr_image_boxes` is registered in `APP_COMMANDS` in `build.rs` and in `capabilities/default.json`.

## Annotations on Retina captures (`annotationScale`, CYBERNEURA-DEV-1237)

Annotation coordinates are in the image's natural-resolution px, so as is, lines and text would look half-size on Retina captures.
A captured image carries `ScreenshotResult.annotation_scale` (2 or 1), and `annotationScale` in `+page.svelte`
multiplies the thickness, white outline, shadow, text and step size by it **only when drawing**.

- **Keep the `thickness` / `fontSize` / `size` values as the toolbar values.** If the multiplied values were saved in the annotation,
  syncing with the toolbar (changing the attributes of the selected annotation) would lose or double the scale.
- **There are two drawing paths, `renderComposite` and each overlay, and both apply it.** Hit testing
  (the `annotationScale` argument of `textHit.ts` / `steps.ts`, the arrow and shape thresholds) uses the same scale.
- It is not applied to masks (mosaic granularity, blur radius). They are neither lines nor text, and the Preferences values mean image px.
- Detection: `png_density_scale` in `share_scale.rs` (the PNG pHYs; macOS screencapture writes 144 dpi on Retina
  and 72 dpi at 1x) → if absent, the scale of the monitor showing the main window (Windows). **Do not decide from the monitor alone** —
  with a MacBook + a 1x external monitor, the monitor captured and the monitor holding the window disagree.
  Unlike `should_scale_down`, it does not compare against the logical size (small Retina area selections are drawn at 2x too).
- Images opened by paste / from a file / from the sidebar are always 1 (the capture scale is unknown. The PNGs Flashcap writes back
  have no pHYs, so using pHYs on a reopened image would not be consistent).

## Downscaling for sharing (src-tauri/src/share_image.rs + share_scale.rs)

"Share at 1x" in the Save Location section of Preferences (`share_scale_down` in the config file, default OFF.
CYBERNEURA-DEV-1211). When ON, **only images handed off for sharing** — ⌘⇧C image copy / drag from the toolbar
(`copyImage` / `handleDragFile` in `+page.svelte` → `make_share_image`) and the image mode of auto-copy after capture
(`decode_image` in `auto_copy.rs`) — are halved if they were captured on a Retina (scale 2 or higher) monitor and are larger than
the logical size. ⌘S write-back and the sidebar images stay at full size.

- Detection and downscaling are in `share_scale.rs` (`should_scale_down` / `maybe_scale_down`; independent of Tauri and unit-testable).
  The monitor is the one holding the main window (or the main monitor if none), with logical size = `size / scale_factor`.
- **Images opened by paste / from a file are not downscaled** (the capture scale is unknown). The frontend keeps
  `imageFromCapture` via `applyScreenshotResult(result, source)` and passes it to `make_share_image` as `captured`.
- **Copy passes the current PNG (`bytes`) as `data_base64` to be downscaled, and does not re-read the file** (after an undo,
  `needsFileWrite` is false and the write-back is skipped, so the file still holds old annotations). Drag hands over the
  file itself, so it reads `path`.
- The downscaled PNG goes to `$TMPDIR/flashcap/share/<original name>.png` (`create_private_dir`, writes that do not follow symlinks), and
  `cleanup_share_dir` deletes it at startup. For copy, `ShareImage.data` (base64) is returned, and the frontend does not
  re-read the file. When not downscaling, the original path is kept (`data` is null). Even on failure, sharing continues at full size.
- The IPC command `make_share_image` is registered both in `APP_COMMANDS` in `build.rs` and in `capabilities/default.json`.

## Step annotations (src/lib/StepOverlay.svelte + src/lib/steps.ts)

Numbered step (counter) badges (CYBERNEURA-DEV-1209). The round `1` button in the toolbar, between arrow / shapes and text.
While the tool is active, a click appends to `steps` with `number = steps.length + 1`;
the selected badge is moved by dragging and removed with Delete / Backspace. **When one is removed, `renumber()` renumbers the
following ones** (the array restored by undo also carries `number`, so after restoring the numbers follow the order).
The coordinates (x, y) are the badge's center, and `size` is the diameter (natural-resolution pixels, like the other annotations).

- Hit testing, renumbering and the digit font size are pure functions in `steps.ts` (`tests/steps.test.mts`).
  Like TextOverlay, they are written in a form that needs neither the DOM nor a mouse.
- As with TextOverlay, undo is pushed **on the first movement**, not on press (`exceedsDragSlop`).
  Additions and deletions are pushed immediately.
- **The whole svg receives pointer events only while the tool is active** (a click on empty space = add). Otherwise the
  svg is `pointer-events: none` and only the badges' `<g>` receive them. `StepOverlay` sits above `ArrowOverlay`,
  so receiving on the whole svg would steal arrow selection when no tool is active. mousemove / mouseup during a drag are
  received on window (the drag continues even if the finger leaves the badge). **Pressing on the image outside the svg clears the selection** (window mousedown. Clicks on the toolbar or sidebar do not clear it — so that settings apply to the selected badge): if it stayed,
  selecting an arrow and pressing Delete would run both overlays' window keydown handlers and delete the badge too
  (`stopPropagation` does not stop other listeners on the same window). The reverse direction (select an arrow, then a badge) is handled by
  the parent (`+page.svelte`) calling the other overlays' `deselect()` from `onSelect`.
- The export (`renderComposite`) draws a circle (the white outline is a white circle of radius +2 underneath; the shadow goes on the white outline if there is one, otherwise on the body,
  via the shared offscreen `transferWithShadow`) + a white bold number in the center. It uses the same geometry as the on-screen SVG (`StepOverlay.svelte`),
  so the look and the export do not drift apart. **Keep the stacking order consistent too**: `renderComposite` draws the badges
  last (above the arrows), so in the template `StepOverlay` is also placed after `ArrowOverlay`.
- The settings are `step_settings` (color / size / whiteStroke / dropShadow). Toolbar changes apply only to the selected badge if there is one
  (`updateStepSetting` → `restyle`; see the "Changing the look of the selected annotation" section).
- No IPC commands were added, so `APP_COMMANDS` in `build.rs` and the capabilities were not touched.

## Image display size (src/routes/+page.svelte + resize_window_for_image)

The goal is to display the image "at 1x + a 20px margin around it", and for that **Rust's
`resize_window_for_image` and the CSS implicitly share the same numbers**. Moving only one of them breaks the 1x display.

- `padding = 20.0` ⇔ the viewport's `p-5`
- `sidebar_w = 224.0` ⇔ `w-56` in `ThumbnailSidebar.svelte`. Added only while `thumbnail_sidebar` in the config file is
  true (the window is not resized at the moment the sidebar is toggled; it takes effect the next time an image is loaded)
- `toolbar_h = 49.0` ⇔ `py-2` (8+8) on the `Toolbar.svelte` root + `h-8` (32) of the tallest child `.tool-btn`
  + `border-b` (1). The root's `min-h-[40px]` is a lower bound and has no effect.
  **If you add a taller element to the toolbar, fix this constant too.**
- The denominator of `displayScale` is the viewport's **content box**. `clientWidth` / `clientHeight` are
  dimensions including padding, so using them without subtracting overestimates by 40px, the image overflows the content box,
  and flex centering gets a negative margin → asymmetric "margin on the left / cut off on the right".

## Thumbnail browser (src-tauri/src/thumbnails.rs + src/lib/ThumbnailSidebar.svelte)

A left sidebar toggled by the leftmost toolbar button (CYBERNEURA-DEV-995). Its visibility is
`thumbnail_sidebar` in the config file. It lists up to 300 images directly inside the save folder (the save location in Preferences),
newest creation time first, showing the thumbnail, file name, relative time and size.
The request said "the far left of the menu bar", but we read it as the in-app toolbar rather than the menu bar (app menu / tray)
(the same interpretation as for auto-copy; see the "Auto-copy after capture" section below).

- **Thumbnails are made by Rust** (`saved_image_thumbnail` returns a PNG with a 400px long side as a data URL).
  We do not show full-size images via the asset protocol because that would widen the scope to an arbitrary save location,
  and would also make the WebView load dozens of full-size Retina screenshots.
- **`saved_image_thumbnail` only reads regular files directly inside the save location** (`resolve_within`).
  Symlinks, subfolders and paths going through `..` are rejected. The list (`list_saved_images`) does not include symlinks either.
  This is so that, even if the frontend passes an arbitrary path, it does not become a way to read images elsewhere.
- **Paths in the list are real paths** (the save location is canonicalized before listing). `load_image_file` canonicalizes the path of an opened image,
  so if the paths were kept lexically, when the save location contains a symlink / `..` they would not match "the image currently open",
  the write-back before dragging would be skipped, and the pre-annotation (pre-mask) file would be handed over.
  The paths returned by capture (`load_image_result`) and paste (`save_pasted_image`) are also real paths.
- **`thumbnail_sidebar` is written without debouncing, and opening an image waits for that write** (`sidebarConfigWrite`).
  `resize_window_for_image` reads from the config file on every load, so a click right after showing the sidebar must not read the old value.
  The button cannot be pressed until the setting has been read (a toggle before reading would not be saved and would be reverted by the load).
  Writes run in order, and if the last requested write fails, the display is reverted to the last value successfully written (`sidebarPersisted`)
  (so the screen and the value Rust reads never disagree).
- Thumbnails are requested two at a time, starting with those that come into view (IntersectionObserver; ones merely scrolled past are removed from the queue). Finished ones are
  kept under `thumbnailCacheKey` (path + modification time + size), so images whose annotations were written back are regenerated.
- The list is reloaded when the image is replaced (`filePath` / `imageRevision`), on write-back (`sidebarRefresh`),
  when the save location changes (`save_directory` in `config-changed`), and when the window gains focus.
- **A click "writes back the current image, then opens"** (`openFromSidebar`). The write-back is
  `saveCompositeToFile()` (it writes only when `needsFileWrite`), and if it fails, the image is not opened (opening would lose the annotations).
  Editing remains possible during write-back and loading, so **before starting the write-back, note `editRevision` (a number that an
  $effect advances whenever the image or annotation contents change; it also catches changes that do not push undo = e.g. the color of the selected text),
  and if it has changed just before replacing, write back again** (until it stops changing; no await between the last check and the replacement).
  If the image was changed in between by a capture, paste, etc., do not replace. This is judged by `imageGeneration` (a number advanced on every replacement),
  not by path (captures / pastes in the same second get the same name). Re-check after every await.
  `saveCompositeToFile()` itself also compares the generation before and after drawing, and does not write if it changed (so an old picture never overwrites a new image).
  Loading calls `load_image_file` with `deferResize: true`, and only after deciding to replace does `fit_window_to_image`
  grow the window (it does not grow for an image that will be discarded).
- **Dragging calls `startDrag` after the pointer moves 4px from the press** (it hands over the file itself, like a file in Finder).
  HTML5 drag events are not used. When dragging the image that is currently open, it writes back first, the same as the toolbar drag.
  **A drop onto our own window is treated the same as a click**
  (identified by `sidebarDragPath`. Routing it to a plain `loadImageFile` would replace the image without writing back).

## Auto-copy after capture (src-tauri/src/auto_copy.rs + src/lib/Toolbar.svelte)

- With a button in the main window's toolbar (left of the capture button), a popover offers
  "Don't copy / Copy the image file path / Copy the image data" (CYBERNEURA-DEV-761).
  The button's glyph (the four corners of a selection + the thing being copied) changes with the choice.
  **It was first placed in the menu bar, but the requester meant the in-app toolbar**, so it was moved.
  The menu bar icon is used for something else, described in "Staying in the menu bar" below.
- The frontend (`changeAutoCopyMode` in `+page.svelte`) saves the choice to
  `auto_copy_on_capture` (`none` / `path` / `image`) in the config file, and Rust only reads it on every capture.
  **Unset and unknown values are `none`** (both the frontend's `parseAutoCopyMode` and Rust's `from_setting`)
  — so that captures by people who used the app before this feature existed do not suddenly start overwriting the clipboard.
- Esc in the popover is **caught in the window's capture phase and its propagation stopped**.
  The keydown in `+page.svelte` maps Esc to "close the window", so without stopping it,
  an Esc meant to close the popover would quit the app.
- The popover is normally aligned to the button's right edge, but when the button is toward the left of the window
  (video mode has no path field, so the toolbar is packed to the left) it is aligned to the left edge.
- Only the results of captures (`take_screenshot_interactive` / `take_screenshot_timer`) are copied.
  Paste, open file, OCR and video are excluded. Copying and the notification (osascript in `ocr::notify`) are
  moved to `spawn_blocking`, so returning the capture result (= showing the window again) does not wait for them.
  In exchange, completion order no longer matches capture order, so a per-capture sequence number (`LATEST_CAPTURE`) is
  checked right before writing, and a copy overtaken by a later capture is discarded. **The check and the write
  happen together inside the `CLIPBOARD_WRITE` lock** (so that a slow write that passed the check cannot
  overwrite a later capture's write). Decoding and notification are outside the lock.

## New Image from Clipboard (CYBERNEURA-DEV-1295)

A menu item that gives the same result as "launch Flashcap fresh and paste an image", without relaunching.
It is in the macOS app menu under File > New Image from Clipboard (⌘N) and in the menu-bar resident menu
(Windows has no app menu, so only the resident menu).

- **Both use the id `menu_bar::NEW_IMAGE_FROM_CLIPBOARD_MENU_ID`**, and once pressed they go through
  `menu_bar::handle_menu_event` → the dispatch in `run` (it does not start before the first paint, while recording, during area selection, or while capturing).
  Rust only shows the window and sends `new-image-from-clipboard` to main.
- The frontend's `newImageFromClipboard()` does "read the clipboard → write back the current image
  (`writeBackBeforeSwitching`; if it fails, do not replace) → save with `save_pasted_image` → replace".
  **Unlike ⌘V paste, no confirmation dialog is shown** (it writes back before discarding, so nothing is lost).
  Edits during write-back / saving are caught with `editRevision` and written back again, as in `openOneFromSidebar`,
  and if `imageGeneration` has changed (a capture etc. cut in), the image is not replaced.
- **The window is shown by Rust (`run`) before emitting** (`show_main_window`). At the point where `run`'s dispatch has confirmed
  there is no capture, recording or area selection in progress, the window will not get captured. If the frontend showed it after its processing,
  it could appear in an OCR (`copy_text_on_screen`) or recording area selection started from the menu bar in the meantime (the frontend's `isCapturing` / `isRecording` do not
  know about those). The "no image on the clipboard" toast also appears in this window.
- Before writing back, `textOverlayRef.deselect()` commits the text being typed. Uncommitted text exists only in
  TextOverlay's `pendingText`, and is in neither the annotations nor `editRevision`, so
  as is it would be left out of the write-back and end up on the replacement image.
- **The saved name for pastes is picked with `unused_file_path` as a free name** (`flashcap-paste-<seconds>.png`,
  and on collision `-2`, `-3` ...). Previously pastes in the same second overwrote each other, so this operation (write back → save → replace)
  could delete an image that had just been pasted and written back. ⌘V paste goes through the same function.
- Before the first paint Rust does not send it, so it is not something `frontend-ready` holds.

## Staying in the menu bar (src-tauri/src/menu_bar.rs)

- Turning ON "Keep Flashcap in the menu bar" in Preferences (`show_in_menu_bar` in the config file,
  OFF when unset) puts an icon in the menu bar (CYBERNEURA-DEV-761).
  Clicking shows Open Flashcap / Capture Screenshot / Capture Screenshot with Timer (Ns) /
  New Image from Clipboard / Record Video / Record Video with Timer (Ns) / Copy Text on Screen (OCR) / Quit Flashcap.
  N is the Timer delay in Preferences.
- Preferences calls `sync_menu_bar` after writing to the config file (also on a Timer delay change,
  so the seconds in the menu follow). `sync` only "reads the saved value and makes the icon's presence and wording match",
  so it can be called any number of times. At startup it is called from setup.
- **Do not use the tray's `TrayIconBuilder::on_menu_event`.** A handler registered there is added to an app-wide
  list and is not removed when the tray is removed. Every OFF → ON of the setting adds another, and one click
  runs the capture twice. Events are routed from `app.on_menu_event` in lib.rs to `menu_bar::handle_menu_event`.
- **While resident, closing the main window hides it instead of quitting** (CloseRequested in `on_window_event`).
  Closing would destroy the frontend's WebView along with it, leaving nobody to receive `do-capture` from the menu.
  Esc / ⌘W / the close button all go through here. Quit with ⌘Q or the menu's Quit.
- Captures are delegated to the frontend's `captureScreen()` via `request_capture(app, CaptureKind)`
  (hiding / showing the window is its job too). **The handshake reservation also keeps the kind**
  (`capture_pending: Option<CaptureKind>`). With a bool, a timer capture pressed right after a cold start
  would turn into a normal capture. The `do-capture` payload is the command name, and the frontend treats
  anything other than `take_screenshot_timer` as a normal capture.
- Menu items are dispatched at the top of `menu_bar::run` so they do not overlap an operation in progress (Quit always goes through):
  - **Before the frontend has painted (right after a cold start), only capture and Quit**. Captures are held by the handshake.
    Starting recording or OCR here would make the frontend-ready side show the main window, and it would appear in the
    area selection or capture. Open would only show a white window, so it is ignored.
  - **While recording (including exporting after stopping), start nothing and show the main window** (the stop button
    exists only there, and finishing an area selection makes `start_video_recording` stop the current recording and replace it).
    Exporting is detected with `video::FINALIZING`. `stop_video_recording` takes the recording out of `RecordingState` before waiting,
    so looking only at that would get it wrong. **`is_recording` reads FINALIZING after taking the lock**
    (the stopping side sets it inside the lock).
  - **During recording area selection (including the timer countdown), only Open is accepted; it cancels the area selection
    and returns to the main window**. The overlay during the countdown passes clicks through, so
    after touching another app Esc no longer reaches it, and this is the way to cancel.
  - **While capturing / doing OCR, do nothing** (`CAPTURES_IN_PROGRESS`. The capture commands and
    `screencapture_and_ocr` count with the `CaptureInProgress` guard. OCR counts until text recognition
    has finished — the toolbar's `ocrCaptureRegion()` brings the main window back after it finishes, so
    it would appear in a capture started during recognition). The menu cannot see the frontend's
    `isCapturing`, so Rust counts.
- Recording calls `video::open_region_selector(app, delay_seconds)` directly. With a timer, the countdown after area selection
  uses that many seconds (in 1-second steps, capped at 60) instead of 3-2-1.
  The timer is for tidying up the screen while waiting, so **once the countdown starts,
  `release_region_selector_for_countdown` closes the overlays on the other displays,
  and this overlay passes clicks through** (it does not darken the screen; it only draws the frame and the remaining seconds).
  If the selection is redone on another display, `__regionClear` stops the countdown.
- OCR is `ocr::run_headless_ocr(app, false)`. If the main window is shown, it is hidden only during the capture and
  brought back afterwards without stealing focus.
- The icon is `src-tauri/icons/tray/menu-bar.png` (a 36x36 template image.
  The tray-icon crate sizes it to 18pt high, so it is 1x on Retina. The glyph is the same line-drawn
  camera as the app icon). **Do not redraw it by hand; regenerate it with `python3 scripts/make-tray-icons.py`**
  (it runs with the standard library only).
- Headless OCR (a cold start with `--capture-screen-text`) does not put the icon
  (the whole process exits as soon as it is done, so the icon would just flash and disappear).

## Windows version (CYBERNEURA-DEV-841)

Places that rely on macOS external commands (screencapture / sips / osascript / pbcopy / swift) are separated with
`#[cfg(target_os = "macos")]`, and the Windows side has its own implementation.

- **Capture lets you select a rectangle on the monitor under the cursor** (the Windows version of `capture_screen_to` +
  `src-tauri/src/region_capture.rs` + `src/routes/capture-region/+page.svelte`. CYBERNEURA-DEV-882).
  Windows has no command equivalent to `screencapture -i`, so it first captures the whole monitor with xcap,
  overlays that still image in a frameless, always-on-top overlay (`capture-region-<n>`) at the monitor's physical coordinates,
  lets you drag a rectangle, and crops it out of the captured image. It is committed on release, Enter takes the whole monitor,
  and Esc / right-click / Alt+F4 cancel (an error containing `"cancelled"`). Only the monitor under the cursor
  can be selected. It waits 300ms after hiding before capturing (a window that is still disappearing would get captured).
  The timer only waits, and the cursor position is taken after waiting.
  - **Do not use the Snipping Tool (`ms-screenclip:`).** Its result only comes back via the clipboard
    (overwriting the user's clipboard), the app has no way to know the user cancelled with Esc,
    and the tool itself can be uninstalled and behaves differently between versions.
  - The overlay is created hidden and shown on `capture_region_ready`, once the image has been drawn (showing it first
    would flash the WebView's white backdrop over the whole monitor). If that never arrives, Rust shows it after 3 seconds
    (if it never showed, there would be no way to get out with Esc).
  - Commands (`capture_region_*`) **are accepted only from the window whose label matches the selection in progress**.
    The label carries a per-capture number so that the `Destroyed` of the previous, closing window does not
    cancel the next selection.
  - Rounding of the rectangle (fractions outward, kept inside the image) exists both in the frontend's `src/lib/regionSelect.ts`
    (for display, `tests/region-select.test.mts`) and in Rust's `crop_bounds` (for cropping; runs in `cargo test` on Linux).
- **Recording, OCR and notifications are macOS only**. The frontend hides the buttons using `isWindows` in `src/lib/platform.ts`
  (decided from the WebView's UA), Rust also removes them from the menu bar (the notification area on Windows)
  items (`MenuAction::is_available`), and the commands return an error if called.
  `ocr::notify` does nothing on Windows. HEIC can be neither opened nor saved (there is no sips).
- **The app menu is macOS only** (`set_app_menu`). On Windows the menu would become the window's
  menu bar, and Edit's Ctrl+C / Ctrl+V / Ctrl+Z would be grabbed as accelerators before
  the WebView's keydown. For Preferences, `+page.svelte` catches Ctrl+, and calls
  `open_preferences`.
- **Shortcuts use `isModKey`** (⌘ on macOS, Ctrl on Windows). With Shift held,
  `e.key` is uppercase on Windows, so Ctrl+Shift+C is compared after lowercasing.
- **Files are protected differently**. Unix mode / uid / O_NOFOLLOW do not exist on Windows, so
  `create_private_dir` and friends are split with cfg. Windows relies on %TEMP% (a per-user ACL),
  and `write_without_following_symlinks` rejects symlinks first and then
  opens with `FILE_FLAG_OPEN_REPARSE_POINT`.
- **Canonicalize with `dunce::canonicalize`**. std returns `\\?\C:\...` on Windows, and
  that would show up in the path field and in copied paths. Matching (`OpenedImages` and `write_image_within`)
  must use the same function.
- On Windows, `flashcap://capture` arrives via argv (`is_capture_url`). **On Windows it does not start a capture;
  it only brings the window to the front and flashes the capture button** (CYBERNEURA-DEV-852).
  A URL arriving while a capture is in progress (`is_capture_in_progress`) is ignored (bringing the window forward would get it captured).
  Originally this was because the Windows capture was an immediate capture with no interaction (any web page or email link
  could make it capture the screen non-interactively). Even after 0.6.0 added area selection, this behavior is kept so that a link cannot
  make the overlay fill the screen (a single Enter would capture the whole monitor). `--capture`
  (`is_capture_arg`; cannot be passed from the web) captures immediately as before. There is no `flashcap://ocr`.
- **`src-tauri/tauri.windows.conf.json`** is merged only for Windows builds
  (it removes the HEIC file association and the bundling of ocr.swift).
- The tray icon on Windows is the app icon (`menu_bar::tray_icon`). The macOS
  template image is solid black, so it would be invisible on a dark taskbar.
- `icons/icon.ico` contains six sizes from 16 to 256 and is **full-size with no margin** (CYBERNEURA-DEV-885).
  The master `icon.png` has a roughly 10% transparent margin around it to suit the macOS Dock, and
  making an ico from it as is makes it look a size smaller on the taskbar. To regenerate, use
  `uv run scripts/make-windows-icon.py` (it crops just the opaque area of the master = the rounded plate
  into an ico. It does not touch the icns). **Do not regenerate the ico with `pnpm exec tauri icon`** —
  it builds from the master with the margin, so the icon goes back to being small. The exe, windows, taskbar,
  installer and notification area (`default_window_icon()`) all use this ico.
- **The Windows target can be checked from Linux**. After `rustup target add x86_64-pc-windows-msvc`,
  run `cargo check --target x86_64-pc-windows-msvc`. However, tauri-build demands
  a resource compiler (`llvm-rc`) and stops, so put an `llvm-rc` on PATH that just creates an empty output file
  (check does not link, so the contents are not needed).
  Windows-only tests (`mod windows_tests`) only run on the CI Windows runner.

## Third-party license notices (src-tauri/src/licenses.rs)

`THIRD-PARTY-NOTICES.txt` is generated by `scripts/generate-third-party-notices.sh` (`pnpm notices`);
`licenses.rs` embeds it with `include_str!` and the `licenses` window (`src/routes/licenses/+page.svelte`)
shows it. The entry points are the macOS app menu (Third-Party Licenses... directly below About) and a button in the
About section of Preferences (since Windows has no app menu). **Do not edit it by hand.**

- **When adding or upgrading dependencies, regenerate it and commit**. If a direct dependency (Cargo.toml / package.json) is
  not listed at the version resolved in Cargo.lock / pnpm-lock.yaml, `cargo test` fails
  (Dependabot PRs do not update the notices either, so they fail. Commit the regeneration on the same branch).
- The Rust side uses cargo-about (`cargo install cargo-about --locked --features cli`. Without `--features cli`
  the binary is not installed). `targets` in `src-tauri/about.toml` narrows it to the distribution targets
  (macOS aarch64 / x86_64 and Windows x64). Linux-only crates are not listed.
  flashcap itself is excluded with `publish = false` + `[private] ignore = true`.
  If a crate with a license not in `accepted` is added, `--fail` stops it. **Do not silently widen accepted**
  (GPL / LGPL / AGPL-family licenses change the distribution terms, so check with a human).
- The npm side covers `dependencies` in `package.json` plus the script's `BUNDLED_RUNTIME`
  (svelte / @sveltejs/kit / esm-env / tailwindcss / runed: devDependencies or transitive dependencies that end up in the client bundle).
  If you change frontend dependencies and more of them end up in the bundle, add them to `BUNDLED_RUNTIME`.
- The `@crabnebula/tauri-plugin-drag` npm package declares neither a license nor a repository in its package.json,
  so the script's `MISSING_METADATA` matches it to the Rust crate from the same repository.

## Manual update check (src-tauri/src/update_check.rs)

"Check for Updates…" (CYBERNEURA-DEV-1127). A port of queryfolio's `update_check.rs` (CYBERNEURA-DEV-1079);
the code is not shared (when fixing it, look at both). The entry points are the macOS app menu (directly below
Third-Party Licenses) and a button in the About section of Preferences (the `check_for_updates` command; on Windows only the latter).

- It looks at `api.github.com/repos/cyberneura/flashcap/releases/latest` **only when chosen**. Do not add network access at startup or on a schedule.
- It offers `brew upgrade --cask flashcap` only when running from a `.app` installed by the cask. The check is not whether a Caskroom exists, but
  whether the place the cask installed to (the `target` of the app artifact in `brew info --cask --json=v2`; for older brew,
  the appdir + name in `Caskroom/flashcap/.metadata/config.json`) matches the running bundle (canonicalized)
  **including letter case**. In 0.8.3 the `.app` name changed from `flashcap.app` → `Flashcap.app`, so loosening this could
  run brew for old versions too. **Do not use the output of `brew list --cask flashcap`** — the artifact list
  (`/Applications/Flashcap.app (...)`) is only printed when stdout is a terminal; through a pipe you get the Caskroom find listing instead
  (Homebrew's `cmd/list.rb`). queryfolio's update_check.rs relies on it, so for the same reason it may not be entering the brew path.
- If the appdir was determined only by `HOMEBREW_CASK_OPTS` (present only in config.json's `env`), do not recommend brew. upgrade takes `env`
  afresh from the runtime environment, so when invoked from the GUI it would reinstall into `/Applications` and the new version would not land where the app is running.
- brew is run with `HOMEBREW_NO_ASK` / `HOMEBREW_NO_UPGRADE_QUIT_CASKS` / `HOMEBREW_AUTO_UPDATE_SECS=0`, and
  once done it checks the installed version with `brew list --cask --versions` before recommending a restart.
- HTTP uses reqwest (`default-features = false` + rustls). webpki-roots' CDLA-Permissive-2.0 is in `accepted` in `about.toml`.

## Installing the `flashcap` command (src-tauri/src/cli_install.rs)

`Install Command Line Tools…` in the macOS app menu (below Check for Updates…)
creates a symlink at `/usr/local/bin/flashcap` pointing to the executable inside the `.app` (the same approach as dagscape / beckwell).
If it cannot write, it asks for the administrator password via `osascript`'s `with administrator privileges` and runs only that one operation
as administrator. An existing symlink is replaced; a file that is not a symlink is not overwritten and results in an error.
It does not link to an App Translocation path (an app opened while still carrying the quarantine attribute), since the location changes on the next launch.
Not available on Windows.

- **A launch through the symlink (`current_exe()` is a symlink) makes LaunchServices relaunch the app at the top of `run()`
  and then exits** (running the `.app` executable from a terminal would leave the GUI attached to the terminal).
  If every argument is an existing file, it uses `open -a <bundle> <absolute paths>` (delivered as `RunEvent::Opened`, the same as Finder's "Open With");
  if it contains `--capture` etc., it uses `open -n <bundle> --args <arguments>`
  (the new process's single-instance hands them to the running instance). The relaunched process's cwd is `/`, so
  existing paths are made absolute against the caller's cwd before being passed.
- `/Applications/Flashcap.app/Contents/MacOS/flashcap --capture`, as registered in launchers, is not a symlink,
  so it does not take this path and works as before.

## Command ACL (src-tauri/build.rs + src-tauri/capabilities/)

The app's own commands (`generate_handler!`) can also only be called if allowed by the per-window capability
(CYBERNEURA-DEV-971). `build.rs` declares all commands with `AppManifest::commands`, so
Tauri applies the ACL to the app's own commands too. **Without the declaration, every command can be called unconditionally
from every window of the local pages** — even with capabilities split per window, only the plugin
commands would be restricted (that was the case up to 0.7.0. Even the transparent overlay could run shell commands
with `config_set` + `run_shell_command`).

- **When you add a command, add it in two places**: `APP_COMMANDS` in `build.rs`, and the capability of the window
  whose page `invoke`s it (`"allow-<kebab-case>"`, e.g. `run_shell_command` → `allow-run-shell-command`).
  A command missing from `build.rs` cannot be called from any window, and a command missing from a capability cannot be called from that window.
  Release builds do not show why a call was denied, so this is easy to miss.
- Mapping of windows, pages and capabilities: `main` → `default.json`, `preferences` → `preferences.json`,
  `licenses` → `licenses.json`, `region-selector-*` (recording area selection) → `region-selector.json`,
  `capture-region-*` (Windows capture area selection) → `capture-region.json`.
  **Do not allow commands that are not used** (do not hand config rewriting or shell execution to a transparent overlay).
  **Do not put several windows in one capability** — the permissions become their union and are granted to all of the windows
  (main and Preferences used to share one, and Preferences could reach `config_set` + `run_shell_command`).
  For the same reason, do not put the same window in several capabilities.
- There are two checks. `cargo test` (`app_command_acl_tests`) verifies that `APP_COMMANDS` matches `generate_handler!`,
  and, using the actual ACL embedded by `generate_context!`, that "window x command" access is as the capabilities say
  (if you add a capability file, also add it to the `include_str!` in `expected_access`).
  `pnpm test` (`tests/capabilities.test.mts`) checks per window that "the `allow-*` in each window's capability = the `invoke`
  commands found by following imports from that window's page" (both missing and extra ones fail).
  An `invoke` that passes the command name in a variable cannot be read, so write the possible values in the test's `DYNAMIC_INVOKES`.
  If you add a window, add it to `WINDOW_PAGES` too.
- Writing a permission that does not exist into a capability makes `tauri-build` fail at build time.
- Autogenerated permissions are output to `src-tauri/permissions/autogenerated/` (gitignored).

## Rust Commands (src-tauri/src/lib.rs)

- `take_screenshot_interactive` - Standard interactive capture (`screencapture -i`)
- `take_screenshot_timer` - Timer capture (`screencapture -i -T <N>`, async to avoid UI freeze)
- `write_image_to_file` - Save annotated image (path restricted to the save directory, plus the
  files the user opened themselves in this session). `load_image_file` records each opened file's
  canonical path in `OpenedImages`, which is what allows Cmd+S to overwrite an image that lives
  outside the save directory. Only that file, with a matching real path, is allowed;
  its folder is not opened up. Right before writing, `encode_for_target_format` re-encodes to match the
  extension of the file being overwritten (the image crate for JPEG etc., sips for HEIC). For extensions with no encoder,
  it **does not write and returns an error** — writing it as PNG would make the extension and contents disagree, and since it overwrites, the original could not be recovered either
- Common result loading: `load_screenshot_result()` shared by both capture commands

## Build & Check

- `cargo check` in `src-tauri/` for Rust type check
- `cargo test` in `src-tauri/` for the Rust unit tests (save-path containment, handshake, encoding)
- `pnpm check` for Svelte/TypeScript check
- `pnpm test` for the frontend unit tests (`tests/*.test.mts`)
- Run all four before committing
- To check the GUI side by side with an installed Flashcap, use a separate identifier and HOME
  (the single-instance socket is derived from the identifier; if it is the same, the launch is handed to the installed app and ends there).
  mise (and asdf) node shims depend on HOME, so put the real node binary first on PATH:
  `PATH="<directory of the real node binary>:$PATH" CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup HOME=<scratch> ./node_modules/.bin/tauri dev --config '{"identifier":"com.cyberneura.flashcap.devtest"}' -- -- <image>`
  (the settings go to `<scratch>/.config/flashcap/config.json`). When files directly in the project root
  (such as `flashcap.code-workspace`) are rewritten, vite reloads everything and the image opened via the launch arguments disappears.
  In that case, launching `target/debug/flashcap <image>` again with the same HOME reopens it through single-instance
- Production build: `cargo build --release` in `src-tauri/` (run before push)

## Release (.github/workflows/release.yml + scripts/release.sh)

Distribution is via GitHub Releases. **If the version in `tauri.conf.json` on main is unreleased, a push to main
runs Actions and publishes a signed + notarized universal dmg**. `pnpm release [patch|minor|major]` only
bumps the version, pushes to main and watches that run. Bumping the version inside a PR and
merging it releases the same way. The Homebrew cask (`cyberneura/homebrew-tap`) follows on its own, since the tap
checks the latest Release every hour, so nothing needs to be done from this side.

- **Triggered by `push: main`, decided by the `plan` job** (implemented in `scripts/release-decide.sh`).
  If `releases/tags/v<version>` is 404 it releases (a draft returns 404; to be safe, a 200 with `.draft` true is also treated as unreleased),
  if 200 it does nothing, and anything else (rate limit, outage) fails the job. Reading an outage as "unreleased" would
  try to publish an already-published version a second time. Because it decides by version rather than by diff, squash / rebase /
  direct push all give the same result (no path filter is attached either).
- **Even if unreleased, a version older than the latest published Release is not published** (compared with `sort -V`; it
  prints a warning and skips). publish uses `--latest`, so publishing it would roll the latest back and downgrade the tap too.
  This happens with a revert of a version-bump commit (going back to a never-published 0.2.0, etc.) and when pending runs are processed in the
  reverse of push order (GitHub does not guarantee ordering). A version skipped in the latter case is included in the newer
  version that overtook it, so it is not republished. For the comparison, versions are limited to X.Y.Z.
- **publish also reruns `release-decide.sh` right before publishing**. "Re-run failed jobs" does not rerun the successful `plan`
  and reuses the `release=true` from back then, so if a newer version was published in the meantime it would roll back.
  If it stops, it fails and leaves the draft.
- **Even on a push to main, if that version is already released it ends with a single ubuntu `plan` job**.
  Note that this is not "nothing happens because the version did not change". While a draft remains after a build failure,
  that version is unreleased, so pushing an unrelated commit builds and publishes from that commit.
  `test` runs only for PRs or when the decision is "release" (`!cancelled()` removes the skipped chain from plan).
  `build` / `publish` are gated by `needs.plan.outputs.release == 'true'`.
- **`workflow_dispatch` is kept for rerunning failed runs**. It goes through `plan`, so it cannot publish an already-published version,
  and `plan` rejects dispatches from refs other than main.
- **Builds macOS and Windows** (the build matrix). macOS is
  `Flashcap_<version>_universal.dmg` from `--target universal-apple-darwin --bundles dmg`
  (x86_64 + arm64, signed + notarized). Windows is
  `Flashcap_<version>_x64-setup.exe` from `--bundles nsis` (unsigned). test also runs on the same two-OS matrix.
  The Apple secrets are passed only to the macOS steps.
- **macOS does not use tauri-action; build → dmg notarization → upload are separate steps**.
  The tauri CLI signs, notarizes and staples the `.app`, but only signs the dmg. Without notarizing the dmg,
  a downloaded dmg is `rejected / Unnotarized Developer ID` by
  `spctl -a -t open --context context:primary-signature`. tauri-action does build and upload in a single step,
  so notarization cannot be inserted in between. The dmg goes through `xcrun notarytool submit --wait` (judged both by the exit code and by
  `status == Accepted`; on failure it prints `notarytool log`) → `stapler staple` →
  `stapler validate` → `spctl` checks, then is uploaded with `gh api` (uploads.github.com) to the draft's release ID
  (it first checks that it is still a draft, and deletes any asset with the same name before uploading). Windows, as before, has
  tauri-action do build and upload.
- **A single `draft` job before the builds creates the draft Release, and its ID is passed to tauri-action (Windows) and the macOS upload**
  (the same setup as queryfolio). With the tagName approach, when macOS and Windows finish almost simultaneously,
  both decide "it does not exist" and create two drafts for the same tag. A draft left by a failed run is
  reused, and its target is corrected to that run's commit.
- **The structure is draft → build → publish**. After build succeeds on all platforms, the publish job
  publishes with `gh release edit --draft=false --latest`. On failure it remains a draft.
- **A released version is never published again**. `plan` does not let a published version go to build.
  The macOS upload step checks that the release is still a draft before uploading and stops otherwise, but the Windows
  tauri-action replaces same-named assets without looking at the publication state, so there only the `plan` decision protects it.
  A draft left by a build failure (there is no tag yet, so it looks like 404 to `plan`) is refilled with the same version
  on the next push or dispatch (the `draft` job finds it in the list and reuses it, and
  tauri-action and the macOS upload step replace the same-named assets).
- **Do not remove `--target "${GITHUB_SHA}"` from publish**. The `draft` job also corrects the target, but
  the tag is created at the target on publication, so it is aligned once more right before publishing. If they drift, the tag and
  the artifacts' contents point to different commits.
- **Do not change `tauriScript: pnpm exec tauri` (Windows) or the macOS `pnpm exec tauri build` to `pnpm tauri`**.
  If omitted, tauri-action runs `pnpm tauri build` for pnpm projects. With `pnpm tauri`, the inline
  `APPLE_SIGNING_IDENTITY=...` in the `tauri` script of `package.json` overrides the workflow's env (an inline shell assignment is
  stronger than the inherited env). As a result CI would try to sign with the identity hard-coded for local use instead of the secret.
  `pnpm exec tauri` does not go through the script, so the secret takes effect.
- **Do not remove `signingIdentity: "-"` from `tauri.conf.json`**. tauri-cli prefers the
  `APPLE_SIGNING_IDENTITY` env if present (env > config). This setting lets CI sign with the secret's
  Developer ID while a plain local build without the env gets an ad-hoc signature.
  The `pnpm tauri` script passes the env, so local builds are also signed with the Developer ID.
- **Every `uses:` is pinned to a commit SHA**. This job places a certificate containing the Apple private key in the keychain,
  so with mutable tags (`@v0` / `@v4` / `@stable`) a single swap could steal the certificate. Pinning only tauri-action
  is not enough: if an action in an earlier step were tampered with, the result would be the same, so pin all of them. When updating, use the
  `# v4` comment at the end of the line as a guide, look up the new SHA and replace it.
- **checkout uses `persist-credentials: false`**. Do not leave the write-permission `GITHUB_TOKEN` in `.git/config`.
  The token needed for Release operations is passed explicitly to each step via env.
- **The certificate is imported into a temporary keychain, which is also added to the search list with `list-keychains`**. codesign
  looks up identities from the search list, not the default keychain. The `find-identity | grep` right after is an
  assertion that defeats the behavior of "exit 0 even with 0 identities", so a state where the certificate cannot be found fails
  at this step rather than deep inside the build.
- **Write both `cancel-in-progress: false` and `queue: max`**. Runs are serialized, so a later run's
  `plan` runs after the earlier run's publish, and the same version is not built twice. The default `queue: single`
  keeps only one pending run, and a new run replaces the existing pending one. `queue: max` queues up to 100.
  PR runs are in a separate group per commit, so they are not caught up in this serialization.
- **Pick the `dtolnay/rust-toolchain` SHA from the master history**. The SHA that `@stable` points to is the tip of the generated branch
  stable; if you pin it, when stable moves on that commit gets GCed and subsequent runs fail before Rust
  setup. Pin a SHA from the master history, and since the toolchain can no longer be inferred from the ref,
  state `toolchain: stable` explicitly.
- **`pnpm publish` cannot be used** (it is a pnpm built-in command and cannot be overridden by scripts).
  The command name must be `release`.
- **The version in `package.json` is decorative**, but for visual consistency release.sh keeps it in sync with
  `tauri.conf.json`. tauri-action reads the one in `tauri.conf.json`.
- **Weakness**: `pnpm release` pushes directly to main. If branch protection (PR required) is enabled this script
  can no longer be used, but bumping the version in `tauri.conf.json` / `package.json` in a PR and merging it releases the same way.
- Required GitHub Secrets (registered): `APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD` /
  `APPLE_SIGNING_IDENTITY` / `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID`.
  `APPLE_PASSWORD` is an app-specific password (notarization does not pass with the normal Apple ID password).

## Framework Note

SvelteKit 2 (Svelte 5 runes) and Tauri 2.x are new frameworks. Use **context7 MCP** to look up current API docs before making changes.

## macOS app icon

Run `bash scripts/generate-macos-icons.sh` with Xcode on macOS. Fixed artwork,
layout measurements, size-specific sources and the verification command are in
`resources/app-icons/README.md`. Small @1x backgrounds reach the canvas edges;
the glyph fits inside the central 80%. Larger backgrounds have 10% outer margins.
Do not replace the generated ICNS with `tauri icon` / `iconutil`: preserve the
ARGB small entries and the matching Assets.car / CFBundleIconName registration.
Native rendering is verified by `.github/workflows/macos-icons.yml`.
