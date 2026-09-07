import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const source = await fs.readFile(new URL('../src-tauri/src/p4.rs', import.meta.url), 'utf8');
const script = source.match(/pub const SYNC_SCRIPT: &str = r#"([\s\S]*?)"#;/)?.[1];
assert.ok(script);
const harness = String.raw`
$ErrorActionPreference = 'Stop'
$selfSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
function p4.exe {
    $tail = @($args | Select-Object -Skip 6)
    $global:LASTEXITCODE = 0
    if ($tail[0] -eq 'clients') { 'Client fixture' }
    elseif ($tail[0] -eq 'sync') {
        $script:syncCalls++
        if ($script:syncCalls -eq 1) {
            $global:LASTEXITCODE = 1
            $file = if ($script:scenario -eq 'outside') { 'C:\Unrelated\Tool.exe' } else { 'K:\Fixture Project\Tools\Tool.exe' }
            'rename: failed to rename ' + $file + ' after 10 attempts: fixture file is locked'
        }
    } elseif ($tail[0] -eq 'resolve') { 'No file(s) to resolve.' }
}
function Get-CimInstance { [pscustomobject]@{ExecutablePath='K:\Fixture Project\Tools\Tool.exe';ProcessId=123} }
function Invoke-CimMethod { param($InputObject,$MethodName) [pscustomobject]@{Sid=$(if ($script:scenario -eq 'other-user') {'other-user'} else {$selfSid})} }
function Get-Process {
    param($Id,$ErrorAction)
    $p = [pscustomobject]@{Path='K:\Fixture Project\Tools\Tool.exe'}
    $p | Add-Member ScriptMethod CloseMainWindow { $script:closeCalls++; return $script:scenario -ne 'refused' }
    $p | Add-Member ScriptMethod WaitForExit { param($timeout) return $true }
    $p
}
function Stop-Process { throw 'Force-close must never be used by this recovery.' }
$env:SB_P4_REQUEST = @{port='fixture';user='fixture';client='fixture';root='K:\Fixture Project';force=$false} | ConvertTo-Json
foreach ($case in @('success','other-user','outside','refused')) {
    $script:scenario=$case; $script:syncCalls=0; $script:closeCalls=0; $failed=$false
    try { & ([scriptblock]::Create($env:SB_SCRIPT)) } catch { $failed=$true }
    if ($case -eq 'success') {
        if ($failed -or $script:syncCalls -ne 2 -or $script:closeCalls -ne 1) { throw 'Expected one normal close and one retry.' }
    } elseif (-not $failed -or $script:syncCalls -ne 1 -or ($case -ne 'refused' -and $script:closeCalls -ne 0)) { throw ('Unsafe recovery for ' + $case) }
}
'mocked locked-program recovery passed'
`;
const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', harness], {
  env: { ...process.env, SB_SCRIPT: script }, windowsHide: true, encoding: 'utf8', timeout: 20000,
});
assert.equal(result.status, 0, result.stderr || result.error?.message);
console.log(result.stdout.trim());
