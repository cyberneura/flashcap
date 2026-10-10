# FlashCap - Project Guide

Screenshot capture & annotation app for macOS and Windows (CYBERNEURA-DEV-841 added Windows).

## Commands

- `pnpm install` - Install dependencies
- `pnpm tauri dev` - Start development server
- `pnpm tauri build` - Production build
- `pnpm check` - TypeScript type check
- `pnpm test` - Edge-snap / crop-aspect / text-hit / region-select / home-path / shell-commands / thumbnail-browser / capabilities の単体テスト (`node --experimental-strip-types`, テストランナー非依存)
- `pnpm release [patch|minor|major]` - Bump version and push to main (the push starts the GitHub Actions release build)

`j-menu.yaml` にも同じ操作を並べてある (`j` で選ぶ)。

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
  - `src/lib/ThumbnailSidebar.svelte` + `src/lib/thumbnailBrowser.ts` - Saved-images sidebar (see サムネイルブラウザ)
- **Types**: `src/lib/types.ts`
- **Preferences**: `src/routes/preferences/+page.svelte`
- **Third-Party Licenses**: `src/routes/licenses/+page.svelte` (see 依存ライブラリのライセンス表示)

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
- Settings stored in `~/.config/flashcap/config.json` (see 設定ファイル)
- ダークテーマ固定。`src/app.css` で html / body の背景と `color-scheme: dark` を当て
  (スクロールの跳ね返りで白が見えないように)、ウインドウは `theme: Dark` + `backgroundColor`
  (main は tauri.conf.json、Preferences / Licenses は WebviewWindowBuilder) で枠と描画前の下地も暗くする。
  ウインドウを足したら同じ 2 つを付けること。

## 設定ファイル (src-tauri/src/config.rs + src/lib/config.ts)

設定はすべて `~/.config/flashcap/config.json` (macOS / Windows 共通) の 1 ファイルに、
キーと値のフラットな JSON オブジェクトとして持つ。Preferences の項目、自動コピー、
ツールの設定 (`arrow_settings` / `mask_settings` / `shape_settings` / `text_settings` /
`crop_snap`)、シェルコマンド (`shell_commands`) がここに入る。tauri-plugin-store と
localStorage は使わない。

- **ホームフォルダの実パスを書かない** (マシン間で共有するため)。パスはフロントが
  `collapseHome()` (`src/lib/homePath.ts`) で `~` に畳んでから保存し、Rust の
  `config::expand_home()` で展開する。パスを持つ設定を増やしたら両方を通すこと。
- Rust の読み手 (`config::get`) はキャッシュせず毎回ファイルを読む。手で書き換えた内容や
  別マシンから同期された内容をそのまま拾うため。
- 書き込みは `config_set` の 1 キー単位で、Rust 側のロックの中で「読む → 差し替える →
  一時ファイルに書いて rename」する。**JSON が壊れていたら書かない** (上書きすると他の
  設定が全部消える)。config.json が symlink (dotfiles 管理) でもリンクを壊さないよう、
  リンク先の実体の隣で rename する。
- 書いた後に `config-changed` (payload はキー) を全ウインドウへ emit する。メインウインドウは
  Preferences のキー (`PREFERENCE_KEYS`) だけ読み直す。
- ツールの設定は変更のたびに `setConfigDebounced` でまとめて書く。**読み込みが終わるまで
  (`configLoaded`) は書かない** — 起動直後の既定値で保存値を上書きしてしまうため。

## シェルコマンド (src-tauri/src/shell_command.rs + src/lib/ShellCommandMenu.svelte)

Preferences で「名前・シェル・コマンド」を複数登録し、画像を開いている時にツールバー右端の
ターミナルボタンから実行する。macOS のみ (Windows はボタンも設定欄も出さない)。

- **画像のパスは環境変数 `IMAGE_PATH` で渡し、文字列置換しない。** コマンド中の
  `${IMAGE_PATH}` はシェル自身が展開する。置換だと Finder から開いた `a;rm -rf ~.png` の
  ようなファイル名がコマンドとして実行される。
- シェルは `-l -c` (ログインシェル) で、ホームフォルダを cwd にして起動する。GUI アプリの
  PATH には Homebrew が無いので、`~/.zprofile` 等を読ませるため。
- フロントは id と画像のパスだけを渡し、中身は Rust が設定ファイルから引き直す。
- 実行前に `saveCompositeToFile()` で注釈・トリミングを焼き込む (パスのコピーと同じ)。
- ログは `$TMPDIR/flashcap/shell-logs/` に 1 実行 1 ファイル (stdout + stderr、先頭に
  コマンドと IMAGE_PATH、末尾に終了ステータス)。シェルが起動できなかった時もログに書いて
  失敗として返す。完了・失敗は svelte-sonner のトーストで出し、「View log」で開ける。
  `open_shell_log` はログの置き場の直下のファイルしか開かない。

