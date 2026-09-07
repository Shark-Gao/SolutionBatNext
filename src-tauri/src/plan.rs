use crate::model::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub watch: bool,
    pub kind: String,
}
impl Step {
    fn new(id: &str, label: &str, program: &str, args: Vec<String>, cwd: &str, kind: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            program: program.into(),
            args,
            cwd: cwd.into(),
            env: BTreeMap::new(),
            watch: false,
            kind: kind.into(),
        }
    }
}

pub const CLOSE_EDITOR: &str = r#"
$ErrorActionPreference = 'Stop'
$ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$matches = @(Get-CimInstance Win32_Process -Filter "Name = 'UnrealEditor.exe' OR Name = 'UnrealEditor-Win64-DebugGame.exe'" | Where-Object {
  (Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid).Sid -eq $ownerSid
})
foreach ($p in $matches) {
  # A leftover Rider debugger can keep a terminated editor and its DLLs alive.
  $parent = Get-CimInstance Win32_Process -Filter ('ProcessId = ' + $p.ParentProcessId)
  if ($parent -and $parent.Name -eq 'LLDBFrontend.exe' -and
      $parent.ExecutablePath -like '*\Rider*\plugins\cidr-debugger-plugin\*' -and
      (Invoke-CimMethod -InputObject $parent -MethodName GetOwnerSid).Sid -eq $ownerSid) {
    $debugger = Get-Process -Id $parent.ProcessId -ErrorAction SilentlyContinue
    if ($debugger -and $debugger.Path -eq $parent.ExecutablePath) {
      Stop-Process -InputObject $debugger -Force -ErrorAction Stop
      if (-not $debugger.WaitForExit(15000)) { throw ('Rider debugger did not exit: ' + $parent.ProcessId) }
      Write-Output ('Closed attached Rider debugger ' + $parent.ProcessId)
    }
  }
  $editor = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
  if ($editor -and $editor.Path -eq $p.ExecutablePath) {
    Stop-Process -InputObject $editor -Force -ErrorAction Stop
    if (-not $editor.WaitForExit(15000)) { throw ('UE process did not exit: ' + $p.ProcessId) }
  }
  $remaining = Get-CimInstance Win32_Process -Filter ('ProcessId = ' + $p.ProcessId)
  if ($remaining -and $remaining.CreationDate -eq $p.CreationDate) { throw ('UE process is still releasing files: ' + $p.ProcessId + '. Retry after it exits.') }
  Write-Output ('Closed UE process ' + $p.ProcessId)
}
if ($matches.Count -eq 0) { Write-Output 'No UE process for the current user.' }
"#;

pub const CLOSE_RIDER: &str = r#"
$ErrorActionPreference = 'Stop'
$ownerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$processes = @(Get-CimInstance Win32_Process -Filter "Name = 'rider64.exe' OR Name = 'Rider.Backend.exe'" | Where-Object {
  (Invoke-CimMethod -InputObject $_ -MethodName GetOwnerSid).Sid -eq $ownerSid
})
foreach ($p in $processes) {
  $rider = Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue
  if ($rider -and $rider.Path -eq $p.ExecutablePath) {
    Stop-Process -InputObject $rider -Force -ErrorAction Stop
    if (-not $rider.WaitForExit(15000)) { throw ('Rider process did not exit: ' + $p.ProcessId) }
  }
  Write-Output ('Closed Rider process ' + $p.ProcessId)
}
"#;

pub const WAIT_UBT: &str = r#"
$ErrorActionPreference = 'Stop'
while (Get-Process -Name UnrealBuildTool -ErrorAction SilentlyContinue) {
    Write-Output 'Waiting for UnrealBuildTool to finish...'
    Start-Sleep -Seconds 5
}
"#;

pub fn effective_tasks(w: &Workspace, scheduled: bool) -> Tasks {
    if scheduled {
        // The original --run-task entry point always runs the full daily build.
        Tasks {
            sync: true,
            build: true,
            typescript: true,
            close_editor: true,
            ..Tasks::default()
        }
    } else {
        w.tasks.clone()
    }
}

