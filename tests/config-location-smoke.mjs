import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const sourceExe = process.env.SB_TEST_EXE || path.join(root, 'release', 'MHAutoUpdateCompilerNext.exe');
const dir = path.join(root, 'test-results', `config-location-${Date.now()}`);
const appDir = path.join(dir, 'Portable App');
const profile = path.join(dir, 'Profile');
const legacyDir = path.join(profile, 'SolutionBatNext');
const targetDir = path.join(appDir, 'config');
await fs.mkdir(appDir, { recursive: true });
const exe = path.join(appDir, 'MHAutoUpdateCompilerNext.exe');
await fs.copyFile(sourceExe, exe);
const env = { ...process.env, LOCALAPPDATA: profile, SOLUTIONBAT_DISABLE_TELEMETRY: '1' };
delete env.SOLUTIONBAT_DATA_DIR;
// A non-existent workspace seeds a test fixture, then fails before creating or executing a job.
const run = (extra = [], environment = env) => spawnSync(exe, ['--run-task', 'missing-test-workspace', '--check-task', ...extra], {
  cwd: dir, env: environment, windowsHide: true, timeout: 15000, encoding: 'utf8',
});
assert.equal(run(['--data-dir', legacyDir]).status, 1);
const configPath = path.join(legacyDir, 'config.json');
const config = JSON.parse(await fs.readFile(configPath, 'utf8'));
config.revision = 42;
config.workspaces[0].name = 'Preserved workspace';
await fs.writeFile(configPath, JSON.stringify(config));
await fs.copyFile(configPath, path.join(legacyDir, 'config.json.bak'));
const settings = JSON.stringify({ theme: 'dark', autoScroll: false });
await fs.writeFile(path.join(legacyDir, 'ui-settings.json'), settings);
for (const [folder, file, text] of [['history', 'record.json', '{"id":"fixture"}'], ['logs', 'record.jsonl', '{"message":"old log"}']]) {
  await fs.mkdir(path.join(legacyDir, folder), { recursive: true });
  await fs.writeFile(path.join(legacyDir, folder, file), text);
}
await fs.mkdir(path.join(legacyDir, 'webview'), { recursive: true });
await fs.writeFile(path.join(legacyDir, 'webview', 'cache'), 'do not migrate cache');
const sourceBefore = await fs.readFile(configPath, 'utf8');

assert.equal(run().status, 1);
assert.equal(await fs.readFile(path.join(targetDir, 'config.json'), 'utf8'), sourceBefore);
assert.equal(await fs.readFile(configPath, 'utf8'), sourceBefore);
for (const file of ['config.json.bak', 'ui-settings.json', 'history/record.json', 'logs/record.jsonl']) {
  assert.equal(await fs.readFile(path.join(targetDir, file), 'utf8'), await fs.readFile(path.join(legacyDir, file), 'utf8'));
}
await assert.rejects(fs.access(path.join(targetDir, 'webview')));
await assert.rejects(fs.access(path.join(targetDir, 'locks')));
assert.equal(config.workspaces[0].id, JSON.parse(await fs.readFile(path.join(targetDir, 'config.json'), 'utf8')).workspaces[0].id);

// New portable settings are authoritative, including after the old copy changes.
config.revision = 43;
config.workspaces[0].name = 'New portable settings';
const edited = JSON.stringify(config);
await fs.writeFile(path.join(targetDir, 'config.json'), edited);
await fs.writeFile(configPath, '{corrupt old backup');
assert.equal(run().status, 1);
assert.equal(await fs.readFile(path.join(targetDir, 'config.json'), 'utf8'), edited);

const isolated = path.join(dir, 'Isolated Config');
assert.equal(run([], { ...env, SOLUTIONBAT_DATA_DIR: isolated }).status, 1);
assert.equal(JSON.parse(await fs.readFile(path.join(isolated, 'config.json'), 'utf8')).revision, 0);
const explicit = path.join(dir, 'Explicit Config');
assert.equal(run(['--data-dir', explicit]).status, 1);
assert.equal(JSON.parse(await fs.readFile(path.join(explicit, 'config.json'), 'utf8')).revision, 0);

// Failed migration must not replace the invalid source with newly generated workspace IDs.
const badAppDir = path.join(dir, 'Bad Migration');
await fs.mkdir(badAppDir, { recursive: true });
const badExe = path.join(badAppDir, 'MHAutoUpdateCompilerNext.exe');
await fs.copyFile(exe, badExe);
const bad = spawnSync(badExe, ['--run-task', 'missing-test-workspace', '--check-task'], { cwd: dir, env, windowsHide: true, timeout: 15000, encoding: 'utf8' });
assert.equal(bad.status, 1);
assert.match(bad.stderr, /配置损坏/);
await assert.rejects(fs.access(path.join(badAppDir, 'config', 'config.json')));
assert.equal(await fs.readFile(configPath, 'utf8'), '{corrupt old backup');
console.log(JSON.stringify({ passed: true, dir, checks: ['exe-relative config', 'legacy migration preserves IDs and history', 'source retained', 'new config not overwritten', 'explicit isolation preserved', 'invalid legacy config not replaced'] }));
