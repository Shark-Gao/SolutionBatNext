use crate::{model::Workspace, runner::hidden};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInfo {
    pub registered: bool,
    pub name: String,
    pub next_run: Option<String>,
    pub last_result: Option<i64>,
    pub legacy_registered: bool,
    pub times: Vec<String>,
    pub state: Option<String>,
    pub executable: Option<String>,
    pub previous_registered: bool,
}

const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$p = ConvertFrom-Json $env:SB_SCHEDULE_REQUEST
function Find-Task($name) {
    Get-ScheduledTask -TaskPath '\' -ErrorAction Stop | Where-Object { $_.TaskName -eq $name } | Select-Object -First 1
}
if ($p.action -eq 'register') {
    $arguments = '--run-task ' + $p.id + ' --data-dir "' + $p.dataDir + '" --show-gui'
    $action = New-ScheduledTaskAction -Execute $p.exe -Argument $arguments -WorkingDirectory ([IO.Path]::GetDirectoryName($p.exe))
    $triggers = @($p.times | ForEach-Object { New-ScheduledTaskTrigger -Daily -At ([DateTime]::ParseExact($_, 'HH:mm', [Globalization.CultureInfo]::InvariantCulture)) })
    $settings = New-ScheduledTaskSettingsSet -StartWhenAvailable -DontStopOnIdleEnd
    $principal = New-ScheduledTaskPrincipal -UserId ([Security.Principal.WindowsIdentity]::GetCurrent().Name) -LogonType Interactive -RunLevel Highest
    Register-ScheduledTask -TaskPath '\' -TaskName $p.name -Action $action -Trigger $triggers -Settings $settings -Principal $principal -Force | Out-Null
    $previous = Find-Task $p.previousName
    if ($previous) { $previous | Unregister-ScheduledTask -Confirm:$false -ErrorAction Stop }
} elseif ($p.action -eq 'unregister') {
    foreach ($name in @($p.name, $p.previousName)) {
        $existing = Find-Task $name
        if ($existing) { $existing | Unregister-ScheduledTask -Confirm:$false -ErrorAction Stop }
    }
}
$daily = Find-Task $p.name
$previous = Find-Task $p.previousName
$task = if ($daily) { $daily } else { $previous }
$next = $null
$last = $null
$state = $null
$exe = $null
$times = @()
if ($task) {
    $info = $task | Get-ScheduledTaskInfo
    if ($info.NextRunTime.Year -gt 2000) { $next = $info.NextRunTime.ToString('yyyy-MM-dd HH:mm:ss') }
    $last = $info.LastTaskResult
    $state = $task.State.ToString()
    $exe = ($task.Actions | Select-Object -First 1).Execute
    $times = @($task.Triggers | Where-Object StartBoundary | ForEach-Object { ([DateTime]$_.StartBoundary).ToString('HH:mm') })
}
@{ registered = [bool]$task; name = $(if ($task) { $task.TaskName } else { $p.name }); nextRun = $next; lastResult = $last; legacyRegistered = [bool]($daily -and $exe -ne $p.exe); times = $times; state = $state; executable = $exe; previousRegistered = [bool]$previous } | ConvertTo-Json -Compress
"#;

pub fn task_name(name: &str) -> String {
    format!("DailyBuild_{}", name.replace([' ', '\\', '/'], "_"))
}

