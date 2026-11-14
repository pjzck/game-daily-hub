use serde::Serialize;
use serde_json::Value;
#[cfg(windows)]
use std::{
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};

#[cfg(windows)]
static STARTING_INSTANCE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    index: u32,
    name: String,
    android_version: Option<String>,
    status: &'static str,
    adb: Option<String>,
    can_start: bool,
}

#[tauri::command]
pub async fn list_mumu_instances() -> Result<Vec<Instance>, String> {
    #[cfg(windows)]
    {
        resolve_installation().await.map(|(_, instances)| instances)
    }
    #[cfg(not(windows))]
    Err("当前 MuMu 查询支持 Windows。".into())
}

#[tauri::command]
pub async fn start_mumu_instance(index: u32, name: String) -> Result<Vec<Instance>, String> {
    #[cfg(windows)]
    {
        let _start = STARTING_INSTANCE
            .try_lock()
            .map_err(|_| "已有 MuMu 实例正在启动。")?;
        let (manager, instances) = resolve_installation().await?;
        let instance = find_instance(&instances, index)?;
        if instance.name != name {
            return Err("MuMu 实例已变化，请刷新列表后重试。".into());
        }
        match instance.status {
            "running" => return Ok(instances),
            "starting" => {}
            _ if instance.can_start => launch(&manager, index).await?,
            _ => return Err("无法确认该实例已停止，请在 MuMu 中检查后刷新。".into()),
        }

        tokio::time::timeout(Duration::from_secs(60), wait_for_android(&manager, index))
            .await
            .map_err(|_| "MuMu 未在 60 秒内完成启动，请刷新实例查看最新状态。")?
    }
    #[cfg(not(windows))]
    {
        let _ = (index, name);
        Err("当前 MuMu 启动支持 Windows。".into())
    }
}

#[cfg(windows)]
async fn resolve_installation() -> Result<(PathBuf, Vec<Instance>), String> {
    let mut error =
        "未找到 MuMu 安装。可用 GAME_DAILY_HUB_MUMU_MANAGER 指定 MuMuManager.exe。".into();
    for manager in managers() {
        match query(&manager).await {
            Ok(instances) => return Ok((manager, instances)),
            Err(reason) => error = reason,
        }
    }
    Err(error)
}

#[cfg(windows)]
fn find_instance(instances: &[Instance], index: u32) -> Result<&Instance, String> {
    instances
        .iter()
        .find(|instance| instance.index == index)
        .ok_or_else(|| "找不到该 MuMu 实例，请刷新列表。".into())
}

#[cfg(windows)]
fn managers() -> Vec<PathBuf> {
    use winreg::{
        RegKey,
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        },
    };

    let mut candidates = Vec::new();
    if let Some(override_path) = std::env::var_os("GAME_DAILY_HUB_MUMU_MANAGER") {
        let path = PathBuf::from(override_path);
        if path.is_absolute()
            && path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("MuMuManager.exe"))
            && path.is_file()
        {
            candidates.push(path);
        }
        return candidates;
    }
    let uninstall = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    for (hive, view) in [
        (HKEY_CURRENT_USER, KEY_WOW64_64KEY),
        (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
        (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
    ] {
        let Ok(root) = RegKey::predef(hive).open_subkey_with_flags(uninstall, KEY_READ | view)
        else {
            continue;
        };
        for key in root.enum_keys().flatten() {
            let Ok(entry) = root.open_subkey_with_flags(key, KEY_READ | view) else {
                continue;
            };
            let name: String = entry.get_value("DisplayName").unwrap_or_default();
            if !name.to_ascii_lowercase().contains("mumu") {
                continue;
            }
            let location: String = entry.get_value("InstallLocation").unwrap_or_default();
            let location =
                PathBuf::from(location.trim_matches(|c: char| c.is_whitespace() || c == '"'));
            if !location.is_absolute() {
                continue;
            }
            for directory in ["nx_main", "shell", ""] {
                let path = location.join(directory).join("MuMuManager.exe");
                if path.is_file() && !candidates.contains(&path) {
                    candidates.push(path);
                }
            }
        }
    }
    candidates
}

#[cfg(windows)]
async fn manager_output(path: &Path, args: &[&str], timeout: Duration) -> Result<Output, String> {
    use std::process::Stdio;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = tokio::process::Command::new(path);
    command
        .args(args)
        .current_dir(path.parent().ok_or("MuMu 管理程序路径无效。")?)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| "MuMu 管理程序响应超时，请刷新实例查看状态。")?
        .map_err(|_| "无法运行 MuMu 管理程序，请检查安装和权限。".into())
}

#[cfg(windows)]
fn check_exit(output: &Output, operation: &str) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "MuMu {operation}失败（退出码 {}）。",
            output
                .status
                .code()
                .map_or_else(|| "未知".to_string(), |code| code.to_string())
        ))
    }
}

