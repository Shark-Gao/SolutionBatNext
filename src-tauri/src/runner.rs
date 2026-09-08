use crate::{
    model::{AfterSuccess, Workspace},
    plan::{self, Step},
    storage,
};
use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    hash::{Hash, Hasher},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

pub type Sink = Arc<dyn Fn(RunEvent) + Send + Sync>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunInfo {
    pub id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub status: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub dry_run: bool,
    pub scheduled: bool,
    pub steps: Vec<Step>,
    pub step_statuses: BTreeMap<String, String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunEvent {
    pub run_id: String,
    pub workspace_id: String,
    pub time: String,
    pub level: String,
    pub message: String,
    pub step_id: Option<String>,
    pub run: Option<RunInfo>,
}

pub struct Job {
    pub info: Mutex<RunInfo>,
    pub cancel: AtomicBool,
    pub logs: Mutex<VecDeque<RunEvent>>,
    file: Mutex<File>,
    text_file: Mutex<File>,
    sink: Sink,
}
impl Job {
    pub fn log(
        &self,
        level: &str,
        message: impl Into<String>,
        step: Option<&str>,
        with_state: bool,
    ) {
        let info = self.info.lock().unwrap().clone();
        let event = RunEvent {
            run_id: info.id,
            workspace_id: info.workspace_id,
            time: Utc::now().to_rfc3339(),
            level: level.into(),
            message: message.into(),
            step_id: step.map(str::to_string),
            run: if with_state {
                Some(self.info.lock().unwrap().clone())
            } else {
                None
            },
        };
        let mut logs = self.logs.lock().unwrap();
        logs.push_back(event.clone());
        if logs.len() > 1200 {
            logs.pop_front();
        }
        drop(logs);
        if let Ok(mut file) = self.file.lock() {
            if file
                .metadata()
                .map(|m| m.len() < 32 * 1024 * 1024)
                .unwrap_or(false)
            {
                if let Ok(line) = serde_json::to_string(&event) {
                    let _ = writeln!(file, "{line}");
                }
            }
        }
        if let Ok(mut file) = self.text_file.lock() {
            if FileExt::lock_exclusive(&*file).is_ok() {
                let _ = writeln!(
                    file,
                    "[{}] [{}] [{}] {}",
                    event.time, event.level, event.workspace_id, event.message
                );
                let _ = FileExt::unlock(&*file);
            }
        }
        (self.sink)(event);
    }
    fn step_status(&self, step: &Step, status: &str) {
        {
            let mut info = self.info.lock().unwrap();
            info.step_statuses.insert(step.id.clone(), status.into());
            if status == "watching" {
                info.status = "watching".into();
            }
        }
        self.log(
            "info",
            format!("{} · {}", step.label, status),
            Some(&step.id),
            true,
        );
    }
    pub fn active(&self) -> bool {
        matches!(
            self.info.lock().unwrap().status.as_str(),
            "running" | "watching" | "cancelling"
        )
    }
}

#[derive(Default)]
pub struct Runtime {
    pub jobs: Mutex<HashMap<String, Arc<Job>>>,
}

pub fn hidden(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

#[cfg(windows)]
struct ChildTree(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl ChildTree {
    fn attach(child: &Child) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().to_string());
            }
            let guard = Self(handle);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle() as _) == 0
            {
                return Err(std::io::Error::last_os_error().to_string());
            }
            Ok(guard)
        }
    }
}
#[cfg(windows)]
impl Drop for ChildTree {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
#[cfg(not(windows))]
struct ChildTree;
#[cfg(not(windows))]
impl ChildTree {
    fn attach(_: &Child) -> Result<Self, String> {
        Ok(Self)
    }
}

pub(crate) fn decode(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes)
        .map(str::to_string)
        .unwrap_or_else(|_| encoding_rs::GBK.decode(bytes).0.into_owned())
}

fn stream<R: std::io::Read + Send + 'static>(
    reader: R,
    job: Arc<Job>,
    step: String,
    level: &'static str,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let text = decode(&line[..line.len().min(32_768)])
                .trim_end()
                .to_string();
            if !text.is_empty() {
                job.log(level, text, Some(&step), false);
            }
        }
    })
}