pub fn action(w: &Workspace, dir: &Path, action: &str) -> Result<ScheduleInfo, String> {
    uuid::Uuid::parse_str(&w.id).map_err(|_| "工作区标识无效")?;
    if !["query", "register", "unregister"].contains(&action) {
        return Err("计划任务操作无效".into());
    }
    if action == "register" {
        w.validate()?;
        if w.schedule.times.is_empty() {
            return Err("请至少添加一个触发时间".into());
        }
        crate::plan::plan(w, true)?;
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let request = serde_json::json!({ "id": w.id, "name": task_name(&w.name), "previousName": format!("SolutionBatNext_{}", w.id),
        "action": action, "exe": exe, "dataDir": dir, "times": w.schedule.times });
    let output = hidden(
        Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .env("SB_SCHEDULE_REQUEST", request.to_string()),
    )
    .output()
    .map_err(|e| format!("无法访问 Windows 计划任务：{e}"))?;
    if !output.status.success() {
        let hint = if action == "query" {
            ""
        } else {
            "\n注册和删除计划任务需要管理员权限，请以管理员身份运行本程序后重试。"
        };
        return Err(format!(
            "计划任务操作失败：{}{hint}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("无法读取计划任务状态：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schedule_name_matches_original_app() {
        assert_eq!(task_name("MHA_Client_main"), "DailyBuild_MHA_Client_main");
        assert_eq!(task_name("a b\\c/d"), "DailyBuild_a_b_c_d");
    }
    #[test]
    fn register_rejects_empty_triggers_without_accessing_scheduler() {
        assert!(action(&Workspace::new("test"), Path::new("."), "register")
            .unwrap_err()
            .contains("至少"));
    }
    #[cfg(windows)]
    #[test]
    fn powershell_script_parses_without_registering_any_tasks() {
        let check = "$e=$null; $t=$null; [System.Management.Automation.Language.Parser]::ParseInput($env:SB_SCRIPT, [ref]$t, [ref]$e) | Out-Null; if ($e.Count) { $e | Out-String | Write-Output; exit 1 }";
        let result = hidden(
            Command::new("powershell.exe")
                .args(["-NoProfile", "-Command", check])
                .env("SB_SCRIPT", SCRIPT),
        )
        .output()
        .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
    }
    #[cfg(windows)]
    #[test]
    fn scheduler_roundtrip_uses_legacy_identity_and_preserves_tasks_on_failure() {
        // Shadow every scheduler cmdlet. This test never reads or writes real Windows tasks.
        let harness = r#"
$ErrorActionPreference = 'Stop'
$script:tasks = @{}
$script:failRegister = $false
function New-ScheduledTaskAction { param($Execute, $Argument, $WorkingDirectory) [pscustomobject]@{ Execute=$Execute; Arguments=$Argument; WorkingDirectory=$WorkingDirectory } }
function New-ScheduledTaskTrigger { param([switch]$Daily, [DateTime]$At) [pscustomobject]@{ StartBoundary=$At.ToString('s'); Daily=[bool]$Daily } }
function New-ScheduledTaskSettingsSet { param([switch]$StartWhenAvailable, [switch]$DontStopOnIdleEnd) [pscustomobject]@{ StartWhenAvailable=[bool]$StartWhenAvailable; DontStopOnIdleEnd=[bool]$DontStopOnIdleEnd } }
function New-ScheduledTaskPrincipal { param($UserId, $LogonType, $RunLevel) [pscustomobject]@{ UserId=$UserId; LogonType=$LogonType; RunLevel=$RunLevel } }
function Get-ScheduledTask { [CmdletBinding()]param($TaskPath) $script:tasks.Values }
function Register-ScheduledTask {
    [CmdletBinding()]param($TaskPath, $TaskName, $Action, $Trigger, $Settings, $Principal, [switch]$Force)
    if ($script:failRegister) { throw 'fixture access denied' }
    if ($TaskPath -ne '\' -or -not $Force) { throw 'wrong task registration flags' }
    $script:tasks[$TaskName] = [pscustomobject]@{ TaskName=$TaskName; Actions=@($Action); Triggers=@($Trigger); Settings=$Settings; Principal=$Principal; State='Ready' }
}
function Unregister-ScheduledTask { [CmdletBinding(SupportsShouldProcess)]param([Parameter(ValueFromPipeline)]$InputObject) process { $script:tasks.Remove($InputObject.TaskName) } }
function Get-ScheduledTaskInfo { [CmdletBinding()]param([Parameter(ValueFromPipeline)]$InputObject) process { [pscustomobject]@{ NextRunTime=[DateTime]'2026-09-06T03:00:00'; LastTaskResult=0 } } }
function Invoke-Fixture($action) {
    $request = @{ id='00000000-0000-4000-8000-000000000001'; name='DailyBuild_a'; previousName='SolutionBatNext_00000000-0000-4000-8000-000000000001'; action=$action; exe='K:\New App\new.exe'; dataDir='K:\My Data'; times=@('03:00', '08:30') }
    $env:SB_SCHEDULE_REQUEST = $request | ConvertTo-Json -Compress
    & ([scriptblock]::Create($env:SB_SCRIPT)) | ConvertFrom-Json
}
$old = [pscustomobject]@{ TaskName='DailyBuild_a'; Actions=@([pscustomobject]@{ Execute='K:\Old\old.exe' }); Triggers=@([pscustomobject]@{ StartBoundary='2026-09-06T03:00:00' }); State='Ready' }
$script:tasks[$old.TaskName] = $old
$script:tasks['DailyBuild_a_extra'] = [pscustomobject]@{ TaskName='DailyBuild_a_extra' }
$q = Invoke-Fixture 'query'
if (-not $q.registered -or -not $q.legacyRegistered -or $q.name -ne 'DailyBuild_a' -or $q.times.Count -ne 1 -or $script:tasks.Count -ne 2) { throw 'existing legacy task was not read accurately' }
$previousName = 'SolutionBatNext_00000000-0000-4000-8000-000000000001'
$script:tasks[$previousName] = [pscustomobject]@{ TaskName=$previousName }
$script:failRegister = $true
$failed = $false
try { Invoke-Fixture 'register' | Out-Null } catch { $failed = $true }
if (-not $failed -or $script:tasks['DailyBuild_a'].Actions[0].Execute -ne 'K:\Old\old.exe' -or -not $script:tasks.ContainsKey($previousName)) { throw 'failed registration damaged existing tasks' }
$script:failRegister = $false
$r = Invoke-Fixture 'register'
$task = $script:tasks['DailyBuild_a']
if (-not $r.registered -or $r.legacyRegistered -or $r.previousRegistered -or $r.times.Count -ne 2) { throw 'registration or previous-version cleanup failed' }
if ($task.Principal.RunLevel -ne 'Highest' -or $task.Principal.LogonType -ne 'Interactive') { throw 'wrong principal' }
if (-not $task.Settings.StartWhenAvailable -or -not $task.Settings.DontStopOnIdleEnd) { throw 'wrong task settings' }
if ($task.Actions[0].Arguments -ne '--run-task 00000000-0000-4000-8000-000000000001 --data-dir "K:\My Data" --show-gui') { throw 'wrong arguments' }
$d = Invoke-Fixture 'unregister'
if ($d.registered -or $script:tasks.Count -ne 1 -or -not $script:tasks.ContainsKey('DailyBuild_a_extra')) { throw 'delete affected unrelated tasks' }
$d = Invoke-Fixture 'unregister'
if ($d.registered) { throw 'delete absent task failed' }
'scheduler mock roundtrip passed'
"#;
        let result = hidden(
            Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", harness])
                .env("SB_SCRIPT", SCRIPT),
        )
        .output()
        .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
