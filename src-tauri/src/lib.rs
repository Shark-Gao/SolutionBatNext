pub mod model;
pub mod monitor;
pub mod p4;
pub mod plan;
pub mod runner;
pub mod schedule;
pub mod storage;
pub mod telemetry;

use model::{AppConfig, Workspace};
use runner::{RunEvent, RunInfo, Runtime};
use std::{fs, path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};

#[tauri::command]
async fn load_config() -> Result<AppConfig, String> {
    storage::load(&storage::data_dir())
}

const HELP_URL: &str = "https://iwiki.woa.com/p/4018100342?from=iWiki_search";

#[tauri::command]
async fn open_help() -> Result<(), String> {
    runner::hidden(std::process::Command::new("explorer.exe").arg(HELP_URL))
        .spawn()
        .map_err(|e| format!("无法打开帮助文档：{e}\n{HELP_URL}"))?;
    Ok(())
}

#[tauri::command]
async fn save_ui_settings(settings: model::Settings) -> Result<(), String> {
    storage::save_ui_settings(&storage::data_dir(), &settings)
}

#[tauri::command]
async fn save_config(config: AppConfig, runtime: State<'_, Runtime>) -> Result<AppConfig, String> {
    let old = storage::load(&storage::data_dir())?;
    for w in &old.workspaces {
        if config
            .workspaces
            .iter()
            .any(|c| c.id == w.id && c.name != w.name)
            && schedule::action(w, &storage::data_dir(), "query")?.registered
        {
            return Err("请先删除此工作区的计划任务，再重命名".into());
        }
        if !config.workspaces.iter().any(|c| c.id == w.id) {
            if runtime.snapshot().iter().any(|r| {
                r.workspace_id == w.id
                    && ["running", "watching", "cancelling"].contains(&r.status.as_str())
            }) {
                return Err("请先停止工作区任务，再删除工作区".into());
            }
            if schedule::action(w, &storage::data_dir(), "query")?.registered {
                return Err("请先删除此工作区的计划任务".into());
            }
        }
    }
    storage::save(&storage::data_dir(), config)
}

#[tauri::command]
async fn import_config(path: String) -> Result<Vec<Workspace>, String> {
    storage::import_file(&PathBuf::from(path))
}

#[tauri::command]
async fn export_config(path: String, config: AppConfig) -> Result<(), String> {
    config.validate()?;
    storage::atomic_write(
        &PathBuf::from(path),
        &serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?,
    )
}

#[tauri::command]
async fn check_environment(workspace: Workspace) -> Vec<plan::Check> {
    plan::diagnostics(&workspace)
}

#[tauri::command]
async fn installed_launchers(action: String) -> Vec<runner::InstalledLauncher> {
    runner::installed_launchers(&action)
}

#[tauri::command]
fn preview_plan(workspace: Workspace, scheduled: bool) -> Result<Vec<plan::Step>, String> {
    plan::plan(&workspace, scheduled)
}

#[tauri::command]
fn start_run(
    workspace_id: String,
    dry_run: bool,
    app: tauri::AppHandle,
    runtime: State<'_, Runtime>,
) -> Result<RunInfo, String> {
    let config = storage::load(&storage::data_dir())?;
    let w = config
        .workspaces
        .into_iter()
        .find(|w| w.id == workspace_id)
        .ok_or("工作区不存在")?;
    runtime.start(
        storage::data_dir(),
        w,
        dry_run,
        Arc::new(move |event| {
            let _ = app.emit("run-event", event);
        }),
    )
}

#[tauri::command]
fn stop_run(workspace_id: String, runtime: State<'_, Runtime>) -> Result<(), String> {
    runtime.cancel(&workspace_id)
}

#[tauri::command]
fn runtime_snapshot(runtime: State<'_, Runtime>) -> Vec<RunInfo> {
    runtime.snapshot()
}

#[tauri::command]
async fn monitored_run(monitor: State<'_, monitor::Monitor>) -> Result<Option<monitor::Snapshot>, String> {
    monitor::read(&storage::data_dir(), &monitor)
}

#[tauri::command]
async fn stop_monitored_run(monitor: State<'_, monitor::Monitor>) -> Result<(), String> {
    monitor::cancel(&storage::data_dir(), &monitor)
}

#[tauri::command]
async fn run_history() -> Vec<RunInfo> {
    runner::history(&storage::data_dir())
}

#[tauri::command]
async fn get_logs(run_id: String) -> Result<Vec<RunEvent>, String> {
    if storage::logs_dir().join(format!("{run_id}.jsonl")).exists() {
        runner::read_logs_at(&storage::logs_dir(), &run_id)
    } else {
        runner::read_logs(&storage::data_dir(), &run_id)
    }
}

