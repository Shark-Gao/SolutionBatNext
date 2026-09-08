param([switch]$Debug, [string]$UpdaterConfig)
. (Join-Path $PSScriptRoot 'environment.ps1')
& (Join-Path $PSScriptRoot 'sync-version.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Version synchronization failed.' }
if ($Debug) { npm.cmd run tauri -- build --debug --no-bundle }
elseif ($UpdaterConfig) { npm.cmd run tauri -- build --config $UpdaterConfig }
else { npm.cmd run desktop:build }
if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }
if (-not $Debug) {
    $releasePath = Join-Path $projectRoot 'release'
    New-Item -ItemType Directory -Force -Path $releasePath | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'src-tauri\target\release\MHAutoUpdateCompilerNext.exe') -Destination (Join-Path $releasePath 'MHAutoUpdateCompilerNext.exe') -Force
    Get-ChildItem -LiteralPath (Join-Path $projectRoot 'src-tauri\target\release\bundle\nsis') -Filter '*-setup.exe' | Copy-Item -Destination $releasePath -Force
    Get-ChildItem -LiteralPath $releasePath -File | Select-Object Name,Length
}
