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
