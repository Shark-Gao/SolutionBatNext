<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import {
  Activity, ArrowDownToLine, ArrowUpFromLine, ArrowLeftRight, Check, CheckCheck, CheckCircle2, ChevronDown, ChevronRight,
  Circle, CircleAlert, Clock3, Copy, Download, Ellipsis, Expand, FileCode2, Folder, FolderOpen, GitBranch,
  Hammer, History, Laptop, ListChecks, LoaderCircle, Minus, Moon, Pencil, Play, Plus, RefreshCw, Save,
  Search, Settings2, ShieldCheck, Square, Sun, Terminal, Trash2, Wifi, X, Zap, BookOpen, CircleHelp, Info,
} from 'lucide-vue-next';
import IconButton from './components/IconButton.vue';
import PathField from './components/PathField.vue';
import SchedulePanel from './components/SchedulePanel.vue';
import HelpView from './components/HelpView.vue';
import { useLogPanelResize } from './useLogPanelResize';
import { api, desktop } from './api';
import { active, derivePaths, executionLabels, mergeWorkspaces, newWorkspace, statusNames, taskNames, taskOrder, validateWorkspace } from './domain';
import type { AppConfig, AppInfo, EnvironmentCheck, InstalledLauncher, RunEvent, RunInfo, ScheduleInfo, Step, TaskKey, Workspace } from './types';

