use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    index: u32,
    name: String,
    android_version: Option<String>,
    status: &'static str,
    adb: Option<String>,
}

#[tauri::command]
pub async fn list_mumu_instances() -> Result<Vec<Instance>, String> {
    #[cfg(windows)]
    {
        let managers = managers();
        if managers.is_empty() {
            return Err(
                "未找到 MuMu 安装。可用 GAME_DAILY_HUB_MUMU_MANAGER 指定 MuMuManager.exe。".into(),
            );
        }
        let mut last_error = String::new();
        for manager in managers {
            match query(&manager).await {
                Ok(instances) => return Ok(instances),
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }
    #[cfg(not(windows))]
    Err("当前 MuMu 查询支持 Windows。".into())
}

#[cfg(windows)]
fn managers() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
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
async fn query(path: &std::path::Path) -> Result<Vec<Instance>, String> {
    use std::{process::Stdio, time::Duration};
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut command = tokio::process::Command::new(path);
    command
        .args(["info", "-v", "all"])
        .current_dir(path.parent().ok_or("MuMu 管理程序路径无效。")?)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(8), command.output())
        .await
        .map_err(|_| "MuMu 实例查询超时，请稍后刷新。")?
        .map_err(|_| "无法运行 MuMu 管理程序，请检查安装和权限。")?;
    if !output.status.success() {
        return Err(format!(
            "MuMu 实例查询失败（退出码 {}）。",
            output
                .status
                .code()
                .map_or_else(|| "未知".to_string(), |code| code.to_string())
        ));
    }
    parse_instances(&output.stdout)
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
        let code = |key: &str| {
            row[key]
                .as_i64()
                .or_else(|| row[key].as_str()?.parse().ok())
                .unwrap_or_default()
        };
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
        });
    }
    if has_rows && instances.is_empty() {
        return Err("MuMu 返回的列表中没有可识别的实例。".into());
    }
    instances.sort_by_key(|instance| instance.index);
    Ok(instances)
}