#[cfg(windows)]
async fn query(path: &Path) -> Result<Vec<Instance>, String> {
    let output = manager_output(path, &["info", "-v", "all"], Duration::from_secs(8)).await?;
    check_exit(&output, "实例查询")?;
    parse_instances(&output.stdout)
}

#[cfg(windows)]
async fn launch(path: &Path, index: u32) -> Result<(), String> {
    let index = index.to_string();
    let output = manager_output(
        path,
        &["control", "-v", &index, "launch"],
        Duration::from_secs(15),
    )
    .await?;
    check_exit(&output, "启动")?;
    check_launch_reply(&output.stdout)
}

fn check_launch_reply(bytes: &[u8]) -> Result<(), String> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    // Some releases report an application error in JSON while exiting with code zero.
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    let error = |reply: &Value| {
        ["errcode", "error_code"]
            .into_iter()
            .filter_map(|key| read_code(&reply[key]))
            .find(|&code| code != 0)
    };
    let code = error(&value).or_else(|| value.as_object()?.values().find_map(error));
    match code {
        Some(code) => Err(format!("MuMu 管理程序拒绝启动（错误码 {code}）。")),
        None => Ok(()),
    }
}

#[cfg(windows)]
async fn wait_for_android(path: &Path, index: u32) -> Result<Vec<Instance>, String> {
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let instances = query(path)
            .await
            .map_err(|reason| format!("已发出启动请求。{reason}"))?;
        match find_instance(&instances, index)?.status {
            "running" => return Ok(instances),
            "error" => return Err("MuMu 实例报告启动异常，请在模拟器中检查。".into()),
            _ => {}
        }
    }
}

fn read_code(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_str()?.parse().ok())
}

fn parse_instances(bytes: &[u8]) -> Result<Vec<Instance>, String> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| "MuMu 返回的数据格式无法识别。")?;
    let rows: Vec<(Option<&str>, &Value)> = match &value {
        Value::Object(entries) => entries
            .iter()
            .map(|(key, value)| (Some(key.as_str()), value))
            .collect(),
        Value::Array(entries) => entries.iter().map(|value| (None, value)).collect(),
        _ => return Err("MuMu 返回的实例列表格式不受支持。".into()),
    };
    let has_rows = !rows.is_empty();
    let mut instances = Vec::new();
    for (key, row) in rows {
        if !row.is_object() {
            continue;
        }
        let index = row["index"]
            .as_u64()
            .and_then(|value| value.try_into().ok())
            .or_else(|| row["index"].as_str().or(key)?.parse::<u32>().ok());
        let Some(index) = index else { continue };
        let code = |key: &str| read_code(&row[key]).unwrap_or_default();
        let android_started = row["is_android_started"].as_bool();
        let process_started = row["is_process_started"].as_bool();
        let status = if code("error_code") != 0 || code("launch_err_code") != 0 {
            "error"
        } else if android_started == Some(true) {
            "running"
        } else if process_started == Some(true) {
            "starting"
        } else if process_started == Some(false) {
            "stopped"
        } else {
            "unknown"
        };
        let adb = row["adb_host_ip"]
            .as_str()
            .filter(|host| !host.is_empty())
            .zip(
                row["adb_port"]
                    .as_u64()
                    .or_else(|| row["adb_port"].as_str()?.parse().ok()),
            )
            .filter(|(_, port)| (1..=65535).contains(port))
            .map(|(host, port)| format!("{host}:{port}"));
        instances.push(Instance {
            index,
            name: row["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .map_or_else(|| format!("MuMu {index}"), str::to_owned),
            android_version: row["android_version"].as_str().map(str::to_owned),
            status,
            adb,
            can_start: process_started == Some(false) && android_started != Some(true),
        });
    }
    if has_rows && instances.is_empty() {
        return Err("MuMu 返回的列表中没有可识别的实例。".into());
    }
    instances.sort_by_key(|instance| instance.index);
    Ok(instances)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_reply_checks_manager_errors_even_when_the_command_exited_normally() {
        assert!(check_launch_reply(br#"{"errcode":0}"#).is_ok());
        assert!(check_launch_reply(b"").is_ok());
        assert!(
            check_launch_reply(br#"{"2":{"errcode":"15"}}"#)
                .unwrap_err()
                .contains("15")
        );
    }

    #[test]
    fn only_a_stopped_process_can_be_launched_and_only_ready_android_is_running() {
        let instances = parse_instances(
            br#"{
            "0":{"is_process_started":false,"is_android_started":false},
            "1":{"is_process_started":true,"is_android_started":false},
            "2":{"is_process_started":true,"is_android_started":true}
        }"#,
        )
        .unwrap();
        assert!(instances[0].can_start);
        assert_eq!(instances[1].status, "starting");
        assert!(!instances[1].can_start);
        assert_eq!(instances[2].status, "running");
        assert!(!instances[2].can_start);
    }
}