## Frontend Ready Handshake (src-tauri/src/lib.rs)

コールド起動 (WebView 未ロード) では `app.emit()` の届け先が存在せず、イベントが黙って
捨てられる。`FrontendHandshake` (`Mutex<{frontend_ready, capture_pending, pending_files}>`) が
frontend-ready を待ってから emit することで、これを 1 箇所で防いでいる。預かる仕事は 2 種類:

- **キャプチャー開始** — 3経路 (`--capture` コールド起動 / single-instance 再起動 /
  macOS の `flashcap://capture` URL スキーム)。`request_capture()` 経由。
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
- **argv で `--capture` と画像を同時に渡された時は capture を優先する**
  (`image_args_for_startup`)。single-instance 経路は `request_capture()` の後に return して
  ファイル引数を見ないので、コールド起動だけ両方処理すると同じコマンドの結果が
  起動状態で変わる (`loadImageFile()` と `captureScreen()` が並走して後勝ちになる)。
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

- 画素差は **RGBA 4 チャンネル**で取るが、正規化の除数は **3 のまま**。不透明な画像
  (スクリーンショット = 主用途) はアルファ差が常に 0 なので、3 で割る限りスコアは
  アルファ導入前と完全に一致する。4 で割ると主用途のスコアが一律 25% 下がり、閾値の
  意味が変わる。アルファを見ないと、透明背景の上の不透明な黒のように RGB が同一で
  アルファだけが違う境界を 1 本も拾えない。
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
  **終点 (`snapEndpoint`) と始点 (`drawAnchor`) の両方に要る。** 始点は mousedown 時点で
  吸着させるが、その時はまだ引く向きが分からない。線の手前から線へ向かって引くと
  始点が動いたぶん span が削られる (始点 96 が線 100 へ吸着 → 104 まで引いても幅 4)。
- 検出は画像 1 枚につき 1 回で `imageRevision` に紐付ける。**メインスレッド同期処理**なので
  (ワーカーには逃がしていない)、crop ツールが開いた見た目を描かせてから走らせる。
- **画像を差し替えたら、キャッシュ世代 (`cropSnapLinesRevision`) も同期的に無効化する。**
  `imageRevision` の更新はデコード待ちの後なので、その隙に crop を開き直されると
  「世代が一致 (どちらも旧世代)」が成立して検出が空のまま早期 return し、その後 revision が
  上がっても再検出されない (吸着が黙って効かなくなる)。
- ON/OFF は crop ツールバーの磁石トグル。設定ファイルの `crop_snap` に持つ。

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

## テキスト注釈 (src/lib/TextOverlay.svelte + src/lib/textHit.ts)

- **1 クリックは「選ぶ」「動かす」で、再編集はダブルクリック** (CYBERNEURA-DEV-695)。
  選択済みのものを押すと移動が始まるので、mousedown で編集に入る分岐は書いても
  到達しない (以前そうなっていた: 分岐は残っていたが、その手前の移動開始が return
  していた)。編集の入口は `ondblclick` ひとつだけにしてある。
- **`onBeforeMutate` (= undo の push) は、押した時ではなく実際に動いた最初の 1 回で呼ぶ。**
  押下で呼ぶと、選び直しやダブルクリックのたびに「何も変わっていない」状態が
  undo に積まれる。動いたかどうかは `exceedsDragSlop` で、`CropOverlay` の
  `CLICK_SLOP` と同じ 2px。完全一致 (dx === 0) では足りない —
  ダブルクリックの 2 回目の押下も移動の開始なので、指が 1px 揺れれば文字がずれる。
- **書き換えの undo も、最初の 1 入力で 1 つだけ積む。** `handleEditInput` は 1 打鍵ごとに
  `onTextsChange` を呼ぶので、そこで毎回積むと undo が打鍵の巻き戻しになる。
  逆に一度も積まないと、書き換えた直後の ⌘Z が**別の操作**を巻き戻す。新規作成ぶんは
  `handleMouseDown` で既に積んであるので、ここで積むのは既にある注釈の書き換えだけ。
- 当たり判定と箱は `textHit.ts`。どれを選ぶか、どれを編集するか、カーソルを何にするかが
  すべてこの箱で決まるので、DOM もマウスも要らない形にしてテスト (`tests/text-hit.test.mts`)
  から直接呼べるようにしてある。**実際に描かれた字面は測らない** — `getBBox()` は
  レンダリング後にしか答えず、マウスが動くたびに呼ぶとレイアウトの再計算が毎フレーム入る。

## 画像の表示サイズ (src/routes/+page.svelte + resize_window_for_image)

画像は「等倍 + 周囲 20px の余白」で表示するのが狙いで、そのために **Rust の
`resize_window_for_image` と CSS が同じ数値を暗黙に共有している**。片方だけ動かすと等倍が崩れる。

