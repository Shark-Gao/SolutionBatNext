import { test, expect, type Page } from '@playwright/test';
import { newWorkspace } from '../../src/domain';

const panelHeight = async (page: Page) => (await page.locator('.log-panel').boundingBox())!.height;
async function dragHeight(page: Page, change: number) {
  const handle = page.getByRole('separator', { name: '调整日志高度' });
  await handle.scrollIntoViewIfNeeded();
  const box = (await handle.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2 - change, { steps: 12 });
  await page.mouse.up();
}

test('resizes logs and restores the custom height after maximize and collapse', async ({ page }, testInfo) => {
  await page.goto('/');
  await expect(page.getByRole('separator', { name: '调整日志高度' })).toBeVisible();
  const initial = await panelHeight(page);
  await dragHeight(page, 160);
  await expect.poll(() => panelHeight(page)).toBeCloseTo(initial + 160, 0);
  const resized = await panelHeight(page);
  await page.getByRole('button', { name: '展开日志区', exact: true }).click();
  await expect(page.locator('.content-scroll')).toBeHidden();
  await expect(page.getByRole('separator')).toHaveCount(0);
  expect(await panelHeight(page)).toBeGreaterThan(resized);
  await page.getByRole('button', { name: '还原日志区', exact: true }).click();
  await expect.poll(() => panelHeight(page)).toBeCloseTo(resized, 0);
  await page.getByRole('button', { name: '折叠日志', exact: true }).click();
  await expect(page.getByRole('log')).toBeHidden();
  await expect(page.getByRole('separator')).toHaveCount(0);
  expect(await panelHeight(page)).toBeLessThan(90);
  await page.getByRole('button', { name: '显示日志', exact: true }).click();
  await expect.poll(() => panelHeight(page)).toBeCloseTo(resized, 0);
  await page.screenshot({ path: testInfo.outputPath('logs-resized.png') });
  await expect(page.locator('html')).not.toHaveClass(/resizing-logs/);
});

test('remembers log height separately from unfinished workspace edits', async ({ page }) => {
  await page.goto('/');
  const root = page.getByLabel('项目根目录', { exact: true });
  const original = await root.inputValue();
  await root.fill('K:\\unsaved-log-resize');
  await dragHeight(page, 100);
  const resized = await panelHeight(page);
  await page.reload();
  await expect(root).toHaveValue(original);
  await expect.poll(() => panelHeight(page)).toBeCloseTo(resized, 0);
  await page.getByRole('separator').dblclick();
  await expect.poll(() => panelHeight(page)).toBeCloseTo(150, 0);
  expect(await page.evaluate(() => localStorage.getItem('solutionbat-next-log-height-v1'))).toBeNull();
});

test('supports keyboard resizing, bounds, window changes and drag cancellation', async ({ page }) => {
  await page.goto('/');
  const handle = page.getByRole('separator', { name: '调整日志高度' });
  await handle.focus();
  await handle.press('ArrowUp');
  await expect.poll(() => panelHeight(page)).toBeCloseTo(160, 0);
  await handle.press('Shift+ArrowUp');
  await expect.poll(() => panelHeight(page)).toBeCloseTo(200, 0);
  const box = (await handle.boundingBox())!;
  await page.mouse.move(box.x + 60, box.y + 4);
  await page.mouse.down();
  await page.mouse.move(box.x + 60, box.y - 140);
  await expect.poll(() => panelHeight(page)).toBeGreaterThan(300);
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect.poll(() => panelHeight(page)).toBeCloseTo(200, 0);
  await expect(page.locator('html')).not.toHaveClass(/resizing-logs/);
  await handle.press('End');
  const maximum = Number(await handle.getAttribute('aria-valuemax'));
  await expect.poll(() => panelHeight(page)).toBeCloseTo(maximum, 0);
  expect((await page.locator('.content-scroll').boundingBox())!.height).toBeGreaterThanOrEqual(140);
  await page.setViewportSize({ width: 1320, height: 680 });
  await expect.poll(() => panelHeight(page)).toBeLessThan(maximum);
  await page.setViewportSize({ width: 1320, height: 920 });
  await expect.poll(() => panelHeight(page)).toBeCloseTo(maximum, 0);
  await handle.press('Home');
  await expect.poll(() => panelHeight(page)).toBeCloseTo(150, 0);
  await dragHeight(page, -60);
  await expect.poll(() => panelHeight(page)).toBeCloseTo(150, 0);
});

