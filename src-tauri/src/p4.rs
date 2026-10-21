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
function Get-LockedFiles([string]$text) {
    @([regex]::Matches($text, '(?im)^rename:\s+failed\s+to\s+rename\s+(.+?)\s+after\s+\d+\s+attempts:') | ForEach-Object {
        $_.Groups[1].Value.Trim()
    } | Select-Object -Unique)
}
function Get-WritableConflictFiles([string]$text) {
    @([regex]::Matches($text, "(?im)^\s*Can't clobber writable file\s+(.+)$") | ForEach-Object {
        $_.Groups[1].Value.Trim()
    } | Select-Object -Unique)
}
function Get-OccupiedFiles([string]$text) {
    @(
        Get-WritableConflictFiles $text
        Get-LockedFiles $text
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -Unique
}
function Get-NonSkippableSyncErrors([string]$text) {
    # Opened files stay local; pending revisions are handled by the resolve stage below.
    # Match known notices exactly so unknown depot errors still stop the run.
    @($text -split "`n" | Where-Object {
        $line = $_.Trim()
        $line -and
            $line -notmatch '(?i)file\(s\) up-to-date' -and
            $line -notmatch "(?i)^Can't clobber writable file\s+" -and
            $line -notmatch '(?i)^rename:\s+failed\s+to\s+rename\s+.+\s+after\s+\d+\s+attempts:' -and
            $line -notmatch "^//.+#\d+ - (?:is opened for add and can't be replaced|is opened and not being changed)$" -and
            $line -notmatch '^\.\.\. //.+ - must resolve #\d+(?:,#\d+)? before submitting$' -and
            $line -notmatch '^(//.+)#\d+ - (?:added as|updating|deleted as|refreshed as|replaced as) '
    })
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
    $files = @(Get-WritableConflictFiles $sync.Text)
    foreach ($file in $files) {
        $root = [IO.Path]::GetFullPath($p.root).TrimEnd('\') + '\'
        $full = [IO.Path]::GetFullPath($file)
        if (-not $full.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw ('Refusing to overwrite a path outside this workspace: ' + $file) }
        [Console]::WriteLine('Force-syncing writable conflict: ' + $file)
        $forced = Invoke-P4 @('sync', '-f', $file)
        if ($forced.Code -ne 0) {
            $forcedLockedFiles = @(Get-OccupiedFiles $forced.Text)
            if ($forcedLockedFiles.Count) {
                foreach ($lockedFile in $forcedLockedFiles) {
                    [Console]::WriteLine('P4 skipped occupied file and continued: ' + $lockedFile)
                }
            }
            $forcedErrors = @(Get-NonSkippableSyncErrors $forced.Text)
            if ($forcedErrors.Count -or [string]::IsNullOrWhiteSpace($forced.Text)) {
                foreach ($syncError in ($forcedErrors | Select-Object -First 5)) { [Console]::WriteLine('P4 blocking sync error: ' + $syncError) }
                throw ('Force sync failed: ' + $file)
            }
        }
    }
    if ($files.Count) { $sync = Invoke-P4 $syncArgs }
    $lockedFiles = @(Get-OccupiedFiles $sync.Text)
    $lockedExecutables = @($lockedFiles | Where-Object { $_ -match '(?i)\.exe$' })
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
                try {
                    if ($process.CloseMainWindow() -and $process.WaitForExit(10000)) {
                        $released = $true
                    } else {
                        [Console]::WriteLine('P4 will skip the occupied file and continue: ' + $full)
                    }
                } catch {
                    [Console]::WriteLine('P4 will skip the occupied file and continue: ' + $full)
                }
            }
        }
    }
    if ($released) {
        [Console]::WriteLine('Retrying P4 sync after the blocking program closed.')
        $sync = Invoke-P4 $syncArgs
    }
    $lockedFiles = @(Get-OccupiedFiles $sync.Text)
    foreach ($file in $lockedFiles) {
        $root = [IO.Path]::GetFullPath($p.root).TrimEnd('\') + '\'
        $full = [IO.Path]::GetFullPath($file)
        if (-not $full.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) { throw ('Refusing to skip a path outside this workspace: ' + $file) }
        [Console]::WriteLine('P4 skipped occupied file and continued: ' + $file)
    }
    $otherErrors = @(Get-NonSkippableSyncErrors $sync.Text)
    if ($sync.Code -ne 0 -and ($otherErrors.Count -or [string]::IsNullOrWhiteSpace($sync.Text))) {
        foreach ($syncError in ($otherErrors | Select-Object -First 5)) { [Console]::WriteLine('P4 blocking sync error: ' + $syncError) }
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
    #[test]
    fn skips_occupied_files_and_continues_to_resolve_without_real_p4() {
        let harness = r#"
$script:calls = New-Object 'System.Collections.Generic.List[string]'
$script:syncCount = 0
function p4.exe {
    if ($args[0] -ne '-p' -or $args[1] -ne 'ssl:fixture:1666' -or $args[4] -ne '-c' -or $args[5] -ne 'fixture') { throw 'Missing explicit P4 scope' }
    $tail = @($args | Select-Object -Skip 6)
    $script:calls.Add(($tail -join ' '))
    $global:LASTEXITCODE = 0
    if ($tail[0] -eq 'clients') { 'Client fixture' }
    elseif ($tail[0] -eq 'sync' -and $tail -notcontains '-n' -and $tail -notcontains '-f') {
        $script:syncCount++
        $global:LASTEXITCODE = 1
        'rename: failed to rename K:\Fixture Project\Table\Buff.xlsx after 10 attempts: The process cannot access the file.'
        "//depot/TriggerAction/Hotfix.ts#1 - is opened for add and can't be replaced"
        '//depot/metadata.json#145 - is opened and not being changed'
        '... //depot/metadata.json - must resolve #144,#145 before submitting'
        '... //depot/utils.ts - must resolve #507 before submitting'
    } elseif ($tail[0] -eq 'resolve' -and $tail[1] -eq '-n') { 'No file(s) to resolve.' }
}
$env:SB_P4_REQUEST = @{ port='ssl:fixture:1666'; user='user'; client='fixture'; root='K:\Fixture Project'; force=$false } | ConvertTo-Json
& ([scriptblock]::Create($env:SB_SCRIPT))
if ($script:syncCount -ne 1) { throw 'Unexpected sync retry for an occupied file' }
if (-not $script:calls.Contains('resolve -n')) { throw 'Resolve stage did not run after occupied file' }
if (@($script:calls | Where-Object { $_ -eq 'resolve -am' }).Count -ne 0) { throw 'Unexpected auto-resolve attempt' }
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
        assert!(String::from_utf8_lossy(&result.stdout)
            .contains("P4 skipped occupied file and continued"));
    }
    #[test]
    fn skips_occupied_files_when_force_sync_cannot_replace_them() {
        let harness = r#"
$script:calls = New-Object 'System.Collections.Generic.List[string]'
$script:syncCount = 0
$script:forceCount = 0
function p4.exe {
    if ($args[0] -ne '-p' -or $args[1] -ne 'ssl:fixture:1666' -or $args[4] -ne '-c' -or $args[5] -ne 'fixture') { throw 'Missing explicit P4 scope' }
    $tail = @($args | Select-Object -Skip 6)
    $script:calls.Add(($tail -join ' '))
    $global:LASTEXITCODE = 0
    if ($tail[0] -eq 'clients') { 'Client fixture' }
    elseif ($tail[0] -eq 'sync' -and $tail[1] -eq '-f') {
        $script:forceCount++
        $global:LASTEXITCODE = 1
        'rename: failed to rename K:\Fixture Project\Table\Buff.xlsx after 10 attempts: The process cannot access the file.'
    } elseif ($tail[0] -eq 'sync' -and $tail -notcontains '-n') {
        $script:syncCount++
        $global:LASTEXITCODE = 1
        "//depot/TriggerAction/Hotfix.ts#1 - is opened for add and can't be replaced"
        '//depot/metadata.json#145 - is opened and not being changed'
        '... //depot/metadata.json - must resolve #144,#145 before submitting'
        if ($script:syncCount -eq 1) { "Can't clobber writable file K:\Fixture Project\Table\Buff.xlsx" }
        else { "Can't clobber writable file K:\Fixture Project\Table\Buff.xlsx" }
    } elseif ($tail[0] -eq 'resolve' -and $tail[1] -eq '-n') { 'No file(s) to resolve.' }
}
$env:SB_P4_REQUEST = @{ port='ssl:fixture:1666'; user='user'; client='fixture'; root='K:\Fixture Project'; force=$false } | ConvertTo-Json
& ([scriptblock]::Create($env:SB_SCRIPT))
if ($script:syncCount -ne 2) { throw 'Sync was not verified after the force-sync conflict' }
if ($script:forceCount -ne 1) { throw 'Missing force-sync attempt' }
if (-not $script:calls.Contains('resolve -n')) { throw 'Resolve stage did not run after force-sync conflict' }
if (@($script:calls | Where-Object { $_ -eq 'resolve -am' }).Count -ne 0) { throw 'Unexpected auto-resolve attempt' }
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
        assert!(String::from_utf8_lossy(&result.stdout)
            .contains("P4 skipped occupied file and continued"));
    }
    #[test]
    fn keeps_real_errors_fatal_alongside_opened_and_occupied_files() {
        let harness = r#"
$ErrorActionPreference = 'Stop'
function p4.exe {
    $tail = @($args | Select-Object -Skip 6)
    $global:LASTEXITCODE = 0
    if ($tail[0] -eq 'clients') { 'Client fixture'; return }
    if ($tail[0] -eq 'resolve') { $script:resolveCalls++; return }
    if ($tail[0] -ne 'sync') { return }
    $global:LASTEXITCODE = 1
    if ($tail[1] -eq '-f') {
        $script:forceCalls++
        'rename: failed to rename K:\Fixture Project\Table\Buff.xlsx after 10 attempts: The process cannot access the file.'
        if ($script:scenario -eq 'force-denied') { 'Access denied to depot file' }
        return
    }
    $script:syncCalls++
    "//depot/TriggerAction/Hotfix.ts#1 - is opened for add and can't be replaced"
    '//depot/metadata.json#145 - is opened and not being changed'
    '... //depot/metadata.json - must resolve #144,#145 before submitting'
    "Can't clobber writable file K:\Fixture Project\Table\Buff.xlsx"
    switch ($script:scenario) {
        'denied' { 'Access denied to depot file' }
        'unknown' { '//depot/unknown#1 - unexpected server failure' }
        'translation' { 'Translation of file content failed near line 1' }
    }
}
$env:SB_P4_REQUEST = @{ port='ssl:fixture:1666'; user='user'; client='fixture'; root='K:\Fixture Project'; force=$false } | ConvertTo-Json
foreach ($case in @('denied', 'force-denied', 'unknown', 'translation')) {
    $script:scenario = $case
    $script:resolveCalls = 0
    $script:syncCalls = 0
    $script:forceCalls = 0
    $failed = $false
    try { & ([scriptblock]::Create($env:SB_SCRIPT)) } catch { $failed = $true }
    if (-not $failed) { throw ('Real sync error was ignored: ' + $case) }
    if ($script:resolveCalls -ne 0) { throw ('Resolve ran after a real sync error: ' + $case) }
    if ($script:forceCalls -ne 1) { throw 'Missing force-sync attempt' }
    if ($case -eq 'force-denied' -and $script:syncCalls -ne 1) { throw 'Force-sync errors must stop immediately' }
}
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
        assert!(String::from_utf8_lossy(&result.stdout).contains("P4 blocking sync error:"));
    }
}
