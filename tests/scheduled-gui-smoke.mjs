import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const source = process.env.SB_TEST_EXE || path.join(root, 'release', 'MHAutoUpdateCompilerNext.exe');
const dir = path.join(root, 'test-results', `scheduled-gui-${Date.now()}`);
await fs.mkdir(dir, { recursive: true });
const exe = path.join(dir, 'MHAutoUpdateCompilerNext.exe');
await fs.copyFile(source, exe);
const results = [];
for (const cancel of [false, true]) {
  const data = path.join(dir, cancel ? 'cancel' : 'success');
  const env = { ...process.env, SOLUTIONBAT_DATA_DIR: data, SOLUTIONBAT_DISABLE_TELEMETRY: '1' };
  const child = spawn(exe, ['--run-task', 'MHA_Client_main', '--dry-run', '--show-gui', '--data-dir', data], { env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  let stderr = '';
  child.stderr.on('data', data => { stderr += data; });
  child.stdout.resume();
  const done = new Promise((resolve, reject) => { child.once('error', reject); child.once('exit', (code, signal) => resolve({ code, signal })); });
  let snapshotFile;
  const started = Date.now();
  while (!snapshotFile && Date.now() - started < 15000) {
    try { snapshotFile = (await fs.readdir(path.join(data, 'live-runs'))).find(name => name.endsWith('.json')); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (!snapshotFile) await new Promise(resolve => setTimeout(resolve, 15));
  }
  assert.ok(snapshotFile, 'worker publishes state before opening GUI');
  const runId = snapshotFile.slice(0, -5);
  if (cancel) await fs.writeFile(path.join(data, 'live-runs', `${runId}.cancel`), 'cancel');
  const outcome = await done;
  child.stdout.destroy(); child.stderr.destroy(); child.unref();
  assert.equal(outcome.code, cancel ? 1 : 0, stderr);
  const snapshot = JSON.parse(await fs.readFile(path.join(data, 'live-runs', snapshotFile), 'utf8'));
  const history = JSON.parse(await fs.readFile(path.join(data, 'history', `${runId}.json`), 'utf8'));
  assert.equal(history.dryRun, true);
  assert.equal(history.scheduled, true);
  assert.equal(history.status, cancel ? 'cancelled' : 'success');
  assert.equal(snapshot.run.status, history.status);
  assert.ok(snapshot.events.some(e => e.message.includes('任务结束')));
  assert.ok(history.steps.some(s => s.id === 'close-rider'));
  assert.ok(history.steps.some(s => s.id === 'ue-build'));
  assert.ok(history.steps.some(s => s.id === 'ts-build'));
  assert.equal(snapshot.workerPid, child.pid);
  assert.ok(snapshot.events.length <= 1200);
  results.push({ cancel, runId, data, workerExited: true, status: history.status });
}
console.log(JSON.stringify({ passed: true, exe, results, note: 'Isolated GUI windows remain open for visual verification; no real build commands ran.' }));
