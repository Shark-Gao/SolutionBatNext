use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub revision: u64,
    pub selected_id: String,
    pub workspaces: Vec<Workspace>,
    #[serde(default)]
    pub settings: Settings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String,
    pub auto_scroll: bool,
    pub rider_path: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "light".into(),
            auto_scroll: true,
            rider_path: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root: String,
    pub p4: P4,
    pub paths: Paths,
    pub build: Build,
    #[serde(default)]
    pub tasks: Tasks,
    #[serde(default)]
    pub schedule: Schedule,
    #[serde(default = "yes")]
    pub auto_paths: bool,
}
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct P4 {
    pub port: String,
    pub user: String,
    pub client: String,
    #[serde(default)]
    pub force: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paths {
    pub project: String,
    pub name: String,
    pub engine: String,
    pub ts_project: String,
    pub ue_exe: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub configuration: String,
    pub target: String,
    pub platform: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tasks {
    pub sync: bool,
    pub build: bool,
    pub typescript: bool,
    pub definitions: bool,
    pub watch: bool,
    pub close_editor: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub times: Vec<String>,
    pub close_rider: bool,
    #[serde(default)]
    pub after_success: AfterSuccess,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AfterSuccess {
    pub action: String,
    pub version: String,
    pub executable: String,
}

impl Default for AfterSuccess {
    fn default() -> Self {
        Self {
            action: "none".into(),
            version: "2024.3.10".into(),
            executable: String::new(),
        }
    }
}

pub fn win_join(root: &str, rest: &str) -> String {
    if root.is_empty() {
        return String::new();
    }
    format!("{}\\{}", root.trim_end_matches(['\\', '/']), rest)
}

impl Workspace {
    pub fn new(name: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            root: String::new(),
            p4: P4::default(),
            paths: Paths {
                name: "MHMobile".into(),
                ..Paths::default()
            },
            build: Build {
                configuration: "Development".into(),
                target: "Editor".into(),
                platform: "Win64".into(),
            },
            tasks: Tasks::default(),
            schedule: Schedule::default(),
            auto_paths: true,
        }
    }
    pub fn derive_paths(&mut self) {
        self.paths.project = win_join(
            &self.root,
            &format!("MHAGame\\{}.uproject", self.paths.name),
        );
        self.paths.engine = win_join(&self.root, "Engine");
        self.paths.ts_project = win_join(&self.root, "MHAGame\\TsProj");
        // Commandlets execute on the Windows host even when the build target is Android/iOS.
        let suffix = if self.build.configuration == "Development" {
            String::new()
        } else {
            format!("Win64-{}-", self.build.configuration)
        };
        self.paths.ue_exe = win_join(
            &self.paths.engine,
            &format!("Binaries\\Win64\\UnrealEditor-{}Cmd.exe", suffix),
        );
    }
    pub fn validate(&self) -> Result<(), String> {
        Uuid::parse_str(&self.id).map_err(|_| "工作区标识无效")?;
        if self.name.trim().is_empty() || self.name.len() > 160 {
            return Err("工作区名称不能为空或过长".into());
        }
        if !["Development", "DebugGame", "Debug", "Shipping", "Test"]
            .contains(&self.build.configuration.as_str())
        {
            return Err("编译配置无效".into());
        }
        if !["Editor", "Game", "Client", "Server"].contains(&self.build.target.as_str()) {
            return Err("编译目标无效".into());
        }
        if ![
            "Win64",
            "Win64-arm64",
            "Win64-arm64ec",
            "Android",
            "IOS",
            "Linux",
        ]
        .contains(&self.build.platform.as_str())
        {
            return Err("目标平台无效".into());
        }
        for value in [
            &self.name,
            &self.root,
            &self.paths.name,
            &self.paths.project,
            &self.paths.engine,
            &self.paths.ts_project,
            &self.paths.ue_exe,
            &self.p4.port,
            &self.p4.user,
            &self.p4.client,
            &self.schedule.after_success.version,
            &self.schedule.after_success.executable,
        ] {
            if value.chars().any(char::is_control) {
                return Err("配置不能包含换行或控制字符".into());
            }
        }
        if self.schedule.times.len() > 24 {
            return Err("每天最多设置 24 个触发时间".into());
        }
        if !["none", "rider", "visualStudio"].contains(&self.schedule.after_success.action.as_str())
        {
            return Err("计划任务成功后的开发工具选项无效".into());
        }
        let mut unique = HashSet::new();
        for time in &self.schedule.times {
            let bytes = time.as_bytes();
            if bytes.len() != 5
                || bytes[2] != b':'
                || !bytes[..2]
                    .iter()
                    .chain(bytes[3..].iter())
                    .all(u8::is_ascii_digit)
                || time[..2].parse::<u8>().unwrap_or(99) > 23
                || time[3..].parse::<u8>().unwrap_or(99) > 59
            {
                return Err(format!("时间格式无效：{time}"));
            }
            if !unique.insert(time) {
                return Err(format!("触发时间重复：{time}"));
            }
        }
        Ok(())
    }
}

impl AppConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("不支持的配置版本".into());
        }
        if self.workspaces.is_empty() || self.workspaces.len() > 100 {
            return Err("工作区数量需在 1 到 100 之间".into());
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let mut task_names = HashSet::new();
        for workspace in &self.workspaces {
            workspace.validate()?;
            if !ids.insert(&workspace.id) || !names.insert(workspace.name.trim().to_lowercase()) {
                return Err("工作区名称或标识重复".into());
            }
            if !task_names.insert(crate::schedule::task_name(&workspace.name).to_lowercase()) {
                return Err("工作区名称转换后的 Windows 计划任务名称重复".into());
            }
        }
        if !ids.contains(&self.selected_id) {
            return Err("当前工作区不存在".into());
        }
        if !["light", "dark", "system"].contains(&self.settings.theme.as_str()) {
            return Err("主题设置无效".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_use_host_editor_and_preserve_spaces() {
        let mut w = Workspace::new("test");
        w.root = r"K:\My Project".into();
        w.build.platform = "Android".into();
        w.build.configuration = "DebugGame".into();
        w.derive_paths();
        assert_eq!(
            w.paths.ue_exe,
            r"K:\My Project\Engine\Binaries\Win64\UnrealEditor-Win64-DebugGame-Cmd.exe"
        );
    }
    #[test]
    fn validates_times_without_panicking_on_unicode() {
        let mut w = Workspace::new("test");
        for time in ["25:00", "03:99", "早上", "é:00", "3:00"] {
            w.schedule.times = vec![time.into()];
            assert!(w.validate().is_err());
        }
        w.schedule.times = vec!["03:00".into(), "03:00".into()];
        assert!(w.validate().is_err());
        w.schedule.times = vec!["00:00".into(), "23:59".into()];
        assert!(w.validate().is_ok());
    }
}
