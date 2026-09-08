import { test, expect } from '@playwright/test';
import { newWorkspace } from '../../src/domain';

test('migrates four workspaces, edits configuration and persists it', async ({ page }) => {
  const errors: string[] = []; page.on('pageerror', e => errors.push(e.message));
  await page.goto('/');
  await expect(page.locator('.workspace-item')).toHaveCount(4);
  await expect(page.getByRole('heading', { name: 'MHA_Client_main', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '展开路径', exact: true }).click();
  await page.getByLabel('项目根目录', { exact: true }).fill('K:\\Project with spaces');
  await expect(page.getByLabel('项目文件', { exact: true })).toHaveValue('K:\\Project with spaces\\MHAGame\\MHMobile.uproject');
  await page.getByRole('button', { name: '保存', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('已保存');
  await page.reload();
  await expect(page.getByLabel('项目根目录', { exact: true })).toHaveValue('K:\\Project with spaces');
  expect(errors).toEqual([]);
});

test('creates a workspace, rejects duplicate names and switches views', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: '新建工作区', exact: true }).click();
  await page.getByRole('dialog').getByLabel('工作区名称', { exact: true }).fill('MHA_Client_main');
  await page.getByRole('button', { name: '创建工作区' }).click();
  await expect(page.getByRole('status')).toContainText('已存在');
  await page.getByRole('dialog').getByLabel('工作区名称', { exact: true }).fill('我的测试分支');
  await page.getByLabel('新工作区根目录').fill('K:\\测试工程');
  await page.getByRole('button', { name: '创建工作区' }).click();
  await expect(page.locator('.workspace-item')).toHaveCount(5);
  await expect(page.getByRole('heading', { name: '我的测试分支' })).toBeVisible();
  await page.getByRole('button', { name: '计划任务', exact: true }).click();
  await page.getByRole('button', { name: '添加时间' }).click();
  await expect(page.getByLabel('触发时间 1', { exact: true })).toHaveValue('08:00');
  await page.getByLabel('触发时间 1', { exact: true }).fill('09:00');
  await page.getByRole('button', { name: '添加时间' }).click();
  await expect(page.getByLabel('触发时间 2', { exact: true })).toHaveValue('08:00');
  await page.getByRole('button', { name: '删除触发时间 1', exact: true }).click();
  await expect(page.getByLabel('触发时间 1', { exact: true })).toHaveValue('08:00');
  await page.getByRole('button', { name: '运行记录', exact: true }).click();
  await expect(page.getByText('还没有运行记录')).toBeVisible();
});

