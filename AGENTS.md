# FlashCap - Project Guide

macOS screenshot capture & annotation app.

## Commands

- `pnpm install` - Install dependencies
- `pnpm tauri dev` - Start development server
- `pnpm tauri build` - Production build
- `pnpm check` - TypeScript type check
- `pnpm test` - Edge-snap / crop-aspect の単体テスト (`node --experimental-strip-types`, テストランナー非依存)
- `pnpm release [patch|minor|major]` - Bump version and run the GitHub Actions release build

`jj-menu.yaml` にも同じ操作を並べてある (`jj` で選ぶ)。

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
- **Types**: `src/lib/types.ts`
- **Preferences**: `src/routes/preferences/+page.svelte`

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
- Settings stored via `tauri-plugin-store` (`settings.json`)

## Frontend Ready Handshake (src-tauri/src/lib.rs)

コールド起動 (WebView 未ロード) では `app.emit()` の届け先が存在せず、イベントが黙って
捨てられる。`FrontendHandshake` (`Mutex<{frontend_ready, capture_pending, pending_files}>`) が
frontend-ready を待ってから emit することで、これを 1 箇所で防いでいる。預かる仕事は 2 種類:

- **キャプチャー開始** — 3経路 (`--capture` コールド起動 / single-instance 再起動 /
  `flashcap://capture` URL スキーム)。`request_capture()` 経由。
- **画像を開く** — Finder の「このアプリケーションで開く」/ Dock へのドロップ
  (どちらも `RunEvent::Opened`)、single-instance の argv、コールド起動の argv。
  `request_open_files()` 経由。

注意点:

- **経路は `show()` しない**。`request_capture()` は `do-capture` を emit するだけ。
  ウィンドウ表示はフロント `captureScreen()` が撮影完了後の `show()` + `setFocus()` で行う。
  → 経路側で `show()` を足すと show→hide の点滅が起きるので追加しないこと。
  `request_open_files()` も未 ready の時は show しない (白いウィンドウが見えるだけ。
  こちらは点滅ではなく描画前表示が理由で、キャプチャー側とは事情が違う)。
  **2 秒フェイルセーフも、キャプチャー予約中と画像の預かり中は表示しない。**
  ここで出すと「未 ready なら show しない」を裏口から破ることになる。
- **フロントは、預かり対象のリスナーが全部登録され終わってから `frontend-ready` を
  一度だけ emit する** (`+page.svelte` の `Promise.allSettled([unlistenDoCapture,
  unlistenOpenFile])`)。**預かるイベントを増やしたらこの配列にも足すこと。**
  足し忘れるとコールド起動でだけ取りこぼす、再現しにくいバグに戻る。
- Finder の「このアプリケーションで開く」は起動中でもコールド起動でも `RunEvent::Opened`
  で来る (起動中のアプリに 2 個目のプロセスは立たない)。argv で来るのはターミナルからの
  `flashcap foo.png` だけ。
- `captureScreen()` の `finally` ではガードフラグ `isCapturing` を `show()`/`setFocus()` の
  await が**全部終わった後**に false へ戻す。先に戻すと復元中の `do-capture` が新キャプチャーを
  開始してインターリーブする。

## Crop Tool (src/lib/CropOverlay.svelte + src/routes/+page.svelte)

- 適用時は `imageBase64` を切り出した PNG に差し替え、注釈は焼き込まずに切り出した原点ぶん
  平行移動する。注釈座標は自然解像度ピクセルなので、この平行移動だけで `renderComposite()` と
  各オーバーレイの座標系が揃う。
- **mask だけは切り出し後の領域へクランプする**。`renderComposite()` の `getImageData` は
  キャンバス外を transparent black で返すため、はみ出した mask を残すと blur はアルファごと
  `putImageData` で焼き付き、mosaic は端のブロックが半透明になって**隠したはずの元画像が透ける**。
  arrow / shape / text はキャンバスにクリップされるだけなので座標をそのまま保持してよい。
- 同時に `imageModified` を立てる。`saveCompositeToFile()` の書き込み判定は
  `needsFileWrite` (= 注釈がある or `imageModified`) で、ここに入れないとトリミング後の
  ⌘C (パスのコピー) / ドラッグで**ディスク上の未トリミングの元ファイル**が相手に渡る。
  判定を外して常に書き戻す形にはしないこと (JPEG など非可逆形式で無駄な再エンコード劣化を招く)。
