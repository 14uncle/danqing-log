#!/usr/bin/env python3
"""从产品 logo 生成 Microsoft Store 所需的图标素材 (MSIX 包内 Assets/)。

用法 (仓库根目录):
    python tools/gen_store_assets.py

读 `assets/logo/log_256.png`, 产出到 `../release-archives/log/msix/assets/`:

    StoreLogo.png        50x50
    Square44x44Logo.png  44x44
    Square150x150Logo.png 150x150
    Wide310x150Logo.png  310x150 (logo 居中)
    SplashScreen.png     620x300 (logo 居中)
    Square44x44Logo.scale-{100,125,150,200,400}.png
    Square44x44Logo.targetsize-{16,24,32,48,256}[_altform-unplated].png

工艺来自 `danqing-pomodoro/tools/gen_store_assets.py` (2026-09-04 商店版实测成稿),
**参数原样照搬, 未自行调**: 本仓 logo 的内建留白 (内容占 80.5% x 64.8%) 与
pomodoro 的 (78.1% 方形) 同量级, 属同一档; 没有真机对照就改参数是瞎猜。

三样东西缺一不可, 少一样任务栏图标就会垫上 Windows 默认蓝底板:

1. **targetsize-*_altform-unplated** —— shell 见到它才用**裸图标**, 不垫底板。
   `BackgroundColor="transparent"` 在任务栏表面会回落成默认蓝。
2. **scale-* 家族** —— shell 走「现代资源解析」路径的触发器; 缺了会落到 legacy
   基础图标路径, 于是又垫底板。
3. **干净的透明** —— alpha=0 的像素 RGB 必须归零。缩放/抗锯齿边缘会留下
   「a=0 但 RGB≠0」的脏像素, Windows 按 premultiplied alpha 合成会据此判定
   非干净透明, 照样垫底板 (`create_logo` 末尾就是在清这个)。
"""

import sys
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    print("ERROR: 未安装 Pillow。请先 pip install Pillow")
    sys.exit(1)

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC_LOGO = REPO_ROOT / "assets" / "logo" / "log_256.png"
OUT_DIR = REPO_ROOT / ".." / "release-archives" / "log" / "msix" / "assets"

# manifest 里直接引用的基线尺寸
ASSETS = {
    "StoreLogo.png": (50, 50),
    "Square44x44Logo.png": (44, 44),
    "Square150x150Logo.png": (150, 150),
    "Wide310x150Logo.png": (310, 150),
    "SplashScreen.png": (620, 300),
}

# ⚠️ 必须是 (0,0,0,0) 干净透明 —— 不能带 RGB 残留 (见模块头第 3 条)
BG_COLOR = (0, 0, 0, 0)

# targetsize 变体: 覆盖任务栏 / Alt-Tab / 标题栏等 shell 表面
TARGETSIZES = (16, 24, 32, 48, 256)

# scale 变体: DPI 缩放资产。物理尺寸 = 44 * N/100
SCALES = {100: 44, 125: 55, 150: 66, 200: 88, 400: 176}


def create_logo(src, size, bg=BG_COLOR, padding_ratio=0.15):
    """把 logo 居中放到给定尺寸的画布上 (带留白), 并清掉脏透明像素。"""
    w, h = size
    canvas = Image.new("RGBA", (w, h), bg)

    padding = int(min(w, h) * padding_ratio)
    target = min(w, h) - padding * 2
    logo = src.copy()
    logo.thumbnail((target, target), Image.Resampling.LANCZOS)

    x = (w - logo.width) // 2
    y = (h - logo.height) // 2
    canvas.paste(logo, (x, y), logo if logo.mode == "RGBA" else None)

    # 清脏透明: alpha=0 的像素 RGB 归零 (straight alpha 语义)
    r, g, b, a = canvas.split()
    opaque = a.point(lambda v: 255 if v > 0 else 0)
    zero = Image.new("L", canvas.size, 0)
    r = Image.composite(r, zero, opaque)
    g = Image.composite(g, zero, opaque)
    b = Image.composite(b, zero, opaque)
    return Image.merge("RGBA", (r, g, b, a))


def main():
    if not SRC_LOGO.exists():
        print(f"ERROR: 源 logo 不存在: {SRC_LOGO}")
        sys.exit(1)

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    src = Image.open(SRC_LOGO).convert("RGBA")

    for name, size in ASSETS.items():
        img = create_logo(src, size)
        img.save(str(OUT_DIR / name), "PNG")
        print(f"  {name:30s} {size[0]}x{size[1]}")

    # 图形与基线款一致 (同 padding)
    for scale, px in SCALES.items():
        name = f"Square44x44Logo.scale-{scale}.png"
        img = create_logo(src, (px, px))
        img.save(str(OUT_DIR / name), "PNG")
        print(f"  {name:52s} {px}x{px}")

    # 双家族: plated (壳可垫底板) + altform-unplated (裸图标)。留白取小值。
    for n in TARGETSIZES:
        for suffix in ("", "_altform-unplated"):
            name = f"Square44x44Logo.targetsize-{n}{suffix}.png"
            img = create_logo(src, (n, n), padding_ratio=0.05)
            img.save(str(OUT_DIR / name), "PNG")
            print(f"  {name:52s} {n}x{n}")

    total = len(ASSETS) + len(SCALES) + len(TARGETSIZES) * 2
    print(f"\n完成: {total} 个素材 -> {OUT_DIR}")


if __name__ == "__main__":
    main()