pub fn plan(w: &Workspace, scheduled: bool) -> Result<Vec<Step>, String> {
    w.validate()?;
    if w.root.trim().is_empty() {
        return Err("请先填写项目根目录".into());
    }
    let t = effective_tasks(w, scheduled);
    let mut steps = Vec::new();
    if scheduled {
        steps.push(Step::new(
            "close-rider",
            "关闭 Rider",
            "powershell.exe",
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                CLOSE_RIDER.into(),
            ],
            &w.root,
            "process",
        ));
    }
    if t.close_editor {
        let step = Step::new(
            "close-editor",
            "关闭 UE 进程",
            "powershell.exe",
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                CLOSE_EDITOR.into(),
            ],
            &w.root,
            "process",
        );
        steps.push(step);
    }
    if t.sync {
        if w.p4.port.is_empty() || w.p4.user.is_empty() || w.p4.client.is_empty() {
            return Err("请完整填写 P4 服务器、用户和工作区".into());
        }
        let base = vec![
            "-p".into(),
            w.p4.port.clone(),
            "-u".into(),
            w.p4.user.clone(),
            "-c".into(),
            w.p4.client.clone(),
        ];
        let mut info = base.clone();
        info.push("info".into());
        steps.push(Step::new(
            "p4-info",
            "检查 P4 连接",
            "p4.exe",
            info,
            &w.root,
            "p4",
        ));
        let mut sync = Step::new(
            "p4-sync",
            "同步 P4 工作区",
            "powershell.exe",
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                crate::p4::SYNC_SCRIPT.into(),
            ],
            &w.root,
            "p4",
        );
        sync.env.insert("SB_P4_REQUEST".into(), serde_json::json!({"port":w.p4.port,"user":w.p4.user,"client":w.p4.client,"force":w.p4.force,"root":w.root}).to_string());
        steps.push(sync);
    }
    if t.build {
        steps.push(Step::new(
            "wait-ubt",
            "等待 UnrealBuildTool",
            "powershell.exe",
            vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                WAIT_UBT.into(),
            ],
            &w.root,
            "process",
        ));
        let gen = win_join(&w.root, "GenerateProjectFiles_MHMobile.bat");
        if Path::new(&gen).is_file() {
            steps.push(Step::new(
                "generate-project",
                "生成 UE 工程文件",
                &gen,
                vec![],
                &w.root,
                "batch",
            ));
        }
        let target = if w.build.target == "Game" {
            w.paths.name.clone()
        } else {
            format!("{}{}", w.paths.name, w.build.target)
        };
        let (platform, arch) = match w.build.platform.as_str() {
            "Win64-arm64" => ("Win64", "arm64"),
            "Win64-arm64ec" => ("Win64", "arm64ec"),
            p => (p, "x64"),
        };
        let mut args = vec![
            target,
            platform.into(),
            w.build.configuration.clone(),
            format!("-Project={}", w.paths.project),
            "-WaitMutex".into(),
            "-FromMsBuild".into(),
        ];
        if platform == "Win64" {
            args.push(format!("-architecture={arch}"));
        }
        steps.push(Step::new(
            "ue-build",
            "编译 UE 工程",
            &win_join(&w.paths.engine, "Build\\BatchFiles\\Build.bat"),
            args,
            &w.root,
            "batch",
        ));
    }
    if t.typescript || t.definitions {
        steps.push(Step::new(
            "ts-definitions",
            "生成 TypeScript 定义",
            &w.paths.ue_exe,
            vec![
                w.paths.project.clone(),
                "-Run=MHEditor".into(),
                "-Gen_UE_D_TS".into(),
                "-unattended".into(),
                "-nopause".into(),
                "-UTF8Output".into(),
            ],
            &w.root,
            "definitions",
        ));
    }
    if t.typescript || t.watch {
        // Use the project's installed compiler. Never download a different tsc during a build.
        let mut args = vec![
            win_join(&w.paths.ts_project, "node_modules\\typescript\\bin\\tsc"),
            "-p".into(),
            "tsconfig.json".into(),
            "--pretty".into(),
            "false".into(),
        ];
        args.push("--watch".into());
        let mut step = Step::new(
            "ts-build",
            "编译 TS 并启动 Watch（独立窗口）",
            "node.exe",
            args,
            &w.paths.ts_project,
            "detached-watch",
        );
        step.watch = true;
        validate_batch(&step)?;
        step.env
            .insert("NODE_OPTIONS".into(), "--max-old-space-size=8192".into());
        steps.push(step);
    }
    if steps.is_empty() {
        return Err("请至少选择一项任务".into());
    }
    Ok(steps)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    pub path: String,
    pub ok: bool,
    pub required: bool,
}

pub fn diagnostics(w: &Workspace) -> Vec<Check> {
    diagnostics_for(w, false)
}

