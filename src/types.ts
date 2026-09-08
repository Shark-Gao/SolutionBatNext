export interface Workspace {
  id: string;
  name: string;
  root: string;
  p4: { port: string; user: string; client: string; force: boolean };
  paths: { project: string; name: string; engine: string; tsProject: string; ueExe: string };
  build: { configuration: string; target: string; platform: string };
  tasks: { sync: boolean; build: boolean; typescript: boolean; definitions: boolean; watch: boolean; closeEditor: boolean };
  schedule: { times: string[]; closeRider: boolean; afterSuccess: { action: 'none' | 'rider' | 'visualStudio'; version: string; executable: string } };
  autoPaths: boolean;
}
export type TaskKey = keyof Workspace['tasks'];
export interface AppConfig {
  schemaVersion: number;
  revision: number;
  selectedId: string;
  workspaces: Workspace[];
  settings: { theme: 'light' | 'dark' | 'system'; autoScroll: boolean; riderPath: string };
}
export interface Step { id: string; label: string; program: string; args: string[]; cwd: string; env: Record<string, string>; watch: boolean; kind: string }
export interface RunInfo {
  id: string; workspaceId: string; workspaceName: string; status: string; startedAt: string; endedAt: string | null;
  dryRun: boolean; scheduled: boolean; steps: Step[]; stepStatuses: Record<string, string>; error: string | null;
}
export interface RunEvent { runId: string; workspaceId: string; time: string; level: string; message: string; stepId: string | null; run: RunInfo | null }
export interface ScheduleInfo { registered: boolean; name: string; nextRun: string | null; lastResult: number | null; legacyRegistered: boolean; times: string[]; state: string | null; executable: string | null; previousRegistered: boolean }
export interface EnvironmentCheck { name: string; path: string; ok: boolean; required: boolean }
export interface InstalledLauncher { version: string; executable: string }
export interface AppInfo { version: string; dataDir: string; logsDir?: string; updaterReady: boolean; monitorRunId?: string | null; startupError?: string | null }
export interface MonitoredRun { run: RunInfo; events: RunEvent[]; workerPid: number }
