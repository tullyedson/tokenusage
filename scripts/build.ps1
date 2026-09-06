param([switch]$SkipTests)
$ErrorActionPreference = 'Stop'
$taskProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $taskProjectRoot
$taskTestTemp = Join-Path $taskProjectRoot 'artifacts\test-temp'
New-Item -ItemType Directory -Path $taskTestTemp -Force | Out-Null
$env:TEMP = $taskTestTemp
$env:TMP = $taskTestTemp
$taskCargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path -LiteralPath $taskCargoBin) { $env:PATH = $taskCargoBin + ';' + $env:PATH }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Install Rust with the MSVC Windows toolchain first.' }
if (-not (Test-Path -LiteralPath (Join-Path $taskProjectRoot 'node_modules'))) {
    npm.cmd ci
    if ($LASTEXITCODE -ne 0) { throw 'Dependency installation failed.' }
}
if (-not $SkipTests) {
    npm.cmd test
    if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed.' }
    cargo test --manifest-path src-tauri/Cargo.toml
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Rust Clippy checks failed.' }
}
npm.cmd run installer
if ($LASTEXITCODE -ne 0) { throw 'Installer build failed.' }
Write-Output 'Installer built in src-tauri\target\release\bundle\nsis.'
