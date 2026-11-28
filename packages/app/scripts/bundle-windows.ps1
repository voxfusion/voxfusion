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
    # Build portable x86-64 AVX2/FMA/F16C Whisper kernels rather than probing
    # the build host CPU. GitHub's windows-latest runners have AVX-512, which
    # causes whisper.cpp (GGML_NATIVE=ON) to emit ZMM instructions that crash
    # with 0xC000001D STATUS_ILLEGAL_INSTRUCTION on consumer Intel/AMD laptops.
    $env:GGML_NATIVE = "OFF"
    $env:GGML_SSE42 = "ON"
    $env:GGML_AVX = "ON"
    $env:GGML_AVX2 = "ON"
    $env:GGML_FMA = "ON"
    $env:GGML_F16C = "ON"
    $env:GGML_BMI2 = "ON"
    $env:GGML_AVX_VNNI = "OFF"
    $env:GGML_AVX512 = "OFF"
    $env:GGML_AVX512_VBMI = "OFF"
    $env:GGML_AVX512_VNNI = "OFF"
    $env:GGML_AVX512_BF16 = "OFF"

    # whisper-rs-sys's build.rs does not emit cargo:rerun-if-env-changed for
    # GGML_* flags, so drop any stale cached whisper-rs-sys build that still has
    # GGML_NATIVE=ON or GGML_AVX512=ON before compiling.
    $WhisperBuildRoot = Join-Path (Join-Path $BuildDir $Profile) "build"
    if (Test-Path $WhisperBuildRoot) {
        $StaleCaches = Get-ChildItem -Path $WhisperBuildRoot -Directory -Filter "whisper-rs-sys-*" -ErrorAction SilentlyContinue |
            Where-Object {
                $cache = Join-Path $_.FullName "out\build\CMakeCache.txt"
                (Test-Path $cache) -and (Select-String -Path $cache -Pattern "^(GGML_NATIVE|GGML_AVX512|GGML_AVX512_VBMI|GGML_AVX512_VNNI|GGML_AVX512_BF16|GGML_AVX_VNNI):BOOL=ON$" -Quiet)
            }
        if ($StaleCaches) {
            Write-Host "Cleaning stale non-portable whisper-rs-sys cache..."
            $CleanArgs = @("clean", "-p", "whisper-rs-sys")
            if (-not $DebugBuild) { $CleanArgs += "--release" }
            if ($Target) { $CleanArgs += @("--target", $Target) }
            & cargo @CleanArgs
        }
    }

    Write-Host "Building VoxFusion $Version ($Profile) with cargo $($BuildArgs -join ' ')"
    & cargo @BuildArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    # Verify the compiled whisper-rs-sys CMakeCache really disabled host-native
    # and AVX-512 codegen so an unportable binary can never be packaged.
    if (Test-Path $WhisperBuildRoot) {
        $WhisperCaches = Get-ChildItem -Path $WhisperBuildRoot -Directory -Filter "whisper-rs-sys-*" -ErrorAction SilentlyContinue |
            ForEach-Object { Join-Path $_.FullName "out\build\CMakeCache.txt" } |
            Where-Object { Test-Path $_ }
        foreach ($cache in $WhisperCaches) {
            if (Select-String -Path $cache -Pattern "^(GGML_NATIVE|GGML_AVX512|GGML_AVX512_VBMI|GGML_AVX512_VNNI|GGML_AVX512_BF16|GGML_AVX_VNNI):BOOL=ON$" -Quiet) {
                throw "Non-portable GGML CPU flags detected in ${cache}; refusing to package AVX-512/native binary."
            }
        }
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

# Bundle Microsoft's app-local Visual C++ CRT DLLs alongside the executable so
# fresh Windows 10/11 PCs without Visual C++ Redistributable installed can run
# both the installed app and portable ZIP without admin prompts or missing DLLs.
$CrtDirs = @()
$VsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $VsWhere) {
    $VsPath = (& $VsWhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null | Select-Object -First 1)
    if ($VsPath) {
        $RedistRoot = Join-Path $VsPath "VC\Redist\MSVC"
        if (Test-Path $RedistRoot) {
            $CrtDirs += Get-ChildItem -Path $RedistRoot -Directory -ErrorAction SilentlyContinue |
                Sort-Object Name -Descending |
                ForEach-Object { Get-ChildItem -Path (Join-Path $_.FullName "x64") -Directory -Filter "Microsoft.VC*.CRT" -ErrorAction SilentlyContinue } |
                Select-Object -ExpandProperty FullName
        }
    }
}
$CrtDirs += "$env:SystemRoot\System32"

$RequiredCrtDlls = @("vcruntime140.dll", "vcruntime140_1.dll", "msvcp140.dll")
$OptionalCrtDlls = @("msvcp140_1.dll", "msvcp140_2.dll", "concrt140.dll")
foreach ($dll in ($RequiredCrtDlls + $OptionalCrtDlls)) {
    $src = $CrtDirs | ForEach-Object { Join-Path $_ $dll } | Where-Object { Test-Path $_ } | Select-Object -First 1
    if ($src) {
        Copy-Item -Force $src (Join-Path $StageDir $dll)
        Write-Host "Bundled CRT DLL: $src"
    } elseif ($RequiredCrtDlls -contains $dll) {
        throw "Required Visual C++ runtime DLL '$dll' not found in: $($CrtDirs -join '; ')"
    }
}

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