- `padding = 20.0` ⇔ viewport の `p-5`
- `sidebar_w = 224.0` ⇔ `ThumbnailSidebar.svelte` の `w-56`。設定ファイルの `thumbnail_sidebar` が
  true の間だけ足す (出し入れした瞬間にはウインドウを広げ直さず、次に画像を読み込んだ時に効く)
- `toolbar_h = 49.0` ⇔ `Toolbar.svelte` root の `py-2` (8+8) + 最も高い子 `.tool-btn` の
  `h-8` (32) + `border-b` (1)。root の `min-h-[40px]` は下限で効いていない。
  **ツールバーに背の高い要素を足したらこの定数も直すこと。**
- `displayScale` の分母は viewport の **content box**。`clientWidth` / `clientHeight` は
  padding を含む寸法なので、引かずに使うと 40px 過大に見積もり、画像が content box を
  はみ出して flex の中央寄せに負の余白が渡る → 「左に余白 / 右は切れる」の非対称になる。

## サムネイルブラウザ (src-tauri/src/thumbnails.rs + src/lib/ThumbnailSidebar.svelte)

ツールバーの一番左のボタンで出し入れする左サイドバー (CYBERNEURA-DEV-995)。表示状態は
設定ファイルの `thumbnail_sidebar`。保存先フォルダ (Preferences の保存先) の直下の画像を
作成日時の新しい順に最大 300 件並べ、サムネイル・ファイル名・相対時刻・容量を出す。
「メニューバーの一番左」という依頼だったが、メニューバー (アプリメニュー / トレイ) ではなく
アプリ内のツールバーを指すと読んだ (自動コピーの時と同じ解釈。次の「撮影後の自動コピー」節を参照)。

- **サムネイルは Rust が作る** (`saved_image_thumbnail`、長辺 400px の PNG を data URL で返す)。
  asset プロトコルで原寸を見せる形にしないのは、scope を任意の保存先へ広げることになり、
  Retina のスクリーンショットを原寸のまま何十枚も WebView に読ませることにもなるため。
- **`saved_image_thumbnail` は保存先の直下の通常ファイルしか読まない** (`resolve_within`)。
  symlink・サブフォルダ・`..` 越しのパスは拒否する。一覧 (`list_saved_images`) も symlink を載せない。
  フロントから任意のパスを渡されても、他の場所の画像を読む口にしないため。
- サムネイルは画面に入ったものから 2 枚ずつ頼む (IntersectionObserver)。作ったものは
  `thumbnailCacheKey` (パス + 更新日時 + 容量) で取り置くので、注釈を書き戻した画像は作り直される。
- 一覧の読み直しは、画像の差し替え (`filePath` / `imageRevision`)、書き戻し (`sidebarRefresh`)、
  保存先の変更 (`config-changed` の `save_directory`)、ウインドウのフォーカスで行う。
- **クリックは「今の画像を書き戻してから開く」** (`openFromSidebar`)。書き戻しは
  `saveCompositeToFile()` (`needsFileWrite` の時だけ書く) で、失敗したら開かない (開くと注釈が失われる)。
- **ドラッグは押してから 4px 動いたら `startDrag`** (Finder のファイルと同じく、ファイルそのものを渡す)。
  HTML5 の drag イベントは使わない。開いている画像そのものをドラッグする時はツールバーのドラッグと
  同じく先に書き戻す。**自分のウインドウに落とされた時はクリックと同じ扱い**
  (`sidebarDragPath` で見分ける。素の `loadImageFile` に回すと書き戻さずに差し替わる)。

## 撮影後の自動コピー (src-tauri/src/auto_copy.rs + src/lib/Toolbar.svelte)

- メインウインドウのツールバー (撮影ボタンの左) のボタンで、ポップオーバーから
  「Don't copy / Copy the image file path / Copy the image data」を選ぶ (CYBERNEURA-DEV-761)。
  ボタンの図柄 (範囲選択の四隅 + コピーするもの) が選択に合わせて変わる。
  **最初はメニューバーに置いていたが、依頼者の意図はアプリ内のツールバーだった**ので移した。
  メニューバーのアイコンは下の「メニューバーへの常駐」で別の用途に使っている。
- 選択はフロント (`+page.svelte` の `changeAutoCopyMode`) が設定ファイルの
  `auto_copy_on_capture` (`none` / `path` / `image`) に保存し、Rust は撮影のたびに読むだけ。
  **未設定・未知の値は `none`** (フロントの `parseAutoCopyMode` と Rust の `from_setting` の両方)
  — この機能より前から使っている人の撮影で、いきなりクリップボードを書き換え始めないため。
- ポップオーバーの Esc は **window のキャプチャーフェーズで拾って伝播を止める**。
  `+page.svelte` の keydown が Esc を「ウインドウを閉じる」に割り当てているので、
  止めないとポップオーバーを閉じるつもりの Esc でアプリが終了する。
