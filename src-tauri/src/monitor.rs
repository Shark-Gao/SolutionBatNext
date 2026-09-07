use crate::{runner::{self, Job, RunEvent, RunInfo}, storage};
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}, process::{Command, Stdio}, sync::{Arc, atomic::Ordering}, thread, time::Duration};

#[derive(Default)]
pub struct Monitor {
    pub run_id: Option<String>,
    pub startup_error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub run: RunInfo,
    pub events: Vec<RunEvent>,
    pub worker_pid: u32,
}

fn snapshot_path(dir: &Path, id: &str) -> Result<PathBuf, String> {
    uuid::Uuid::parse_str(id).map_err(|_| "运行标识无效")?;
    Ok(dir.join("live-runs").join(format!("{id}.json")))
}

fn cancel_path(dir: &Path, id: &str) -> Result<PathBuf, String> {
    Ok(snapshot_path(dir, id)?.with_extension("cancel"))
}

fn write_snapshot(path: &Path, job: &Job) -> Result<(), String> {
    let snapshot = Snapshot {
        run: job.info.lock().unwrap().clone(),
        events: job.logs.lock().unwrap().iter().cloned().collect(),
        worker_pid: std::process::id(),
    };
    storage::atomic_write(path, &serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?)
}

// A bounded live snapshot keeps the GUI current even after the archived JSONL reaches its size limit.
pub fn report(dir: &Path, job: Arc<Job>) -> Result<thread::JoinHandle<Result<(), String>>, String> {
    let id = job.info.lock().unwrap().id.clone();
    let path = snapshot_path(dir, &id)?;
    let cancel = cancel_path(dir, &id)?;
    let dir = dir.to_path_buf();
    write_snapshot(&path, &job)?;
    Ok(thread::spawn(move || {
        let mut reported_write_error = false;
        loop {
            if cancel.is_file() {
                job.cancel.store(true, Ordering::SeqCst);
                if job.active() {
                    job.info.lock().unwrap().status = "cancelling".into();
                    job.log("info", "正在停止计划任务及其子进程", None, true);
                }
                let _ = fs::remove_file(&cancel);
            }
            match write_snapshot(&path, &job) {
                Ok(()) => {
                    if reported_write_error {
                        job.log("info", "计划任务窗口状态刷新已恢复", None, false);
                        reported_write_error = false;
                    }
                }
                Err(error) => {
                    if !reported_write_error {
                        job.log("warning", format!("计划任务窗口状态暂时无法写入，将自动重试：{error}"), None, false);
                        reported_write_error = true;
                    }
                }
            }
            if !job.active() { return finish(&dir, &job); }
            thread::sleep(Duration::from_millis(750));
        }
    }))
}

pub fn finish(dir: &Path, job: &Job) -> Result<(), String> {
    let id = job.info.lock().unwrap().id.clone();
    let path = snapshot_path(dir, &id)?;
    for attempt in 0..30 {
        match write_snapshot(&path, job) {
            Ok(()) => return Ok(()),
            Err(error) if attempt == 29 => return Err(error),
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
    unreachable!()
}

pub fn launch(dir: &Path, flag: &str, value: &str) -> Result<(), String> {
    runner::hidden(Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args([flag, value, "--data-dir"])
        .arg(dir)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()))
        .spawn().map_err(|e| format!("无法打开计划任务窗口：{e}"))?;
    Ok(())
}

fn worker_exited(pid: u32) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER}, System::Threading::*};
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() { return GetLastError() == ERROR_INVALID_PARAMETER; }
        let mut code = 259;
        let ok = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        ok != 0 && code != 259
    }
    #[cfg(not(windows))]
    { let _ = pid; false }
}

pub fn read(dir: &Path, monitor: &Monitor) -> Result<Option<Snapshot>, String> {
    let Some(id) = &monitor.run_id else { return Ok(None); };
    let mut snapshot: Snapshot = serde_json::from_slice(&fs::read(snapshot_path(dir, id)?).map_err(|e| format!("无法读取计划任务状态：{e}"))?)
        .map_err(|e| format!("计划任务状态损坏：{e}"))?;
    if snapshot.run.id != *id { return Err("计划任务运行标识不匹配".into()); }
    if matches!(snapshot.run.status.as_str(), "running" | "watching" | "cancelling") && worker_exited(snapshot.worker_pid) {
        // A crash must not leave a permanently spinning task window.
        if let Ok(bytes) = fs::read(dir.join("history").join(format!("{id}.json"))) {
            if let Ok(info) = serde_json::from_slice::<RunInfo>(&bytes) {
                if !matches!(info.status.as_str(), "running" | "watching" | "cancelling") {
                    snapshot.run = info;
                    return Ok(Some(snapshot));
                }
            }
        }
        snapshot.run.status = "failed".into();
        snapshot.run.error = Some("计划任务进程已退出，未收到正常结束记录，请查看启动日志".into());
        snapshot.run.ended_at = Some(chrono::Utc::now().to_rfc3339());
        for status in snapshot.run.step_statuses.values_mut() {
            if matches!(status.as_str(), "running" | "watching" | "cancelling") { *status = "failed".into(); }
            else if status == "pending" { *status = "skipped".into(); }
        }
    }
    Ok(Some(snapshot))
}

pub fn cancel(dir: &Path, monitor: &Monitor) -> Result<(), String> {
    let snapshot = read(dir, monitor)?.ok_or("当前窗口没有关联计划任务")?;
    if matches!(snapshot.run.status.as_str(), "running" | "watching" | "cancelling") {
        storage::atomic_write(&cancel_path(dir, &snapshot.run.id)?, b"cancel")?;
    }
    Ok(())
}

pub fn active(dir: &Path, monitor: &Monitor) -> bool {
    read(dir, monitor).ok().flatten().is_some_and(|s| matches!(s.run.status.as_str(), "running" | "watching" | "cancelling"))
}
