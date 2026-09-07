import { describe, expect, it } from 'vitest';
import { derivePaths, executionLabels, mergeWorkspaces, newWorkspace, validateWorkspace } from '../src/domain';
import type { AppConfig } from '../src/types';

describe('workspace behavior', () => {
  it('uses the Windows host editor for cross platform builds', () => {
    const w = newWorkspace('branch', 'K:\\A Project'); w.build.platform = 'Android'; w.build.configuration = 'DebugGame'; derivePaths(w);
    expect(w.paths.ueExe).toBe('K:\\A Project\\Engine\\Binaries\\Win64\\UnrealEditor-Win64-DebugGame-Cmd.exe');
  });
  it('does not duplicate TypeScript stages when all switches are selected', () => {
    const w = newWorkspace('branch'); w.tasks.typescript = w.tasks.definitions = w.tasks.watch = true;
    expect(executionLabels(w)).toEqual(['TS 定义', 'TS 编译 + Watch']);
  });
  it('merges imported profiles without replacing existing names', () => {
    const w = newWorkspace('branch');
    const config: AppConfig = { schemaVersion: 1, revision: 0, workspaces: [w], selectedId: w.id, settings: { theme: 'light', autoScroll: true, riderPath: '' } };
    mergeWorkspaces(config, [newWorkspace('branch'), newWorkspace('branch')]);
    expect(config.workspaces.map(w => w.name)).toEqual(['branch', 'branch (2)', 'branch (3)']);
    expect(new Set(config.workspaces.map(w => w.id)).size).toBe(3);
  });
  it('rejects empty names and duplicate schedules', () => {
    const w = newWorkspace(''); expect(validateWorkspace(w)).toBeTruthy(); w.name = 'branch';
    w.schedule.times = ['03:00', '03:00']; expect(validateWorkspace(w)).toBeTruthy();
    w.schedule.times = ['03:00', '05:30']; expect(validateWorkspace(w)).toBeNull();
  });
});
