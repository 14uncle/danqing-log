# 截图脚本: 启动应用打开指定文件, 等待渲染稳定后按 HWND PrintWindow 截窗口, 然后杀进程。
# 用途: 分发/商店物料的零交互截图 (只能拍「打开即得」的画面, 需交互构图的仍由人工拍)。
#
# 用法 (仓库根目录):
#   powershell -NoProfile -File tools/take_shot.ps1 -File <日志文件路径> -Out <输出png路径> [-SettleSec 6]
#
# 注意 (都是 2026-10-08 实测踩过的):
# - **必须用 PrintWindow 按 HWND 截窗口, 别用 CopyFromScreen 截屏幕** —— 后者在本机
#   (双屏, 副屏 2560x1440 在上方) 截到的是裸壁纸, 窗口明明 visible=True 也截不到。
# - PW_RENDERFULLCONTENT(2) 对 wgpu/DX12 自绘窗口实测有效。
# - 最大化窗口的 rect 含 8px 隐形 resize 边框 (-8,-8 起), 默认裁掉 (-CropBorder 8)。
# - Start-Process -PassThru 拿真 PID, Stop-Process 直接杀 (不优雅退出 = 不写 config, 正是要的)。
# - 杀的是本脚本启动的那个实例; 若机器上已有别的 danqing-log 在跑, 先关掉再拍。

param(
    [Parameter(Mandatory=$true)][string]$File,
    [Parameter(Mandatory=$true)][string]$Out,
    [int]$SettleSec = 6,
    [int]$CropBorder = 8
)

$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class Win32Cap {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
}
public struct RECT { public int Left, Top, Right, Bottom; }
'@ -ReferencedAssemblies System.Drawing

$exe = Join-Path $PSScriptRoot '..\target\release\danqing-log.exe'
if (-not (Test-Path $exe)) { throw "release 二进制不存在: $exe (先 cargo build --release)" }
if (-not (Test-Path $File)) { throw "输入文件不存在: $File" }

$outDir = Split-Path -Parent $Out
if ($outDir -and -not (Test-Path $outDir)) { New-Item -ItemType Directory -Path $outDir -Force | Out-Null }

$p = Start-Process -FilePath $exe -ArgumentList "`"$File`"" -PassThru
try {
    Start-Sleep -Seconds $SettleSec
    $p.Refresh()
    $h = $p.MainWindowHandle
    if ($h -eq [IntPtr]::Zero) { throw "进程无主窗口 (启动失败?)" }

    $r = New-Object RECT
    [void][Win32Cap]::GetWindowRect($h, [ref]$r)
    $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top

    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    $ok = [Win32Cap]::PrintWindow($h, $hdc, 2)
    $g.ReleaseHdc($hdc)
    if (-not $ok) { throw "PrintWindow 失败" }

    if ($CropBorder -gt 0) {
        $cw = $w - $CropBorder * 2; $ch = $ht - $CropBorder * 2
        $crop = $bmp.Clone([System.Drawing.Rectangle]::new($CropBorder, $CropBorder, $cw, $ch), $bmp.PixelFormat)
        $bmp.Dispose()
        $bmp = $crop
    }
    $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Host "已保存 $Out ($($bmp.Width)x$($bmp.Height))"
}
finally {
    if (-not $p.HasExited) { $p.Kill(); $p.WaitForExit() }
}
