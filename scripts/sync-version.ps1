param([switch]$Check)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$packagePath = Join-Path $projectRoot 'package.json'
$package = Get-Content -LiteralPath $packagePath -Raw | ConvertFrom-Json
$version = [string]$package.version

if ($version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
    throw "package.json version is not valid semver: $version"
}

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Sync-VersionText([string]$path, [string]$pattern) {
    $text = [IO.File]::ReadAllText($path)
    $regex = [regex]::new($pattern)
    if (-not $regex.IsMatch($text)) {
        throw "Could not find the version field in $path"
    }

    $updated = $regex.Replace($text, [Text.RegularExpressions.MatchEvaluator]{
        param($match)
        $match.Groups[1].Value + $version + $match.Groups[2].Value
    }, 1)

    if ($Check) {
        if ($updated -ne $text) {
            throw "Version metadata is out of sync: $path"
        }
    } elseif ($updated -ne $text) {
        [IO.File]::WriteAllText($path, $updated, $utf8NoBom)
    }
}

Sync-VersionText (Join-Path $projectRoot 'package-lock.json') '(?m)^([ ]{2}"version"[ \t]*:[ \t]*")[^"]+(")'
Sync-VersionText (Join-Path $projectRoot 'package-lock.json') '(?m)^([ ]{6}"version"[ \t]*:[ \t]*")[^"]+(")'
Sync-VersionText (Join-Path $projectRoot 'src-tauri/Cargo.toml') '(?m)^(version[ \t]*=[ \t]*")[^"]+(")'
Sync-VersionText (Join-Path $projectRoot 'src-tauri/Cargo.lock') '(?s)(\[\[package\]\]\r?\nname = "solution-bat-next"\r?\nversion = ")[^"]+(")'

Write-Output "Version synchronized: $version"
