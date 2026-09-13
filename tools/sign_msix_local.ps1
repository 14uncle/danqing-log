# sign_msix_local.ps1 - MSIX 侧载实测: 自签证书 + 信任 + 签名 + 安装
#
# **仅用于本地测试。** 提交商店的包**不签名** —— 商店收录后用它自己的证书重签,
# 而且本地签过的包反而会被拒 (你的证书主题不可能等于 Partner Center 给的 CN=<GUID>)。
#
# 用法 (仓库根目录):
#   powershell -NoProfile -File tools/sign_msix_local.ps1
#
# 四步:
#   1. 内存生成自签名证书 (主题必须 == manifest 的 Publisher, 否则 0x8007000B), 导出 PFX
#   2. 证书加进 CurrentUser\TrustedPeople
#   3. signtool 签名
#   4. 卸载旧包 -> 安装新包
#
# 若第 4 步报 0x800B0109 (证书链不受信任): AppX 部署服务以 SYSTEM 身份验签,
# **只认 LocalMachine\TrustedPeople** —— 跑 tools/trust_cert_machine.ps1 (要 UAC)。
#
# 清理: certmgr.msc -> 当前用户 -> 受信任的发布者 / 受信任人, 删 CN=DanqingLog-LocalTest;
#       卸载: 设置 -> 应用 -> 丹青日志 LogLens。
# 本机坑 (承自 pomodoro memory/msix-sideload-workflow.md): 本机 `Cert:` PSDrive 不可用
# (Security 模块加载失败), 故全程走 .NET, 不用 New-SelfSignedCertificate。

param(
    # 留空 = 自动取 release-archives 里最新的那个包 (避免签错版本)
    [string]$MsixPath = "",
    # ⚠️ 必须与 build_msix.ps1 的 -PublisherCN 完全一致
    [string]$PublisherCN = "CN=DanqingLog-LocalTest",
    [string]$AppName = "14uncle.DanqingLog",
    [string]$PfxPassword = "sideload"
)

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path "$PSScriptRoot\.."
$Signtool = Join-Path $RepoRoot "tools\sdk-tools\bin\10.0.22621.0\x64\signtool.exe"
$MsixDir = Join-Path $RepoRoot "..\release-archives\log\msix"
$PfxPath = Join-Path $MsixDir "sideload-signing.pfx"

if (-not (Test-Path $Signtool)) {
    Write-Host "ERROR: 找不到 signtool: $Signtool (见 tools/build_msix.ps1 头部补法)"
    exit 1
}

if ($MsixPath -eq "") {
    $cand = Get-ChildItem -Path $MsixDir -Filter "danqing-log-store-v*-x64.msix" -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending
    if (-not $cand) {
        Write-Host "ERROR: $MsixDir 里没有 msix, 先跑 tools/build_msix.ps1"
        exit 1
    }
    $Msix = $cand[0].FullName
} else {
    $Msix = Join-Path $RepoRoot $MsixPath
}
if (-not (Test-Path $Msix)) { Write-Host "ERROR: 包不存在: $Msix"; exit 1 }
Write-Host "包: $Msix"

Write-Host ""
Write-Host "=== 1/4 自签名证书 (PFX 幂等复用) ==="
# **必须复用既有 PFX**: 每次新建证书会换指纹, 而 LocalMachine\TrustedPeople 里信任的
# 是旧指纹 —— 新签的包会 0x800B0109。换证书必须重跑 trust_cert_machine.ps1。
if (Test-Path $PfxPath) {
    Write-Host "复用已有 PFX (换过证书就必须重跑 trust_cert_machine.ps1)"
    $cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($PfxPath, $PfxPassword)
} else {
    $rsa = [System.Security.Cryptography.RSA]::Create(2048)
    $req = New-Object System.Security.Cryptography.X509Certificates.CertificateRequest(
        $PublisherCN, $rsa,
        [System.Security.Cryptography.HashAlgorithmName]::SHA256,
        [System.Security.Cryptography.RSASignaturePadding]::Pkcs1)
    # EKU: 代码签名 (1.3.6.1.5.5.7.3.3) —— 缺了验签直接失败
    $eku = New-Object System.Security.Cryptography.OidCollection
    [void]$eku.Add("1.3.6.1.5.5.7.3.3")
    $req.CertificateExtensions.Add(
        (New-Object System.Security.Cryptography.X509Certificates.X509EnhancedKeyUsageExtension($eku, $false)))
    $req.CertificateExtensions.Add(
        (New-Object System.Security.Cryptography.X509Certificates.X509BasicConstraintsExtension($false, $false, 0, $true)))
    $cert = $req.CreateSelfSigned(
        [System.DateTimeOffset]::UtcNow.AddDays(-1),
        [System.DateTimeOffset]::UtcNow.AddYears(2))
    [System.IO.File]::WriteAllBytes($PfxPath,
        $cert.Export([System.Security.Cryptography.X509Certificates.X509ContentType]::Pfx, $PfxPassword))
    Write-Host "新建 PFX: $PfxPath"
    Write-Host "首次使用需再跑一次 (要 UAC): tools/trust_cert_machine.ps1"
}
Write-Host "Thumbprint: $($cert.Thumbprint)"
Write-Host "Subject:    $($cert.Subject)"

Write-Host ""
Write-Host "=== 2/4 加入 CurrentUser\TrustedPeople ==="
$store = New-Object System.Security.Cryptography.X509Certificates.X509Store("TrustedPeople", "CurrentUser")
$store.Open("ReadWrite")
$store.Add($cert)
$store.Close()

Write-Host ""
Write-Host "=== 3/4 signtool 签名 ==="
& $Signtool sign /fd SHA256 /f $PfxPath /p $PfxPassword $Msix
if ($LASTEXITCODE -ne 0) { Write-Host "ERROR: signtool 失败"; exit 1 }

Write-Host ""
Write-Host "=== 4/4 卸载旧包 -> 安装 ==="
# 先卸载: 同版本号覆盖安装**可能不换二进制** (pomodoro 实测), 别信
# -ForceUpdateFromAnyVersion 会静默保留旧文件。卸载重装才是可靠的验证回路。
$old = Get-AppxPackage -Name $AppName -ErrorAction SilentlyContinue
if ($old) {
    Write-Host "卸载旧包 $($old.PackageFullName) ..."
    Remove-AppxPackage -Package $old.PackageFullName
} else {
    Write-Host "无旧包, 直接装。"
}
Add-AppxPackage -ForceApplicationShutdown $Msix

$pkg = Get-AppxPackage -Name $AppName
Write-Host ""
Write-Host "=== 完成 ==="
Write-Host ("已安装: {0}" -f $pkg.PackageFullName)
Write-Host ("AUMID:   {0}!App" -f $pkg.PackageFamilyName)
# 用 Start-Process, 不要用 `explorer.exe shell:AppsFolder\...` —— 2026-09-13 实测
# 后者在本机静默不启动 (explorer 启动 App 的返回码恒为 1, 无从判断), 排查白费。
Write-Host ("启动:    powershell -NoProfile -Command `"Start-Process 'shell:AppsFolder\{0}!App'`"" -f $pkg.PackageFamilyName)
