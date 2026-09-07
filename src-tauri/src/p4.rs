pub const SYNC_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$p = ConvertFrom-Json $env:SB_P4_REQUEST
$base = @('-p', $p.port, '-u', $p.user, '-c', $p.client)
function Invoke-P4([string[]]$arguments) {
    $lines = New-Object 'System.Collections.Generic.List[string]'
    $oldPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & p4.exe @base @arguments 2>&1 | ForEach-Object { $line = $_.ToString(); $lines.Add($line); [Console]::WriteLine($line) }
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $oldPreference }
    return [pscustomobject]@{ Code=$code; Text=($lines -join "`n") }
}
$clients = Invoke-P4 @('clients', '-e', $p.client)
if ($clients.Code -ne 0 -or [string]::IsNullOrWhiteSpace($clients.Text)) { throw ('P4 Client does not exist: ' + $p.client) }
[Console]::WriteLine('Checking opened files...')
$opened = Invoke-P4 @('opened')
$syncArgs = @('sync')
if ($p.force) { $syncArgs += '-f' }
$syncArgs += ('//' + $p.client + '/...#head')
$sync = Invoke-P4 $syncArgs
if ($sync.Code -ne 0) {
    $files = @([regex]::Matches($sync.Text, "(?m)Can't clobber writable file\s+(.+)$") | ForEach-Object { $_.Groups[1].Value.Trim() } | Select-Object -Unique)
    foreach ($file in $files) {
        $root = [IO.Path]::GetFullPath($p.root).TrimEnd('\') + '\'
        $full = [IO.Path]::GetFullPath($file)
        if (-not $full.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw ('Refusing to overwrite a path outside this workspace: ' + $file) }
        [Console]::WriteLine('Force-syncing writable conflict: ' + $file)
        $forced = Invoke-P4 @('sync', '-f', $file)
        if ($forced.Code -ne 0) { throw ('Force sync failed: ' + $file) }
    }
    if ($files.Count) { $sync = Invoke-P4 $syncArgs }
    $lockedExecutables = @([regex]::Matches($sync.Text, '(?m)rename: failed to rename (.+\.exe) after \d+ attempts:') | ForEach-Object { $_.Groups[1].Value.Trim() } | Select-Object -Unique)
    $released = $false
    foreach ($file in $lockedExecutables) {
        $root = [IO.Path]::GetFullPath($p.root).TrimEnd('\') + '\'
        $full = [IO.Path]::GetFullPath($file)
        if (-not $full.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw ('Refusing to close a program outside this workspace: ' + $file) }
        $ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        $holders = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $full -and (Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid).Sid -eq $ownerSid })
        foreach ($holder in $holders) {
            $process = Get-Process -Id $holder.ProcessId -ErrorAction SilentlyContinue
            if ($process -and $process.Path -eq $full) {
                [Console]::WriteLine('Requesting normal close of a program blocking P4 sync: ' + $full)
                if (-not $process.CloseMainWindow() -or -not $process.WaitForExit(10000)) { throw ('Program is still in use. Save and close it before retrying: ' + $full) }
                $released = $true
            }
        }
    }
    if ($released) {
        [Console]::WriteLine('Retrying P4 sync after the blocking program closed.')
        $sync = Invoke-P4 $syncArgs
    }
    $otherErrors = @($sync.Text -split "`n" | Where-Object { $_.Trim() -and $_ -notmatch '(?i)file\(s\) up-to-date' })
    if ($sync.Code -ne 0 -and ($otherErrors.Count -or [string]::IsNullOrWhiteSpace($sync.Text))) {
        if ($sync.Text -match 'Translation of file content failed') { [Console]::WriteLine('Check the Perforce file type: binary data may be marked as text.') }
        throw ('P4 sync failed, exit code ' + $sync.Code)
    }
}
for ($attempt = 1; $attempt -le 3; $attempt++) {
    $pending = Invoke-P4 @('resolve', '-n')
    if ([string]::IsNullOrWhiteSpace($pending.Text) -or $pending.Text -match '(?i)no file\(s\) to resolve') { break }
    if ($pending.Code -ne 0) { throw 'Unable to check pending P4 resolves' }
    [Console]::WriteLine(('Auto-resolve attempt {0}/3' -f $attempt))
    $resolved = Invoke-P4 @('resolve', '-am')
    if ($attempt -eq 3) { [Console]::WriteLine('Reached the auto-resolve limit. Remaining conflicts require manual resolution.') }
}
"#;

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::process::Command;
    #[test]
    fn refuses_outside_paths_and_does_not_mask_sync_errors() {
        let harness = r#"
$ErrorActionPreference = 'Stop'
$script:forceCalls = 0
function p4.exe {
    $tail = @($args | Select-Object -Skip 6)
    $global:LASTEXITCODE = 0
    if ($tail[0] -eq 'clients') { 'Client fixture' }
    elseif ($tail[0] -eq 'sync') {
        if ($tail[1] -eq '-f') { $script:forceCalls++; return }
        $global:LASTEXITCODE = 1
        if ($script:scenario -eq 'outside') { "Can't clobber writable file C:\Unrelated\important.txt" }
        else { 'file(s) up-to-date.'; 'Access denied to depot file' }
    }
}
$env:SB_P4_REQUEST = @{ port='ssl:fixture:1666'; user='user'; client='fixture'; root='K:\Fixture Project'; force=$false } | ConvertTo-Json
foreach ($case in @('outside', 'denied')) {
    $script:scenario = $case
    $failed = $false
    try { & ([scriptblock]::Create($env:SB_SCRIPT)) } catch { $failed = $true }
    if (-not $failed) { throw ('Unsafe recovery accepted: ' + $case) }
}
if ($script:forceCalls -ne 0) { throw 'Overwrote an unrelated path' }
"#;
        let result = crate::runner::hidden(
            Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", harness])
                .env("SB_SCRIPT", SYNC_SCRIPT),
        )
        .output()
        .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[test]
    fn retries_writable_conflicts_and_bounds_auto_merge_without_real_p4() {
        let harness = r#"
$script:calls = New-Object 'System.Collections.Generic.List[string]'
$script:syncCount = 0
function p4.exe {
    if ($args[0] -ne '-p' -or $args[1] -ne 'ssl:fixture:1666' -or $args[4] -ne '-c' -or $args[5] -ne 'fixture') { throw 'Missing explicit P4 scope' }
    $tail = @($args | Select-Object -Skip 6)
    $script:calls.Add(($tail -join ' '))
    $global:LASTEXITCODE = 0
    if ($tail[0] -eq 'clients') { 'Client fixture' }
    elseif ($tail[0] -eq 'sync' -and $tail[1] -ne '-f') {
        $script:syncCount++
        if ($script:syncCount -eq 1) { $global:LASTEXITCODE = 1; "Can't clobber writable file K:\Fixture Project\file.ts" }
    } elseif ($tail[0] -eq 'resolve' -and $tail[1] -eq '-n') { 'fixture needs resolve' }
}
$env:SB_P4_REQUEST = @{ port='ssl:fixture:1666'; user='user'; client='fixture'; root='K:\Fixture Project'; force=$false } | ConvertTo-Json
& ([scriptblock]::Create($env:SB_SCRIPT))
if (-not $script:calls.Contains('sync -f K:\Fixture Project\file.ts')) { throw 'Missing conflict retry' }
if (@($script:calls | Where-Object { $_ -eq 'resolve -am' }).Count -ne 3) { throw 'Wrong resolve limit' }
if ($script:syncCount -ne 2) { throw 'Sync was not verified after recovery' }
"#;
        let result = crate::runner::hidden(
            Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", harness])
                .env("SB_SCRIPT", SYNC_SCRIPT),
        )
        .output()
        .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