- undo スナップショット (`EditSnapshot`) は画像 base64 と寸法も持つ。ただし `imageModified` は
  undo で戻さない — 既にファイルへ書き戻していた場合、巻き戻してもディスク上は変更済みで
  「メモリ = ファイル」とは言えないため。
- 画像を差し替えたら `bumpImageRevision()` を呼ぶ。MaskOverlay のモザイクは表示中の `<img>` から
  サンプリングするので、デコード完了を待って revision を上げないと旧画像から取った絵で固まる。
- 枠が画像いっぱいの間は内側ドラッグを「範囲の引き直し」に回す。枠の外が存在しないため、
  move に倒すと引き直す手段が無くなる。
- `undo()` で土台画像が差し替わったら crop ツールを閉じる。`Cmd+Z` は crop 表示中でも
  通るため、閉じないと旧画像の座標の枠が新しい画像からはみ出したまま残る。

### 境界線への吸着 (src/lib/edgeSnap.ts)

- 射影プロファイル (列/行ごとの画素差の積算) で線を検出する。**絶対閾値だけでは足りない** —
  写真やテクスチャはどこを切っても差分が出るので全列が候補になり、吸着先が実質ランダムに
  なる。近傍平均に対する突出度 (`PROMINENCE_RATIO`) を併せて要求して切り分ける。
- 検出結果は「線」(`EdgeSnapRun`) の列で、**1 本が start / end の 2 境界を持つ**。1px の枠線は
  左右 2 本の境界を作り、「枠線を含めて切る」「外して切る」のどちらも正当な意図なので、
  片方に丸めない。
- **吸着の許容距離は画面 px 基準 (`6 / scale`)、検出は画像 px 基準**。この 2 つが噛み合うのは
  等倍表示の時だけで、Retina のスクショを縮小表示すると 6 画面 px が 25 画像 px 以上に
  広がり、候補が詰まって枠の辺を線以外へ置けなくなる。そのため `snapPositions()` が
  **表示スケールを見て毎回間引く** (許容距離の 3 倍未満に隣接する線は強い方だけ残す)。
  間引きを検出結果のキャッシュに焼き付けないこと — ウインドウのリサイズで scale が変わる。
- draw 中は、吸着した結果が `MIN_SIZE` を割るならその軸は吸着させない。mouseup が
  最小サイズ未満の引き直しを破棄するので、吸着が原因で選択ごと消えてしまう。
- 検出は画像 1 枚につき 1 回で `imageRevision` に紐付ける。**メインスレッド同期処理**なので
  (ワーカーには逃がしていない)、crop ツールが開いた見た目を描かせてから走らせる。
- **画像を差し替えたら、キャッシュ世代 (`cropSnapLinesRevision`) も同期的に無効化する。**
  `imageRevision` の更新はデコード待ちの後なので、その隙に crop を開き直されると
  「世代が一致 (どちらも旧世代)」が成立して検出が空のまま早期 return し、その後 revision が
  上がっても再検出されない (吸着が黙って効かなくなる)。
- ON/OFF は crop ツールバーの磁石トグル。`localStorage` の `flashcap-crop-snap` に持つ。

### 縦横比の固定 (src/lib/cropAspect.ts)

- crop ツールバーの `1:1` / `16:9` トグル。排他で、押し直すと解除。**セッションを跨いで
  覚えない** — 次の起動でトリミングが勝手に固定されていると驚くため (吸着とは扱いが違う)。
- 幾何計算はすべて `cropAspect.ts` の純関数 (テストは `tests/crop-aspect.test.mts`)。角ハンドルと引き直しは同じ
  `aspectRectFromCorner()` で表せる (引き直しは「ドラッグ開始点を固定した角」)。
- 大きい方の軸に合わせる (cover)。小さい方に合わせると、対角線から外れた方向へ
  ポインタを動かした時に枠が縮んで追従しなくなる。
