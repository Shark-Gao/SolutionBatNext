import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';

const root = path.resolve(import.meta.dirname, '..');
const dataDir = path.join(root, 'test-results', `native-${Date.now()}`);
await fs.mkdir(dataDir, { recursive: true });
const exe = process.env.SB_TEST_EXE || path.join(root, 'src-tauri', 'target', 'debug', 'solution-bat-next.exe');
const app = spawn(exe, [], { cwd: root, windowsHide: true, env: { ...process.env, SOLUTIONBAT_DISABLE_TELEMETRY: '1', SOLUTIONBAT_DATA_DIR: dataDir, SOLUTIONBAT_TEST_PORT: '9223', WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9223' } });
let appOutput = '';
app.stdout.on('data', chunk => { appOutput += chunk.toString(); });
app.stderr.on('data', chunk => { appOutput += chunk.toString(); });
let browser;
try {
  let ready = false;
  for (let i = 0; i < 100; i++) {
    if (app.exitCode !== null) throw new Error(`Desktop application exited: ${app.exitCode}\n${appOutput}`);
    try { const r = await fetch('http://127.0.0.1:9223/json/version'); if (r.ok) { ready = true; break; } } catch {}
    await new Promise(resolve => setTimeout(resolve, 200));
  }
  assert.ok(ready, 'WebView2 remote debugging became available');
  browser = await chromium.connectOverCDP('http://127.0.0.1:9223');
  const context = browser.contexts()[0];
  let page;
  for (let i = 0; i < 50; i++) { page = context.pages()[0]; if (page) break; await new Promise(r => setTimeout(r, 100)); }
  assert.ok(page);
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  await page.locator('.workspace-item').first().waitFor();
  assert.equal(await page.locator('.workspace-item').count(), 4);
  assert.match(await page.locator('.sidebar-foot').innerText(), /本地桌面端/);
  await page.getByRole('button', { name: '检查环境', exact: true }).click();
  await page.getByRole('dialog', { name: '环境检查' }).waitFor();
  assert.equal(await page.locator('.check-row').count(), 9);
  await page.screenshot({ path: path.join(dataDir, 'native-environment.png') });
  await page.getByRole('button', { name: '关闭对话框' }).click();
  await page.getByRole('button', { name: '预演', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('.log-title .status-badge')?.textContent?.includes('已完成'), { timeout: 15000 });
  assert.match(await page.getByRole('log').innerText(), /仅输出任务命令/);
  assert.match(await page.getByRole('log').innerText(), /UnrealEditor-Cmd.exe/);
  assert.equal(await page.locator('.run-stages .success').count(), 3);
  await page.screenshot({ path: path.join(dataDir, 'native-workspace.png') });
  await page.getByRole('button', { name: '运行记录', exact: true }).click();
  assert.equal(await page.locator('button.history-row').count(), 1);
  await page.locator('button.history-row').click();
  await page.locator('.page-nav').getByRole('button', { name: /^计划任务/ }).click();
  await page.locator('.schedule-panel').waitFor();
  await page.waitForFunction(() => !document.querySelector('.schedule-status-band')?.textContent?.includes('正在检查'));
  assert.match(await page.locator('.schedule-panel').innerText(), /DailyBuild_MHA_Client_main/);
  assert.match(await page.locator('.schedule-panel').innerText(), /计划任务已注册/);
  assert.match(await page.locator('.schedule-panel').innerText(), /下次运行/);
  assert.equal(await page.getByLabel('触发时间 1', { exact: true }).getAttribute('type'), 'time');
  await page.screenshot({ path: path.join(dataDir, 'native-schedule.png') });
  assert.equal(await page.locator('.sidebar').getByRole('button', { name: '帮助文档', exact: true }).count(), 0);
  assert.equal(await page.locator('.sidebar').getByRole('button', { name: '关于', exact: true }).count(), 0);
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.getByRole('button', { name: '关于', exact: true }).click();
  assert.match(await page.getByRole('dialog').innerText(), /作者：sharkgao/);
  await page.getByRole('button', { name: '关闭对话框' }).click();
  await page.locator('.settings-page').getByRole('button', { name: '离线使用手册', exact: true }).click();
  assert.match(await page.locator('.help-document').innerText(), /程序所在目录的 logs 文件夹/);
  await page.screenshot({ path: path.join(dataDir, 'native-help.png') });
  const config = JSON.parse(await fs.readFile(path.join(dataDir, 'config.json'), 'utf8'));
  const records = await fs.readdir(path.join(dataDir, 'history'));
  assert.equal(records.length, 1);
  const record = JSON.parse(await fs.readFile(path.join(dataDir, 'history', records[0]), 'utf8'));
  assert.equal(record.dryRun, true); assert.equal(record.status, 'success');
  assert.equal(config.workspaces.length, 4);
  const dailyLog = (await fs.readdir(path.join(dataDir, 'logs'))).find(name => /^manual_\d{8}\.log$/.test(name));
  assert.ok(dailyLog);
  assert.match(await fs.readFile(path.join(dataDir, 'logs', dailyLog), 'utf8'), /预演开始/);
  assert.deepEqual(errors, []);
  await fs.writeFile(path.join(dataDir, 'verification.json'), JSON.stringify({ passed: true, checks: ['native WebView2 renders', 'four profiles migrated', 'nine environment checks', 'Rust command preview completes', 'live step events', 'history and logs persisted', 'schedule status queried without mutations', 'no frontend errors'], exe, dataDir }, null, 2));
  console.log(JSON.stringify({ passed: true, dataDir }));
} finally {
  if (browser) await browser.close();
  app.kill();
}