- ポップオーバーは普段ボタンの右端に揃えるが、ボタンがウインドウの左寄りにある時
  (動画モードはパス欄が無く、ツールバーが左に詰まる) は左端に揃える。
- コピーするのは撮影 (`take_screenshot_interactive` / `take_screenshot_timer`) の結果だけ。
  貼り付け・ファイルを開く・OCR・動画は対象外。コピーと通知 (`ocr::notify` の osascript) は
  `spawn_blocking` に逃がしてあり、撮影結果の返却 (= ウインドウの再表示) を待たせない。
  その代わり完了順が撮影順と一致しないので、撮影ごとの通し番号 (`LATEST_CAPTURE`) を
  書き込む直前に照合し、後の撮影に追い越されたコピーは捨てる。**照合と書き込みは
  `CLIPBOARD_WRITE` のロックの中で一緒に行う** (照合を通った後の遅い書き込みが、
  後の撮影の書き込みを上書きしないため)。デコードと通知はロックの外。

## メニューバーへの常駐 (src-tauri/src/menu_bar.rs)

- Preferences の「Keep FlashCap in the menu bar」(設定ファイルの `show_in_menu_bar`、
  未設定は OFF) を ON にすると、メニューバーにアイコンを置く (CYBERNEURA-DEV-761)。
  クリックで Open FlashCap / Capture Screenshot / Capture Screenshot with Timer (Ns) /
  Record Video / Record Video with Timer (Ns) / Copy Text on Screen (OCR) / Quit FlashCap。
  N は Preferences の Timer delay。
- Preferences は設定ファイルに書いた後に `sync_menu_bar` を呼ぶ (Timer delay の変更でも呼ぶ。
  メニューの秒数を追従させるため)。`sync` は「保存値を読んでアイコンの有無と文言を合わせる」
  だけなので何度呼んでもよい。起動時は setup から呼ぶ。
- **トレイの `TrayIconBuilder::on_menu_event` は使わない。** そこで登録したハンドラは app 全体の
  リストに積まれ、トレイを外しても消えない。設定を OFF → ON するたびに増えて、1 回のクリックで
  撮影が 2 回走る。イベントは lib.rs の `app.on_menu_event` から `menu_bar::handle_menu_event` に回す。
- **常駐中はメインウインドウを閉じても終了せず隠す** (`on_window_event` の CloseRequested)。
  閉じるとフロントの WebView ごと破棄され、メニューからの `do-capture` を受け取る相手が
  いなくなる。Esc / ⌘W / 閉じるボタンがすべてここを通る。終了は ⌘Q かメニューの Quit。
- 撮影は `request_capture(app, CaptureKind)` 経由でフロントの `captureScreen()` に任せる
  (ウインドウの hide / show もあちらの仕事)。**ハンドシェイクの予約も種類ごと持つ**
  (`capture_pending: Option<CaptureKind>`)。bool だとコールド起動直後に押された
  タイマー付き撮影が通常の撮影に化ける。`do-capture` の payload はコマンド名で、フロントは
  `take_screenshot_timer` 以外を通常の撮影として扱う。
- メニューの項目は、進行中の操作と重ならないよう `menu_bar::run` の先頭で振り分ける (Quit は常に通す):
  - **フロントの描画前 (コールド起動の直後) は撮影と Quit だけ**。撮影はハンドシェイクが預かる。
    録画と OCR をここで始めると frontend-ready 側がメインウインドウを出してしまい、範囲選択や
    撮影に写り込む。Open は白いウインドウが見えるだけなので無視する。
  - **録画中 (停止後の書き出し中を含む) は何も始めずにメインウインドウを出す** (停止ボタンは
    そこにしか無く、範囲選択を終えると `start_video_recording` が録画中のものを止めて差し替える)。
    書き出し中の判定は `video::FINALIZING`。`stop_video_recording` は待つ前に `RecordingState` から
    録画を取り出すので、そちらだけ見ると取り違える。**`is_recording` はロックを取ってから
    FINALIZING を読む** (停止側がロックの中で立てるため)。
  - **録画の範囲選択中 (タイマーのカウントダウン中を含む) は Open だけを受け付け、範囲選択を
    やめてメインウインドウに戻す**。カウントダウン中のオーバーレイはクリックを透過するので、
    他のアプリを触った後は Esc が届かず、ここがキャンセルの手段になる。
  - **撮影中 / OCR 中は何もしない** (`CAPTURES_IN_PROGRESS`。撮影コマンドと
    `screencapture_and_ocr` が `CaptureInProgress` のガードで数える。OCR は文字認識が
    終わるまで数える — ツールバーの `ocrCaptureRegion()` は終わった後にメインウインドウを
    戻すので、認識中に始まった撮影に写り込む)。メニューはフロントの
    `isCapturing` を見られないので、Rust 側で数えている。