fn command_for(step: &Step) -> Result<Command, String> {
    let mut command = if step.kind == "batch" {
        plan::validate_batch(step)?;
        let mut c = Command::new("cmd.exe");
        c.arg("/D").arg("/S").arg("/C");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let args = step
                .args
                .iter()
                .map(|a| format!("\"{a}\""))
                .collect::<Vec<_>>()
                .join(" ");
            c.raw_arg(format!("\"\"{}\" {}\"", step.program, args));
        }
        #[cfg(not(windows))]
        {
            return Err("UE 批处理编译仅支持 Windows".into());
        }
        c
    } else {
        let mut c = Command::new(&step.program);
        c.args(&step.args);
        c
    };
    hidden(&mut command)
        .current_dir(&step.cwd)
        .envs(&step.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(command)
}

fn execute(step: &Step, job: &Arc<Job>) -> Result<(), String> {
    if step.kind == "detached-watch" {
        let child = launch_watch(step)?;
        job.log(
            "info",
            format!("TypeScript Watch 已在独立窗口启动，进程 {}", child.id()),
            Some(&step.id),
            false,
        );
        return Ok(());
    }
    let mut command = command_for(step)?;
    let mut child = command
        .spawn()
        .map_err(|e| format!("无法启动 {}：{e}", step.program))?;
    let tree = match ChildTree::attach(&child) {
        Ok(tree) => tree,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("无法管理子进程：{e}"));
        }
    };
    let stdout = stream(
        child.stdout.take().unwrap(),
        job.clone(),
        step.id.clone(),
        "info",
    );
    let stderr = stream(
        child.stderr.take().unwrap(),
        job.clone(),
        step.id.clone(),
        "warning",
    );
    if step.watch {
        job.step_status(step, "watching");
    }
    let result = loop {
        if job.cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            break Err("任务已停止".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                break if status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "{}失败，退出码 {}",
                        step.label,
                        status.code().unwrap_or(-1)
                    ))
                }
            }
            Ok(None) => thread::sleep(Duration::from_millis(80)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(e.to_string());
            }
        }
    };
    drop(tree);
    let _ = stdout.join();
    let _ = stderr.join();
    result
}

fn launch_watch(step: &Step) -> Result<Child, String> {
    plan::validate_batch(step)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let args = step
            .args
            .iter()
            .map(|a| format!("\"{a}\""))
            .collect::<Vec<_>>()
            .join(" ");
        // Daily builds finish after starting the legacy standalone Watch window.
        // It must not join the build's kill-on-close job or inherit its output pipes.
        Command::new("cmd.exe")
            .args(["/D", "/S", "/K"])
            .raw_arg(format!("\"\"{}\" {}\"", step.program, args))
            .creation_flags(0x00000010)
            .current_dir(&step.cwd)
            .envs(&step.env)
            .spawn()
            .map_err(|e| format!("无法启动 TypeScript Watch：{e}"))
    }
    #[cfg(not(windows))]
    {
        Err("独立 Watch 窗口仅支持 Windows".into())
    }
}

fn lock_run(dir: &Path, root: &str) -> Result<File, String> {
    let root = fs::canonicalize(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| root.to_string())
        .to_lowercase();
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hash);
    fs::create_dir_all(dir.join("locks")).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("locks").join(format!("{:x}.lock", hash.finish())))
        .map_err(|e| e.to_string())?;
    lock.try_lock_exclusive()
        .map_err(|_| "此项目已在运行（可能是另一个窗口或计划任务），请先停止现有任务")?;
    Ok(lock)
}

fn definitions_backup(w: &Workspace, run_id: &str) -> Result<Option<(PathBuf, PathBuf)>, String> {
    let file = PathBuf::from(&w.paths.ts_project)
        .join("Typing")
        .join("ue")
        .join("ue.d.ts");
    if !file.exists() {
        return Ok(None);
    }
    let canonical = fs::canonicalize(&file).map_err(|e| e.to_string())?;
    let root = fs::canonicalize(&w.paths.ts_project).map_err(|e| e.to_string())?;
    if !canonical.starts_with(&root) {
        return Err("类型定义文件位于 TS 项目目录之外".into());
    }
    let backup = file.with_extension(format!("ts.backup-{run_id}"));
    fs::rename(&file, &backup).map_err(|e| format!("无法备份 ue.d.ts：{e}"))?;
    Ok(Some((file, backup)))
}