pub fn diagnostics_for(w: &Workspace, scheduled: bool) -> Vec<Check> {
    let t = effective_tasks(w, scheduled);
    let mut checks = Vec::new();
    for (name, path, required, file) in [
        ("项目根目录", w.root.clone(), true, false),
        (
            "UE 项目文件",
            w.paths.project.clone(),
            t.build || t.typescript || t.definitions,
            true,
        ),
        (
            "UE 构建脚本",
            win_join(&w.paths.engine, "Build\\BatchFiles\\Build.bat"),
            t.build,
            true,
        ),
        (
            "UE 命令行程序",
            w.paths.ue_exe.clone(),
            t.typescript || t.definitions,
            true,
        ),
        (
            "TS 项目目录",
            w.paths.ts_project.clone(),
            t.typescript || t.watch || t.definitions,
            false,
        ),
        (
            "TypeScript 配置",
            win_join(&w.paths.ts_project, "tsconfig.json"),
            t.typescript || t.watch,
            true,
        ),
        (
            "TypeScript 编译器",
            win_join(&w.paths.ts_project, "node_modules\\typescript\\bin\\tsc"),
            t.typescript || t.watch,
            true,
        ),
    ] {
        let ok = if file {
            Path::new(&path).is_file()
        } else {
            !path.is_empty() && Path::new(&path).is_dir()
        };
        checks.push(Check {
            name: name.into(),
            path,
            ok,
            required,
        });
    }
    for (name, required) in [("p4.exe", t.sync), ("node.exe", t.typescript || t.watch)] {
        let found = std::env::var_os("PATH").and_then(|p| {
            std::env::split_paths(&p)
                .map(|dir| dir.join(name))
                .find(|p| p.is_file())
        });
        checks.push(Check {
            name: name.into(),
            path: found
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "PATH".into()),
            ok: found.is_some(),
            required,
        });
    }
    checks
}

pub fn validate_batch(step: &Step) -> Result<(), String> {
    for text in std::iter::once(&step.program).chain(step.args.iter()) {
        if text.chars().any(|c| "\"%!?&|<>^\r\n".contains(c)) {
            return Err("批处理命令的路径和参数不能包含引号或命令解释符（例如 &、%、!）".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn workspace() -> Workspace {
        let mut w = Workspace::new("test");
        w.root = r"K:\Project with spaces".into();
        w.derive_paths();
        w
    }
    #[test]
    fn deduplicates_interactive_ts_tasks() {
        let mut w = workspace();
        w.tasks.typescript = true;
        w.tasks.definitions = true;
        w.tasks.watch = true;
        let interactive = plan(&w, false).unwrap();
        assert_eq!(interactive.len(), 2);
        assert!(interactive[1].watch);
    }
    #[test]
    fn scheduled_build_matches_legacy_regardless_of_manual_selection() {
        let mut w = workspace();
        w.p4 = P4 {
            port: "ssl:example:1666".into(),
            user: "user".into(),
            client: "branch".into(),
            force: false,
        };
        w.schedule.close_rider = false;
        let original = serde_json::to_string(&w).unwrap();
        let scheduled = plan(&w, true).unwrap();
        let ids: Vec<_> = scheduled.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "close-rider",
                "close-editor",
                "p4-info",
                "p4-sync",
                "wait-ubt",
                "ue-build",
                "ts-definitions",
                "ts-build"
            ]
        );
        let watch = scheduled.last().unwrap();
        assert!(watch.watch);
        assert_eq!(watch.kind, "detached-watch");
        assert!(watch.args.contains(&"--watch".into()));
        assert_eq!(serde_json::to_string(&w).unwrap(), original);
        w.tasks = Tasks {
            watch: true,
            ..Tasks::default()
        };
        assert_eq!(plan(&w, true).unwrap().len(), scheduled.len());
        let required: Vec<_> = diagnostics_for(&w, true)
            .into_iter()
            .filter(|c| c.required)
            .map(|c| c.name)
            .collect();
        assert!(required.contains(&"UE 构建脚本".into()));
        assert!(required.contains(&"p4.exe".into()));
    }
    #[test]
    fn p4_explicitly_scopes_server_user_and_workspace() {
        let mut w = workspace();
        w.tasks.sync = true;
        w.p4 = P4 {
            port: "ssl:example:1666".into(),
            user: "user".into(),
            client: "branch".into(),
            force: false,
        };
        let p = plan(&w, false).unwrap();
        assert_eq!(
            p[0].args,
            vec![
                "-p",
                "ssl:example:1666",
                "-u",
                "user",
                "-c",
                "branch",
                "info"
            ]
        );
        let request: serde_json::Value = serde_json::from_str(&p[1].env["SB_P4_REQUEST"]).unwrap();
        assert_eq!(request["client"], "branch");
        assert_eq!(request["port"], "ssl:example:1666");
        assert_eq!(request["force"], false);
    }
    #[test]
    fn rejects_shell_injection_but_accepts_spaces() {
        let mut w = workspace();
        w.tasks.build = true;
        let mut p = plan(&w, false).unwrap();
        assert!(validate_batch(p.last().unwrap()).is_ok());
        p.last_mut().unwrap().args.push("x & calc.exe".into());
        assert!(validate_batch(p.last().unwrap()).is_err());
    }
}