- 録画は `video::open_region_selector(app, delay_seconds)` を直接呼ぶ。タイマー付きは範囲選択後の
  カウントダウンを 3-2-1 の代わりにその秒数 (1 秒刻み、60 秒で頭打ち) で行う。
  待つ間に画面を整えるためのタイマーなので、**カウントダウンに入ったら
  `release_region_selector_for_countdown` で他のディスプレイのオーバーレイを閉じ、
  このオーバーレイはクリックを透過させる** (画面を暗くせず、枠と残り秒数だけを描く)。
  別のディスプレイで選び直した時は `__regionClear` がカウントダウンを止める。
- OCR は `ocr::run_headless_ocr(app, false)`。メインウインドウが出ていれば撮影の間だけ隠し、
  終わったらフォーカスを奪わずに戻す。
- アイコンは `src-tauri/icons/tray/menu-bar.png` (36x36 のテンプレート画像。
  tray-icon crate が高さ 18pt に揃えるので Retina で等倍。図柄はアプリアイコンと同じ線画の
  カメラ)。**手で描き直さず `python3 scripts/make-tray-icons.py` で作り直す**
  (標準ライブラリだけで動く)。
- ヘッドレス OCR (`--capture-screen-text` のコールド起動) ではアイコンを置かない
  (終わり次第プロセスごと終了するので、一瞬出て消えるだけになる)。

## Windows 版 (CYBERNEURA-DEV-841)

macOS の外部コマンド (screencapture / sips / osascript / pbcopy / swift) に頼る箇所は
`#[cfg(target_os = "macos")]` で分け、Windows 側を別実装にしてある。

- **撮影はカーソルのあるモニターの上で矩形を選ばせる** (`capture_screen_to` の Windows 版 +
  `src-tauri/src/region_capture.rs` + `src/routes/capture-region/+page.svelte`。CYBERNEURA-DEV-882)。
  Windows には `screencapture -i` に当たるコマンドが無いので、先にモニター全体を xcap で撮り、
  その静止画を枠なし・最前面のオーバーレイ (`capture-region-<n>`) にモニターの物理座標で重ねて
  矩形を引かせ、撮ってあった画像から切り出す。離した時点で確定、Enter でモニター全体、
  Esc / 右クリック / Alt+F4 でやめる (`"cancelled"` を含むエラー)。選べるのはカーソルのある
  モニターだけ。hide の後 300ms 待ってから撮る (消えかけのウインドウが写り込むため)。
  タイマーは待つだけで、カーソル位置は待った後に取る。
  - **切り取りツール (`ms-screenclip:`) は使わない。** 結果がクリップボード経由でしか返らず
    (ユーザーのクリップボードを上書きする)、Esc でやめたことをアプリから知る手段が無く、
    ツール自体がアンインストールできて版ごとに挙動も変わるため。
  - オーバーレイは非表示で作り、画像を描き終えた `capture_region_ready` で出す (先に出すと
    WebView の白い下地がモニター全体に一瞬出る)。届かなくても 3 秒で Rust が出す
    (出ないままだと Esc で抜ける手段が無い)。
  - コマンド (`capture_region_*`) は **進行中の選択のラベルと一致するウインドウからしか受け付けない**。
    ラベルに撮影ごとの番号を付けているのは、閉じかけの前回のウインドウの `Destroyed` が
    次の選択を取り消さないため。
  - 矩形の丸め (端数は外側へ・画像の内側に収める) はフロントの `src/lib/regionSelect.ts`
    (表示用、`tests/region-select.test.mts`) と Rust の `crop_bounds` (切り出し用、Linux の
    `cargo test` で走る) の両方にある。
- **録画・OCR・通知は macOS 専用**。フロントは `src/lib/platform.ts` の `isWindows`
  (WebView の UA で判定) でボタンを出さず、Rust もメニューバー (Windows では通知領域)
  の項目から外し (`MenuAction::is_available`)、コマンドが呼ばれてもエラーを返す。
  `ocr::notify` は Windows では何もしない。HEIC は開けも保存もできない (sips が無い)。
- **アプリメニューは macOS だけ** (`set_app_menu`)。Windows ではメニューがウインドウの
  メニューバーになり、Edit の Ctrl+C / Ctrl+V / Ctrl+Z がアクセラレーターとして
  WebView の keydown より先に奪われる。Preferences は `+page.svelte` が Ctrl+, を拾って
  `open_preferences` を呼ぶ。
- **ショートカットは `isModKey`** (macOS は ⌘、Windows は Ctrl)。Shift を押した時の
  `e.key` は Windows だと大文字になるので、Ctrl+Shift+C は小文字に揃えて比べる。
- **ファイルの守り方が違う**。Unix の mode / uid / O_NOFOLLOW は Windows に無いので、
  `create_private_dir` などは cfg で分けてある。Windows は %TEMP% (ユーザー専用の ACL)
  に頼り、`write_without_following_symlinks` は symlink を先に弾いたうえで
  `FILE_FLAG_OPEN_REPARSE_POINT` で開く。