pub fn create_job(
    dir: &Path,
    w: &Workspace,
    dry_run: bool,
    scheduled: bool,
    sink: Sink,
) -> Result<(Arc<Job>, File), String> {
    create_job_at(dir, &dir.join("logs"), w, dry_run, scheduled, sink)
}

pub fn create_job_at(
    dir: &Path,
    logs_dir: &Path,
    w: &Workspace,
    dry_run: bool,
    scheduled: bool,
    sink: Sink,
) -> Result<(Arc<Job>, File), String> {
    let steps = plan::plan(w, scheduled)?;
    if !dry_run {
        let missing: Vec<String> = plan::diagnostics_for(w, scheduled)
            .into_iter()
            .filter(|c| c.required && !c.ok)
            .map(|c| format!("{}：{}", c.name, c.path))
            .collect();
        if !missing.is_empty() {
            return Err(format!("环境检查未通过\n{}", missing.join("\n")));
        }
    }
    let lock = lock_run(dir, &w.root)?;
    let id = uuid::Uuid::new_v4().to_string();
    fs::create_dir_all(logs_dir)
        .map_err(|e| format!("无法创建日志目录 {}：{e}", logs_dir.display()))?;
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(logs_dir.join(format!("{id}.jsonl")))
        .map_err(|e| e.to_string())?;
    let prefix = if scheduled {
        "scheduled_task"
    } else {
        "manual"
    };
    let text_file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(logs_dir.join(format!(
            "{prefix}_{}.log",
            chrono::Local::now().format("%Y%m%d")
        )))
        .map_err(|e| format!("无法写入程序目录下的运行日志：{e}"))?;
    let step_statuses = steps
        .iter()
        .map(|s| (s.id.clone(), "pending".into()))
        .collect();
    let job = Arc::new(Job {
        info: Mutex::new(RunInfo {
            id,
            workspace_id: w.id.clone(),
            workspace_name: w.name.clone(),
            status: "running".into(),
            started_at: Utc::now().to_rfc3339(),
            ended_at: None,
            dry_run,
            scheduled,
            steps,
            step_statuses,
            error: None,
        }),
        cancel: AtomicBool::new(false),
        logs: Mutex::new(VecDeque::new()),
        file: Mutex::new(file),
        text_file: Mutex::new(text_file),
        sink,
    });
    Ok((job, lock))
}

pub fn run(dir: PathBuf, w: Workspace, job: Arc<Job>, _lock: File) -> RunInfo {
    let dry_run = job.info.lock().unwrap().dry_run;
    job.log(
        "info",
        if dry_run {
            "预演开始：仅输出任务命令，不执行外部操作"
        } else {
            "任务开始"
        },
        None,
        true,
    );
    let initial = job.info.lock().unwrap().clone();
    let mut failure = None;
    for step in &initial.steps {
        if job.cancel.load(Ordering::SeqCst) {
            break;
        }
        job.step_status(step, "running");
        if initial.dry_run {
            job.log(
                "info",
                format!(
                    "{} {}\n工作目录：{}",
                    step.program,
                    step.args
                        .iter()
                        .map(|s| format!("\"{s}\""))
                        .collect::<Vec<_>>()
                        .join(" "),
                    step.cwd
                ),
                Some(&step.id),
                false,
            );
            thread::sleep(Duration::from_millis(100));
            job.step_status(step, "success");
            continue;
        }
        let result = (|| {
            let backup = if step.kind == "definitions" {
                definitions_backup(&w, &initial.id)?
            } else {
                None
            };
            let result = execute(step, &job);
            if let Some((file, backup)) = backup {
                if result.is_err() || !file.exists() {
                    if file.exists() {
                        let _ = fs::remove_file(&file);
                    }
                    fs::rename(&backup, &file).map_err(|e| {
                        format!("无法恢复类型定义，备份位于 {}：{e}", backup.display())
                    })?;
                    if result.is_ok() {
                        return Err("UE 未生成新的 ue.d.ts，已恢复旧文件".into());
                    }
                } else {
                    fs::remove_file(backup).map_err(|e| e.to_string())?;
                }
            } else if step.kind == "definitions"
                && !Path::new(&w.paths.ts_project)
                    .join("Typing/ue/ue.d.ts")
                    .exists()
            {
                if result.is_ok() {
                    return Err("UE 进程已退出，但没有生成 ue.d.ts".into());
                }
            }
            result
        })();
        match result {
            Ok(()) => job.step_status(step, "success"),
            Err(e) => {
                let cancelled = job.cancel.load(Ordering::SeqCst);
                job.step_status(step, if cancelled { "cancelled" } else { "failed" });
                job.log(
                    if cancelled { "info" } else { "error" },
                    &e,
                    Some(&step.id),
                    false,
                );
                failure = Some(e);
                break;
            }
        }
    }
    {
        let mut info = job.info.lock().unwrap();
        info.status = if job.cancel.load(Ordering::SeqCst) {
            "cancelled"
        } else if failure.is_some() {
            "failed"
        } else {
            "success"
        }
        .into();
        info.error = failure;
        info.ended_at = Some(Utc::now().to_rfc3339());
        for status in info.step_statuses.values_mut() {
            if status == "pending" {
                *status = "skipped".into();
            }
        }
    }
    let final_info = job.info.lock().unwrap().clone();
    if let Err(error) = save_history(&dir, &final_info) {
        job.log("error", format!("运行记录保存失败：{error}"), None, false);
    }
    job.log(
        if final_info.status == "failed" {
            "error"
        } else {
            "info"
        },
        format!("任务结束 · {}", final_info.status),
        None,
        true,
    );
    final_info
}