#[tauri::command]
async fn export_logs(run_id: String, path: String) -> Result<(), String> {
    uuid::Uuid::parse_str(&run_id).map_err(|_| "运行标识无效")?;
    let filename = format!("{run_id}.jsonl");
    let current = storage::logs_dir().join(&filename);
    let source = if current.exists() {
        current
    } else {
        storage::data_dir().join("logs").join(filename)
    };
    let bytes = fs::read(source).map_err(|e| e.to_string())?;
    let mut output = String::new();
    for line in String::from_utf8_lossy(&bytes).lines() {
        if let Ok(event) = serde_json::from_str::<RunEvent>(line) {
            output.push_str(&format!(
                "{} [{}] {}\n",
                event.time, event.level, event.message
            ));
        }
    }
    storage::atomic_write(&PathBuf::from(path), output.as_bytes())
}

#[tauri::command]
async fn schedule_action(
    workspace_id: String,
    action: String,
) -> Result<schedule::ScheduleInfo, String> {
    let config = storage::load(&storage::data_dir())?;
    let w = config
        .workspaces
        .iter()
        .find(|w| w.id == workspace_id)
        .ok_or("工作区不存在")?;
    schedule::action(w, &storage::data_dir(), &action)
}

#[tauri::command]
fn open_directory(path: Option<String>) -> Result<(), String> {
    let path = path.map(PathBuf::from).unwrap_or_else(storage::data_dir);
    if !path.is_dir() {
        return Err("目录不存在".into());
    }
    runner::hidden(std::process::Command::new("explorer.exe").arg(path))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_logs_directory() -> Result<(), String> {
    let dir = storage::logs_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    open_directory(Some(dir.display().to_string()))
}

#[tauri::command]
fn app_info(app: tauri::AppHandle, monitor: State<'_, monitor::Monitor>) -> serde_json::Value {
    let updater = app.config().plugins.0.get("updater");
    let updater_ready = updater.is_some_and(|c| {
        c.get("pubkey")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty())
            && c.get("endpoints")
                .and_then(|v| v.as_array())
                .is_some_and(|v| !v.is_empty())
    });
    serde_json::json!({ "version": app.package_info().version.to_string(), "dataDir": storage::data_dir(), "logsDir": storage::logs_dir(), "updaterReady": updater_ready,
        "monitorRunId": monitor.run_id, "startupError": monitor.startup_error })
}

pub fn run_app() {
    run_app_with_monitor(monitor::Monitor::default());
}

pub fn run_app_with_monitor(monitor: monitor::Monitor) {
    let mut context = tauri::generate_context!();
    if let Some(window) = context.config_mut().app.windows.first_mut() {
        window.create = false;
        if monitor.run_id.is_some() || monitor.startup_error.is_some() {
            window.title = "SolutionBat Next - 计划任务".into();
        }
        #[cfg(debug_assertions)]
        if let Ok(port) = std::env::var("SOLUTIONBAT_TEST_PORT") {
            if let Ok(port) = port.parse::<u16>() {
                window.additional_browser_args = Some(format!("--remote-debugging-port={port}"));
            }
        }
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if app.config().plugins.0.get("updater").is_some_and(|c| {
                c.get("pubkey")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty())
            }) {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }
            // Absolute data paths must use the builder; declarative config accepts relative paths only.
            if let Some(window) = app.config().app.windows.first().cloned() {
                tauri::WebviewWindowBuilder::from_config(app, &window)?
                    .data_directory(storage::webview_dir())
                    .build()?;
            }
            telemetry::launch();
            Ok(())
        })
        .manage(Runtime::default())
        .manage(monitor)
        .invoke_handler(tauri::generate_handler![
            load_config,
            open_help,
            save_ui_settings,
            save_config,
            import_config,
            export_config,
            check_environment,
            installed_launchers,
            preview_plan,
            start_run,
            stop_run,
            runtime_snapshot,
            monitored_run,
            stop_monitored_run,
            run_history,
            get_logs,
            export_logs,
            schedule_action,
            open_directory,
            open_logs_directory,
            app_info
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.state::<Runtime>().active() || monitor::active(&storage::data_dir(), &window.state::<monitor::Monitor>()) {
                    api.prevent_close();
                    let _ = window.emit("close-with-active-runs", ());
                }
            }
        })
        .run(context)
        .expect("failed to run SolutionBat Next");
    telemetry::close();
}
