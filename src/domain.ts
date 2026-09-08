import type { AppConfig, TaskKey, Workspace } from './types';

export const taskNames: Record<TaskKey, string> = {
  closeEditor: '关闭 UE 进程', sync: '更新 P4 全工程', build: '编译 UE 工程', typescript: '生成并构建 TS', definitions: '生成 TS 定义', watch: '编译 TS 并启动 Watch',
};
export const statusNames: Record<string, string> = {
  running: '运行中', watching: '监听中', cancelling: '正在停止', success: '已完成', failed: '失败', cancelled: '已停止', pending: '待执行', skipped: '已跳过',
};
export const taskOrder: TaskKey[] = ['closeEditor', 'sync', 'build', 'typescript', 'definitions', 'watch'];
export const join = (root: string, rest: string) => root ? `${root.replace(/[\\/]+$/, '')}\\${rest}` : '';
export function derivePaths(w: Workspace) {
  w.paths.project = join(w.root, `MHAGame\\${w.paths.name}.uproject`);
  w.paths.engine = join(w.root, 'Engine');
  w.paths.tsProject = join(w.root, 'MHAGame\\TsProj');
  const suffix = w.build.configuration === 'Development' ? '' : `Win64-${w.build.configuration}-`;
  w.paths.ueExe = join(w.paths.engine, `Binaries\\Win64\\UnrealEditor-${suffix}Cmd.exe`);
}
export function newWorkspace(name: string, root = ''): Workspace {
  const w: Workspace = {
    id: crypto.randomUUID(), name, root,
    p4: { port: '', user: '', client: '', force: false },
    paths: { name: 'MHMobile', project: '', engine: '', tsProject: '', ueExe: '' },
    build: { configuration: 'Development', target: 'Editor', platform: 'Win64' },
    tasks: { closeEditor: false, sync: false, build: false, typescript: false, definitions: false, watch: false },
    schedule: { times: [], closeRider: false, afterSuccess: { action: 'none', version: '2024.3.10', executable: '' } }, autoPaths: true,
  };
  derivePaths(w); return w;
}
export function validateWorkspace(w: Workspace): string | null {
  if (!w.name.trim()) return '请输入工作区名称';
  if (w.schedule.times.some(t => !/^([01]\d|2[0-3]):[0-5]\d$/.test(t))) return '请输入有效的触发时间';
  if (new Set(w.schedule.times).size !== w.schedule.times.length) return '触发时间不能重复';
  if (!['none', 'rider', 'visualStudio'].includes(w.schedule.afterSuccess.action)) return '计划任务成功后的开发工具选项无效';
  return null;
}
export function mergeWorkspaces(config: AppConfig, incoming: Workspace[]): number {
  const names = new Set(config.workspaces.map(w => w.name.toLowerCase()));
  for (const workspace of incoming) {
    const base = workspace.name; let number = 2;
    while (names.has(workspace.name.toLowerCase())) workspace.name = `${base} (${number++})`;
    workspace.id = crypto.randomUUID(); names.add(workspace.name.toLowerCase()); config.workspaces.push(workspace);
  }
  return incoming.length;
}
export function active(status?: string) { return !!status && ['running', 'watching', 'cancelling'].includes(status); }
export function executionLabels(w: Workspace): string[] {
  const t = w.tasks; const labels: string[] = [];
  if (t.closeEditor) labels.push('关闭 UE'); if (t.sync) labels.push('P4 同步'); if (t.build) labels.push('UE 编译');
  if (t.typescript || t.definitions) labels.push('TS 定义'); if (t.typescript || t.watch) labels.push('TS 编译 + Watch');
  return labels;
}