fn push_candidate(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    if !candidates.iter().any(|candidate| candidate == &path) {
        candidates.push(path);
    }
}

fn path_candidates(executable: &str) -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(executable))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledLauncher {
    pub version: String,
    pub executable: String,
}

fn installation_roots() -> Vec<PathBuf> {
    ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"]
        .into_iter()
        .filter_map(|name| std::env::var_os(name).map(PathBuf::from))
        .collect()
}

fn product_version(install_dir: &Path) -> Option<String> {
    let path = [
        install_dir.join("product-info.json"),
        install_dir.join("bin").join("product-info.json"),
    ]
    .into_iter()
    .find(|path| path.is_file())?;
    let bytes = fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn push_installation(
    installations: &mut Vec<InstalledLauncher>,
    install_dir: &Path,
    executable_name: &str,
    fallback_version: Option<String>,
) {
    let executable = install_dir.join("bin").join(executable_name);
    if !executable.is_file() {
        return;
    }
    let version = product_version(install_dir)
        .or(fallback_version)
        .unwrap_or_else(|| "未知版本".into());
    if !installations
        .iter()
        .any(|item| item.executable == executable.display().to_string())
    {
        installations.push(InstalledLauncher {
            version,
            executable: executable.display().to_string(),
        });
    }
}

fn folder_version(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().to_string();
    let lower = name.to_ascii_lowercase();
    for prefix in ["jetbrains rider ", "rider "] {
        if lower.starts_with(prefix) {
            return Some(name[prefix.len()..].to_string());
        }
    }
    None
}

fn rider_installations() -> Vec<InstalledLauncher> {
    let mut installations = Vec::new();
    for root in installation_roots() {
        for parent in [root.join("JetBrains"), root.join("Programs")] {
            let Ok(entries) = fs::read_dir(parent) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase());
                if name.as_deref().is_some_and(|name| {
                    name == "rider" || name.starts_with("rider ") || name.starts_with("jetbrains rider ")
                }) {
                    push_installation(&mut installations, &path, "rider64.exe", folder_version(&path));
                }
            }
        }

        // JetBrains Toolbox keeps versioned Rider folders below this directory.
        let toolbox = root.join("JetBrains").join("Toolbox").join("apps").join("Rider");
        let mut pending = vec![(toolbox, 0u8)];
        while let Some((directory, depth)) = pending.pop() {
            if depth > 5 { continue; }
            let Ok(entries) = fs::read_dir(directory) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    push_installation(&mut installations, &path, "rider64.exe", folder_version(&path));
                    pending.push((path, depth + 1));
                }
            }
        }
    }
    installations
}