- **canonicalize は `dunce::canonicalize`**。std は Windows で `\\?\C:\...` を返し、
  パス欄やコピーしたパスにそれが出る。突き合わせ (`OpenedImages` と `write_image_within`)
  も同じ関数で揃えること。
- `flashcap://capture` は Windows では argv で届く (`is_capture_url`)。**Windows では撮影を
  始めず、ウインドウを前に出して撮影ボタンを点滅させるだけ** (CYBERNEURA-DEV-852)。
  撮影中 (`is_capture_in_progress`) に届いた URL は無視する (前に出すと撮影に写り込む)。
  当初は Windows の撮影が操作を挟まない即時撮影だったため (任意の Web ページやメールのリンクから
  非対話で画面を撮らせられる)。0.6.0 で範囲選択が入った後も、リンクからオーバーレイを
  画面いっぱいに出させない (Enter 1 回でモニター全体が撮れる) ためにこの挙動を残している。`--capture`
  (`is_capture_arg`。Web からは渡せない) は従来どおり即撮影。`flashcap://ocr` は無い。
- **`src-tauri/tauri.windows.conf.json`** が Windows のビルドでだけ重なる
  (HEIC の関連付けと ocr.swift の同梱を外している)。
- トレイのアイコンは Windows ではアプリのアイコン (`menu_bar::tray_icon`)。macOS の
  テンプレート画像は黒一色なので、暗いタスクバーで見えなくなる。
- `icons/icon.ico` は 16〜256 の 6 サイズ入りで、**余白の無いフルサイズ** (CYBERNEURA-DEV-885)。
  マスターの `icon.png` は macOS の Dock に合わせて周囲に約 10% の透明な余白があり、
  そのまま ico にするとタスクバーで一回り小さく見える。作り直す時は
  `uv run scripts/make-windows-icon.py` (マスターの不透明な範囲 = 角丸の板だけを切り出して
  ico にする。icns には触らない)。**`pnpm exec tauri icon` で ico を作り直さないこと** —
  余白付きのマスターから作るので小さいアイコンに戻る。exe・ウインドウ・タスクバー・
  インストーラー・通知領域 (`default_window_icon()`) はすべてこの ico を使う。
- **Linux から Windows ターゲットを検査できる**。`rustup target add x86_64-pc-windows-msvc`
  のうえ `cargo check --target x86_64-pc-windows-msvc`。ただし tauri-build が
  リソースコンパイラ (`llvm-rc`) を要求して止まるので、出力ファイルを空で作るだけの
  `llvm-rc` を PATH に置く (check はリンクしないので中身は要らない)。
  Windows 専用のテスト (`mod windows_tests`) は CI の Windows ランナーでしか走らない。

## 依存ライブラリのライセンス表示 (src-tauri/src/licenses.rs)

`THIRD-PARTY-NOTICES.txt` は `scripts/generate-third-party-notices.sh` (`pnpm notices`) の生成物で、
`licenses.rs` が `include_str!` で埋め込み、`licenses` ウインドウ (`src/routes/licenses/+page.svelte`)
に出す。入口は macOS のアプリメニュー (About の直下の Third-Party Licenses...) と、Preferences の
About 節のボタン (Windows にはアプリメニューが無いため)。**手で編集しない。**

- **依存を足す・上げる時は流し直してコミットする**。直接依存 (Cargo.toml / package.json) が
  Cargo.lock / pnpm-lock.yaml の解決 version で載っていないと `cargo test` が落ちる
  (Dependabot の PR も notices を更新しないので落ちる。同じブランチで再生成をコミットする)。
- Rust 側は cargo-about (`cargo install cargo-about --locked --features cli`。`--features cli` が
  無いとバイナリが入らない)。`src-tauri/about.toml` の `targets` で配布ターゲット
  (macOS の aarch64 / x86_64 と Windows x64) だけに絞っている。Linux 専用の crate は載らない。
  flashcap 自身は `publish = false` + `[private] ignore = true` で除いている。
  `accepted` に無いライセンスの crate が入ると `--fail` で止まる。**accepted を黙って広げない**
  (GPL / LGPL / AGPL 系は配布条件が変わるので人間に確認する)。
- npm 側は `package.json` の `dependencies` と、スクリプトの `BUNDLED_RUNTIME`
  (svelte / @sveltejs/kit / esm-env / tailwindcss / runed。devDependencies や推移依存だが client bundle に
  入る)。フロントの依存を変えて bundle に入るものが増えたら `BUNDLED_RUNTIME` に足す。
- `@crabnebula/tauri-plugin-drag` の npm パッケージは package.json に license も repository も
  書いていないので、スクリプトの `MISSING_METADATA` で同じリポジトリの Rust crate に合わせている。

## コマンドの ACL (src-tauri/build.rs + src-tauri/capabilities/)