- 辺ハンドルでは、導いた側は**余地がある間だけ枠の中心を保ち、画像の端に当たったら滑らせる**。
  右辺を引いた時に高さを上下どちらへ伸ばすかは決めようがないので、基本は中心を保つ。
  ただし**中心の維持を制約 (上限) にしてはいけない** — 「中心を保ったまま伸ばせる範囲」を
  上限にすると、枠が端に接している時にそれが現在の寸法と一致して**ハンドルが完全に死ぬ**
  (どこまで引いても元へ丸め戻される)。枠を端まで move すれば必ず踏む。
- **吸着は縦横比に負ける**。動かす軸だけを吸着させ、もう一方は比率から導く。
  両軸を吸着させると比率が崩れる。
- **固定中は「引き直しが小さすぎるか」を結果の寸法で判定できない**。比率を保つために
  枠が最小サイズまで自動で広がるので、クリックしただけでも MIN_SIZE の枠ができる。
  `drawMoved` (生のポインタが `CLICK_SLOP` 以上動いたか) で判定する。
  **ここで `MIN_SIZE` を閾値に流用しない** — 3px 引いただけでも画面には正当な枠が
  出ているので、離した瞬間にそれが消えることになる。判定は吸着**前**の座標で行う
  (吸着が終点を始点へ引き戻すと、引いているのに「動いていない」になる)。

## 画像の表示サイズ (src/routes/+page.svelte + resize_window_for_image)

画像は「等倍 + 周囲 20px の余白」で表示するのが狙いで、そのために **Rust の
`resize_window_for_image` と CSS が同じ数値を暗黙に共有している**。片方だけ動かすと等倍が崩れる。

- `padding = 20.0` ⇔ viewport の `p-5`
- `toolbar_h = 49.0` ⇔ `Toolbar.svelte` root の `py-2` (8+8) + 最も高い子 `.tool-btn` の
  `h-8` (32) + `border-b` (1)。root の `min-h-[40px]` は下限で効いていない。
  **ツールバーに背の高い要素を足したらこの定数も直すこと。**
- `displayScale` の分母は viewport の **content box**。`clientWidth` / `clientHeight` は
  padding を含む寸法なので、引かずに使うと 40px 過大に見積もり、画像が content box を
  はみ出して flex の中央寄せに負の余白が渡る → 「左に余白 / 右は切れる」の非対称になる。

## Rust Commands (src-tauri/src/lib.rs)

- `take_screenshot_interactive` - Standard interactive capture (`screencapture -i`)
- `take_screenshot_timer` - Timer capture (`screencapture -i -T <N>`, async to avoid UI freeze)
- `write_image_to_file` - Save annotated image (path restricted to the save directory, plus the
  files the user opened themselves in this session). `load_image_file` records each opened file's
  canonical path in `OpenedImages`, which is what allows Cmd+S to overwrite an image that lives
  outside the save directory. 許可されるのは実体が一致するそのファイルだけで、
  そのフォルダは開放しない。書き込み直前に `encode_for_target_format` が上書き先の拡張子へ
  合わせて詰め直す (JPEG などは image crate、HEIC は sips)。変換先が無い拡張子は
  **書かずにエラー** — PNG のまま書くと拡張子と中身が食い違い、上書きなので原本も戻せない
- Common result loading: `load_screenshot_result()` shared by both capture commands

## Build & Check

- `cargo check` in `src-tauri/` for Rust type check
- `cargo test` in `src-tauri/` for the Rust unit tests (save-path containment, handshake, encoding)
- `pnpm check` for Svelte/TypeScript check
- `pnpm test` for the edge-snap / crop-aspect unit tests (`tests/*.test.mts`)
- Run all four before committing
- Production build: `cargo build --release` in `src-tauri/` (run before push)

## Release (.github/workflows/release.yml + scripts/release.sh)

配布は GitHub Release。`pnpm release [patch|minor|major]` で version 採番 → main へ push →
`workflow_dispatch` で Actions を起動 → 署名+公証済み universal dmg が公開される。

- **`workflow_dispatch` のみ**。push では自動ビルドしない (無駄な CI を避ける)。
- **macOS のみビルドする**。screencapture / Vision Framework 依存の macOS 専用アプリなので
  Windows ビルドは作らない。成果物は `--target universal-apple-darwin --bundles dmg` の
  `flashcap_<version>_universal.dmg` (x86_64 + arm64)。