fn visual_studio_installations() -> Vec<InstalledLauncher> {
    let mut installations = Vec::new();
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        let Some(root) = std::env::var_os(variable).map(PathBuf::from) else { continue };
        let base = root.join("Microsoft Visual Studio");
        let Ok(years) = fs::read_dir(base) else { continue };
        for year in years.flatten() {
            let year_path = year.path();
            if !year_path.is_dir() { continue; }
            let version = year.file_name().to_string_lossy().to_string();
            let Ok(editions) = fs::read_dir(year_path) else { continue };
            for edition in editions.flatten() {
                let executable = edition.path().join("Common7").join("IDE").join("devenv.exe");
                if executable.is_file() && !installations.iter().any(|item: &InstalledLauncher| item.executable == executable.display().to_string()) {
                    installations.push(InstalledLauncher { version: version.clone(), executable: executable.display().to_string() });
                }
            }
        }
    }
    installations
}

pub fn installed_launchers(action: &str) -> Vec<InstalledLauncher> {
    let mut result = match action {
        "rider" => rider_installations(),
        "visualStudio" => visual_studio_installations(),
        _ => Vec::new(),
    };
    result.sort_by(|left, right| right.version.cmp(&left.version).then(left.executable.cmp(&right.executable)));
    result
}

fn version_matches(actual: &str, requested: &str) -> bool {
    requested.trim().is_empty()
        || actual.eq_ignore_ascii_case(requested.trim())
        || actual.contains(requested.trim())
        || requested.trim().contains(actual)
}

fn rider_candidates(version: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for installation in installed_launchers("rider") {
        if version_matches(&installation.version, version) {
            push_candidate(&mut candidates, PathBuf::from(installation.executable));
        }
    }
    let mut roots = Vec::new();
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local_app_data));
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        roots.push(PathBuf::from(program_files));
    }
    if let Some(program_files_x86) = std::env::var_os("ProgramFiles(x86)") {
        roots.push(PathBuf::from(program_files_x86));
    }

    for root in &roots {
        let base = root.join("JetBrains");
        if !version.trim().is_empty() {
            push_candidate(
                &mut candidates,
                base.join(format!("Rider {}", version))
                    .join("bin")
                    .join("rider64.exe"),
            );
            push_candidate(
                &mut candidates,
                base.join(format!("JetBrains Rider {}", version))
                    .join("bin")
                    .join("rider64.exe"),
            );
        }
        for name in ["Rider 2", "Rider", "JetBrains Rider"] {
            push_candidate(
                &mut candidates,
                root.join("Programs")
                    .join(name)
                    .join("bin")
                    .join("rider64.exe"),
            );
        }
        for name in ["Rider", "JetBrains Rider"] {
            push_candidate(
                &mut candidates,
                base.join(name).join("bin").join("rider64.exe"),
            );
        }
    }
    candidates.extend(path_candidates("rider64.exe"));
    candidates.extend(path_candidates("rider.bat"));
    candidates
}

fn visual_studio_candidates(version: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for installation in installed_launchers("visualStudio") {
        if version_matches(&installation.version, version) {
            push_candidate(&mut candidates, PathBuf::from(installation.executable));
        }
    }
    candidates.extend(path_candidates("devenv.exe"));
    candidates
}

fn configured_launcher(
    launch: &AfterSuccess,
    legacy_rider_path: &str,
) -> Option<(&'static str, PathBuf)> {
    let mut action = launch.action.trim();
    let mut executable = launch.executable.trim();
    // Keep configurations created by the previous global Rider setting working once.
    if action == "none" && executable.is_empty() && !legacy_rider_path.trim().is_empty() {
        action = "rider";
        executable = legacy_rider_path.trim();
    }
    if action == "none" {
        return None;
    }
    let tool = match action {
        "rider" => "Rider",
        "visualStudio" => "Visual Studio",
        _ => return None,
    };
    let mut candidates = Vec::new();
    if !executable.is_empty() {
        push_candidate(&mut candidates, PathBuf::from(executable));
    }
    candidates.extend(if action == "rider" {
        rider_candidates(&launch.version)
    } else {
        visual_studio_candidates(&launch.version)
    });
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .map(|path| (tool, path))
}

