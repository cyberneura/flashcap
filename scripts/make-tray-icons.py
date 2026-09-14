#!/usr/bin/env python3
"""メニューバー (トレイ) のアイコン 3 枚を描き出す (標準ライブラリだけ)。

    python3 scripts/make-tray-icons.py

出力は `src-tauri/icons/tray/auto-copy-{none,path,image}.png`。どれも 36x36 の
RGBA で、色は黒・形はアルファだけで表す。macOS のテンプレート画像
(`icon_as_template(true)`) として使うので、ダーク / ライトの切り替えは OS が行う。

**36px にしているのは tray-icon crate がメニューバーのアイコンを高さ 18pt に
揃えるため。** Retina では 18pt = 36px なので、等倍で載って縮小のボケが出ない。

図柄は「スクリーンショットの範囲選択」を表す四隅のかぎ括弧が共通で、中身だけが
設定ごとに変わる:

- none  : 括弧だけ (何もコピーしない)
- path  : 文字の行 (パス = テキストをコピーする)
- image : 山と太陽 (画像データをコピーする)

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


def polygon(points: list[tuple[float, float]]):
    """偶奇規則で内側を判定する (凹多角形でもよい)"""

    def inside(x: float, y: float) -> bool:
        hit = False
        n = len(points)
        for i in range(n):
            x1, y1 = points[i]
            x2, y2 = points[(i + 1) % n]
            if (y1 > y) != (y2 > y):
                cross = x1 + (y - y1) * (x2 - x1) / (y2 - y1)
                if x < cross:
                    hit = not hit
        return hit

    return inside


def corner_brackets():
    """範囲選択の四隅。図柄の共通部分"""
    lo, hi, arm, width = 3.5, 32.5, 9.0, 3.0
    shapes = []
    for cx, sx in ((lo, 1), (hi, -1)):
        for cy, sy in ((lo, 1), (hi, -1)):
            shapes.append(capsule(cx, cy, cx + sx * arm, cy, width))
            shapes.append(capsule(cx, cy, cx, cy + sy * arm, width))
    return shapes


def text_lines():
    width = 2.6
    return [
        capsule(11.5, 13.0, 24.5, 13.0, width),
        capsule(11.5, 18.0, 24.5, 18.0, width),
        capsule(11.5, 23.0, 19.5, 23.0, width),
    ]


def picture():
    return [
        polygon([(9.5, 25.5), (15.5, 15.5), (19.8, 21.8), (22.3, 18.6), (26.5, 25.5)]),
        circle(23.0, 12.0, 2.7),
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
    variants = {
        "none": corner_brackets(),
        "path": corner_brackets() + text_lines(),
        "image": corner_brackets() + picture(),
    }
    for name, shapes in variants.items():
        out = OUT_DIR / f"auto-copy-{name}.png"
        write_png(out, render(shapes))
        print(out)


if __name__ == "__main__":
    main()