- **draft → publish の 2 ジョブ構成**。tauri-action はビルド前に Release を作るため、
  `releaseDraft: false` だとビルド失敗時に空の Release が公開されてしまう。draft で作り、
  build 成功後に publish ジョブが `gh release edit --draft=false --latest` で公開する。
  失敗時は draft のまま残る。
- **version は毎回インクリメント必須**。公開済みと同じ version で再実行すると tauri-action が
  draft 状態の不一致でエラーになる。`scripts/release.sh` が採番を自動化して bump 忘れを構造的に消す。
- **`tauriScript: pnpm exec tauri` は消さない**。省略すると tauri-action は pnpm プロジェクトに
  対して `pnpm tauri build` を実行し、`package.json` の `tauri` スクリプトが持つインラインの
  `APPLE_SIGNING_IDENTITY=...` が workflow の env を上書きしてしまう (シェルのインライン代入は
  継承 env より強い)。その結果 CI が Secret ではなくローカル用にハードコードした identity で
  署名しようとする。`pnpm exec tauri` はスクリプトを経由しないので Secret が効く。
- **`tauri.conf.json` の `signingIdentity: "-"` は消さない**。tauri-cli は
  `APPLE_SIGNING_IDENTITY` env があればそれを優先する (env > config)。CI は Secret の
  Developer ID で署名、env の無い素のローカルビルドは ad-hoc 署名、という両立のための設定。
  `pnpm tauri` スクリプトは env を渡しているのでローカルも Developer ID で署名される。
- **`uses:` はすべて commit SHA 固定**。Apple の秘密鍵入り証明書を keychain に置くジョブなので、
  可変タグ (`@v0` / `@v4` / `@stable`) だと差し替え1つで証明書を抜かれうる。tauri-action だけ
  固定しても、先行ステップの action が改変されれば同じことなので全部固定する。更新時は行末の
  `# v4` コメントを頼りに、新しい SHA を調べて置き換えること。
- **checkout は `persist-credentials: false`**。write 権限の `GITHUB_TOKEN` を `.git/config` に
  残さない。Release 操作に必要な token は各ステップに env で明示的に渡している。
- **証明書は一時 keychain に import し、`list-keychains` で検索リストにも入れる**。codesign は
  default keychain ではなく検索リストから identity を引く。直後の `find-identity | grep` は
  「identity 0 件でも exit 0」という仕様を潰すためのアサーションで、証明書が引けない状態を
  ビルドの奥ではなくこのステップで落とす。
- **`cancel-in-progress: false` + `queue: max` の両方を書く**。1 dispatch = 1 version なので、
  キャンセルされた run の version は (bump コミットは main に残ったまま) 永久に公開されない。
  走行中を守る `cancel-in-progress: false` だけでは不十分で、既定の `queue: single` は pending を
  1 件しか保持せず、新しい dispatch が既存 pending を置き換える (走行中 1 + dispatch 2 回で
  真ん中の version が消える)。`queue: max` は pending を 100 件まで積む。CI 分数より取りこぼし防止。
- **`dtolnay/rust-toolchain` の SHA は master 履歴から選ぶ**。`@stable` の指す SHA は生成ブランチ
  stable の先端で、それを pin すると stable が進んだ時に commit が GC され、以降の run が Rust
  セットアップ前に落ちる。master 履歴の SHA を pin し、ref から toolchain を推測できなくなる分
  `toolchain: stable` を明示する。
- **`pnpm publish` は使えない** (pnpm 組み込みコマンドで scripts から上書き不可)。
  コマンド名は必ず `release`。
- **`package.json` の version は飾り**だが、見た目の一貫性のため release.sh が
  `tauri.conf.json` と同期させている。tauri-action が読むのは `tauri.conf.json` の方。
- **弱点**: `pnpm release` は main へ直接 push する。ブランチ保護 (PR 必須) を掛けると破綻する。
  掛ける運用にするなら tag 駆動 (CI で version 注入) へ切り替えること。
- 必要な GitHub Secrets (登録済み): `APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD` /
  `APPLE_SIGNING_IDENTITY` / `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID`。
  `APPLE_PASSWORD` は App 用パスワード (通常の Apple ID パスワードでは公証が通らない)。

## Framework Note

SvelteKit 2 (Svelte 5 runes) and Tauri 2.x are new frameworks. Use **context7 MCP** to look up current API docs before making changes.