test('keeps resized logs and selection controls usable at different window widths', async ({ page }, testInfo) => {
  await page.goto('/');
  const all = page.getByRole('button', { name: '全选任务', exact: true });
  const invert = page.getByRole('button', { name: '反选任务', exact: true });
  await expect(all).toHaveText('全选');
  await expect(invert).toHaveText('反选');
  await all.click();
  await expect(page.locator('.task-list input:checked')).toHaveCount(6);
  await invert.click();
  await expect(page.locator('.task-list input:checked')).toHaveCount(0);
  await page.getByRole('checkbox', { name: '编译 UE 工程', exact: true }).check();
  await invert.click();
  await expect(page.locator('.task-list input:checked')).toHaveCount(5);
  await expect(page.getByRole('checkbox', { name: '编译 UE 工程', exact: true })).not.toBeChecked();
  for (const viewport of [{ width: 1320, height: 920 }, { width: 760, height: 680 }, { width: 390, height: 844 }]) {
    await page.setViewportSize(viewport);
    const handle = page.getByRole('separator');
    await handle.scrollIntoViewIfNeeded();
    await handle.press('Home');
    await handle.press('Shift+ArrowUp');
    await expect.poll(() => panelHeight(page)).toBeCloseTo(190, 0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    const allBox = (await all.boundingBox())!;
    const invertBox = (await invert.boundingBox())!;
    expect(allBox.x + allBox.width).toBeLessThanOrEqual(invertBox.x);
    await page.screenshot({ path: testInfo.outputPath(`logs-${viewport.width}.png`), fullPage: true });
  }
});

test('keeps populated logs scrollable and following the latest output while resizing', async ({ page }, testInfo) => {
  const w = newWorkspace('log fixture', 'K:\\fixture');
  await page.addInitScript(workspace => {
    const host = window as any;
    host.isTauri = true;
    const run = { id: 'fixture-run', workspaceId: workspace.id, workspaceName: workspace.name, status: 'success', startedAt: '2026-09-05T08:00:00Z', endedAt: '2026-09-05T08:00:01Z', dryRun: false, scheduled: false, steps: [{ id: 'fixture', label: '编译 TS 并启动 Watch', program: 'fixture', args: [], cwd: '', env: {}, watch: false, kind: 'fixture' }], stepStatuses: { fixture: 'success' }, error: null };
    host.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (cmd: string) => {
        if (cmd === 'load_config') return { schemaVersion: 1, revision: 0, selectedId: workspace.id, workspaces: [workspace], settings: { theme: 'dark', autoScroll: true, riderPath: '' } };
        if (cmd === 'app_info') return { version: '0.1.0', dataDir: 'fixture', updaterReady: false };
        if (cmd.startsWith('plugin:event|')) return 1;
        if (cmd === 'runtime_snapshot' || cmd === 'run_history') return [run];
        if (cmd === 'get_logs') return Array.from({ length: 200 }, (_, index) => ({ runId: run.id, workspaceId: workspace.id, time: run.startedAt, level: 'info', message: `输出 ${index + 1}`, stepId: 'fixture', run: null }));
        if (cmd === 'schedule_action') return { registered: false, name: '', nextRun: null, lastResult: null, legacyRegistered: false, times: [], state: null, executable: null, previousRegistered: false };
        throw new Error(`Unexpected fixture IPC: ${cmd}`);
      },
    };
    host.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  }, w);
  await page.goto('/');
  const body = page.getByRole('log');
  await expect(body.locator('.log-line')).toHaveCount(200);
  const atBottom = () => body.evaluate(element => Math.abs(element.scrollHeight - element.scrollTop - element.clientHeight) <= 2);
  await dragHeight(page, 160);
  await expect.poll(atBottom).toBe(true);
  await page.getByRole('separator').press('Home');
  await expect.poll(() => panelHeight(page)).toBeCloseTo(150, 0);
  await expect.poll(atBottom).toBe(true);
  expect(await body.evaluate(element => element.scrollHeight > element.clientHeight)).toBe(true);
  await dragHeight(page, 200);
  await page.screenshot({ path: testInfo.outputPath('logs-populated.png') });
  await page.getByLabel('搜索日志').fill('输出 199');
  await expect(body.locator('.log-line')).toHaveCount(1);
  await page.getByRole('button', { name: '展开日志区', exact: true }).click();
  await expect(body.locator('.log-line')).toHaveCount(1);
  await page.getByRole('button', { name: '还原日志区', exact: true }).click();
  await expect.poll(() => panelHeight(page)).toBeCloseTo(350, 0);
});
