# build_msix.ps1 - 打 Microsoft Store 用的 MSIX 包。
#
# 产物: ../release-archives/log/msix/danqing-log-store-v<Version>-x64.msix
# 商店会在收录后**自动重签**, 所以提交用的包**不需要签名**。
# (本地侧载测试要签名 —— 那一步在 tools/sign_msix_local.ps1。)
#
# 用法 (仓库根目录):
#   powershell -NoProfile -File tools/build_msix.ps1
#   powershell -NoProfile -File tools/build_msix.ps1 -Version 1.0.1
#
# 默认标识**就是** Partner Center 的真实值 (2026-09-13 回填) —— 直接跑出来的包即可提交。
# 要改身份走 `-AppName` / `-PublisherCN` / `-PublisherDisplayName` 覆盖。
#
# 工具链: makeappx / makepri / signtool 从 tools/sdk-tools 取, **不进仓库**
# (19MB, 见 .gitignore)。缺了用 nuget 补:
#   nuget install Microsoft.Windows.SDK.BuildTools
# 工艺与踩坑来自 ../danqing-pomodoro/tools/build_msix.ps1 (2026-09 商店版实测成稿)。

param(
    [string]$Version = "",
    [string]$OutDir = "..\release-archives\log\msix",
    # Partner Center 真实标识 (2026-09-13 回填, 应用和游戏 -> 产品标识 页)。
    # 三个值**区分大小写、逐字符**匹配, 首次提交通过后**不可更改** —— 别手打, 用这里的默认值。
    # 注: Publisher 的 GUID 是**账号级**的 (与 danqing-pomodoro 同一个), 改产品名不影响它。
    # Name 曾预留为 `14uncle.57340CE8CAE9E` (显示名「丹青-日志」), 同日改用本名 ——
    # 理由: 窗口标题 / README / 仓库 / 包内全叫「丹青日志 LogLens」, 只有商店页另叫一个名
    # 就是本仓一直在打的「同一内容写两处然后漂了」。未发布时改是白改, 发布后就贵了。
    [string]$PublisherCN = "CN=5F2A7EA5-3366-4B8A-8C0D-3BE22575711A",
    [string]$AppName = "14uncle.LogLens",
    [string]$DisplayName = "丹青日志 LogLens",
    [string]$PublisherDisplayName = "14uncle",
    [string]$Description = "大文件日志 / JSONL 查看分析器 —— 秒开 1GB, 级别计数一键筛",
    [string]$BinaryName = "danqing-log",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path "$PSScriptRoot\.."
$SdkBin = Join-Path $RepoRoot "tools\sdk-tools\bin\10.0.22621.0\x64"
$MakeAppx = Join-Path $SdkBin "makeappx.exe"
$MakePri  = Join-Path $SdkBin "makepri.exe"

foreach ($tool in @($MakeAppx, $MakePri)) {
    if (-not (Test-Path $tool)) {
        Write-Host "ERROR: 找不到 $tool"
        Write-Host "补法: nuget install Microsoft.Windows.SDK.BuildTools 然后把 x64 目录放到 tools\sdk-tools\bin\10.0.22621.0\"
        exit 1
    }
}

# 版本号默认从 Cargo.toml 读
if (-not $PSBoundParameters.ContainsKey('Version')) {
    $CargoToml = Join-Path $RepoRoot "Cargo.toml"
    $Match = [regex]::Match((Get-Content $CargoToml -Raw), '(?s)\[package\][^[]*?version\s*=\s*"([^"]+)"')
    if (-not $Match.Success) { Write-Host "ERROR: Cargo.toml 里没读到 version"; exit 1 }
    $Version = $Match.Groups[1].Value
    Write-Host ("Version from Cargo.toml: {0}" -f $Version)
}

# manifest 的 Version 必须是**四段** (Major.Minor.Build.Revision), 商店要求末位为 0
$VerParts = @($Version.Split('.'))
while ($VerParts.Count -lt 3) { $VerParts += "0" }
$MsixVersion = "$($VerParts[0]).$($VerParts[1]).$($VerParts[2]).0"

if (-not $SkipBuild) {
    Write-Host "=== cargo build --release ==="
    Push-Location $RepoRoot
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { Write-Host "ERROR: cargo build 失败"; exit 1 }
    } finally { Pop-Location }
}