const config = ref<AppConfig>();
const saved = ref('');
const fatal = ref('');
const info = ref<AppInfo>({ version: '0.1.0', dataDir: '', updaterReady: false });
const page = ref<'workspace' | 'schedule' | 'history' | 'settings' | 'help'>('workspace');
const search = ref('');
const busy = ref(false);
const toast = ref<{ text: string; error: boolean }>();
const menus = ref(false);
const modal = ref<'' | 'new' | 'rename' | 'delete' | 'environment' | 'plan' | 'force' | 'close' | 'about'>('');
const modalName = ref(''); const modalRoot = ref('');
const checks = ref<EnvironmentCheck[]>([]);
const steps = ref<Step[]>([]);
const showPaths = ref(false);
const logExpanded = ref(false); const logCollapsed = ref(false);
const { mainArea, logPanel, panelStyle, actualHeight, minHeight, maxHeight, resizing, startResize, moveResize, endResize, cancelResize, resetHeight, resizeKeydown } = useLogPanelResize(computed(() => !logExpanded.value && !logCollapsed.value));
const logSearch = ref(''); const logLevel = ref('all');
const logBody = ref<HTMLElement>();
const runs = ref<Record<string, RunInfo>>({});
const events = ref<Record<string, RunEvent[]>>({});
const history = ref<RunInfo[]>([]);
const selectedHistory = ref<RunInfo>();
const schedules = ref<Record<string, ScheduleInfo>>({});
const scheduleErrors = ref<Record<string, string>>({});
const launchers = ref<InstalledLauncher[]>([]);
const launchersLoading = ref(false);
const launchersError = ref('');
const now = ref(Date.now());
const updateState = ref('');
const monitorError = ref('');
const systemTheme = window.matchMedia('(prefers-color-scheme: dark)');
const systemDark = ref(systemTheme.matches);
const systemThemeChanged = (event: MediaQueryListEvent) => { systemDark.value = event.matches; };
let toastTimer: ReturnType<typeof setTimeout>;
let tick: ReturnType<typeof setInterval>;
let monitorTick: ReturnType<typeof setInterval> | undefined;
let monitorPending = false;
let unsub = () => {}; let unsubClose = () => {};
const current = computed(() => config.value?.workspaces.find(w => w.id === config.value?.selectedId) || config.value?.workspaces[0]);
const dirty = computed(() => !!config.value && JSON.stringify(config.value) !== saved.value);
const visibleWorkspaces = computed(() => config.value?.workspaces.filter(w => `${w.name} ${w.root}`.toLowerCase().includes(search.value.toLowerCase())) || []);
const currentRun = computed(() => current.value ? runs.value[current.value.id] : undefined);
const monitoredCurrent = computed(() => !!currentRun.value && currentRun.value.id === info.value.monitorRunId);
const displayRun = computed(() => page.value === 'history' && selectedHistory.value ? selectedHistory.value : currentRun.value);
const running = computed(() => active(currentRun.value?.status));
const allRunning = computed(() => Object.values(runs.value).filter(r => active(r.status)));
const logs = computed(() => {
  const run = displayRun.value; if (!run) return [];
  return (events.value[run.id] || []).filter(e => (logLevel.value === 'all' || e.level === logLevel.value) && e.message.toLowerCase().includes(logSearch.value.toLowerCase()));
});
const schedule = computed(() => current.value ? schedules.value[current.value.id] : undefined);
const scheduleError = computed(() => current.value ? scheduleErrors.value[current.value.id] : '');
const chosenCount = computed(() => current.value ? Object.values(current.value.tasks).filter(Boolean).length : 0);
const flow = computed(() => monitoredCurrent.value ? currentRun.value!.steps.map(s => s.label) : current.value ? executionLabels(current.value) : []);
const visibleHistory = computed(() => history.value.filter(r => r.workspaceId === current.value?.id));
const taskIcons = { closeEditor: Square, sync: RefreshCw, build: Hammer, typescript: FileCode2, definitions: ListChecks, watch: Activity };
const taskDetail: Record<TaskKey, string> = { closeEditor: '当前用户', sync: 'Perforce', build: 'Unreal Engine', typescript: '定义 + Watch', definitions: 'ue.d.ts', watch: '编译 + 持续监听' };
const themeDark = computed(() => config.value?.settings.theme === 'dark' || (config.value?.settings.theme === 'system' && systemDark.value));
function notify(text: string, error = false) { clearTimeout(toastTimer); toast.value = { text, error }; toastTimer = setTimeout(() => { toast.value = undefined; }, error ? 10000 : 3500); }
function errorText(error: unknown) { return error instanceof Error ? error.message : String(error); }
async function perform(fn: () => Promise<void>) { if (busy.value) return; busy.value = true; try { await fn(); } catch (e) { notify(errorText(e), true); } finally { busy.value = false; } }
function validate() {
  if (!config.value) throw new Error('配置尚未载入');
  for (const w of config.value.workspaces) { const error = validateWorkspace(w); if (error) throw new Error(`${w.name}：${error}`); }
  const names = config.value.workspaces.map(w => w.name.trim().toLowerCase());
  if (new Set(names).size !== names.length) throw new Error('工作区名称不能重复');
}
async function persist() {
  validate(); const result = await api.save(JSON.parse(JSON.stringify(config.value)));
  config.value = result; saved.value = JSON.stringify(result);
}
async function save() { await perform(async () => { await persist(); notify('工作区配置已保存'); }); }
function choose(w: Workspace) { if (config.value) config.value.selectedId = w.id; menus.value = false; selectedHistory.value = undefined; }
async function refreshSchedule(id?: string) {
  if (!id) return;
  try { schedules.value[id] = await api.schedule(id); delete scheduleErrors.value[id]; }
  catch (e) { scheduleErrors.value[id] = errorText(e); }
}
async function refreshLaunchers(action?: Workspace['schedule']['afterSuccess']['action']) {
  if (!desktop || !action || action === 'none') { launchers.value = []; launchersError.value = ''; return; }
  launchersLoading.value = true; launchersError.value = '';
  try { launchers.value = await api.launchers(action); }
  catch (error) { launchers.value = []; launchersError.value = errorText(error); }
  finally { launchersLoading.value = false; }
}
function beginNew() { modalName.value = ''; modalRoot.value = ''; menus.value = false; modal.value = 'new'; }
function beginRename() { modalName.value = current.value?.name || ''; menus.value = false; modal.value = 'rename'; }
async function acceptName() {
  await perform(async () => {
    if (!config.value || !current.value) return;
    const name = modalName.value.trim(); if (!name) throw new Error('请输入工作区名称');
    if (config.value.workspaces.some(w => w.name.toLowerCase() === name.toLowerCase() && (modal.value === 'new' || w.id !== current.value!.id))) throw new Error('该名称已存在');
    if (modal.value === 'new') { const w = newWorkspace(name, modalRoot.value.trim()); config.value.workspaces.push(w); config.value.selectedId = w.id; }
    else {
      const existing = (JSON.parse(saved.value) as AppConfig).workspaces.find(w => w.id === current.value!.id);
      if (existing && name !== existing.name && (await api.schedule(existing.id)).registered) throw new Error('请先删除此工作区的计划任务，再重命名');
      current.value.name = name;
    }
    modal.value = ''; page.value = 'workspace';
  });
}
function duplicate() {
  if (!config.value || !current.value) return;
  const w = JSON.parse(JSON.stringify(current.value)) as Workspace;
  w.schedule.times = []; mergeWorkspaces(config.value, [w]); config.value.selectedId = w.id; menus.value = false;
  notify('已复制工作区，计划任务未注册');
}
async function removeWorkspace() {
  await perform(async () => {
    if (!config.value || !current.value) return;
    if (config.value.workspaces.length === 1) throw new Error('至少保留一个工作区');
    if (running.value) throw new Error('请先停止此工作区的任务');
    const savedConfig = JSON.parse(saved.value) as AppConfig;
    if (savedConfig.workspaces.some(w => w.id === current.value!.id)) {
      const state = await api.schedule(current.value.id); if (state.registered) throw new Error('请先移除此工作区的计划任务');
    }
    const previous = JSON.stringify(config.value);
    config.value.workspaces = config.value.workspaces.filter(w => w.id !== current.value?.id);
    config.value.selectedId = config.value.workspaces[0].id;
    try { await persist(); } catch (e) { config.value = JSON.parse(previous); throw e; }
    modal.value = ''; notify('工作区已删除，项目文件未改动');
  });
}
async function browseRoot() { await perform(async () => { const path = await api.browse(true); if (path && current.value) { current.value.root = path; pathsChanged(); } }); }
async function browsePath(field: keyof Workspace['paths'], directory: boolean) { await perform(async () => { const path = await api.browse(directory); if (path && current.value) current.value.paths[field] = path; }); }
async function browseLaunch() { await perform(async () => { const path = await api.browse(false); if (path && current.value) current.value.schedule.afterSuccess.executable = path; }); }
function pathsChanged() { if (current.value?.autoPaths) derivePaths(current.value); }
function toggleAll(invert = false) { if (current.value) for (const key of taskOrder) current.value.tasks[key] = invert ? !current.value.tasks[key] : true; }
async function importConfig() { await perform(async () => {
  const ws = await api.import(); if (ws.length && config.value) { mergeWorkspaces(config.value, ws); notify(`已导入 ${ws.length} 个工作区，保存后生效`); }
}); }
async function checkEnvironment() { await perform(async () => { if (current.value) { checks.value = await api.environment(current.value); modal.value = 'environment'; } }); }
async function showPlan() { await perform(async () => { if (current.value) { steps.value = monitoredCurrent.value ? currentRun.value!.steps : await api.plan(current.value, page.value === 'schedule'); modal.value = 'plan'; } }); }
async function start(dryRun: boolean, confirmed = false) {
  if (!current.value) return;
  if (!dryRun && !confirmed && current.value.tasks.sync && current.value.p4.force) { modal.value = 'force'; return; }
  await perform(async () => {
    if (!desktop) throw new Error('请打开桌面程序执行本机任务。浏览器中的配置仅用于预览。');
    await persist(); const id = current.value!.id;
    const run = await api.start(id, dryRun);
    if (!runs.value[id] || runs.value[id].id !== run.id) runs.value[id] = run;
    events.value[run.id] ||= []; modal.value = ''; logCollapsed.value = false; page.value = 'workspace';
    notify(dryRun ? '开始预演任务命令' : '任务已开始');
  });
}
async function stopRun(run: RunInfo) {
  if (run.id === info.value.monitorRunId) await api.stopMonitored();
  else await api.stop(run.workspaceId);
}
async function stop() { await perform(async () => { if (currentRun.value) await stopRun(currentRun.value); }); }
async function stopAll() { await perform(async () => { for (const run of allRunning.value) await stopRun(run); modal.value = ''; notify('正在停止所有运行任务'); }); }
async function changeSchedule(action: 'register' | 'unregister') {
  await perform(async () => {
    const id = current.value!.id;
    if (action === 'register') {
      if (!current.value!.schedule.times.length) throw new Error('请至少添加一个触发器时间');
      await persist();
    } else if (!(JSON.parse(saved.value) as AppConfig).workspaces.some(w => w.id === id)) {
      notify('计划任务不存在'); return;
    }
    try {
      schedules.value[id] = await api.schedule(id, action);
      delete scheduleErrors.value[id];
      notify(action === 'register' ? '计划任务已注册，关闭本程序后仍会按时执行' : '计划任务已删除');
    } catch (e) {
      await refreshSchedule(id);
      scheduleErrors.value[id] = errorText(e); throw e;
    }
  });
}
async function viewHistory(run: RunInfo) { selectedHistory.value = run; logCollapsed.value = false; await perform(async () => { events.value[run.id] = await api.logs(run.id); }); }
function onEvent(event: RunEvent) {
  const list = events.value[event.runId] ||= []; list.push(event); if (list.length > 1200) list.splice(0, list.length - 1200);
  if (event.run) {
    runs.value[event.workspaceId] = event.run;
    if (!active(event.run.status)) { history.value = [event.run, ...history.value.filter(r => r.id !== event.runId)].slice(0, 100); }
  }
}
function time(value: string) { return new Date(value).toLocaleTimeString('zh-CN', { hour12: false }); }
function date(value: string) { return new Date(value).toLocaleString('zh-CN', { hour12: false }); }
function duration(run: RunInfo) { const seconds = Math.max(0, Math.floor(((run.endedAt ? Date.parse(run.endedAt) : now.value) - Date.parse(run.startedAt)) / 1000)); return `${Math.floor(seconds / 60)}分 ${seconds % 60}秒`; }
async function setPreferences(patch: Partial<AppConfig['settings']>) {
  await perform(async () => {
    if (!config.value) return;
    const previous = { ...config.value.settings };
    config.value.settings = { ...previous, ...patch };
    try {
      await api.saveSettings({ ...config.value.settings });
      const baseline = JSON.parse(saved.value) as AppConfig;
      baseline.settings = { ...config.value.settings }; saved.value = JSON.stringify(baseline);
    } catch (e) { config.value.settings = previous; throw e; }
  });
}
function setTheme(theme: AppConfig['settings']['theme']) { return setPreferences({ theme }); }
function openHelp() { return perform(() => api.help()); }
async function copyLogs() { await perform(async () => { await navigator.clipboard.writeText(logs.value.map(e => `${time(e.time)} [${e.level}] ${e.message}`).join('\n')); notify('日志已复制'); }); }
async function update() {
  await perform(async () => {
    if (!info.value.updaterReady) { updateState.value = '尚未配置更新地址和签名公钥'; return; }
    const { check } = await import('@tauri-apps/plugin-updater'); const available = await check();
    if (!available) { updateState.value = '当前已是最新版本'; return; }
    updateState.value = `正在下载 ${available.version}`;
    await available.downloadAndInstall(); updateState.value = '更新已安装，关闭后重新打开程序生效';
  });
}
watch(current, async (value, old) => {
  if (value?.id !== old?.id) {
    refreshSchedule(value?.id);
    const run = value ? runs.value[value.id] : undefined;
    if (run && !events.value[run.id]) {
      try { events.value[run.id] = await api.logs(run.id); } catch (e) { notify(errorText(e), true); }
    }
  }
});
watch(() => current.value?.schedule.afterSuccess.action, action => { void refreshLaunchers(action); }, { immediate: true });
watch(themeDark, dark => { document.documentElement.dataset.theme = dark ? 'dark' : 'light'; }, { immediate: true });
watch([() => logs.value.length, actualHeight], async () => { if (config.value?.settings.autoScroll) { await nextTick(); logBody.value?.scrollTo({ top: logBody.value.scrollHeight }); } });
watch(modal, async value => { if (value) { await nextTick(); document.querySelector<HTMLInputElement>('.modal input')?.focus(); } });
function keydown(event: KeyboardEvent) {
  if (event.key === 'Escape') { modal.value = ''; menus.value = false; }
  if ((event.ctrlKey || event.metaKey) && event.key === 's') { event.preventDefault(); save(); }
  if (modal.value && event.key === 'Tab') {
    const elements = [...document.querySelectorAll<HTMLElement>('.modal button:not([disabled]), .modal input:not([disabled]), .modal select')];
    const first = elements[0], last = elements.at(-1);
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
  }
}
async function initialize() {
  fatal.value = '';
  clearInterval(monitorTick); unsub(); unsubClose();
  try {
    config.value = await api.load(); saved.value = JSON.stringify(config.value); info.value = await api.info();
    if (info.value.startupError) throw new Error(info.value.startupError);
    unsub = await api.subscribe(onEvent); unsubClose = await api.onClose(() => { modal.value = 'close'; });
    const snapshots = await api.snapshot(); for (const r of snapshots) { runs.value[r.workspaceId] = r; events.value[r.id] = await api.logs(r.id); }
    history.value = await api.history();
    for (const r of history.value) if (!runs.value[r.workspaceId]) { runs.value[r.workspaceId] = r; }
    if (currentRun.value && !events.value[currentRun.value.id]) events.value[currentRun.value.id] = await api.logs(currentRun.value.id);
    if (info.value.monitorRunId) {
      await refreshMonitoredRun(true);
      monitorTick = setInterval(() => { refreshMonitoredRun(); }, 1000);
    }
    await refreshSchedule(current.value?.id);
  } catch (e) { fatal.value = errorText(e); }
}
async function refreshMonitoredRun(select = false) {
  if (monitorPending) return;
  monitorPending = true;
  try {
    const snapshot = await api.monitored();
    if (!snapshot) throw new Error('没有找到关联的计划任务');
    const run = snapshot.run;
    runs.value[run.workspaceId] = run;
    events.value[run.id] = snapshot.events;
    if (select && config.value) {
      if (!config.value.workspaces.some(w => w.id === run.workspaceId)) throw new Error('计划任务对应的工作区已被删除');
      config.value.selectedId = run.workspaceId;
      const baseline = JSON.parse(saved.value) as AppConfig;
      baseline.selectedId = run.workspaceId; saved.value = JSON.stringify(baseline);
      page.value = 'workspace'; logCollapsed.value = false;
    }
    if (!active(run.status)) {
      history.value = [run, ...history.value.filter(r => r.id !== run.id)].slice(0, 100);
      clearInterval(monitorTick);
    }
    monitorError.value = '';
    if (config.value?.settings.autoScroll) { await nextTick(); logBody.value?.scrollTo({ top: logBody.value.scrollHeight }); }
  } catch (error) { monitorError.value = errorText(error); }
  finally { monitorPending = false; }
}
onMounted(() => { initialize(); tick = setInterval(() => { now.value = Date.now(); }, 1000); window.addEventListener('keydown', keydown); systemTheme.addEventListener('change', systemThemeChanged); });
onUnmounted(() => { clearInterval(tick); clearInterval(monitorTick); clearTimeout(toastTimer); unsub(); unsubClose(); window.removeEventListener('keydown', keydown); systemTheme.removeEventListener('change', systemThemeChanged); });
</script>

