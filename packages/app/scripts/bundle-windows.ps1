<#
.SYNOPSIS
Builds VoxFusion for Windows and packages the portable ZIP and/or Inno Setup installer.

.DESCRIPTION
Mirrors packages/app/scripts/bundle-macos.sh for Windows builds.

Usage:
  pwsh packages/app/scripts/bundle-windows.ps1 [-Target <rust target>] [-DebugBuild]
                                               [-Installer] [-Zip] [-Version <semver>]

Output: packages/app/dist/<target>/
  VoxFusion/voxfusion-app.exe
  VoxFusion-windows-x64.zip        (with -Zip)
  VoxFusion-windows-x64-setup.exe  (with -Installer)
#>
[CmdletBinding()]
param(
    [string]$Target = "",
    [switch]$DebugBuild,
    [switch]$Installer,
    [switch]$Zip,
    [string]$Version = ""
)

$ErrorActionPreference = "Stop"

$AppDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$RepoRoot = (Resolve-Path (Join-Path $AppDir "..\..")).Path
$CargoToml = Join-Path $AppDir "Cargo.toml"

if (-not $Version) {
    $match = Select-String -Path $CargoToml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if (-not $match) {
        throw "Could not read version from $CargoToml"
    }
    $Version = $match.Matches[0].Groups[1].Value
}

$Profile = if ($DebugBuild) { "debug" } else { "release" }
$BuildArgs = @("build")
$BuildDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $AppDir "target" }

if ($Target) {
    $BuildArgs += @("--target", $Target)
    $BuildDir = Join-Path $BuildDir $Target
    $OutTarget = $Target
} else {
    $OutTarget = "x86_64-pc-windows-msvc"
}

if (-not $DebugBuild) {
    $BuildArgs += "--release"
}

$OutDir = Join-Path $AppDir (Join-Path "dist" $OutTarget)
$StageDir = Join-Path $OutDir "VoxFusion"

Push-Location $AppDir
try {
    $env:VOXFUSION_REQUIRE_WINDOWS_ICON = "1"
    Write-Host "Building VoxFusion $Version ($Profile) with cargo $($BuildArgs -join ' ')"
    & cargo @BuildArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}

$BuiltExe = Join-Path (Join-Path $BuildDir $Profile) "voxfusion-app.exe"
if (-not (Test-Path $BuiltExe)) {
    throw "Expected binary not found at $BuiltExe"
}

if (Test-Path $StageDir) {
    Remove-Item -Recurse -Force $StageDir
}
New-Item -ItemType Directory -Force -Path $StageDir | Out-Null

$StagedExe = Join-Path $StageDir "voxfusion-app.exe"
Copy-Item -Force $BuiltExe $StagedExe

$LicenseFile = Join-Path $RepoRoot "LICENSE"
if (Test-Path $LicenseFile) {
    Copy-Item -Force $LicenseFile (Join-Path $StageDir "LICENSE.txt")
}

$VerInfo = (Get-Item $StagedExe).VersionInfo
Write-Host "Embedded version info: ProductName='$($VerInfo.ProductName)' FileVersion='$($VerInfo.FileVersion)'"
if ($VerInfo.ProductName -ne "VoxFusion") {
    throw "Windows resource metadata missing from $StagedExe (ProductName='$($VerInfo.ProductName)')"
}

if ($Zip) {
    $ZipPath = Join-Path $OutDir "VoxFusion-windows-x64.zip"
    if (Test-Path $ZipPath) {
        Remove-Item -Force $ZipPath
    }
    Compress-Archive -Path $StageDir -DestinationPath $ZipPath -CompressionLevel Optimal
    Write-Host "Built $ZipPath"
}

if ($Installer) {
    $IsccCandidates = @(
        (Get-Command iscc -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
    ) | Where-Object { $_ -and (Test-Path $_) }

    $Iscc = $IsccCandidates | Select-Object -First 1
    if (-not $Iscc) {
        throw "Inno Setup compiler (ISCC.exe) not found. Install Inno Setup 6 or add ISCC.exe to PATH."
    }

    $IssScript = Join-Path $AppDir "windows\installer.iss"
    $SetupBase = "VoxFusion-windows-x64-setup"
    $SetupPath = Join-Path $OutDir "$SetupBase.exe"
    if (Test-Path $SetupPath) {
        Remove-Item -Force $SetupPath
    }

    & $Iscc `
        "/DAppVersion=$Version" `
        "/DSourceDir=$StageDir" `
        "/DOutputDir=$OutDir" `
        "/DOutputBaseFilename=$SetupBase" `
        $IssScript
    if ($LASTEXITCODE -ne 0) {
        throw "ISCC.exe failed with exit code $LASTEXITCODE"
    }
    if (-not (Test-Path $SetupPath)) {
        throw "Expected installer not found at $SetupPath"
    }
    Write-Host "Built $SetupPath"
}

Write-Host "Built $StageDir"
