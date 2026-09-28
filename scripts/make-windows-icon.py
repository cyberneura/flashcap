#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow>=10"]
# ///
"""Windows 用のアプリアイコン (`src-tauri/icons/icon.ico`) を作る。

    uv run scripts/make-windows-icon.py

マスター (`src-tauri/icons/icon.png`) は macOS 用で、角丸の板の周囲に約 10% の
透明な余白がある。macOS の Dock はその余白込みで他のアプリと大きさが揃うが、
Windows のタスクバーは画像いっぱいに描く前提なので、そのまま使うと一回り
小さく見える (CYBERNEURA-DEV-885)。

そこで、マスターの不透明な範囲 (= 角丸の板) だけを切り出し、余白の無い
favicon と同じ作りで 16 / 24 / 32 / 48 / 64 / 256 の ico を書き出す。
icns (macOS) には触らない。

**`pnpm exec tauri icon src-tauri/icons/icon.png` で ico を作り直さないこと。**
余白付きのマスターから作るので、小さいアイコンに戻る。
"""

from __future__ import annotations

from pathlib import Path

from PIL import Image

ICONS = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
MASTER = ICONS / "icon.png"
OUT = ICONS / "icon.ico"

# tauri icon が作っていた ico と同じ組み合わせ
SIZES = [16, 24, 32, 48, 64, 256]


def main() -> None:
    master = Image.open(MASTER).convert("RGBA")
    bbox = master.getchannel("A").getbbox()
    if bbox is None:
        raise SystemExit(f"{MASTER} is fully transparent")
    left, top, right, bottom = bbox
    if right - left != bottom - top:
        raise SystemExit(f"opaque area of {MASTER} is not square: {bbox}")

    plate = master.crop(bbox)
    frames = [plate.resize((s, s), Image.Resampling.LANCZOS) for s in SIZES]
    # 最大のフレームを基準に保存し、残りは append_images で渡す
    # (Pillow は sizes に合う提供済みフレームをそのまま使う)
    frames[-1].save(
        OUT,
        format="ICO",
        sizes=[(s, s) for s in SIZES],
        append_images=frames[:-1],
    )

    written = Image.open(OUT)
    got = sorted(written.info["sizes"])
    want = sorted((s, s) for s in SIZES)
    if got != want:
        raise SystemExit(f"{OUT} has sizes {got}, expected {want}")
    print(f"cropped {MASTER.name} to {bbox} ({right - left}px) -> {OUT} {SIZES}")


if __name__ == "__main__":
    main()
