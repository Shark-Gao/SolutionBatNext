$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$localToolchain = Join-Path $projectRoot '.toolchain'
if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -and -not (Test-Path -LiteralPath (Join-Path $localToolchain 'cargo\bin\cargo.exe'))) {
    New-Item -ItemType Directory -Force -Path $localToolchain | Out-Null
    $env:CARGO_HOME = Join-Path $localToolchain 'cargo'
    $env:RUSTUP_HOME = Join-Path $localToolchain 'rustup'
    $installerPath = Join-Path $localToolchain 'rustup-init.exe'
    Invoke-WebRequest -Uri 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile $installerPath
    & $installerPath -y --no-modify-path --profile minimal --default-toolchain stable
    if ($LASTEXITCODE -ne 0) { throw 'Rust installation failed.' }
}
. (Join-Path $PSScriptRoot 'environment.ps1')
npm.cmd ci
if ($LASTEXITCODE -ne 0) { throw 'Frontend dependencies failed to install.' }
cargo fetch --locked --manifest-path src-tauri/Cargo.toml
if ($LASTEXITCODE -ne 0) { throw 'Rust dependencies failed to download.' }
