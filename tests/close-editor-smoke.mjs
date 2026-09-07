import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const source = await fs.readFile(new URL('../src-tauri/src/plan.rs', import.meta.url), 'utf8');
const script = source.match(/pub const CLOSE_EDITOR: &str = r#"([\s\S]*?)"#;/)?.[1];
assert.ok(script);
const harness = String.raw`
$ErrorActionPreference = 'Stop'
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
function Get-CimInstance {
  param($ClassName,$Filter)
  if ($Filter -like "Name =*") { return [pscustomobject]@{ProcessId=100;ParentProcessId=200;ExecutablePath='K:\Fixture\UnrealEditor.exe';CreationDate='fixture'} }
  if ($Filter -eq 'ProcessId = 200') {
    return [pscustomobject]@{ProcessId=200;Name='LLDBFrontend.exe';ExecutablePath=$(if ($case -eq 'unrelated') {'C:\Unrelated\LLDBFrontend.exe'} else {'C:\Apps\Rider 2\plugins\cidr-debugger-plugin\bin\LLDBFrontend.exe'})}
  }
  if ($Filter -eq 'ProcessId = 100' -and $case -eq 'still-releasing') { return [pscustomobject]@{CreationDate='fixture'} }
}
function Invoke-CimMethod {
  param($InputObject,$MethodName)
  [pscustomobject]@{Sid=$(if (($case -eq 'other-user' -and $InputObject.ProcessId -eq 100) -or ($case -eq 'other-debugger-owner' -and $InputObject.ProcessId -eq 200)) {'other'} else {$sid})}
}
function Get-Process {
  param($Id,$ErrorAction)
  if ($Id -eq 100 -and $case -eq 'already-exited') { return }
  $p = [pscustomobject]@{Id=$Id;Path=$(if ($Id -eq 100) {'K:\Fixture\UnrealEditor.exe'} elseif ($case -eq 'pid-reused') {'C:\Other.exe'} else {'C:\Apps\Rider 2\plugins\cidr-debugger-plugin\bin\LLDBFrontend.exe'})}
  $p | Add-Member ScriptMethod WaitForExit { param($timeout) return $case -ne 'timeout' }
  $p
}
function Stop-Process {
  param($InputObject,[switch]$Force,$ErrorAction)
  if (-not $Force -or $InputObject.Id -notin @(100,200)) { throw 'Unexpected process operation' }
  $script:stopped.Add($InputObject.Id)
}
foreach ($case in @('success','already-exited','other-user','unrelated','other-debugger-owner','pid-reused','timeout','still-releasing')) {
  $script:stopped = [Collections.Generic.List[int]]::new()
  $failed = $false
  try { & ([scriptblock]::Create($env:SB_SCRIPT)) } catch { $failed=$true }
  if ($case -in @('timeout','still-releasing')) { if (-not $failed) { throw ('Expected failure: ' + $case) }; continue }
  if ($failed) { throw ('Unexpected failure: ' + $case) }
  $expected = switch ($case) { 'other-user' {''} 'already-exited' {'200'} 'success' {'200,100'} default {'100'} }
  if (($script:stopped -join ',') -ne $expected) { throw ('Unsafe process selection: ' + $case + ': ' + ($script:stopped -join ',')) }
}
'mocked editor shutdown checks passed'
`;
const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', harness], {
  env: { ...process.env, SB_SCRIPT: script }, windowsHide: true, encoding: 'utf8', timeout: 20000,
});
assert.equal(result.status, 0, result.stderr || result.error?.message);
console.log(result.stdout.trim());
