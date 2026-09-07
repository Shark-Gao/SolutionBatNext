import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const sourceExe = process.env.SB_TEST_EXE || path.join(root, 'release', 'SolutionBatNext.exe');
const dir = path.join(root, 'test-results', `startup-${Date.now()}`);
const appDir = path.join(dir, 'Portable App');
const configDir = path.join(dir, 'Config');
await fs.mkdir(appDir, { recursive: true });
await fs.mkdir(configDir, { recursive: true });
const exe = path.join(appDir, 'SolutionBatNext.exe');
await fs.copyFile(sourceExe, exe);
const env = { ...process.env, SOLUTIONBAT_DISABLE_TELEMETRY: '1' };
delete env.SOLUTIONBAT_DATA_DIR;
const run = (id, extra = [], environment = env) => spawnSync(exe, ['--run-task', id, '--data-dir', configDir, ...extra], { cwd: appDir, windowsHide: true, timeout: 15000, encoding: 'utf8', env: environment });
const logText = async () => {
  const logs = path.join(appDir, 'logs');
  const name = (await fs.readdir(logs)).find(name => /^scheduled_task_\d{8}\.log$/.test(name));
  assert.ok(name, 'startup errors have a daily log even before job creation');
  return fs.readFile(path.join(logs, name), 'utf8');
};

// An unknown workspace must fail before any build or process operation.
const missing = run('missing-workspace', ['--check-task']);
assert.equal(missing.status, 1, missing.error || missing.stderr);
assert.match(await logText(), /工作区不存在/);
await assert.rejects(fs.access(path.join(configDir, 'history')));
await assert.rejects(fs.access(path.join(configDir, 'logs')));

const configPath = path.join(configDir, 'config.json');
const config = JSON.parse(await fs.readFile(configPath, 'utf8'));
const w = config.workspaces[0];
w.name = 'Startup fixture';
w.root = path.join(dir, 'Missing Project');
w.paths.project = path.join(w.root, 'Game.uproject');
w.paths.engine = path.join(w.root, 'Engine');
w.paths.tsProject = path.join(w.root, 'TsProj');
w.paths.ueExe = path.join(w.paths.engine, 'Binaries', 'Win64', 'UnrealEditor-Cmd.exe');
config.workspaces = [w]; config.selectedId = w.id;
await fs.writeFile(configPath, JSON.stringify(config));
const failed = run(w.id, ['--check-task']);
assert.equal(failed.status, 1, failed.error || failed.stderr);
const report = JSON.parse(failed.stdout);
assert.equal(report.ok, false);
assert.ok(report.checks.some(c => c.name === '项目根目录' && !c.ok));
assert.match(await logText(), /环境检查未通过/);

// Stub all command entry points as an additional guard against accidental execution.
const bin = path.join(dir, 'Stub Tools');
await fs.mkdir(bin, { recursive: true });
for (const name of ['node.exe', 'p4.exe', 'powershell.exe']) await fs.writeFile(path.join(bin, name), 'fixture: never execute');
for (const filename of [w.paths.project, w.paths.ueExe, path.join(w.paths.engine, 'Build', 'BatchFiles', 'Build.bat'), path.join(w.paths.tsProject, 'tsconfig.json'), path.join(w.paths.tsProject, 'node_modules', 'typescript', 'bin', 'tsc')]) {
  await fs.mkdir(path.dirname(filename), { recursive: true });
  await fs.writeFile(filename, 'fixture: never execute');
}
const checked = run(w.id, ['--check-task', '--show-gui'], { ...env, PATH: bin });
assert.equal(checked.status, 0, checked.error || checked.stderr);
assert.equal(JSON.parse(checked.stdout).ok, true);
await assert.rejects(fs.access(path.join(configDir, 'history')));
await assert.rejects(fs.access(path.join(configDir, 'locks')));
await assert.rejects(fs.access(path.join(configDir, 'live-runs')));
assert.match(await logText(), /环境检查通过，未执行同步、编译或进程操作/);

await fs.writeFile(configPath, '{broken');
const corrupt = run(w.id);
assert.equal(corrupt.status, 1, corrupt.error || corrupt.stderr);
assert.match(await logText(), /配置损坏/);
console.log(JSON.stringify({ passed: true, dir, checks: ['unknown workspace logged', 'preflight failure logged', 'check-only mode creates no run', 'invalid config logged', 'logs next to exe'] }));