アプリ独自のコマンド (`generate_handler!`) も、窓ごとの capability で許可したものしか呼べない
(CYBERNEURA-DEV-971)。`build.rs` が `AppManifest::commands` で全コマンドを宣言しているので、
Tauri はアプリ独自のコマンドにも ACL を適用する。**宣言しないと、ローカルのページの全ウインドウから
全コマンドを無条件に呼べる** — capability を窓ごとに分けていても、絞れているのはプラグインの
コマンドだけになる (0.7.0 までそうだった。透明オーバーレイからも `config_set` + `run_shell_command`
でシェルコマンドを実行できた)。

- **コマンドを足したら 2 か所に足す**: `build.rs` の `APP_COMMANDS` と、それを `invoke` するページの
  窓の capability (`"allow-<kebab-case>"`。例: `run_shell_command` → `allow-run-shell-command`)。
  `build.rs` に無いコマンドはどの窓からも呼べず、capability に無いコマンドはその窓から呼べない。
  リリースビルドでは拒否の理由が出ないので、気付きにくい。
- 窓とページと capability の対応: `main` → `default.json`、`preferences` → `preferences.json`、
  `licenses` → `licenses.json`、`region-selector-*` (録画の範囲選択) → `region-selector.json`、
  `capture-region-*` (Windows の撮影範囲選択) → `capture-region.json`。
  **使っていないコマンドは許可しない** (透明オーバーレイに設定の書き換えやシェルの実行を渡さない)。
  **1 つの capability に複数の窓を載せない** — 許可はその和集合になって全部の窓に渡る
  (以前は main と Preferences が同居していて、Preferences から `config_set` + `run_shell_command`
  に届いた)。同じ窓を複数の capability に載せるのも同じ理由でしない。
- 検査は 2 つ。`cargo test` (`app_command_acl_tests`) が `APP_COMMANDS` と `generate_handler!` の
  一致と、`generate_context!` が埋め込む実際の ACL で「窓 x コマンド」の可否が capability どおりかを
  見る (capability のファイルを足したら `expected_access` の `include_str!` にも足す)。
  `pnpm test` (`tests/capabilities.test.mts`) は「各窓の capability の `allow-*` = その窓のページから
  import を辿って見つかる `invoke` のコマンド」を窓ごとに見る (過不足どちらも落ちる)。
  コマンド名を変数で渡す `invoke` は名前を読めないので、テストの `DYNAMIC_INVOKES` に
  取りうる値を書く。ウインドウを足したら `WINDOW_PAGES` にも足す。
- capability に存在しない permission を書くと `tauri-build` がビルド時に落とす。
- 自動生成された permission は `src-tauri/permissions/autogenerated/` に出る (gitignore 済み)。

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
- `pnpm test` for the frontend unit tests (`tests/*.test.mts`)
- Run all four before committing
- GUI の確認をインストール版の FlashCap と並べて行うには、identifier と HOME を分ける
  (single-instance の socket は identifier から決まり、同じだとインストール版に引き渡されて終わる)。
  asdf の node shim は HOME に依存するので、node の実体を PATH の先頭に置く:
  `PATH="<node の実体のディレクトリ>:$PATH" CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup HOME=<scratch> ./node_modules/.bin/tauri dev --config '{"identifier":"com.cyberneura.flashcap.devtest"}' -- -- <画像>`
  (設定は `<scratch>/.config/flashcap/config.json` になる)。プロジェクト直下のファイル
  (`flashcap.code-workspace` など) が書き換わると vite が全体をリロードし、起動引数で開いた画像が消える。
  その時は同じ HOME で `target/debug/flashcap <画像>` をもう一度起動すれば single-instance 経由で開き直せる
- Production build: `cargo build --release` in `src-tauri/` (run before push)

## Release (.github/workflows/release.yml + scripts/release.sh)

配布は GitHub Release。**main の `tauri.conf.json` の version が未リリースなら、main への push で
Actions が走り、署名+公証済み universal dmg が公開される**。`pnpm release [patch|minor|major]` は
version を採番して main へ push し、その run を watch するだけ。PR の中で version を上げて
マージしても同じくリリースされる。Homebrew cask (`cyberneura/homebrew-tap`) は tap 側が
1 時間ごとに最新 Release を見て追従するので、こちらからは何もしない。

- **起動は `push: main`、判定は `plan` ジョブ** (中身は `scripts/release-decide.sh`)。
  `releases/tags/v<version>` が 404 ならリリース (draft は 404 になる。念のため 200 でも `.draft` が true なら未リリース扱い)、
  200 なら何もしない、それ以外 (rate limit・障害) は失敗させる。障害を「未リリース」と読むと
  公開済み version を二重に出しにいくため。diff ではなく version で決めるので、squash / rebase /
  直 push のどれでも結果が同じになる (path フィルターも付けない)。
