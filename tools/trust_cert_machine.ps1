# trust_cert_machine.ps1 - 把侧载测试证书 (仅公钥) 加进 LocalMachine\TrustedPeople
#
# 什么时候需要跑: sign_msix_local.ps1 的安装步骤报 **0x800B0109** (证书链不受信任) 时。
# 原因: AppX 部署服务 (Msixvc) 以 SYSTEM 身份验签, **只认本机存储**;
#       sign_msix_local.ps1 写进 CurrentUser\TrustedPeople 的那份它看不见。
#
# 前提: 已跑过 tools/sign_msix_local.ps1 (生成了 release-archives/log/msix/sideload-signing.pfx)
#
# 用法 (触发 UAC 提权, 把下面整行粘进普通 PowerShell 或 cmd):
#   powershell -NoProfile -Command "Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile','-File','F:/github/farm01/danqing-log/tools/trust_cert_machine.ps1'"
#
# 清理: certlm.msc -> 受信任的发布者 / 受信任人, 删 CN=DanqingLog-LocalTest。

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path "$PSScriptRoot\.."
$PfxPath = Join-Path $RepoRoot "..\release-archives\log\msix\sideload-signing.pfx"
# 提权窗口一闪而过看不到结果: 落日志 + 结束前暂停
$LogPath = Join-Path $RepoRoot "..\release-archives\log\msix\trust-cert.log"
Start-Transcript -Path $LogPath -Force | Out-Null

if (-not (Test-Path $PfxPath)) {
    Write-Host "ERROR: $PfxPath 不存在, 先跑 tools/sign_msix_local.ps1"
    Stop-Transcript | Out-Null
    Read-Host "按回车关闭"
    exit 1
}

# 直接入库 PFX 加载出的证书对象 (私钥留在用户密钥容器, 机器存储只持久化证书本身)
$pfx = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($PfxPath, "sideload")

$store = New-Object System.Security.Cryptography.X509Certificates.X509Store("TrustedPeople", "LocalMachine")
$store.Open("ReadWrite")
$store.Add($pfx)
$store.Close()

Write-Host "OK: 已加入 LocalMachine\TrustedPeople"
Write-Host "  Thumbprint: $($pfx.Thumbprint)"
Write-Host "  Subject:    $($pfx.Subject)"
Stop-Transcript | Out-Null
Read-Host "按回车关闭本窗口"
