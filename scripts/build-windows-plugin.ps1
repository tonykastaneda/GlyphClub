# Builds the Windows Illustrator plug-in for x64 and ARM64 into
# plugin/build-win-<arch>/Release/GlyphClub.aip, and, with $env:PS_SDK set,
# the Photoshop one into plugin-photoshop/build-win-<arch>/Release/GlyphClub.8li,
# where package-windows.ps1 picks them up. Needs Visual Studio 2022 (C++ workload with the ARM64
# tools), Python 3, and Adobe's Illustrator SDK:
#   $env:AI_SDK = "C:\path\to\AdobeIllustratorSDK"; ./scripts/build-windows-plugin.ps1
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

if (-not $env:AI_SDK -or -not (Test-Path "$env:AI_SDK/illustratorapi/illustrator/AITypes.h")) {
    throw "Set AI_SDK to the Illustrator SDK's root (got '$env:AI_SDK')"
}

foreach ($Arch in @(@{ Name = "x64"; Platform = "x64" }, @{ Name = "arm64"; Platform = "ARM64" })) {
    $Build = "plugin/build-win-$($Arch.Name)"
    Write-Host "==> Building the $($Arch.Name) Illustrator plug-in"
    cmake -S plugin -B $Build -A $Arch.Platform "-DAI_SDK=$env:AI_SDK"
    if ($LASTEXITCODE -ne 0) { throw "Configuring the $($Arch.Name) Illustrator plug-in failed" }
    cmake --build $Build --config Release
    if ($LASTEXITCODE -ne 0) { throw "Building the $($Arch.Name) Illustrator plug-in failed" }

    # The Photoshop plug-in (plugin-photoshop/), when its SDK is there too.
    if ($env:PS_SDK) {
        $Build = "plugin-photoshop/build-win-$($Arch.Name)"
        Write-Host "==> Building the $($Arch.Name) Photoshop plug-in"
        cmake -S plugin-photoshop -B $Build -A $Arch.Platform "-DPS_SDK=$env:PS_SDK"
        if ($LASTEXITCODE -ne 0) { throw "Configuring the $($Arch.Name) Photoshop plug-in failed" }
        cmake --build $Build --config Release
        if ($LASTEXITCODE -ne 0) { throw "Building the $($Arch.Name) Photoshop plug-in failed" }
    }
}

# The InDesign plug-in (plugin-indesign/), x64 only (the SDK has no ARM64
# libraries), when its SDK is there: the SDK's own MSBuild setup, so the
# project goes into its build\win\prj, which its property sheets expect.
if ($env:ID_SDK) {
    Write-Host "==> Building the x64 InDesign plug-in"
    $Version = (Get-Content plugin-indesign/VERSION -Raw).Trim()
    $Commas = ($Version.Split(".") + "0") -join ","
    Set-Content plugin-indesign/Source/GCVersion.h @"
// Written by scripts/build-windows-plugin.ps1 from plugin-indesign/VERSION.
#define GLYPHCLUB_ID_VERSION "$Version"
#define GLYPHCLUB_VERSION "$Version"
#define GLYPHCLUB_ID_VERSION_COMMAS $Commas
"@
    $Prj = Join-Path $env:ID_SDK "build/win/prj"
    Copy-Item plugin-indesign/win/GlyphClub.sdk.vcxproj "$Prj/GlyphClub.sdk.vcxproj" -Force
    $Src = (Resolve-Path plugin-indesign/Source).Path
    $Shared = (Resolve-Path plugin/Source).Path
    $Out = Join-Path (Get-Location) "plugin-indesign/build-win-x64/Release/"
    New-Item -ItemType Directory -Force -Path $Out | Out-Null
    $MSBuild = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -requires Microsoft.Component.MSBuild -find "MSBuild\**\Bin\MSBuild.exe" | Select-Object -First 1
    & $MSBuild "$Prj/GlyphClub.sdk.vcxproj" /nologo /m /p:Configuration=Release /p:Platform=x64 "/p:GlyphClubSrc=$Src" "/p:GlyphClubShared=$Shared" "/p:GlyphClubOut=$Out"
    if ($LASTEXITCODE -ne 0) { throw "Building the InDesign plug-in failed" }
}
