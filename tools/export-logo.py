#!/usr/bin/env python3
"""从 assets/logo/log.svg 的几何导出全部位图资产 (与 SVG 手工同步).

与 danqing / danqing-clipboard / danqing-pomodoro 同一工艺: Pillow 手工几何 +
4x 超采样抗锯齿, 不依赖系统 Cairo。改了 SVG 必须同步改这里的常量。

依赖: pip install Pillow
用法: python tools/export-logo.py

输出:
    assets/logo/log_{16,24,32,48,128,256}.png
    assets/logo.ico  (build.rs 嵌入 exe; 引擎运行时另读 log_16/256.png)
"""

from pathlib import Path

from PIL import Image, ImageDraw

REPO_ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = REPO_ROOT / "assets" / "logo"
ICO_PATH = REPO_ROOT / "assets" / "logo.ico"

PNG_SIZES = [16, 24, 32, 48, 128, 256]
ICO_SIZES = [16, 24, 32, 48, 256]

JADE = (15, 118, 110, 255)  # 玉色 #0F766E
CINNABAR = (227, 66, 52, 255)  # 朱砂 #E34234 (品牌专属, 不进 theme token)
GLASS = (255, 255, 255, 216)  # 玻璃白 0.85

SCALE = 4  # 超采样抗锯齿


def rr(draw: ImageDraw.ImageDraw, xy, radius, fill) -> None:
    draw.rounded_rectangle(xy, radius=radius, fill=fill)


def render_logo(size: int) -> Image.Image:
    canvas = size * SCALE
    img = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)
    s = canvas / 256.0

    # 视窗外框: 玉色实心 + 玻璃白内填 (x=28,y=48,w=200,h=160,r=28,stroke=20)
    rr(draw, (28 * s, 48 * s, 228 * s, 208 * s), 28 * s, JADE)
    rr(draw, (48 * s, 68 * s, 208 * s, 188 * s), 8 * s, GLASS)

    # 日志行: 前三条玉色 + 底部一条朱砂 (x=62, 行高 16, pitch 32)
    rr(draw, (62 * s, 72 * s, 182 * s, 88 * s), 8 * s, JADE)
    rr(draw, (62 * s, 104 * s, 144 * s, 120 * s), 8 * s, JADE)
    rr(draw, (62 * s, 136 * s, 192 * s, 152 * s), 8 * s, JADE)
    rr(draw, (62 * s, 168 * s, 200 * s, 184 * s), 8 * s, CINNABAR)

    return img.resize((size, size), Image.Resampling.LANCZOS)


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    rendered: dict[int, Image.Image] = {}
    for size in PNG_SIZES:
        img = render_logo(size)
        rendered[size] = img
        path = OUT_DIR / f"log_{size}.png"
        img.save(path, "PNG")
        print(f"Exported {path}")

    # ico 多尺寸帧: ICO 插件静默忽略 append_images (Pillow 11.x 只写首帧), 用
    # sizes 参数由 256 母版缩出全部尺寸帧 (同 danqing-clipboard 修复方案)。
    ico_sizes = [(s, s) for s in ICO_SIZES]
    rendered[256].save(ICO_PATH, format="ICO", sizes=ico_sizes)
    print(f"Exported {ICO_PATH}")


if __name__ == "__main__":
    main()
