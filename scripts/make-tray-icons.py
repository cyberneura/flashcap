#!/usr/bin/env python3
"""メニューバー (トレイ) のアイコンを描き出す (標準ライブラリだけ)。

    python3 scripts/make-tray-icons.py

出力は `src-tauri/icons/tray/menu-bar.png`。36x36 の RGBA で、色は黒・形は
アルファだけで表す。macOS のテンプレート画像 (`icon_as_template(true)`) として
使うので、ダーク / ライトの切り替えは OS が行う。

**36px にしているのは tray-icon crate がメニューバーのアイコンを高さ 18pt に
揃えるため。** Retina では 18pt = 36px なので、等倍で載って縮小のボケが出ない。

図柄はアプリアイコン (`src-tauri/icons/icon.png`) と同じ線画のカメラ。
メニューバーは線の太さが揃っていないと浮くので、どの線も同じ太さで描く。

Pillow / ImageMagick / rsvg が無い環境でも作り直せるよう、図形は距離関数で書いて
4x4 のスーパーサンプリングで塗っている。
"""

from __future__ import annotations

import math
import struct
import zlib
from pathlib import Path

SIZE = 36
SUBSAMPLES = 4
OUT_DIR = Path(__file__).resolve().parent.parent / "src-tauri" / "icons" / "tray"


def capsule(ax: float, ay: float, bx: float, by: float, width: float):
    """線分 (ax, ay)-(bx, by) を太さ width の丸い端で描く"""
    r = width / 2

    def inside(x: float, y: float) -> bool:
        dx, dy = bx - ax, by - ay
        length2 = dx * dx + dy * dy
        t = 0.0 if length2 == 0 else max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / length2))
        px, py = ax + t * dx, ay + t * dy
        return math.hypot(x - px, y - py) <= r

    return inside


def circle(cx: float, cy: float, r: float):
    def inside(x: float, y: float) -> bool:
        return math.hypot(x - cx, y - cy) <= r

    return inside


LINE_WIDTH = 2.6


def rounded_rect_outline(x0: float, y0: float, x1: float, y1: float, radius: float, width: float):
    """角の丸い長方形の輪郭 (内側を塗らない)"""
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    hx, hy = (x1 - x0) / 2 - radius, (y1 - y0) / 2 - radius

    def inside(x: float, y: float) -> bool:
        qx, qy = abs(x - cx) - hx, abs(y - cy) - hy
        outside = math.hypot(max(qx, 0.0), max(qy, 0.0))
        distance = outside + min(max(qx, qy), 0.0) - radius
        return abs(distance) <= width / 2

    return inside


def ring(cx: float, cy: float, r: float, width: float):
    def inside(x: float, y: float) -> bool:
        return abs(math.hypot(x - cx, y - cy) - r) <= width / 2

    return inside


def camera():
    w = LINE_WIDTH
    return [
        # 本体
        rounded_rect_outline(3.5, 11.0, 32.5, 30.5, 4.5, w),
        # 上のファインダーの出っ張り
        capsule(12.0, 11.0, 14.5, 6.5, w),
        capsule(14.5, 6.5, 21.5, 6.5, w),
        capsule(21.5, 6.5, 24.0, 11.0, w),
        # レンズ
        ring(18.0, 20.5, 5.5, w),
        # フラッシュ
        circle(9.0, 16.0, 1.5),
    ]


def render(shapes) -> bytes:
    step = 1.0 / SUBSAMPLES
    offsets = [(i + 0.5) * step for i in range(SUBSAMPLES)]
    total = SUBSAMPLES * SUBSAMPLES
    rows = bytearray()
    for py in range(SIZE):
        rows.append(0)  # フィルタ種別 None
        for px in range(SIZE):
            covered = 0
            for oy in offsets:
                for ox in offsets:
                    x, y = px + ox, py + oy
                    if any(shape(x, y) for shape in shapes):
                        covered += 1
            rows += bytes((0, 0, 0, round(255 * covered / total)))
    return bytes(rows)


def write_png(path: Path, raw_rows: bytes) -> None:
    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw_rows, 9))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(png)


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    out = OUT_DIR / "menu-bar.png"
    write_png(out, render(camera()))
    print(out)


if __name__ == "__main__":
    main()
