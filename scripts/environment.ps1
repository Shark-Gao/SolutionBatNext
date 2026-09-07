$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$localToolchain = Join-Path $projectRoot '.toolchain'
if (Test-Path -LiteralPath (Join-Path $localToolchain 'cargo\bin\cargo.exe')) {
    $env:CARGO_HOME = Join-Path $localToolchain 'cargo'
    $env:RUSTUP_HOME = Join-Path $localToolchain 'rustup'
    $env:Path = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:Path
}
Set-Location -LiteralPath $projectRoot
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'Rust is missing. Run scripts\setup.ps1 first.' }
if (-not (Get-Command npm.cmd -ErrorAction SilentlyContinue)) { throw 'Node.js 22.12+ is required.' }
