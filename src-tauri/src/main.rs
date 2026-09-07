#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use fs2::FileExt;
use solution_bat_next_lib::{monitor, plan, runner, storage};
use std::{fs, io::Write, path::PathBuf, sync::Arc};

fn task_log(level: &str, id: &str, message: &str) -> Result<(), String> {
    let dir = storage::logs_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("无法创建日志目录 {}：{e}", dir.display()))?;
    let path = dir.join(format!("scheduled_task_{}.log", chrono::Local::now().format("%Y%m%d")));
    let mut file = fs::OpenOptions::new().create(true).read(true).append(true)
        .open(&path).map_err(|e| format!("无法写入日志 {}：{e}", path.display()))?;
    file.lock_exclusive().map_err(|e| e.to_string())?;
    writeln!(file, "[{}] [{}] [{}] {}", chrono::Local::now().to_rfc3339(), level, id, message)
        .map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--monitor-run" || a == "--startup-error") {
        let dir = data_directory(&args).unwrap_or_else(|_| storage::data_dir());
        if let Err(error) = storage::configure_data_dir(dir) { eprintln!("{error}"); return; }
        let value = args.get(pos + 1).cloned();
        let monitor = if args[pos] == "--monitor-run" {
            monitor::Monitor { run_id: value, startup_error: None }
        } else {
            monitor::Monitor { run_id: None, startup_error: value }
        };
        solution_bat_next_lib::run_app_with_monitor(monitor);
        return;
    }
    if let Some(pos) = args.iter().position(|a| a == "--run-task") {
        let task_id = args.get(pos + 1).map(String::as_str).unwrap_or("未指定工作区");
        let check_only = args.iter().any(|a| a == "--check-task");
        let show_gui = !check_only && args.iter().any(|a| a == "--show-gui");
        let mut gui_started = false;
        let result = (|| -> Result<(), String> {
            let id = args.get(pos + 1).ok_or("缺少工作区标识")?;
            let dir = data_directory(&args)?;
            // Record failures before environment validation and run-history creation.
            task_log("info", id, &format!("计划任务入口：{}\n程序：{}\n配置目录：{}",
                if check_only { "仅检查环境，不执行任务" } else if args.iter().any(|a| a == "--dry-run") { "命令预演" } else { "执行任务" },
                std::env::current_exe().map_err(|e| e.to_string())?.display(), dir.display()))?;
            let config = storage::load(&dir)?;
            let rider_path = config.settings.rider_path.clone();
            let w = config
                .workspaces
                .into_iter()
                .find(|w| &w.id == id || &w.name == id)
                .ok_or("工作区不存在")?;
            if check_only {
                plan::plan(&w, true)?;
                let checks = plan::diagnostics_for(&w, true);
                let missing: Vec<String> = checks.iter().filter(|c| c.required && !c.ok)
                    .map(|c| format!("{}：{}", c.name, c.path)).collect();
                println!("{}", serde_json::json!({ "workspace": w.name, "ok": missing.is_empty(), "checks": checks }));
                if !missing.is_empty() {
                    return Err(format!("环境检查未通过\n{}", missing.join("\n")));
                }
                task_log("info", id, "环境检查通过，未执行同步、编译或进程操作")?;
                return Ok(());
            }
            let dry_run = args.iter().any(|a| a == "--dry-run");
            let sink = Arc::new(|e: runner::RunEvent| {
                println!("{} [{}] {}", e.time, e.level, e.message);
            });
            // Scheduled tasks pass --data-dir for configuration, not log placement.
            let logs = storage::logs_dir();
            let (job, lock) = runner::create_job_at(&dir, &logs, &w, dry_run, true, sink)?;
            let reporter = if show_gui {
                let reporter = monitor::report(&dir, job.clone())?;
                let run_id = job.info.lock().unwrap().id.clone();
                match monitor::launch(&dir, "--monitor-run", &run_id) {
                    Ok(()) => gui_started = true,
                    Err(error) => { task_log("warning", id, &error)?; }
                }
                Some(reporter)
            } else { None };
            let rider_workspace = w.clone();
            let result = runner::run(dir.clone(), w, job.clone(), lock);
            if result.status == "success" && !dry_run {
                runner::open_rider_solution(&rider_workspace, &job, &rider_path);
            }
            if let Some(reporter) = reporter {
                if let Ok(Err(error)) = reporter.join() {
                    task_log("warning", id, &format!("计划任务界面状态写入失败：{error}"))?;
                }
                if let Err(error) = monitor::finish(&dir, &job) {
                    task_log("warning", id, &format!("计划任务最终状态写入失败：{error}"))?;
                }
            }
            if result.status == "success" {
                Ok(())
            } else {
                Err(result.error.unwrap_or(result.status))
            }
        })();
        if let Err(error) = result {
            if let Err(log_error) = task_log("error", task_id, &error) {
                eprintln!("{log_error}");
            }
            eprintln!("{error}");
            if show_gui && !gui_started {
                let dir = data_directory(&args).unwrap_or_else(|_| storage::data_dir());
                let _ = monitor::launch(&dir, "--startup-error", &error);
            }
            std::process::exit(1);
        }
    } else {
        solution_bat_next_lib::run_app();
    }
}

fn data_directory(args: &[String]) -> Result<PathBuf, String> {
    if let Some(pos) = args.iter().position(|a| a == "--data-dir") {
        Ok(PathBuf::from(args.get(pos + 1).ok_or("缺少数据目录")?))
    } else { Ok(storage::data_dir()) }
}
