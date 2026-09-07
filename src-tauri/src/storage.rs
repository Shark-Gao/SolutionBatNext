use crate::model::*;
use fs2::FileExt;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::OnceLock,
};

static DATA_DIR_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

pub fn configure_data_dir(dir: PathBuf) -> Result<(), String> {
    DATA_DIR_OVERRIDE.set(dir).map_err(|_| "配置目录已经初始化".into())
}

pub fn data_dir() -> PathBuf {
    DATA_DIR_OVERRIDE.get().cloned().or_else(|| std::env::var_os("SOLUTIONBAT_DATA_DIR")
        .map(PathBuf::from)
    )
        .unwrap_or_else(|| executable_dir().join("config"))
}

fn legacy_data_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_else(|| ".".into()))
        .join("SolutionBatNext")
}

pub fn webview_dir() -> PathBuf {
    std::env::var_os("SOLUTIONBAT_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(legacy_data_dir)
        .join("webview")
}

fn executable_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn logs_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SOLUTIONBAT_DATA_DIR") {
        return PathBuf::from(dir).join("logs");
    }
    executable_dir().join("logs")
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("文件目录无效")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    tmp.write_all(bytes).map_err(|e| e.to_string())?;
    tmp.as_file().sync_all().map_err(|e| e.to_string())?;
    tmp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn parse_legacy(bytes: &[u8]) -> Result<Vec<Workspace>, String> {
    let decoded = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::GBK.decode(bytes).0.into_owned(),
    };
    let doc = roxmltree::Document::parse(decoded.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("旧版 XML 无法读取：{e}"))?;
    if doc.root_element().tag_name().name() != "config" {
        return Err("请选择旧版 config.xml".into());
    }
    let mut result = Vec::new();
    for node in doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("workspace"))
    {
        let get = |section: &str, key: &str| -> String {
            node.children()
                .find(|n| n.has_tag_name(section))
                .and_then(|n| n.children().find(|c| c.has_tag_name(key)))
                .and_then(|n| n.text())
                .unwrap_or("")
                .trim()
                .to_string()
        };
        let flag = |section: &str, key: &str| {
            matches!(
                get(section, key).to_lowercase().as_str(),
                "true" | "1" | "yes"
            )
        };
        let mut w = Workspace::new(node.attribute("name").unwrap_or("导入的工作区"));
        w.root = get("paths", "project_root");
        w.p4 = P4 {
            port: get("p4", "port"),
            user: get("p4", "user"),
            client: get("p4", "client"),
            force: flag("options", "force_sync"),
        };
        let config = get("build", "configuration");
        if !config.is_empty() {
            w.build.configuration = config;
        }
        let platform = get("build", "platform");
        if !platform.is_empty() {
            w.build.platform = if platform.eq_ignore_ascii_case("ios") {
                "IOS".into()
            } else {
                platform
            };
        }
        let target = get("build", "target");
        if !target.is_empty() {
            w.build.target = target;
        }
        let name = get("paths", "project_name");
        if !name.is_empty() {
            w.paths.name = name;
        }
        w.derive_paths();
        for (field, key) in [
            (&mut w.paths.project, "project"),
            (&mut w.paths.engine, "engine"),
            (&mut w.paths.ts_project, "ts_proj"),
            (&mut w.paths.ue_exe, "ue_exe"),
        ] {
            let value = get("paths", key);
            if !value.is_empty() {
                *field = value;
            }
        }
        let mut derived = w.clone();
        derived.derive_paths();
        w.auto_paths =
            serde_json::to_value(&derived.paths).ok() == serde_json::to_value(&w.paths).ok();
        w.tasks = Tasks {
            sync: flag("tasks", "run_p4_update"),
            build: flag("tasks", "run_project_compile"),
            typescript: flag("tasks", "run_ts_compile"),
            definitions: flag("tasks", "run_ts_gen"),
            watch: flag("tasks", "run_ts_build"),
            close_editor: if get("tasks", "kill_ue_process").is_empty() {
                true
            } else {
                flag("tasks", "kill_ue_process")
            },
        };
        let times = get("schedule", "triggers");
        if !times.is_empty() {
            w.schedule.times = times
                .split(';')
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
        } else {
            let h = get("schedule", "hour");
            let m = get("schedule", "minute");
            if !h.is_empty() && !m.is_empty() {
                w.schedule.times = vec![format!("{h:0>2}:{m:0>2}")];
            }
        }
        w.validate()?;
        result.push(w);
    }
    if result.is_empty() {
        return Err("配置文件没有工作区".into());
    }
    Ok(result)
}

pub fn seed() -> Result<AppConfig, String> {
    let workspaces = parse_legacy(include_bytes!("../fixtures/legacy-config.xml"))?;
    Ok(AppConfig {
        schema_version: 1,
        revision: 0,
        selected_id: workspaces[0].id.clone(),
        workspaces,
        settings: Settings::default(),
    })
}

fn lock_config(dir: &Path) -> Result<fs::File, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("config.lock"))
        .map_err(|e| e.to_string())?;
    file.lock_exclusive().map_err(|e| e.to_string())?;
    Ok(file)
}

fn read_config(path: &Path) -> Result<AppConfig, String> {
    let config: AppConfig = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("配置损坏，请从 config.json.bak 恢复：{e}"))?;
    config.validate()?;
    Ok(config)
}

pub fn load(dir: &Path) -> Result<AppConfig, String> {
    let _lock = lock_config(dir)?;
    let path = dir.join("config.json");
    if path.try_exists().map_err(|e| format!("无法访问配置 {}：{e}", path.display()))? {
        return read_config(&path).map(|config| apply_ui_settings(dir, config));
    }
    if let Some(config) = migrate_legacy(dir)? {
        return Ok(apply_ui_settings(dir, config));
    }
    let config = seed()?;
    atomic_write(
        &path,
        &serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?,
    )?;
    Ok(apply_ui_settings(dir, config))
}

