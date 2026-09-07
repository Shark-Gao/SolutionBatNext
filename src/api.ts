import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open, save } from '@tauri-apps/plugin-dialog';
import legacyXml from '../src-tauri/fixtures/legacy-config.xml?raw';
import { newWorkspace } from './domain';
import type { AppConfig, AppInfo, EnvironmentCheck, MonitoredRun, RunEvent, RunInfo, ScheduleInfo, Step, Workspace } from './types';

export const desktop = isTauri();
const previewKey = 'solutionbat-next-preview-v1';
const previewSettingsKey = 'solutionbat-next-ui-v1';
export const helpUrl = 'https://iwiki.woa.com/p/4018100342?from=iWiki_search';
export function previewSeed(): AppConfig {
  const xml = new DOMParser().parseFromString(legacyXml, 'text/xml');
  const workspaces = [...xml.querySelectorAll('workspace')].map((node, i) => {
    const text = (section: string, key: string) => node.querySelector(`${section} > ${key}`)?.textContent?.trim() || '';
    const flag = (section: string, key: string) => /^(true|1|yes)$/i.test(text(section, key));
    const w = newWorkspace(node.getAttribute('name') || '工作区', text('paths', 'project_root'));
    w.id = `00000000-0000-4000-8000-${String(i + 1).padStart(12, '0')}`;
    w.p4 = { port: text('p4', 'port'), user: text('p4', 'user'), client: text('p4', 'client'), force: flag('options', 'force_sync') };
    w.paths = { project: text('paths', 'project'), name: text('paths', 'project_name'), engine: text('paths', 'engine'), tsProject: text('paths', 'ts_proj'), ueExe: text('paths', 'ue_exe') };
    w.build = { configuration: text('build', 'configuration') || 'Development', platform: text('build', 'platform') || 'Win64', target: text('build', 'target') || 'Editor' };
    w.tasks = { sync: flag('tasks', 'run_p4_update'), build: flag('tasks', 'run_project_compile'), typescript: flag('tasks', 'run_ts_compile'), definitions: flag('tasks', 'run_ts_gen'), watch: flag('tasks', 'run_ts_build'), closeEditor: true };
    w.schedule.times = text('schedule', 'triggers').split(';').filter(Boolean); return w;
  });
  return { schemaVersion: 1, revision: 0, selectedId: workspaces[0].id, workspaces, settings: { theme: 'light', autoScroll: true, riderPath: '' } };
}
function requireDesktop() { if (!desktop) throw new Error('此操作需要使用 SolutionBat Next 桌面程序。'); }
export const api = {
  async load(): Promise<AppConfig> {
    if (desktop) return invoke('load_config');
    const existing = localStorage.getItem(previewKey); const config = existing ? JSON.parse(existing) : previewSeed();
    const settings = localStorage.getItem(previewSettingsKey); if (settings) config.settings = { theme: 'light', autoScroll: true, riderPath: '', ...JSON.parse(settings) };
    else config.settings = { theme: 'light', autoScroll: true, riderPath: '', ...config.settings };
    return config;
  },
  async save(config: AppConfig): Promise<AppConfig> {
    if (desktop) return invoke('save_config', { config });
    const result = { ...config, revision: config.revision + 1 }; localStorage.setItem(previewKey, JSON.stringify(result)); return result;
  },
  async info(): Promise<AppInfo> { return desktop ? invoke('app_info') : { version: '0.1.0', dataDir: '浏览器本地预览', updaterReady: false }; },
  async saveSettings(settings: AppConfig['settings']) {
    if (desktop) await invoke('save_ui_settings', { settings });
    else localStorage.setItem(previewSettingsKey, JSON.stringify(settings));
  },
  async help() {
    if (desktop) await invoke('open_help');
    else window.open(helpUrl, '_blank', 'noopener,noreferrer');
  },
  async browse(directory: boolean): Promise<string | null> {
    requireDesktop(); const path = await open({ directory, multiple: false }); return typeof path === 'string' ? path : null;
  },
  async import(): Promise<Workspace[]> {
    requireDesktop(); const path = await open({ filters: [{ name: '工作区配置', extensions: ['xml', 'json'] }], multiple: false });
    return typeof path === 'string' ? invoke('import_config', { path }) : [];
  },
  async export(config: AppConfig) {
    if (!desktop) { download(JSON.stringify(config, null, 2), 'solutionbat-config.json'); return; }
    const path = await save({ defaultPath: 'solutionbat-config.json', filters: [{ name: 'JSON', extensions: ['json'] }] });
    if (path) await invoke('export_config', { path, config });
  },
  async environment(workspace: Workspace): Promise<EnvironmentCheck[]> { requireDesktop(); return invoke('check_environment', { workspace }); },
  async plan(workspace: Workspace, scheduled = false): Promise<Step[]> { requireDesktop(); return invoke('preview_plan', { workspace, scheduled }); },
  async start(workspaceId: string, dryRun: boolean): Promise<RunInfo> { requireDesktop(); return invoke('start_run', { workspaceId, dryRun }); },
  async stop(workspaceId: string) { requireDesktop(); await invoke('stop_run', { workspaceId }); },
  async snapshot(): Promise<RunInfo[]> { return desktop ? invoke('runtime_snapshot') : []; },
  async monitored(): Promise<MonitoredRun | null> { return desktop ? invoke('monitored_run') : null; },
  async stopMonitored() { requireDesktop(); await invoke('stop_monitored_run'); },
  async history(): Promise<RunInfo[]> { return desktop ? invoke('run_history') : []; },
  async logs(runId: string): Promise<RunEvent[]> { return desktop ? invoke('get_logs', { runId }) : []; },
  async exportLogs(runId: string) {
    requireDesktop(); const path = await save({ defaultPath: 'build.log', filters: [{ name: '运行日志', extensions: ['log', 'txt'] }] });
    if (path) await invoke('export_logs', { runId, path });
  },
  async schedule(workspaceId: string, action = 'query'): Promise<ScheduleInfo> {
    if (!desktop && action === 'query') return { registered: false, name: '', nextRun: null, lastResult: null, legacyRegistered: false, times: [], state: null, executable: null, previousRegistered: false };
    requireDesktop(); return invoke('schedule_action', { workspaceId, action });
  },
  async openDirectory(path?: string) { requireDesktop(); await invoke('open_directory', { path: path || null }); },
  async openLogsDirectory() { requireDesktop(); await invoke('open_logs_directory'); },
  async subscribe(callback: (e: RunEvent) => void): Promise<() => void> { return desktop ? listen<RunEvent>('run-event', e => callback(e.payload)) : () => {}; },
  async onClose(callback: () => void): Promise<() => void> { return desktop ? listen('close-with-active-runs', callback) : () => {}; },
};
export function download(text: string, filename: string) {
  const url = URL.createObjectURL(new Blob([text], { type: 'application/json;charset=utf-8' }));
  const anchor = document.createElement('a'); anchor.href = url; anchor.download = filename; anchor.click(); URL.revokeObjectURL(url);
}