$ExePath = Join-Path $RepoRoot "target\release\${BinaryName}.exe"
if (-not (Test-Path $ExePath)) { Write-Host "ERROR: 找不到 $ExePath"; exit 1 }

# ---- 清空 + 建 staging ----
$OutPath = Join-Path $RepoRoot $OutDir
$Stage = Join-Path $OutPath "msix-staging"
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Path $Stage -Force | Out-Null

Write-Host "=== 拷贝应用文件 ==="
Copy-Item $ExePath $Stage

# 运行时资产: 引擎启动时读 assets/logo/{name}_16.png 与 _256.png (经 danqing::asset::resolve,
# exe 目录优先、CWD 回退 —— MSIX 下 CWD 是 System32, 所以必须跟 exe 同级带过去)
$AssetsDir = Join-Path $RepoRoot "assets"
if (-not (Test-Path $AssetsDir)) { Write-Host "ERROR: 仓库根没有 assets/"; exit 1 }
Copy-Item -Recurse $AssetsDir (Join-Path $Stage "assets")
Write-Host "已拷贝 assets/"

# ---- Store 素材 ----
$StoreAssets = Join-Path $OutPath "assets"
if (-not (Test-Path $StoreAssets)) {
    Write-Host "Store 素材缺失, 跑 gen_store_assets.py ..."
    Push-Location $RepoRoot
    try { python tools\gen_store_assets.py } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { Write-Host "ERROR: gen_store_assets.py 失败"; exit 1 }
}
$DestAssets = Join-Path $Stage "Assets"
New-Item -ItemType Directory -Path $DestAssets -Force | Out-Null
Copy-Item "$StoreAssets\*" $DestAssets

# 运行时资产目录名必须是 "Assets" (与 manifest 引用**同大小写**):
# shell 装包时按包内路径索引图标, 大小写不匹配 → 任务栏/磁贴图标落回蓝色占位块。
# 运行时读取在 Windows 上不区分大小写, 不受影响。
# (纯大小写改名在 Windows 上要两步, 直接改会 IOException)
$LowerAssets = Join-Path $Stage "assets"
if (Test-Path $LowerAssets) {
    Rename-Item $LowerAssets "assets-case-tmp"
    Rename-Item (Join-Path $Stage "assets-case-tmp") "Assets"
}

# ---- AppxManifest.xml ----
Write-Host "=== 生成 AppxManifest.xml ==="
$ManifestLines = @(
    '<?xml version="1.0" encoding="utf-8"?>'
    '<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"'
    '         xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"'
    '         xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"'
    '         IgnorableNamespaces="uap rescap">'
    ''
    '  <Identity Name="' + $AppName + '"'
    '            Publisher="' + $PublisherCN + '"'
    '            Version="' + $MsixVersion + '"'
    '            ProcessorArchitecture="x64" />'
    ''
    '  <Properties>'
    '    <DisplayName>' + $DisplayName + '</DisplayName>'
    '    <PublisherDisplayName>' + $PublisherDisplayName + '</PublisherDisplayName>'
    '    <Logo>Assets\StoreLogo.png</Logo>'
    '    <Description>' + $Description + '</Description>'
    '  </Properties>'
    ''
    '  <Dependencies>'
    '    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.17763.0" MaxVersionTested="10.0.22621.0" />'
    '  </Dependencies>'
    ''
    '  <Resources>'
    '    <Resource Language="zh-CN" />'
    '  </Resources>'
    ''
    '  <Applications>'
    '    <Application Id="App"'
    '                 Executable="' + $BinaryName + '.exe"'
    '                 EntryPoint="Windows.FullTrustApplication">'
    '      <uap:VisualElements DisplayName="' + $DisplayName + '"'
    '                          Description="' + $Description + '"'
    # BackgroundColor 用 transparent = 任务栏裸图标。
    # ⚠️ 不要加回 <uap:DefaultTile> / <uap:SplashScreen>: 2026-09-04 pomodoro 实测,
    # 两者存在时 Win11 任务栏把 transparent 渲染成默认蓝底板; 去掉后与 VS Code
    # 结构一致。任务栏图标另有两条硬前提 —— resources.pri (下面用 MakePri 生成)
    # 与 Assets 目录大小写, 见对应注释。
    '                          BackgroundColor="transparent"'
    '                          Square150x150Logo="Assets\Square150x150Logo.png"'
    '                          Square44x44Logo="Assets\Square44x44Logo.png">'
    '      </uap:VisualElements>'
    '    </Application>'
    '  </Applications>'
    ''
    '  <Capabilities>'
    '    <rescap:Capability Name="runFullTrust" />'
    '  </Capabilities>'
    ''
    '</Package>'
)
# **无 BOM** 写盘: 带 BOM 的 manifest 会让包安装失败 (pomodoro 8/30 的 fix_msix.py 修的就是它)
$ManifestPath = Join-Path $Stage "AppxManifest.xml"
[System.IO.File]::WriteAllText($ManifestPath, ($ManifestLines -join "`n"), (New-Object System.Text.UTF8Encoding($false)))

