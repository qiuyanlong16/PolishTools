# Build portable (green) package
# Usage: powershell -ExecutionPolicy Bypass -File build.ps1

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $Root

$cargo = "cargo"
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $fallback = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
    if (Test-Path $fallback) { $cargo = $fallback } else { throw "cargo not found. Install Rust first: https://rustup.rs" }
}

Write-Host "==> Rust toolchain" -ForegroundColor Cyan
& $cargo --version

Write-Host "==> Building release" -ForegroundColor Cyan
& $cargo build --release

$exe = Join-Path $Root "target\release\polish-tools.exe"
if (-not (Test-Path $exe)) { throw "Build output missing: $exe" }

$dist = Join-Path $Root "dist\PolishTools"
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory -Path $dist | Out-Null

Copy-Item $exe (Join-Path $dist "PolishTools.exe")
Copy-Item (Join-Path $Root "README.md") $dist -ErrorAction SilentlyContinue

# default config without any secret
$config = @{
    api_base          = "https://api.deepseek.com"
    api_key           = ""
    model             = "deepseek-chat"
    temperature       = 0.2
    max_chars         = 6000
    restore_clipboard = $true
    paused            = $false
    autostart         = $false
    hotkey            = @{ repeat = 3; interval_ms = 400; swallow = $true }
} | ConvertTo-Json -Depth 4
[System.IO.File]::WriteAllText((Join-Path $dist "config.json"), $config, (New-Object System.Text.UTF8Encoding($false)))

Write-Host "==> Packaging zip" -ForegroundColor Cyan
$zip = Join-Path $Root "dist\PolishTools-portable.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $dist "*") -DestinationPath $zip

Write-Host ""
Write-Host "Done." -ForegroundColor Green
Write-Host "  Portable folder: $dist"
Write-Host "  Zip package    : $zip"
Write-Host "  Fill in your DeepSeek API Key on first run."