- **未リリースでも、公開中の最新 Release より古い version は出さない** (`sort -V` で比較し、warning を
  出して skip)。publish は `--latest` を付けるので、出すと最新が巻き戻って tap もダウングレードする。
  version を上げた commit の revert (一度も出していない 0.2.0 に戻る等) と、pending の run が push 順と
  逆に消化された時 (GitHub は順序を保証しない) に起きる。後者で飛ばされた version は、それを追い越した
  新しい version に含まれているので出し直さない。比較のため version は X.Y.Z に限る。
- **publish も公開直前に `release-decide.sh` をやり直す**。「Re-run failed jobs」は成功済みの `plan` を
  再実行せず当時の `release=true` を使い回すので、その間に新しい version が出ていると巻き戻すため。
  止めた場合は失敗させ、draft を残す。
- **main への push でも、その version がリリース済みなら ubuntu の `plan` 1 本で終わる**。
  「version を変えていないから何も起きない」ではない点に注意。build 失敗で draft が残っている間は
  その version が未リリースなので、無関係な commit を push しても、その commit でビルド・公開される。
  `test` は PR か「リリースする」判定の時だけ走る (`!cancelled()` で plan の skipped 連鎖を外している)。
  `build` / `publish` は `needs.plan.outputs.release == 'true'` で止める。
- **`workflow_dispatch` は失敗した run の再実行用に残してある**。`plan` を通るので公開済み version は
  出せず、main 以外の ref からの dispatch は `plan` が拒否する。
- **macOS と Windows をビルドする** (build の matrix)。macOS は
  `--target universal-apple-darwin --bundles dmg` の `flashcap_<version>_universal.dmg`
  (x86_64 + arm64、署名 + 公証)。Windows は `--bundles nsis` の
  `flashcap_<version>_x64-setup.exe` (署名なし)。test も同じ 2 OS の matrix で走る。
  Apple の Secret は macOS のジョブにだけ渡す (Windows では空文字)。
- **draft Release はビルド前の `draft` ジョブが 1 つ作り、tauri-action には `releaseId` で渡す**
  (queryfolio と同じ構成)。tagName 方式だと、macOS と Windows がほぼ同時に終わった時に
  両方が「無い」と判断して同じ tag の draft を 2 つ作る。失敗した run が残した draft は
  再利用し、target をその run の commit に直す。
- **draft → build → publish の構成**。build が全プラットフォーム成功した後に、publish ジョブが
  `gh release edit --draft=false --latest` で公開する。失敗時は draft のまま残る。
- **リリース済みの version は二度と出ない**。公開済みと同じ version で build まで進むと
  tauri-action が draft 状態の不一致でエラーになるが、`plan` がそこへ行かせない。
  build 失敗で残った draft (tag がまだ無いので `plan` からは 404 に見える) は、次の push か
  dispatch で同じ version のまま埋め直される (`draft` ジョブが一覧から探して再利用し、
  tauri-action が同名 asset を差し替える)。
- **publish の `--target "${GITHUB_SHA}"` は消さない**。`draft` ジョブも target を直しているが、
  tag は公開時に target へ作られるので、公開の直前にもう一度合わせておく。ずれると tag と
  成果物の中身が別の commit を指すことになる。
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
- **`cancel-in-progress: false` + `queue: max` の両方を書く**。run 単位で直列にするので、後続 run の
  `plan` は先行 run の publish 後に走り、同じ version を二重にビルドしない。既定の `queue: single` は
  pending を 1 件しか保持せず、新しい run が既存 pending を置き換える。`queue: max` は 100 件まで積む。
  PR の run は commit ごとの別グループなので、この直列化に巻き込まない。
- **`dtolnay/rust-toolchain` の SHA は master 履歴から選ぶ**。`@stable` の指す SHA は生成ブランチ
  stable の先端で、それを pin すると stable が進んだ時に commit が GC され、以降の run が Rust
  セットアップ前に落ちる。master 履歴の SHA を pin し、ref から toolchain を推測できなくなる分
  `toolchain: stable` を明示する。
- **`pnpm publish` は使えない** (pnpm 組み込みコマンドで scripts から上書き不可)。
  コマンド名は必ず `release`。
- **`package.json` の version は飾り**だが、見た目の一貫性のため release.sh が
  `tauri.conf.json` と同期させている。tauri-action が読むのは `tauri.conf.json` の方。
- **弱点**: `pnpm release` は main へ直接 push する。ブランチ保護 (PR 必須) を掛けるとこのスクリプトは
  使えなくなるが、PR で `tauri.conf.json` / `package.json` の version を上げてマージすれば同じくリリースされる。
- 必要な GitHub Secrets (登録済み): `APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD` /
  `APPLE_SIGNING_IDENTITY` / `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID`。
  `APPLE_PASSWORD` は App 用パスワード (通常の Apple ID パスワードでは公証が通らない)。

## Framework Note

SvelteKit 2 (Svelte 5 runes) and Tauri 2.x are new frameworks. Use **context7 MCP** to look up current API docs before making changes.