/// Open the workspace solution after a scheduled run succeeds. This is detached from the
/// worker so closing SolutionBatNext does not close the selected development tool.
pub fn open_after_success(
    w: &Workspace,
    job: &Job,
    launch: &AfterSuccess,
    legacy_rider_path: &str,
) {
    let solution = PathBuf::from(&w.root).join("MHAGame").join("MHMobile.sln");
    if launch.action == "none" && legacy_rider_path.trim().is_empty() {
        return;
    }
    if !solution.is_file() {
        job.log(
            "warning",
            format!(
                "计划任务已成功，但未找到开发工具解决方案：{}",
                solution.display()
            ),
            None,
            false,
        );
        return;
    }
    let Some((tool, launcher)) = configured_launcher(launch, legacy_rider_path) else {
        job.log(
            "warning",
            "计划任务已成功，但未找到配置的开发工具启动程序，已跳过",
            None,
            false,
        );
        return;
    };
    match launch_desktop_program(&launcher, &solution, Path::new(&w.root)) {
        Ok(()) => job.log(
            "info",
            format!(
                "计划任务已成功，已用 {} 打开 {}（{}）",
                tool,
                solution.display(),
                launcher.display()
            ),
            None,
            false,
        ),
        Err(error) => job.log(
            "warning",
            format!("计划任务已成功，但启动 {} 失败：{error}", tool),
            None,
            false,
        ),
    }
}

fn launch_desktop_program(
    launcher: &Path,
    solution: &Path,
    working_dir: &Path,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let extension = launcher
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let mut command = if extension == "bat" || extension == "cmd" {
            // `start` treats a batch file as the command shell and can swallow the
            // solution argument. Invoke batch launchers directly so their arguments survive.
            let mut command = Command::new("cmd.exe");
            command.args(["/D", "/S", "/C"]).raw_arg(format!(
                " call \"{}\" \"{}\"",
                launcher.display(),
                solution.display()
            ));
            command
        } else {
            // Shell-start the GUI so Windows activates the user's desktop and JetBrains can
            // forward the solution to an existing IDE instance when possible.
            let command_line = format!(
                "start \"\" \"{}\" \"{}\"",
                launcher.display(),
                solution.display()
            );
            let mut command = Command::new("cmd.exe");
            command
                .args(["/D", "/S", "/C"])
                .raw_arg(format!(" {command_line}"));
            command
        };
        command
            .creation_flags(0x08000000)
            .current_dir(working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("无法通过 Windows Shell 启动开发工具：{error}"))
    }
    #[cfg(not(windows))]
    {
        Command::new(launcher)
            .arg(solution)
            .current_dir(working_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("无法启动开发工具：{error}"))
    }
}

fn save_history(dir: &Path, info: &RunInfo) -> Result<(), String> {
    fs::create_dir_all(dir.join("history")).map_err(|e| e.to_string())?;
    storage::atomic_write(
        &dir.join("history").join(format!("{}.json", info.id)),
        &serde_json::to_vec_pretty(info).map_err(|e| e.to_string())?,
    )
}

pub fn history(dir: &Path) -> Vec<RunInfo> {
    let mut result = Vec::new();
    if let Ok(files) = fs::read_dir(dir.join("history")) {
        for file in files.flatten() {
            if file.path().extension().is_some_and(|e| e == "json") {
                if let Ok(bytes) = fs::read(file.path()) {
                    if let Ok(info) = serde_json::from_slice::<RunInfo>(&bytes) {
                        result.push(info);
                    }
                }
            }
        }
    }
    result.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    result.truncate(100);
    result
}

pub fn read_logs(dir: &Path, run_id: &str) -> Result<Vec<RunEvent>, String> {
    read_logs_at(&dir.join("logs"), run_id)
}

pub fn read_logs_at(logs_dir: &Path, run_id: &str) -> Result<Vec<RunEvent>, String> {
    uuid::Uuid::parse_str(run_id).map_err(|_| "运行标识无效")?;
    let file = File::open(logs_dir.join(format!("{run_id}.jsonl"))).map_err(|e| e.to_string())?;
    let mut logs = VecDeque::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if let Ok(event) = serde_json::from_str::<RunEvent>(&line) {
            logs.push_back(event);
            if logs.len() > 1200 {
                logs.pop_front();
            }
        }
    }
    Ok(logs.into())
}