test('workspace summary opens the schedule page and schedule settings persist', async ({ page }) => {
  await page.goto('/');
  await expect(page.locator('.schedule-summary')).toBeVisible();
  await expect(page.locator('.workspace-layout .schedule-panel')).toHaveCount(0);
  await page.getByRole('button', { name: '管理计划任务', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Windows 计划任务' });
  await expect(panel.getByRole('button', { name: '注册计划任务', exact: true })).toBeVisible();
  await expect(panel.getByRole('button', { name: '删除计划任务', exact: true })).toBeVisible();
  await expect(page.getByRole('table', { name: '每日触发时间' })).toBeVisible();
  await expect(panel.getByLabel('打开项目工程', { exact: true })).toHaveValue('none');
  await panel.getByLabel('打开项目工程', { exact: true }).selectOption('visualStudio');
  await expect(panel.getByLabel('开发工具版本', { exact: true })).toHaveValue('2022');
  await panel.getByLabel('打开项目工程', { exact: true }).selectOption('rider');
  await expect(panel.getByLabel('开发工具版本', { exact: true })).toHaveValue('2024.3.10');
  await panel.getByLabel('指定启动程序（可选）', { exact: true }).fill('K:\\JetBrains\\Rider\\bin\\rider64.exe');
  await page.getByLabel('触发时间 1', { exact: true }).fill('06:30');
  await page.getByRole('button', { name: '保存', exact: true }).click();
  await page.reload();
  await page.locator('.page-nav').getByRole('button', { name: /^计划任务/ }).click();
  await expect(page.getByLabel('触发时间 1', { exact: true })).toHaveValue('06:30');
  await expect(page.getByLabel('打开项目工程', { exact: true })).toHaveValue('rider');
  await expect(page.getByLabel('指定启动程序（可选）', { exact: true })).toHaveValue('K:\\JetBrains\\Rider\\bin\\rider64.exe');
  await expect(page.getByText('单次编译', { exact: true })).toHaveCount(0);
  await expect(page.getByLabel('执行前关闭 Rider', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '删除触发时间 1', exact: true }).click();
  await page.getByRole('button', { name: '注册计划任务', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('请至少添加一个触发器时间');
});

test('registers saved triggers and can delete a task even with invalid unsaved times', async ({ page }, testInfo) => {
  const w = newWorkspace('schedule fixture', 'K:\\Fixture');
  w.schedule.times = ['03:00']; w.tasks.watch = true;
  await page.addInitScript(seed => {
    const host = window as any;
    host.isTauri = true;
    host.fixtureRequests = [];
    let config = seed;
    let state = { registered: true, name: 'DailyBuild_schedule_fixture', nextRun: '2026-09-06 03:00:00', lastResult: 0, legacyRegistered: true, times: ['03:00'], state: 'Ready', executable: 'K:\\Old App\\old.exe', previousRegistered: false };
    host.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (cmd: string, args: any) => {
        host.fixtureRequests.push({ cmd, args });
        if (cmd === 'load_config') return structuredClone(config);
        if (cmd === 'app_info') return { version: '0.1.0', dataDir: 'fixture', updaterReady: false };
        if (cmd === 'runtime_snapshot' || cmd === 'run_history') return [];
        if (cmd.startsWith('plugin:event|')) return 1;
        if (cmd === 'save_config') { config = structuredClone(args.config); config.revision++; return structuredClone(config); }
        if (cmd === 'schedule_action') {
          if (args.action === 'register') state = { ...state, legacyRegistered: false, times: [...config.workspaces[0].schedule.times], executable: 'K:\\New App\\new.exe' };
          if (args.action === 'unregister') state = { ...state, registered: false, times: [] };
          return structuredClone(state);
        }
        throw new Error(`Unexpected fixture IPC: ${cmd}`);
      },
    };
    host.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  }, { schemaVersion: 1, revision: 0, selectedId: w.id, workspaces: [w], settings: { theme: 'light', autoScroll: true, riderPath: '' } });
  await page.goto('/');
  await page.locator('.page-nav').getByRole('button', { name: /^计划任务/ }).click();
  await expect(page.locator('.schedule-status-band')).toContainText('计划任务已注册');
  await expect(page.locator('.schedule-status-band')).toContainText('下次运行');
  await expect(page.locator('.schedule-details')).toContainText('DailyBuild_schedule_fixture');
  for (const viewport of [{ width: 1320, height: 920 }, { width: 760, height: 680 }, { width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await page.screenshot({ path: testInfo.outputPath(`schedule-${viewport.width}.png`), fullPage: true });
  }
  await page.getByLabel('触发时间 1', { exact: true }).fill('04:00');
  await expect(page.locator('.schedule-panel')).toContainText('触发时间尚未注册');
  await page.locator('.page-nav').getByRole('button', { name: '工作区配置', exact: true }).click();
  await expect(page.locator('.schedule-summary p')).toHaveText('03:00');
  await page.getByRole('button', { name: '管理计划任务', exact: true }).click();
  await expect(page.getByLabel('触发时间 1', { exact: true })).toHaveValue('04:00');
  await page.getByRole('button', { name: '注册计划任务', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('计划任务已注册');
  const calls = await page.evaluate(() => (window as any).fixtureRequests);
  const save = calls.find((call: any) => call.cmd === 'save_config');
  expect(save.args.config.workspaces[0].tasks).toEqual(w.tasks);
  expect(save.args.config.workspaces[0].schedule.times).toEqual(['04:00']);
  await page.locator('.page-nav').getByRole('button', { name: '工作区配置', exact: true }).click();
  await expect(page.locator('.schedule-summary p')).toHaveText('04:00');
  await page.getByRole('button', { name: '管理计划任务', exact: true }).click();
  await page.getByLabel('触发时间 1', { exact: true }).fill('');
  await page.getByRole('button', { name: '删除计划任务', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('计划任务已删除');
  expect(await page.evaluate(() => (window as any).fixtureRequests.filter((call: any) => call.cmd === 'save_config').length)).toBe(1);
  await page.locator('.page-nav').getByRole('button', { name: '工作区配置', exact: true }).click();
  await expect(page.locator('.schedule-summary')).toContainText('未注册');
});

test('does not pretend to run local tasks in the browser', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: '预演', exact: true }).click();
  await expect(page.getByRole('status')).toContainText('桌面程序');
  await expect(page.getByText('等待任务运行')).toBeVisible();
});

test('keeps help and about in settings without duplicate sidebar entries', async ({ page }, testInfo) => {
  await page.addInitScript(() => { window.open = ((url: string) => { (window as any).openedHelpUrl = url; return null; }) as typeof window.open; });
  await page.goto('/');
  await expect(page.locator('.sidebar').getByRole('button', { name: '帮助文档', exact: true })).toHaveCount(0);
  await expect(page.locator('.sidebar').getByRole('button', { name: '关于', exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: '设置', exact: true }).click();
  await page.getByRole('button', { name: '在线帮助文档', exact: true }).click();
  expect(await page.evaluate(() => (window as any).openedHelpUrl)).toBe('https://iwiki.woa.com/p/4018100342?from=iWiki_search');
  await page.getByRole('button', { name: '关于', exact: true }).click();
  await expect(page.getByRole('dialog')).toContainText('作者：sharkgao');
  await expect(page.getByRole('dialog')).toContainText('MHAutoUpdateCompiler');
  await page.getByRole('button', { name: '关闭对话框' }).click();
  await page.locator('.settings-page').getByRole('button', { name: '离线使用手册', exact: true }).click();
  await expect(page.locator('.help-document')).toContainText('程序所在目录的 logs 文件夹');
  await expect(page.locator('.help-document')).toContainText('Windows 计划任务');
  await page.screenshot({ path: testInfo.outputPath('help-desktop.png'), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('help-mobile.png'), fullPage: true });
});

test('theme auto-saves without saving unfinished project edits', async ({ page }) => {
  await page.goto('/');
  const root = await page.getByLabel('项目根目录', { exact: true }).inputValue();
  await page.getByLabel('项目根目录', { exact: true }).fill('K:\\unsaved changes');
  await page.getByRole('button', { name: '切换深浅主题' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await expect(page.getByLabel('项目根目录', { exact: true })).toHaveValue(root);
});

test('can remove a new unsaved workspace', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: '新建工作区', exact: true }).click();
  await page.getByRole('dialog').getByLabel('工作区名称', { exact: true }).fill('temporary profile');
  await page.getByRole('button', { name: '创建工作区' }).click();
  await page.getByRole('button', { name: '工作区操作', exact: true }).click();
  await page.getByRole('button', { name: '删除工作区', exact: true }).click();
  await page.getByRole('dialog').getByRole('button', { name: '删除工作区', exact: true }).click();
  await expect(page.locator('.workspace-item')).toHaveCount(4);
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('renders light, dark and narrow layouts without horizontal overflow', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', e => errors.push(e.message));
  await page.goto('/');
  await expect(page.locator('.brand img')).toBeVisible();
  await expect(page.getByRole('checkbox', { name: '编译 TS 并启动 Watch', exact: true })).toBeVisible();
  await expect(page.getByRole('checkbox', { name: '启动 TS Watch', exact: true })).toHaveCount(0);
  expect(await page.locator('.brand img').evaluate((img: HTMLImageElement) => img.complete && img.naturalWidth > 0)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('desktop-light.png'), fullPage: true });
  await page.getByRole('button', { name: '切换深浅主题' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  await page.screenshot({ path: testInfo.outputPath('desktop-dark.png'), fullPage: true });
  for (const viewport of [{ width: 760, height: 680 }, { width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await expect(page.getByRole('button', { name: '保存并运行', exact: true })).toBeVisible();
    const watchLabel = page.locator('.task-row').filter({ has: page.getByRole('checkbox', { name: '编译 TS 并启动 Watch', exact: true }) });
    expect(await watchLabel.evaluate(row => {
      const text = row.querySelector('strong')!;
      const subtitle = row.querySelector('small')!;
      return text.scrollWidth <= text.clientWidth && text.getBoundingClientRect().bottom <= subtitle.getBoundingClientRect().top;
    })).toBe(true);
    await page.screenshot({ path: testInfo.outputPath(`viewport-${viewport.width}.png`), fullPage: true });
  }
  expect(errors).toEqual([]);
});
