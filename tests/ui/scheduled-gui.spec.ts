import { test, expect } from '@playwright/test';
import { newWorkspace } from '../../src/domain';
import { appVersion } from '../test-version';

test('scheduled window selects its workspace, streams logs, and stops the scheduled worker', async ({ page }, testInfo) => {
  const other = newWorkspace('Other workspace', 'K:\\Other');
  const workspace = newWorkspace('Scheduled workspace', 'K:\\Scheduled');
  const run = {
    id: '00000000-0000-4000-8000-000000000009', workspaceId: workspace.id, workspaceName: workspace.name,
    status: 'running', startedAt: new Date().toISOString(), endedAt: null, dryRun: false, scheduled: true, error: null,
    steps: [{ id: 'p4-sync', label: '同步 P4 工作区', program: 'fixture', args: [], cwd: workspace.root, env: {}, watch: false, kind: 'p4' }],
    stepStatuses: { 'p4-sync': 'running' },
  };
  const config = { schemaVersion: 1, revision: 0, selectedId: other.id, workspaces: [other, workspace], settings: { theme: 'dark', autoScroll: true, riderPath: '' } };
  await page.route(/\/src\/api\.ts(?:\?|$)/, async route => {
    const response = await route.fetch();
    await route.fulfill({ response, body: `${await response.text()}
      const fixtureRun = ${JSON.stringify(run)};
      let polls = 0;
      api.load = async () => (${JSON.stringify(config)});
      api.info = async () => ({version:${JSON.stringify(appVersion)},dataDir:'K:/App/config',updaterReady:false,monitorRunId:fixtureRun.id});
      api.history = async () => [];
      api.snapshot = async () => [];
      api.start = async () => { throw new Error('Must not start a second build'); };
      api.stop = async () => { throw new Error('Must stop the external worker'); };
      api.stopMonitored = async () => { fixtureRun.status='cancelled'; fixtureRun.endedAt=new Date().toISOString(); fixtureRun.stepStatuses['p4-sync']='cancelled'; };
      api.monitored = async () => ({run:{...fixtureRun},workerPid:123,events:[{runId:fixtureRun.id,workspaceId:fixtureRun.workspaceId,time:new Date().toISOString(),level:'info',message:'Scheduled log '+(++polls),stepId:'p4-sync',run:null}]});
    ` });
  });
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Scheduled workspace', exact: true })).toBeVisible();
  await expect(page.locator('.heading-meta .tag').filter({ hasText: '计划任务' })).toBeVisible();
  await expect(page.locator('.change-dot')).toHaveCount(0);
  await expect(page.getByRole('heading', { name: '计划任务进度' })).toBeVisible();
  await expect(page.getByLabel('计划任务执行步骤')).toContainText('同步 P4 工作区');
  await expect(page.getByRole('button', { name: '保存并运行', exact: true })).toHaveCount(0);
  await expect(page.getByRole('log')).toContainText('Scheduled log');
  await expect.poll(async () => page.getByRole('log').innerText()).not.toContain('Scheduled log 1');
  await page.screenshot({ path: testInfo.outputPath('scheduled-running.png') });
  await page.getByRole('button', { name: '停止运行', exact: true }).click();
  await expect(page.locator('.status-badge')).toHaveClass(/cancelled/);
  await expect(page.getByRole('log')).toContainText('Scheduled log');
  await page.setViewportSize({ width: 800, height: 680 });
  await expect(page.getByRole('heading', { name: 'Scheduled workspace', exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('scheduled-finished-narrow.png') });
});

test('scheduled startup errors stay visible without starting a manual task', async ({ page }) => {
  await page.route(/\/src\/api\.ts(?:\?|$)/, async route => {
    const response = await route.fetch();
    await route.fulfill({ response, body: `${await response.text()}
      api.info = async () => ({version:${JSON.stringify(appVersion)},dataDir:'K:/App/config',updaterReady:false,startupError:'计划任务环境检查未通过：测试路径不存在'});
      api.start = async () => { throw new Error('Must not start a manual task'); };
    ` });
  });
  await page.goto('/');
  await expect(page.locator('.boot-state')).toContainText('计划任务环境检查未通过：测试路径不存在');
  await expect(page.getByRole('button', { name: '保存并运行', exact: true })).toHaveCount(0);
});
