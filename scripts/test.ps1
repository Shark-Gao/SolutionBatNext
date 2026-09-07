. (Join-Path $PSScriptRoot 'environment.ps1')
npm.cmd run build
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
npm.cmd test
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --locked --manifest-path src-tauri/Cargo.toml
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
npm.cmd run test:ui
exit $LASTEXITCODE
