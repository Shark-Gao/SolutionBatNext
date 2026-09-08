import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const source = process.env.SB_TEST_EXE || path.join(root, 'release', 'MHAutoUpdateCompilerNext.exe');
const data = path.join(root, 'test-results', `monitor-lock-${Date.now()}`);
await fs.mkdir(data, { recursive: true });
const exe = path.join(data, 'MHAutoUpdateCompilerNext.exe');
await fs.copyFile(source, exe);
const env = { ...process.env, SOLUTIONBAT_DATA_DIR: data, SOLUTIONBAT_DISABLE_TELEMETRY: '1' };
const script = String.raw`
$ErrorActionPreference='Stop'
[Console]::WriteLine('READY')
$timer=[Diagnostics.Stopwatch]::StartNew()
do {
  $file=Get-ChildItem -LiteralPath (Join-Path $env:SOLUTIONBAT_DATA_DIR 'live-runs') -Filter '*.json' -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $file) { Start-Sleep -Milliseconds 10 }
} while (-not $file -and $timer.Elapsed.TotalSeconds -lt 15)
if (-not $file) { throw 'No fixture snapshot' }
$stream=[IO.File]::Open($file.FullName,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::ReadWrite)
try { [Console]::WriteLine('LOCKED'); Start-Sleep -Milliseconds 2100 } finally { $stream.Dispose() }
`;
const locker = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
let lockOutput = '', lockError = '';
locker.stderr.on('data', data => { lockError += data; });
const lockDone = new Promise(resolve => locker.once('exit', resolve));
await new Promise((resolve, reject) => {
  locker.once('error', reject);
  locker.stdout.on('data', data => { lockOutput += data; if (lockOutput.includes('READY')) resolve(); });
});
const worker = spawn(exe, ['--run-task', 'MHA_Client_main', '--dry-run', '--show-gui', '--data-dir', data], { env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
worker.stdout.resume(); worker.stderr.resume();
const outcome = await new Promise((resolve, reject) => { worker.once('error', reject); worker.once('exit', resolve); });
worker.stdout.destroy(); worker.stderr.destroy(); worker.unref();
assert.equal(await lockDone, 0, lockError);
assert.match(lockOutput, /LOCKED/);
assert.equal(outcome, 0, 'a temporary snapshot lock must not fail the build');
const file = (await fs.readdir(path.join(data, 'live-runs'))).find(name => name.endsWith('.json'));
const snapshot = JSON.parse(await fs.readFile(path.join(data, 'live-runs', file), 'utf8'));
if (process.argv.includes('--expect-stale')) {
  assert.equal(snapshot.run.status, 'running', 'the old implementation reproduces a stalled snapshot');
} else {
  assert.equal(snapshot.run.status, 'success', 'snapshot recovers after a temporary sharing lock');
  assert.ok(snapshot.events.some(e => e.message.includes('任务结束')));
}
console.log(JSON.stringify({ passed: true, expectedOldBug: process.argv.includes('--expect-stale'), status: snapshot.run.status, data, exe, workerExited: true }));