// Commit config.json last so an interrupted migration can be retried without new workspace IDs.
fn migrate_legacy(dir: &Path) -> Result<Option<AppConfig>, String> {
    if std::env::var_os("SOLUTIONBAT_DATA_DIR").is_some() || dir != data_dir() {
        return Ok(None);
    }
    let old = legacy_data_dir();
    let source = old.join("config.json");
    if old == dir || !source.try_exists().map_err(|e| format!("无法访问旧配置 {}：{e}", source.display()))? {
        return Ok(None);
    }
    let _lock = lock_config(&old)?;
    let config = read_config(&source)?;
    for name in ["config.json.bak", "ui-settings.json"] {
        copy_migration_file(&old.join(name), &dir.join(name))?;
    }
    for name in ["history", "logs"] {
        let folder = old.join(name);
        if folder.try_exists().map_err(|e| e.to_string())? {
            for entry in fs::read_dir(&folder).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_file() {
                    copy_migration_file(&entry.path(), &dir.join(name).join(entry.file_name()))?;
                }
            }
        }
    }
    atomic_write(&dir.join("config.json"), &fs::read(&source).map_err(|e| e.to_string())?)?;
    Ok(Some(config))
}

fn copy_migration_file(source: &Path, target: &Path) -> Result<(), String> {
    if source.try_exists().map_err(|e| e.to_string())? && !target.try_exists().map_err(|e| e.to_string())? {
        atomic_write(target, &fs::read(source).map_err(|e| e.to_string())?)
            .map_err(|e| format!("迁移 {} 到 {} 失败：{e}", source.display(), target.display()))?;
    }
    Ok(())
}

fn apply_ui_settings(dir: &Path, mut config: AppConfig) -> AppConfig {
    if let Ok(bytes) = fs::read(dir.join("ui-settings.json")) {
        if let Ok(settings) = serde_json::from_slice::<Settings>(&bytes) {
            if ["light", "dark", "system"].contains(&settings.theme.as_str()) {
                config.settings = settings;
            }
        }
    }
    config
}

pub fn save_ui_settings(dir: &Path, settings: &Settings) -> Result<(), String> {
    if !["light", "dark", "system"].contains(&settings.theme.as_str()) {
        return Err("主题设置无效".into());
    }
    atomic_write(
        &dir.join("ui-settings.json"),
        &serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?,
    )
}

pub fn save(dir: &Path, mut config: AppConfig) -> Result<AppConfig, String> {
    config.validate()?;
    let _lock = lock_config(dir)?;
    let path = dir.join("config.json");
    if path.exists() {
        let old = read_config(&path)?;
        if old.revision != config.revision {
            return Err("配置已被另一窗口修改，请重新载入后再保存".into());
        }
        atomic_write(
            &dir.join("config.json.bak"),
            &serde_json::to_vec_pretty(&old).map_err(|e| e.to_string())?,
        )?;
    }
    config.revision += 1;
    atomic_write(
        &path,
        &serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?,
    )?;
    Ok(config)
}

pub fn import_file(path: &Path) -> Result<Vec<Workspace>, String> {
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 5 * 1024 * 1024 {
        return Err("配置文件超过 5 MB".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut workspaces = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        let c: AppConfig = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        c.validate()?;
        c.workspaces
    } else {
        parse_legacy(&bytes)?
    };
    // Imported profiles get fresh identities, so they cannot inherit another installation's scheduled tasks.
    for w in &mut workspaces {
        w.id = uuid::Uuid::new_v4().to_string();
    }
    Ok(workspaces)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ui_preferences_do_not_rewrite_workspace_config() {
        let dir = tempfile::tempdir().unwrap();
        let before = load(dir.path()).unwrap();
        let bytes = fs::read(dir.path().join("config.json")).unwrap();
        save_ui_settings(
            dir.path(),
            &Settings {
                theme: "dark".into(),
                auto_scroll: true,
                rider_path: String::new(),
            },
        )
        .unwrap();
        let after = load(dir.path()).unwrap();
        assert_eq!(after.settings.theme, "dark");
        assert_eq!(after.revision, before.revision);
        assert_eq!(fs::read(dir.path().join("config.json")).unwrap(), bytes);
    }
    #[test]
    fn migrates_all_real_profiles_and_flags() {
        let c = seed().unwrap();
        assert_eq!(c.workspaces.len(), 4);
        assert!(c.workspaces[0].tasks.typescript);
        assert!(!c.workspaces[0].tasks.sync);
        assert_eq!(c.workspaces[0].schedule.times, vec!["03:00"]);
        assert_eq!(c.workspaces[2].root, r"I:\sharkgao_MHAClient_battle");
    }
    #[test]
    fn atomic_save_backs_up_and_rejects_stale_writes() {
        let dir = tempfile::tempdir().unwrap();
        let c = load(dir.path()).unwrap();
        let mut edit = c.clone();
        edit.workspaces[0].name = "研发 & 工具".into();
        let saved = save(dir.path(), edit).unwrap();
        assert_eq!(saved.revision, 1);
        assert_eq!(load(dir.path()).unwrap().workspaces[0].name, "研发 & 工具");
        assert!(save(dir.path(), c).unwrap_err().contains("另一窗口"));
        assert!(dir.path().join("config.json.bak").exists());
    }
    #[test]
    fn corrupted_config_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("config.json"), b"oops").unwrap();
        assert!(load(dir.path()).is_err());
        assert_eq!(fs::read(dir.path().join("config.json")).unwrap(), b"oops");
    }
}
