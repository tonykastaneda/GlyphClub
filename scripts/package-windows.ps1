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
cargo build --release

$StageDir = "target/package/windows"
$ZipName = "GlyphClub-$Version-Windows.zip"
Remove-Item -Recurse -Force $StageDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $StageDir | Out-Null
Copy-Item "target/release/GlyphClub.exe" "$StageDir/GlyphClub.exe"

Write-Host "==> Zipping"
$ZipPath = "target/package/$ZipName"
Remove-Item -Force $ZipPath -ErrorAction SilentlyContinue
Compress-Archive -Path "$StageDir/*" -DestinationPath $ZipPath

Write-Host "==> Done: $ZipPath"