# ---- resources.pri ----
Write-Host "=== 生成 resources.pri (MakePri) ==="
# **这条是任务栏图标裸底的关键, 别删**。shell 靠 resources.pri 做「限定资源解析」,
# 才知道图标有 scale / targetsize / altform-unplated 变体可挑; 缺了它只能拿基础
# Square44x44Logo.png 垫 BackgroundColor 底板 → transparent 回落成默认蓝。
# pomodoro 为此排查了一整轮 (BackgroundColor、DefaultTile/SplashScreen、scale 家族、
# 清图标缓存、PNG 脏透明像素全试过都无效)。对照 ScreenToGif / rufus 的包均含此文件。
Push-Location $Stage
try {
    & $MakePri createconfig /cf priconfig.xml /dq lang-zh-CN /pv 10.0.0 /o
    if ($LASTEXITCODE -ne 0) { Write-Host "ERROR: makepri createconfig 失败"; exit 1 }
    & $MakePri new /pr . /cf priconfig.xml /of resources.pri /o
    if ($LASTEXITCODE -ne 0) { Write-Host "ERROR: makepri new 失败"; exit 1 }
    # priconfig.xml 只是生成 resources.pri 的中间配置, 不进包
    Remove-Item priconfig.xml -ErrorAction SilentlyContinue
} finally { Pop-Location }

# ---- 打包 ----
Write-Host "=== makeappx pack ==="
$MsixPath = Join-Path $OutPath "danqing-log-store-v${Version}-x64.msix"
if (Test-Path $MsixPath) { Remove-Item $MsixPath -Force }

& $MakeAppx pack /d $Stage /p $MsixPath /o
if ($LASTEXITCODE -ne 0) { Write-Host "ERROR: makeappx 失败"; exit 1 }

# 注意: 这里算的是**未签名**的字节。tools/sign_msix_local.ps1 签完会就地重算并回写这个
# .sha256 —— 所以签过名的包按本行此刻的值核对**必然对不上**, 那不是包坏了。
$Bytes = [System.IO.File]::ReadAllBytes($MsixPath)
$Sha = [System.Security.Cryptography.SHA256]::Create()
$Hash = ([System.BitConverter]::ToString($Sha.ComputeHash($Bytes)) -replace '-', '').ToLower()
$Sha.Dispose()
[System.IO.File]::WriteAllText("$MsixPath.sha256", $Hash, [System.Text.Encoding]::ASCII)

Write-Host ""
Write-Host "=== MSIX 已生成 ==="
Write-Host ("包:      {0}" -f $MsixPath)
Write-Host ("身份:    {0} / {1} / x64 (v{2})" -f $AppName, $PublisherCN, $MsixVersion)
Write-Host ("大小:    {0:N0} bytes" -f (Get-Item $MsixPath).Length)
Write-Host ("SHA256:  {0}" -f $Hash)
Write-Host ""
Write-Host "商店会收录后自动重签, 提交用的包不用签名。"
Write-Host "本地侧载实测: powershell -NoProfile -File tools/sign_msix_local.ps1"
