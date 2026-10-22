# Builds and zips GlyphClub for Windows. Run from the repo root, on
# Windows (this can't be cross-compiled reliably from macOS — the
# x86_64-pc-windows-* rustup targets exist there but their std/core is
# broken without a real MSVC/mingw toolchain alongside them, so this script
# and the release CI job are the only supported way to produce this build).
#
# No code signing here — there's no Windows code-signing certificate yet,
# so the built .exe is unsigned and Windows SmartScreen will show an
# "unknown publisher" warning on first run until one is added later.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

$Version = if ($env:VERSION) { $env:VERSION } else {
    (Select-String -Path Cargo.toml -Pattern '^version = "(.*)"').Matches[0].Groups[1].Value
}

Write-Host "==> Building release ($Version)"
# Stamps the crate's own version to match — otherwise `env!("CARGO_PKG_VERSION")`
# (what the in-app About overlay, Settings, and the update-checker's version
# comparison all read) stays whatever Cargo.toml last had it at, and every
# release would need a separate manual Cargo.toml edit+commit to keep it in
# sync with the tag actually being built. No-op when $Version already
# matches Cargo.toml (a local run with no $env:VERSION override).
(Get-Content Cargo.toml) -replace '^version = ".*"', "version = `"$Version`"" | Set-Content Cargo.toml
cargo build --release --locked --bin GlyphClub --bin glyphclub-cli
if ($LASTEXITCODE -ne 0) { throw "Release build failed" }

$StageDir = "target/package/windows"
# Deliberately not version-suffixed — the docs site's download button links
# straight to releases/latest/download/GlyphClub-Windows.zip, which only
# resolves if every release publishes an asset under this exact same name.
$ZipName = "GlyphClub-Windows.zip"
Remove-Item -Recurse -Force $StageDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $StageDir | Out-Null
Copy-Item "target/release/GlyphClub.exe" "$StageDir/GlyphClub.exe"
# The bundled free icon font sets, with their licenses (see src/icons.rs).
Copy-Item -Recurse "assets/icon-fonts" "$StageDir/icon-fonts"
# The Illustrator plug-in, one per Illustrator architecture, built first with
# plugin/CMakeLists.txt (needs Adobe's SDK; see the release workflow).
# Settings > Auto Activation installs the matching one (src/integrations.rs).
foreach ($Arch in "x64", "arm64") {
    $Built = "plugin/build-win-$Arch/Release/GlyphClub.aip"
    if (Test-Path $Built) {
        New-Item -ItemType Directory -Force -Path "$StageDir/plugins/$Arch" | Out-Null
        Copy-Item $Built "$StageDir/plugins/$Arch/GlyphClub.aip"
        Write-Host "==> Included the $Arch Illustrator plug-in"
    } elseif ($env:REQUIRE_PLUGIN) {
        throw "Missing $Built (build it with plugin/CMakeLists.txt)"
    } else {
        Write-Warning "No $Arch Illustrator plug-in at $Built; packaging without it"
    }
    $Built = "plugin-photoshop/build-win-$Arch/Release/GlyphClub.8li"
    if (Test-Path $Built) {
        New-Item -ItemType Directory -Force -Path "$StageDir/plugins/$Arch" | Out-Null
        Copy-Item $Built "$StageDir/plugins/$Arch/GlyphClub.8li"
        Write-Host "==> Included the $Arch Photoshop plug-in"
    } elseif ($env:REQUIRE_PLUGIN) {
        throw "Missing $Built (build it with plugin-photoshop/CMakeLists.txt)"
    }
}

Write-Host "==> Zipping"
$ZipPath = "target/package/$ZipName"
Remove-Item -Force $ZipPath -ErrorAction SilentlyContinue
Compress-Archive -Path "$StageDir/*" -DestinationPath $ZipPath

# The installer (packaging/windows/GlyphClub.iss), so people don't have to
# keep the icon fonts and plug-ins next to the .exe themselves. Inno Setup 6
# is preinstalled on GitHub's Windows runners.
$Iscc = @(
    (Get-Command iscc.exe -ErrorAction SilentlyContinue).Source,
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
) | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if ($Iscc) {
    Write-Host "==> Building the installer"
    $Stage = (Resolve-Path $StageDir).Path
    & $Iscc /Q "/DAppVersion=$Version" "/DSourceDir=$Stage" packaging/windows/GlyphClub.iss
    if ($LASTEXITCODE -ne 0) { throw "Building the installer failed" }
    Write-Host "==> Done: target/package/GlyphClub-Setup.exe"
} elseif ($env:REQUIRE_INSTALLER) {
    throw "Inno Setup 6 (ISCC.exe) isn't installed"
} else {
    Write-Warning "Inno Setup 6 isn't installed; skipping the installer"
}

Write-Host "==> Done: $ZipPath"
