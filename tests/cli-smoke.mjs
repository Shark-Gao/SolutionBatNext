import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const exe = process.env.SB_TEST_EXE || path.join(root, 'release', 'MHAutoUpdateCompilerNext.exe');
const dataDir = path.join(root, 'test-results', `cli-${Date.now()}`);
await fs.mkdir(dataDir, { recursive: true });
const testEnv = { ...process.env, SOLUTIONBAT_DATA_DIR: dataDir, SOLUTIONBAT_DISABLE_TELEMETRY: '1' };
for (const name of ['MHA_Client_main', 'MHA_Client_stable', 'MHAClient_battle', 'MHAClient_PrePublic']) {
  const result = spawnSync(exe, ['--run-task', name, '--dry-run', '--data-dir', dataDir], { env: testEnv, windowsHide: true, timeout: 20000, encoding: 'utf8' });
  assert.equal(result.status, 0, `${name}: ${result.error || result.stderr}`);
}
const files = await fs.readdir(path.join(dataDir, 'history'));
assert.equal(files.length, 4);
for (const file of files) {
  const result = JSON.parse(await fs.readFile(path.join(dataDir, 'history', file), 'utf8'));
  assert.equal(result.dryRun, true);
  assert.equal(result.scheduled, true);
  assert.equal(result.status, 'success');
  for (const id of ['close-rider', 'close-editor', 'p4-sync', 'wait-ubt', 'ue-build', 'ts-definitions', 'ts-build']) {
    assert.ok(result.steps.some(step => step.id === id), `${result.workspaceName}: missing ${id}`);
  }
  const watch = result.steps.find(step => step.id === 'ts-build');
  assert.equal(watch.label, '编译 TS 并启动 Watch（独立窗口）');
  assert.equal(watch.kind, 'detached-watch');
  assert.equal(watch.watch, true);
  assert.ok(watch.args.includes('--watch'));
  const log = await fs.readFile(path.join(dataDir, 'logs', `${result.id}.jsonl`), 'utf8');
  assert.ok(log.includes('仅输出任务命令'));
}
// Exercise the production executable-relative log path without touching the real user's data.
const portableDir = path.join(dataDir, 'Portable App');
const isolatedProfile = path.join(dataDir, 'user-profile');
await fs.mkdir(portableDir, { recursive: true });
const portableExe = path.join(portableDir, 'MHAutoUpdateCompilerNext.exe');
await fs.copyFile(exe, portableExe);
const env = { ...process.env, LOCALAPPDATA: isolatedProfile, SOLUTIONBAT_DISABLE_TELEMETRY: '1' };
delete env.SOLUTIONBAT_DATA_DIR;
const portable = spawnSync(portableExe, ['--run-task', 'MHA_Client_main', '--dry-run'], { cwd: dataDir, env, windowsHide: true, timeout: 20000, encoding: 'utf8' });
assert.equal(portable.status, 0, portable.error || portable.stderr);
const portableConfig = path.join(portableDir, 'config', 'config.json');
assert.ok((await fs.stat(portableConfig)).isFile());
await assert.rejects(fs.access(path.join(isolatedProfile, 'SolutionBatNext', 'config.json')));
const logDir = path.join(portableDir, 'logs');
const daily = (await fs.readdir(logDir)).find(name => /^scheduled_task_\d{8}\.log$/.test(name));
assert.ok(daily);
assert.match(await fs.readFile(path.join(logDir, daily), 'utf8'), /预演开始/);
await assert.rejects(fs.access(path.join(isolatedProfile, 'SolutionBatNext', 'logs')));

// Match registered tasks: workspace UUID plus an explicit configuration directory.
const scheduledDataDir = path.join(dataDir, 'Scheduled Config');
await fs.mkdir(scheduledDataDir, { recursive: true });
const savedConfig = JSON.parse(await fs.readFile(portableConfig, 'utf8'));
await fs.writeFile(path.join(scheduledDataDir, 'config.json'), JSON.stringify(savedConfig));
const scheduled = spawnSync(portableExe, ['--run-task', savedConfig.workspaces[0].id, '--data-dir', scheduledDataDir, '--dry-run'], { cwd: dataDir, env, windowsHide: true, timeout: 20000, encoding: 'utf8' });
assert.equal(scheduled.status, 0, scheduled.error || scheduled.stderr);
const scheduledHistory = await fs.readdir(path.join(scheduledDataDir, 'history'));
assert.equal(scheduledHistory.length, 1);
const scheduledRun = JSON.parse(await fs.readFile(path.join(scheduledDataDir, 'history', scheduledHistory[0]), 'utf8'));
assert.equal(scheduledRun.scheduled, true);
assert.equal(scheduledRun.dryRun, true);
assert.equal(scheduledRun.status, 'success');
assert.match(await fs.readFile(path.join(logDir, `${scheduledRun.id}.jsonl`), 'utf8'), /仅输出任务命令/);
const dailyLog = await fs.readFile(path.join(logDir, daily), 'utf8');
assert.equal((dailyLog.match(/预演开始/g) || []).length, 2);
await assert.rejects(fs.access(path.join(scheduledDataDir, 'logs')));
await assert.rejects(fs.access(path.join(dataDir, 'logs', `${scheduledRun.id}.jsonl`)));
console.log(JSON.stringify({ passed: true, profiles: 4, executable: exe, dataDir, portableLogDir: logDir, scheduledDataDir, registeredTaskLogPathVerified: true }));