impl Runtime {
    pub fn start(
        &self,
        dir: PathBuf,
        w: Workspace,
        dry: bool,
        sink: Sink,
    ) -> Result<RunInfo, String> {
        let mut jobs = self.jobs.lock().unwrap();
        if jobs.get(&w.id).is_some_and(|j| j.active()) {
            return Err("当前工作区已有运行中的任务".into());
        }
        let (job, lock) = create_job_at(&dir, &storage::logs_dir(), &w, dry, false, sink)?;
        let info = job.info.lock().unwrap().clone();
        jobs.insert(w.id.clone(), job.clone());
        thread::spawn(move || {
            run(dir, w, job, lock);
        });
        Ok(info)
    }
    pub fn cancel(&self, workspace_id: &str) -> Result<(), String> {
        let jobs = self.jobs.lock().unwrap();
        let job = jobs.get(workspace_id).ok_or("工作区没有运行记录")?;
        {
            let mut info = job.info.lock().unwrap();
            if !matches!(info.status.as_str(), "running" | "watching" | "cancelling") {
                return Ok(());
            }
            job.cancel.store(true, Ordering::SeqCst);
            info.status = "cancelling".into();
        }
        job.log("info", "正在停止任务及其子进程", None, true);
        Ok(())
    }
    pub fn snapshot(&self) -> Vec<RunInfo> {
        self.jobs
            .lock()
            .unwrap()
            .values()
            .map(|j| j.info.lock().unwrap().clone())
            .collect()
    }
    pub fn active(&self) -> bool {
        self.jobs.lock().unwrap().values().any(|j| j.active())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn w() -> Workspace {
        let mut w = Workspace::new("test");
        w.root = "K:\\Nonexistent Test Project".into();
        w.tasks.close_editor = true;
        w
    }
    #[test]
    fn preview_does_not_execute_commands() {
        let dir = tempfile::tempdir().unwrap();
        let w = w();
        let (j, lock) = create_job(dir.path(), &w, true, false, Arc::new(|_| {})).unwrap();
        let result = run(dir.path().into(), w, j, lock);
        assert_eq!(result.status, "success");
        assert!(result.dry_run);
        assert_eq!(history(dir.path()).len(), 1);
        assert!(!read_logs(dir.path(), &result.id).unwrap().is_empty());
    }
    #[test]
    fn logs_use_the_requested_executable_directory_not_the_config_directory() {
        let config = tempfile::tempdir().unwrap();
        let executable_dir = tempfile::tempdir().unwrap();
        let logs = executable_dir.path().join("logs");
        let w = w();
        let (j, lock) =
            create_job_at(config.path(), &logs, &w, true, false, Arc::new(|_| {})).unwrap();
        let result = run(config.path().into(), w, j, lock);
        assert!(!config.path().join("logs").exists());
        assert!(!read_logs_at(&logs, &result.id).unwrap().is_empty());
        let daily = logs.join(format!(
            "manual_{}.log",
            chrono::Local::now().format("%Y%m%d")
        ));
        assert!(fs::read_to_string(daily).unwrap().contains("预演开始"));
        assert_eq!(history(config.path()).len(), 1);
    }
    #[test]
    fn prevents_simultaneous_runs_on_same_root() {
        let dir = tempfile::tempdir().unwrap();
        let w = w();
        let (_job, _lock) = create_job(dir.path(), &w, true, false, Arc::new(|_| {})).unwrap();
        assert!(create_job(dir.path(), &w, true, false, Arc::new(|_| {})).is_err());
    }
    #[test]
    fn refuses_log_path_traversal() {
        assert!(read_logs(Path::new("."), "../../secrets").is_err());
    }
    #[cfg(windows)]
    #[test]
    fn process_failure_is_reported_and_logs_stream() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = w();
        w.root = dir.path().display().to_string();
        let (j, _lock) = create_job(dir.path(), &w, true, false, Arc::new(|_| {})).unwrap();
        let step = Step {
            id: "fixture".into(),
            label: "fixture".into(),
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-Command".into(),
                "Write-Output 'fixture output'; exit 7".into(),
            ],
            cwd: w.root,
            env: BTreeMap::new(),
            watch: false,
            kind: "fixture".into(),
        };
        assert!(execute(&step, &j).unwrap_err().contains('7'));
        assert!(j
            .logs
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.message.contains("fixture output")));
    }
    #[cfg(windows)]
    #[test]
    fn stop_cancels_a_running_process() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = w();
        w.root = dir.path().display().to_string();
        let (j, _lock) = create_job(dir.path(), &w, true, false, Arc::new(|_| {})).unwrap();
        let j2 = j.clone();
        let cancel = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            j2.cancel.store(true, Ordering::SeqCst);
        });
        let step = Step {
            id: "fixture".into(),
            label: "fixture".into(),
            program: "powershell.exe".into(),
            args: vec![
                "-NoProfile".into(),
                "-Command".into(),
                "Start-Sleep -Seconds 30".into(),
            ],
            cwd: w.root,
            env: BTreeMap::new(),
            watch: true,
            kind: "fixture".into(),
        };
        assert!(execute(&step, &j).unwrap_err().contains("停止"));
        cancel.join().unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn batch_files_support_paths_and_arguments_with_spaces() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("Project with spaces");
        fs::create_dir(&sub).unwrap();
        let batch = sub.join("test build.bat");
        fs::write(&batch, "@echo off\r\necho ARG=%~1\r\nexit /b 0\r\n").unwrap();
        let mut w = w();
        w.root = sub.display().to_string();
        let (j, _lock) = create_job(dir.path(), &w, true, false, Arc::new(|_| {})).unwrap();
        let step = Step {
            id: "batch".into(),
            label: "batch".into(),
            program: batch.display().to_string(),
            args: vec!["value with spaces".into()],
            cwd: w.root,
            env: BTreeMap::new(),
            watch: false,
            kind: "batch".into(),
        };
        execute(&step, &j).unwrap();
        assert!(j
            .logs
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.message.contains("ARG=value with spaces")));
    }
    #[cfg(windows)]
    #[test]
    fn standalone_watch_starts_without_waiting_for_its_exit() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("watch fixture.cmd");
        fs::write(&script, "@echo off\r\npowershell.exe -NoProfile -Command \"Start-Sleep -Milliseconds 700\"\r\n>\"%~dp0watch-started.txt\" echo %~1\r\nexit\r\n").unwrap();
        let step = Step {
            id: "ts-build".into(),
            label: "fixture".into(),
            program: script.display().to_string(),
            args: vec!["argument with spaces".into()],
            cwd: dir.path().display().to_string(),
            env: BTreeMap::new(),
            watch: true,
            kind: "detached-watch".into(),
        };
        let mut child = launch_watch(&step).unwrap();
        let returned_before_exit = child.try_wait().unwrap().is_none();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        let finished = child.try_wait().unwrap().is_some();
        if !finished {
            let _ = child.kill();
            let _ = child.wait();
        }
        assert!(returned_before_exit && finished);
        assert_eq!(
            fs::read_to_string(dir.path().join("watch-started.txt"))
                .unwrap()
                .trim(),
            "argument with spaces"
        );
    }
    #[cfg(windows)]
    #[test]
    fn failed_stage_skips_following_commands() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = w();
        w.root = dir.path().display().to_string();
        let (j, lock) = create_job(dir.path(), &w, true, false, Arc::new(|_| {})).unwrap();
        let first = Step {
            id: "failure".into(),
            label: "failure".into(),
            program: "powershell.exe".into(),
            args: vec!["-NoProfile".into(), "-Command".into(), "exit 9".into()],
            cwd: w.root.clone(),
            env: BTreeMap::new(),
            watch: false,
            kind: "fixture".into(),
        };
        let second = Step {
            id: "sentinel".into(),
            args: vec![
                "-NoProfile".into(),
                "-Command".into(),
                "Write-Output 'should not execute'".into(),
            ],
            ..first.clone()
        };
        {
            let mut info = j.info.lock().unwrap();
            info.dry_run = false;
            info.steps = vec![first, second];
            info.step_statuses = BTreeMap::from([
                ("failure".into(), "pending".into()),
                ("sentinel".into(), "pending".into()),
            ]);
        }
        let result = run(dir.path().into(), w, j.clone(), lock);
        assert_eq!(result.status, "failed");
        assert_eq!(result.step_statuses["sentinel"], "skipped");
        assert!(!j
            .logs
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.message.contains("should not execute")));
    }
}
