# package_portable.ps1 - portable Windows zip for danqing-log release.
#
# Stages .cargo-target/release/danqing-log.exe (shared farm target,
# see .cargo/config.toml) (+ any *.dll), assets/, README and any LICENSE*
# files into a staging dir, then zips into
# ../release-archives/clipboard/danqing-log-v<version>-win-x64.zip with SHA256.
#
# Usage (from repo root):
#   powershell -NoProfile -File tools/package_portable.ps1
#   powershell -NoProfile -File tools/package_portable.ps1 -Version 1.0.0
#
# Always runs cargo build --release before packaging.

param(
    [string]$BinaryName = "danqing-log",
    [string]$Version = "",
    [string]$OutDir = "..\release-archives\log",
    [string]$IcoPath = "assets\logo.ico"
)

$ErrorActionPreference = "Stop"
$RepoRoot = Resolve-Path "$PSScriptRoot\.."

# Default $Version from Cargo.toml's [package].version when -Version not passed.
if (-not $PSBoundParameters.ContainsKey('Version')) {
    $CargoToml = Join-Path $RepoRoot "Cargo.toml"
    if (Test-Path $CargoToml) {
        $Match = [regex]::Match(
            (Get-Content $CargoToml -Raw),
            '(?s)\[package\][^[]*?version\s*=\s*"([^"]+)"'
        )
        if ($Match.Success) {
            $Version = $Match.Groups[1].Value
            Write-Host ("Version from Cargo.toml: {0}" -f $Version)
        } else {
            Write-Host "ERROR: -Version not provided and Cargo.toml [package] section has no version"
            exit 1
        }
    } else {
        Write-Host "ERROR: -Version not provided and Cargo.toml missing"
        exit 1
    }
}

$ReleaseDir = Join-Path $RepoRoot "target\release"  # 本仓独立 target (shared target 已废除 2026-09-10)
$Stage = Join-Path $OutDir "stage"
$ArchiveBase = "${BinaryName}-v${Version}-win-x64"
$ZipPath = Join-Path $OutDir "${ArchiveBase}.zip"

# Always build release before packaging (ensure binary is up-to-date).
Write-Host "Building release ..."
Push-Location $RepoRoot
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { exit 1 }
} finally {
    Pop-Location
}

# Locate binary.
$BinaryPath = Join-Path $ReleaseDir "${BinaryName}.exe"

if (-not (Test-Path $BinaryPath)) {
    Write-Host "ERROR: binary not found after build: $BinaryPath"
    exit 1
}

Write-Host ("Using binary: {0}" -f $BinaryPath)

# Clean + create stage dir.
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Path $Stage -Force | Out-Null

# Main exe.
Copy-Item $BinaryPath $Stage

# Runtime DLLs in release root (if any).
$Dlls = @(Get-ChildItem -Path $ReleaseDir -Filter "*.dll" -File -ErrorAction SilentlyContinue)
if ($Dlls.Count -gt 0) {
    foreach ($d in $Dlls) { Copy-Item $d.FullName $Stage }
    Write-Host ("Copied {0} runtime DLL(s)" -f $Dlls.Count)
} else {
    Write-Host "No runtime DLLs in shared release dir (statically linked or system-provided)."
}

# assets/.
$AssetsDir = Join-Path $RepoRoot "assets"
if (Test-Path $AssetsDir) {
    Copy-Item -Recurse $AssetsDir (Join-Path $Stage "assets")
    Write-Host "Copied assets/"
} else {
    Write-Warning "No assets/ directory at repo root."
}

# LICENSE* files (warn if missing).
$LicenseNames = @("LICENSE", "LICENSE.md", "LICENSE.txt",
                  "LICENSE-APACHE", "LICENSE-APACHE.txt",
                  "LICENSE-MIT", "LICENSE-MIT.txt")
$LicenseFound = $false
foreach ($name in $LicenseNames) {
    $p = Join-Path $RepoRoot $name
    if (Test-Path $p) {
        Copy-Item $p $Stage
        $LicenseFound = $true
    }
}
if (-not $LicenseFound) {
    Write-Warning "No LICENSE* file at repo root."
}

# README.
$ReadmePath = Join-Path $RepoRoot "README.md"
if (Test-Path $ReadmePath) {
    Copy-Item $ReadmePath $Stage
}

# Create zip (omit -CompressionLevel for PS 5.1 compat; default = Optimal).
if (Test-Path $ZipPath) { Remove-Item $ZipPath -Force }
Compress-Archive -Path "$Stage\*" -DestinationPath $ZipPath

# SHA256 sidecar via .NET (avoids Get-FileHash cmdlet dependency).
$Bytes = [System.IO.File]::ReadAllBytes($ZipPath)
$Sha256 = [System.Security.Cryptography.SHA256]::Create()
$Hash = ([System.BitConverter]::ToString($Sha256.ComputeHash($Bytes)) -replace '-', '').ToLower()
$Sha256.Dispose()
[System.IO.File]::WriteAllText("$ZipPath.sha256", $Hash, [System.Text.Encoding]::ASCII)

Write-Host ""
Write-Host ("Built:  {0}" -f $ZipPath)
Write-Host ("SHA256: {0}" -f $Hash)
Write-Host ("Size:   {0:N0} bytes ({1:N1} KB)" -f (Get-Item $ZipPath).Length, ((Get-Item $ZipPath).Length / 1KB))