<template>
  <div v-if="fatal" class="boot-state"><CircleAlert :size="32" /><h1>无法载入工作区</h1><p>{{ fatal }}</p><button class="button primary" @click="initialize"><RefreshCw :size="16" />重新载入</button></div>
  <div v-else-if="!config || !current" class="boot-state"><LoaderCircle class="spin" :size="28" /><span>正在载入工作区</span></div>
  <div v-else class="app-shell" :class="{ 'logs-expanded': logExpanded, 'logs-collapsed': logCollapsed, 'empty-logs': !displayRun }">
    <aside class="sidebar">
      <div class="brand"><img src="/app-icon.png" alt="" /><div><strong>SolutionBat</strong><span>BUILD WORKSPACE <b>NEXT</b></span></div></div>
      <div class="workspace-list-heading"><span>工作区</span><span class="count">{{ config.workspaces.length }}</span><IconButton label="新建工作区" @click="beginNew"><Plus :size="17" /></IconButton></div>
      <label class="sidebar-search"><Search :size="14" /><input v-model="search" aria-label="搜索工作区" placeholder="搜索工作区" /></label>
      <nav class="workspace-list" aria-label="工作区列表">
        <button v-for="w in visibleWorkspaces" :key="w.id" class="workspace-item" :class="{ selected: w.id === current.id }" :title="w.name + '\n' + w.root" @click="choose(w)">
          <GitBranch :size="17" /><span><strong>{{ w.name }}</strong><small>{{ w.root || '尚未配置路径' }}</small></span>
          <span v-if="active(runs[w.id]?.status)" class="live-dot" />
        </button>
        <p v-if="!visibleWorkspaces.length" class="muted small search-empty">没有匹配的工作区</p>
      </nav>
      <div class="sidebar-divider" />
      <nav class="page-nav" aria-label="功能导航">
        <button :class="{ selected: page === 'workspace' }" @click="page = 'workspace'"><FolderOpen :size="17" />工作区配置</button>
        <button :class="{ selected: page === 'schedule' }" @click="page = 'schedule'"><Clock3 :size="17" />计划任务<span v-if="current.schedule.times.length" class="nav-count">{{ current.schedule.times.length }}</span></button>
        <button :class="{ selected: page === 'history' }" @click="page = 'history'"><History :size="17" />运行记录</button>
      </nav>
      <div class="sidebar-bottom">
        <button class="settings-nav" :class="{ selected: page === 'settings' }" @click="page = 'settings'"><Settings2 :size="17" />设置<ChevronRight :size="14" /></button>
        <div class="sidebar-foot"><span><i :class="{ live: allRunning.length }" />{{ desktop ? (allRunning.length ? `${allRunning.length} 个任务运行中` : '本地桌面端') : '浏览器预览' }}</span><span>v{{ info.version }}</span></div>
      </div>
    </aside>

    <main class="main-area" ref="mainArea">
      <header class="topbar">
        <div class="breadcrumb"><span>工作区</span><ChevronRight :size="14" /><strong :title="current.name">{{ current.name }}</strong><span class="change-dot" v-if="dirty" title="有未保存的更改" /></div>
        <div class="topbar-actions"><span class="engine-tag">UNREAL ENGINE</span><IconButton label="离线使用手册" @click="page = 'help'"><BookOpen :size="17" /></IconButton><IconButton label="导入配置" @click="importConfig"><ArrowDownToLine :size="17" /></IconButton><IconButton label="切换深浅主题" @click="setTheme(themeDark ? 'light' : 'dark')"><Sun v-if="themeDark" :size="17" /><Moon v-else :size="17" /></IconButton><IconButton class="mobile-settings" label="打开设置" @click="page = 'settings'"><Settings2 :size="17" /></IconButton></div>
      </header>
      <div v-if="!desktop" class="preview-banner"><Laptop :size="14" /><span>浏览器预览</span><span>本机编译与计划任务在桌面程序中运行</span></div>

      <div class="content-scroll" v-show="!logExpanded">
        <div class="page-heading">
          <div class="heading-title"><span class="eyebrow">{{ page === 'workspace' ? 'WORKSPACE' : page === 'schedule' ? 'SCHEDULE' : page === 'history' ? 'RUN HISTORY' : page === 'help' ? 'USER GUIDE' : 'PREFERENCES' }}</span>
            <div class="heading-name"><h1>{{ page === 'workspace' ? current.name : page === 'schedule' ? '计划任务' : page === 'history' ? '运行记录' : page === 'help' ? '使用手册' : '设置' }}</h1>
              <div class="menu-wrap" v-if="page === 'workspace'"><IconButton label="工作区操作" :active="menus" @click="menus = !menus"><Ellipsis :size="20" /></IconButton>
                <div v-if="menus" class="dropdown"><button @click="beginRename"><Pencil :size="15" />重命名</button><button @click="duplicate"><Copy :size="15" />复制工作区</button><button class="danger-text" @click="modal = 'delete'; menus = false"><Trash2 :size="15" />删除工作区</button></div>
              </div>
            </div>
            <div class="heading-meta" v-if="page === 'workspace'"><span v-if="currentRun?.id === info.monitorRunId" class="tag">计划任务</span><span class="tag">{{ current.build.platform }}</span><span>{{ current.build.configuration }}</span><span class="meta-divider" /><span>{{ current.paths.name }}{{ current.build.target === 'Game' ? '' : current.build.target }}</span></div>
            <div class="heading-meta" v-else><GitBranch :size="14" /><span>{{ current.name }}</span></div>
          </div>
          <div class="page-actions" v-if="page !== 'help'">
            <button v-if="!monitoredCurrent" class="button" :disabled="busy" @click="save"><Save :size="16" />保存<span class="unsaved" v-if="dirty" /></button>
            <template v-if="page === 'workspace'"><button v-if="!monitoredCurrent" class="button" :disabled="busy || running || !chosenCount" @click="start(true)"><ListChecks :size="16" />预演</button>
              <button v-if="running" class="button danger" :disabled="busy || currentRun?.status === 'cancelling'" @click="stop"><Square :size="15" />停止运行</button>
              <button v-else-if="!monitoredCurrent" class="button primary" :disabled="busy || !chosenCount" @click="start(false)"><Play :size="16" fill="currentColor" />保存并运行</button>
            </template>
          </div>
        </div>

        <div v-if="page === 'workspace'" class="workspace-layout">
          <fieldset class="config-column" :disabled="busy">
            <section class="form-section">
              <div class="section-heading"><span class="section-number">01</span><h2>项目路径</h2><button class="text-button" @click="checkEnvironment"><ShieldCheck :size="14" />检查环境</button></div>
              <PathField label="项目根目录" v-model="current.root" browse placeholder="选择项目所在目录" @update:model-value="pathsChanged" @browse="browseRoot" />
              <div class="section-inline"><label class="switch-label"><input class="switch" type="checkbox" v-model="current.autoPaths" @change="pathsChanged" />自动关联项目路径</label><button class="text-button" @click="showPaths = !showPaths">{{ showPaths ? '收起路径' : '展开路径' }}<ChevronDown :size="14" :class="{ rotated: showPaths }" /></button></div>
              <div v-if="showPaths" class="derived-paths">
                <PathField label="项目文件" v-model="current.paths.project" :readonly="current.autoPaths" browse @browse="browsePath('project', false)" />
                <PathField label="引擎目录" v-model="current.paths.engine" :readonly="current.autoPaths" browse @browse="browsePath('engine', true)" />
                <PathField label="TS 项目目录" v-model="current.paths.tsProject" :readonly="current.autoPaths" browse @browse="browsePath('tsProject', true)" />
                <PathField label="UE 命令行程序" v-model="current.paths.ueExe" :readonly="current.autoPaths" browse @browse="browsePath('ueExe', false)" />
              </div>
            </section>
            <section class="form-section">
              <div class="section-heading"><span class="section-number blue">02</span><h2>Perforce</h2><span class="section-note">版本控制</span></div>
              <div class="p4-grid"><label class="field"><span>P4 服务器</span><input v-model="current.p4.port" placeholder="ssl:server:1666" spellcheck="false" /></label><label class="field"><span>用户</span><input v-model="current.p4.user" spellcheck="false" /></label><label class="field"><span>P4 工作区</span><input v-model="current.p4.client" spellcheck="false" /></label></div>
              <div class="force-row"><label class="switch-label"><input class="switch" type="checkbox" v-model="current.p4.force" />强制同步</label><span v-if="current.p4.force" class="warning-inline"><CircleAlert :size="13" />可能覆盖未检出的本地更改</span></div>
            </section>
            <section class="form-section last-section">
              <div class="section-heading"><span class="section-number amber">03</span><h2>编译配置</h2><span class="section-note">Unreal Engine / TypeScript</span></div>
              <div class="build-grid"><label class="field"><span>项目名称</span><input v-model="current.paths.name" @input="pathsChanged" /></label><label class="field"><span>编译配置</span><select v-model="current.build.configuration" @change="pathsChanged"><option v-for="v in ['Development', 'DebugGame', 'Debug', 'Shipping', 'Test']" :key="v">{{ v }}</option></select></label><label class="field"><span>构建目标</span><select v-model="current.build.target"><option v-for="v in ['Editor', 'Game', 'Client', 'Server']" :key="v">{{ v }}</option></select></label><label class="field"><span>目标平台</span><select v-model="current.build.platform" @change="pathsChanged"><option v-for="v in ['Win64', 'Win64-arm64', 'Win64-arm64ec', 'Android', 'IOS', 'Linux']" :key="v">{{ v }}</option></select></label></div>
            </section>
          </fieldset>
          <aside class="task-column">
            <div v-if="monitoredCurrent" class="task-heading"><h2>计划任务进度</h2><span class="count">{{ Object.values(currentRun!.stepStatuses).filter(s => s === 'success').length }} / {{ currentRun!.steps.length }}</span></div>
            <div v-else class="task-heading"><h2>执行任务</h2><span class="count">{{ chosenCount }} / 6</span><div class="task-selection"><button class="text-button" aria-label="全选任务" :disabled="busy || running" @click="toggleAll()"><CheckCheck :size="14" />全选</button><button class="text-button" aria-label="反选任务" :disabled="busy || running" @click="toggleAll(true)"><ArrowLeftRight :size="14" />反选</button></div></div>
            <div v-if="monitoredCurrent" class="task-list" aria-label="计划任务执行步骤"><div v-for="step in currentRun!.steps" :key="step.id" class="task-row scheduled-step" :class="{ checked: currentRun!.stepStatuses[step.id] === 'success' }"><LoaderCircle v-if="['running', 'watching'].includes(currentRun!.stepStatuses[step.id])" :size="17" class="spin task-icon" /><CheckCircle2 v-else-if="currentRun!.stepStatuses[step.id] === 'success'" :size="17" class="task-icon" /><CircleAlert v-else-if="currentRun!.stepStatuses[step.id] === 'failed'" :size="17" class="danger-text" /><Circle v-else :size="17" class="muted" /><span><strong>{{ step.label }}</strong><small>{{ statusNames[currentRun!.stepStatuses[step.id]] || currentRun!.stepStatuses[step.id] }}</small></span></div></div>
            <fieldset v-else class="task-list" :disabled="busy || running">
              <label v-for="key in taskOrder" :key="key" class="task-row" :class="{ checked: current.tasks[key] }"><input type="checkbox" v-model="current.tasks[key]" :aria-label="taskNames[key]" /><component :is="taskIcons[key]" :size="17" class="task-icon" /><span><strong>{{ taskNames[key] }}</strong><small>{{ taskDetail[key] }}</small></span></label>
            </fieldset>
            <div class="flow-heading"><span>执行顺序</span><IconButton label="查看完整命令" @click="showPlan"><Terminal :size="15" /></IconButton></div>
            <div class="execution-flow"><template v-for="(label, i) in flow" :key="label"><ChevronRight v-if="i" :size="11" /><span>{{ label }}</span></template><span v-if="!flow.length" class="muted">未选择任务</span></div>
            <div class="schedule-summary">
              <div><Clock3 :size="15" /><span>计划任务</span><span class="status-dot" :class="{ enabled: schedule?.registered && schedule.state !== 'Disabled' && !scheduleError }" /></div>
              <p>{{ schedule?.registered ? schedule.times.join(' / ') || '--:--' : '--:--' }}</p>
              <small>{{ scheduleError ? '状态查询失败' : !schedule ? '正在检查' : schedule.registered ? schedule.state === 'Disabled' ? '已禁用' : '每天执行' : '未注册' }}</small>
              <button class="text-button" @click="page = 'schedule'">管理计划任务<ChevronRight :size="14" /></button>
            </div>
          </aside>
        </div>

        <div v-else-if="page === 'schedule'" class="secondary-page schedule-page">
          <SchedulePanel v-model="current.schedule.times" v-model:after-success="current.schedule.afterSuccess" :info="schedule" :error="scheduleError" :busy="busy" :launchers="launchers" :launchers-loading="launchersLoading" :launchers-error="launchersError" @register="changeSchedule('register')" @unregister="changeSchedule('unregister')" @refresh="refreshSchedule(current.id)" @browse-launch="browseLaunch" />
        </div>

        <div v-else-if="page === 'history'" class="secondary-page">
          <div class="history-heading"><span class="muted">{{ visibleHistory.length }} 次运行</span><button class="text-button" @click="perform(async () => { history = await api.history(); })"><RefreshCw :size="14" />刷新记录</button></div>
          <div class="history-table"><div class="history-row table-head"><span>运行时间</span><span>类型</span><span>状态</span><span>耗时</span><span /></div><button v-for="run in visibleHistory" :key="run.id" class="history-row" :class="{ selected: selectedHistory?.id === run.id }" @click="viewHistory(run)"><span>{{ date(run.startedAt) }}</span><span>{{ run.dryRun ? '命令预演' : run.scheduled ? '定时任务' : '手动运行' }}</span><span><span class="status-badge" :class="run.status">{{ statusNames[run.status] }}</span></span><span class="muted">{{ duration(run) }}</span><ChevronRight :size="15" /></button></div>
          <div v-if="!visibleHistory.length" class="empty-view"><History :size="36" /><strong>还没有运行记录</strong><span>{{ current.name }}</span></div>
        </div>

        <div v-else-if="page === 'help'" class="secondary-page"><HelpView @open-help="openHelp" /></div>
        <div v-else class="secondary-page settings-page">
          <section class="form-section"><div class="section-heading"><h2>运行日志</h2></div><div class="setting-row"><div class="setting-path"><strong>日志目录</strong><p>{{ info.logsDir || '桌面程序所在目录 / logs' }}</p></div><IconButton label="打开日志目录" @click="perform(() => api.openLogsDirectory())"><FolderOpen :size="17" /></IconButton></div></section>
          <section class="form-section"><div class="section-heading"><h2>帮助与关于</h2></div><div class="inline-actions help-actions"><button class="button" @click="openHelp"><CircleHelp :size="16" />在线帮助文档</button><button class="button" @click="page = 'help'"><BookOpen :size="16" />离线使用手册</button><button class="button" @click="modal = 'about'"><Info :size="16" />关于</button></div></section>
          <section class="form-section"><div class="section-heading"><h2>外观</h2></div><div class="setting-row"><strong>界面主题</strong><div class="segmented"><button :class="{ selected: config.settings.theme === 'light' }" @click="setTheme('light')"><Sun :size="15" />浅色</button><button :class="{ selected: config.settings.theme === 'dark' }" @click="setTheme('dark')"><Moon :size="15" />深色</button><button :class="{ selected: config.settings.theme === 'system' }" @click="setTheme('system')"><Laptop :size="15" />跟随系统</button></div></div><div class="setting-row"><strong>日志自动滚动</strong><input type="checkbox" class="switch" v-model="config.settings.autoScroll" aria-label="日志自动滚动" /></div></section>
          <section class="form-section"><div class="section-heading"><h2>配置与数据</h2></div><div class="setting-row"><div><strong>工作区配置</strong><p>{{ config.workspaces.length }} 个工作区</p></div><div class="inline-actions"><button class="button" @click="importConfig"><ArrowDownToLine :size="15" />导入</button><button class="button" @click="perform(async () => { await api.export(config!); })"><ArrowUpFromLine :size="15" />导出</button></div></div><div class="setting-row"><div class="setting-path"><strong>数据目录</strong><p :title="info.dataDir">{{ info.dataDir }}</p></div><IconButton label="打开数据目录" @click="perform(async () => { await api.openDirectory(); })"><FolderOpen :size="17" /></IconButton></div></section>
          <section class="form-section"><div class="section-heading"><h2>软件更新</h2></div><div class="setting-row"><div><strong>SolutionBat Next <span class="version-label">v{{ info.version }}</span></strong><p>{{ updateState || (info.updaterReady ? '更新服务已配置' : '更新服务尚未配置') }}</p></div><button class="button" :disabled="busy || allRunning.length > 0" @click="update"><RefreshCw :size="15" />检查更新</button></div></section>
          <div class="about-app"><img src="/app-icon.png" alt="" /><div><strong>SolutionBat Next</strong><span>MHAutoUpdateCompiler · 新版工作区</span></div><span>sharkgao</span></div>
        </div>
      </div>

      <section class="log-panel" id="runtime-logs" ref="logPanel" :style="panelStyle">
        <div v-if="!logExpanded && !logCollapsed" class="log-resize-handle" :class="{ dragging: resizing }" role="separator" tabindex="0" aria-label="调整日志高度" aria-controls="runtime-logs" aria-orientation="horizontal" :aria-valuemin="minHeight" :aria-valuemax="maxHeight" :aria-valuenow="actualHeight" title="拖动调整日志高度，双击恢复默认" @pointerdown="startResize" @pointermove="moveResize" @pointerup="endResize" @pointercancel="cancelResize" @lostpointercapture="cancelResize" @keydown="resizeKeydown" @dblclick="resetHeight" />
        <div class="log-toolbar"><div class="log-title"><Terminal :size="16" /><strong>运行日志</strong><span v-if="displayRun" class="status-badge" :class="displayRun.status">{{ displayRun.dryRun ? '预演 · ' : '' }}{{ statusNames[displayRun.status] }}</span><span v-if="displayRun" class="elapsed">{{ duration(displayRun) }}</span></div>
          <div class="log-controls"><label class="log-search"><Search :size="13" /><input v-model="logSearch" aria-label="搜索日志" placeholder="搜索日志" /></label><select v-model="logLevel" aria-label="日志级别"><option value="all">全部</option><option value="warning">警告</option><option value="error">错误</option></select><IconButton label="复制日志" :disabled="!logs.length" @click="copyLogs"><Copy :size="14" /></IconButton><IconButton label="导出日志" :disabled="!displayRun" @click="perform(async () => { if (displayRun) await api.exportLogs(displayRun.id); })"><Download :size="15" /></IconButton><IconButton :label="logExpanded ? '还原日志区' : '展开日志区'" @click="logExpanded = !logExpanded; logCollapsed = false"><Expand :size="15" /></IconButton><IconButton :label="logCollapsed ? '显示日志' : '折叠日志'" @click="logCollapsed = !logCollapsed; logExpanded = false"><ChevronDown v-if="logCollapsed" :size="15" /><Minus v-else :size="15" /></IconButton></div>
        </div>
        <div v-if="displayRun && !logCollapsed" class="run-stages"><span v-for="step in displayRun.steps" :key="step.id" :class="displayRun.stepStatuses[step.id]"><LoaderCircle v-if="['running', 'watching'].includes(displayRun.stepStatuses[step.id])" :size="12" class="spin" /><CheckCircle2 v-else-if="displayRun.stepStatuses[step.id] === 'success'" :size="12" /><CircleAlert v-else-if="displayRun.stepStatuses[step.id] === 'failed'" :size="12" /><Circle v-else :size="11" />{{ step.label }}</span></div>
        <div v-if="monitorError || displayRun?.error" class="run-error" role="alert"><CircleAlert :size="15" /><span>{{ monitorError || displayRun?.error }}</span></div>
        <div class="log-body" v-show="!logCollapsed" ref="logBody" role="log" aria-label="运行输出"><div v-for="(entry, i) in logs" :key="entry.runId + i" class="log-line" :class="entry.level"><time>{{ time(entry.time) }}</time><span class="log-level">{{ entry.level === 'error' ? 'ERR' : entry.level === 'warning' ? 'WARN' : 'INFO' }}</span><pre>{{ entry.message }}</pre></div>
          <div v-if="!logs.length" class="log-empty"><Terminal :size="25" /><div><strong>{{ displayRun ? '没有匹配的日志' : '等待任务运行' }}</strong><span>{{ displayRun ? displayRun.workspaceName : '尚无运行输出' }}</span></div></div>
        </div>
      </section>
      <footer class="statusbar"><span><span class="status-dot" :class="{ enabled: running }" />{{ running ? statusNames[currentRun!.status] : '空闲' }}</span><span class="statusbar-center">{{ current.root }}</span><span>{{ dirty ? '有未保存的更改' : '所有配置已保存' }}<Check v-if="!dirty" :size="12" /></span></footer>
    </main>

    <div v-if="toast" class="toast" :class="{ error: toast.error }" role="status"><CircleAlert v-if="toast.error" :size="18" /><CheckCircle2 v-else :size="18" /><span>{{ toast.text }}</span><IconButton label="关闭通知" @click="toast = undefined"><X :size="15" /></IconButton></div>
    <div v-if="modal" class="modal-backdrop" @click.self="modal = ''"><section class="modal" :class="{ wide: modal === 'environment' || modal === 'plan' }" role="dialog" aria-modal="true" :aria-label="modal === 'new' ? '新建工作区' : modal === 'rename' ? '重命名工作区' : modal === 'environment' ? '环境检查' : modal === 'plan' ? '执行命令' : '确认操作'">
      <header><h2>{{ modal === 'new' ? '新建工作区' : modal === 'rename' ? '重命名工作区' : modal === 'delete' ? '删除工作区' : modal === 'environment' ? '环境检查' : modal === 'plan' ? '执行命令' : modal === 'close' ? '仍有任务运行中' : modal === 'about' ? '关于' : '确认强制同步' }}</h2><IconButton label="关闭对话框" @click="modal = ''"><X :size="18" /></IconButton></header>
      <form v-if="modal === 'new' || modal === 'rename'" @submit.prevent="acceptName"><label class="field"><span>工作区名称</span><input v-model="modalName" aria-label="工作区名称" placeholder="例如 MHA_Client_main" maxlength="80" /></label><label v-if="modal === 'new'" class="field"><span>项目根目录</span><input v-model="modalRoot" aria-label="新工作区根目录" placeholder="K:\MHA_Client_main" /></label><footer><button type="button" class="button" @click="modal = ''">取消</button><button class="button primary" :disabled="busy || !modalName.trim()">{{ modal === 'new' ? '创建工作区' : '保存名称' }}</button></footer></form>
      <template v-else-if="modal === 'environment'"><div class="checks"><div v-for="check in checks" :key="check.name" class="check-row"><CheckCircle2 v-if="check.ok" :size="18" class="success-text" /><CircleAlert v-else :size="18" :class="check.required ? 'danger-text' : 'muted'" /><div><strong>{{ check.name }}<small v-if="!check.required">可选</small></strong><p>{{ check.path }}</p></div><span>{{ check.ok ? '已就绪' : '未找到' }}</span></div></div><footer><button class="button" @click="modal = ''">关闭</button></footer></template>
      <template v-else-if="modal === 'plan'"><div class="command-list"><div v-for="(step, i) in steps" :key="step.id"><h3><span>{{ String(i + 1).padStart(2, '0') }}</span>{{ step.label }}</h3><code>{{ step.program }} {{ step.args.map(a => JSON.stringify(a)).join(' ') }}</code><p>{{ step.cwd }}</p></div></div><footer><button class="button" @click="modal = ''">关闭</button></footer></template>
      <template v-else-if="modal === 'delete'"><p>删除工作区「{{ current.name }}」的配置？项目文件不会删除。</p><footer><button class="button" @click="modal = ''">取消</button><button class="button danger" :disabled="busy || config.workspaces.length === 1" @click="removeWorkspace">删除工作区</button></footer></template>
      <template v-else-if="modal === 'close'"><p>有 {{ allRunning.length }} 个任务或 TS Watch 正在运行。停止任务后即可关闭窗口。</p><footer><button class="button" @click="modal = ''">继续运行</button><button class="button danger" :disabled="busy" @click="stopAll">停止所有任务</button></footer></template>
      <template v-else-if="modal === 'about'"><div class="about-dialog"><img src="/app-icon.png" alt="" /><h3>SolutionBat Next <span>v{{ info.version }}</span></h3><p>MHAutoUpdateCompiler</p><p>Perforce 同步、UE 项目编译、TypeScript 编译与 Windows 计划任务管理。</p><p>作者：sharkgao</p><p class="muted">Tauri / Vue / TypeScript / Rust</p></div><footer><button class="button" @click="openHelp"><CircleHelp :size="15" />帮助文档</button><button class="button" @click="modal = ''">关闭</button></footer></template>
      <template v-else><p>将对「{{ current.p4.client }}」执行强制同步，可能覆盖未检出的本地修改。</p><footer><button class="button" @click="modal = ''">取消</button><button class="button danger" :disabled="busy" @click="start(false, true)">确认并运行</button></footer></template>
    </section></div>
  </div>
</template>
