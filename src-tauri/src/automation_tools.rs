use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Maa,
    M9a,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Status {
    Ready,
    Running,
    Missing,
}

#[derive(Serialize)]
pub struct ToolStatus {
    tool: Tool,
    status: Status,
}

#[tauri::command]
pub async fn list_automation_tools() -> Result<Vec<ToolStatus>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(windows_launcher::list)
            .await
            .map_err(|_| "无法读取本机辅助工具。".into())
    }
    #[cfg(not(windows))]
    Err("当前辅助工具启动支持 Windows。".into())
}

#[tauri::command]
pub async fn launch_automation_tool(tool: Tool) -> Result<ToolStatus, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || windows_launcher::launch(tool))
            .await
            .map_err(|_| "无法启动辅助工具。")?
    }
    #[cfg(not(windows))]
    {
        let _ = tool;
        Err("当前辅助工具启动支持 Windows。".into())
    }
}

#[cfg(windows)]
mod windows_launcher {
    use super::{Status, Tool, ToolStatus};
    use std::{
        env,
        ffi::OsString,
        fs,
        os::windows::{
            ffi::{OsStrExt, OsStringExt},
            io::{FromRawHandle, OwnedHandle},
        },
        path::{Path, PathBuf},
        process::Command,
        sync::Mutex,
    };
    use windows::{
        Win32::{
            Foundation::{HANDLE, RPC_E_CHANGED_MODE},
            System::{
                Com::{
                    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance,
                    CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ,
                },
                Diagnostics::ToolHelp::{
                    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                    TH32CS_SNAPPROCESS,
                },
                Threading::{
                    OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
                    QueryFullProcessImageNameW,
                },
            },
            UI::Shell::{
                FOLDERID_CommonPrograms, FOLDERID_Desktop, FOLDERID_Programs,
                FOLDERID_PublicDesktop, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
                ShellLink,
            },
        },
        core::{GUID, Interface, PCWSTR, PWSTR},
    };

    const TOOLS: [Tool; 2] = [Tool::Maa, Tool::M9a];
    static LAUNCHING: Mutex<()> = Mutex::new(());

    impl Tool {
        fn executable(self) -> &'static str {
            match self {
                Tool::Maa => "MAA.exe",
                Tool::M9a => "M9A.exe",
            }
        }

        fn path_variable(self) -> &'static str {
            match self {
                Tool::Maa => "GAME_DAILY_HUB_MAA_EXE",
                Tool::M9a => "GAME_DAILY_HUB_M9A_EXE",
            }
        }

        fn matches(self, path: &Path) -> bool {
            path.file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(self.executable()))
        }
    }

    pub(super) fn list() -> Vec<ToolStatus> {
        let running = running_paths();
        TOOLS
            .into_iter()
            .map(|tool| {
                let status = match find_executable(tool) {
                    Some(path) if is_running(&path, &running) => Status::Running,
                    Some(_) => Status::Ready,
                    None => Status::Missing,
                };
                ToolStatus { tool, status }
            })
            .collect()
    }

    pub(super) fn launch(tool: Tool) -> Result<ToolStatus, String> {
        let path = find_executable(tool).ok_or_else(|| {
            format!(
                "未找到 {}。请添加桌面快捷方式，或设置 {} 后重启 Game Daily Hub。",
                tool.executable(),
                tool.path_variable()
            )
        })?;
        let _guard = LAUNCHING.lock().unwrap_or_else(|error| error.into_inner());
        if !is_running(&path, &running_paths()) {
            let directory = path.parent().ok_or("辅助工具安装目录无效。")?;
            Command::new(&path)
                .current_dir(directory)
                .spawn()
                .map_err(|error| format!("无法打开 {}（{}）。", tool.executable(), error))?;
        }
        Ok(ToolStatus {
            tool,
            status: Status::Running,
        })
    }

    fn find_executable(tool: Tool) -> Option<PathBuf> {
        let valid = |path: &Path| path.is_absolute() && tool.matches(path) && path.is_file();
        if let Some(path) = env::var_os(tool.path_variable()).map(PathBuf::from) {
            return valid(&path).then_some(path);
        }

        let _com = ComApartment::initialize()?;
        [
            (&FOLDERID_Desktop, 0),
            (&FOLDERID_PublicDesktop, 0),
            (&FOLDERID_Programs, 4),
            (&FOLDERID_CommonPrograms, 4),
        ]
        .into_iter()
        .find_map(|(folder, depth)| find_shortcut_target(&known_folder(folder)?, depth, &valid))
    }

    // Shell links can be read on an existing COM apartment too. Balance only
    // the initializations that succeed on this thread.
    struct ComApartment(bool);

    impl ComApartment {
        fn initialize() -> Option<Self> {
            let status = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            (status.is_ok() || status == RPC_E_CHANGED_MODE).then_some(Self(status.is_ok()))
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    fn known_folder(folder: &GUID) -> Option<PathBuf> {
        unsafe {
            let path = SHGetKnownFolderPath(folder, KF_FLAG_DEFAULT, None).ok()?;
            let directory = PathBuf::from(OsString::from_wide(path.as_wide()));
            CoTaskMemFree(Some(path.0.cast()));
            Some(directory)
        }
    }

    fn find_shortcut_target(
        directory: &Path,
        depth: usize,
        valid: &impl Fn(&Path) -> bool,
    ) -> Option<PathBuf> {
        for entry in fs::read_dir(directory).ok()?.flatten() {
            let path = entry.path();
            if depth > 0 && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                if let Some(target) = find_shortcut_target(&path, depth - 1, valid) {
                    return Some(target);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
                && let Some(target) = shortcut_target(&path)
                && valid(&target)
            {
                return Some(target);
            }
        }
        None
    }

    fn shortcut_target(path: &Path) -> Option<PathBuf> {
        let filename: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let mut target = vec![0; 32_768];
        unsafe {
            let link: IShellLinkW =
                CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
            link.cast::<IPersistFile>()
                .ok()?
                .Load(PCWSTR(filename.as_ptr()), STGM_READ)
                .ok()?;
            link.GetPath(&mut target, std::ptr::null_mut(), 0).ok()?;
        }
        Some(path_from_buffer(&target))
    }

    fn path_from_buffer(buffer: &[u16]) -> PathBuf {
        let end = buffer
            .iter()
            .position(|&value| value == 0)
            .unwrap_or(buffer.len());
        PathBuf::from(OsString::from_wide(&buffer[..end]))
    }

    fn running_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
            return paths;
        };
        let _handle = unsafe { OwnedHandle::from_raw_handle(snapshot.0) };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut result = unsafe { Process32FirstW(snapshot, &mut entry) };
        while result.is_ok() {
            let executable = path_from_buffer(&entry.szExeFile);
            if TOOLS.iter().any(|tool| tool.matches(&executable))
                && let Some(path) = process_path(entry.th32ProcessID)
            {
                paths.push(path);
            }
            result = unsafe { Process32NextW(snapshot, &mut entry) };
        }
        paths
    }

    fn process_path(pid: u32) -> Option<PathBuf> {
        let mut buffer = vec![0; 32_768];
        let mut len = buffer.len() as u32;
        unsafe {
            let HANDLE(raw) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let _handle = OwnedHandle::from_raw_handle(raw);
            QueryFullProcessImageNameW(
                HANDLE(raw),
                PROCESS_NAME_FORMAT(0),
                PWSTR(buffer.as_mut_ptr()),
                &mut len,
            )
            .ok()?;
        }
        Some(path_from_buffer(&buffer[..len as usize]))
    }

    fn is_running(path: &Path, running: &[PathBuf]) -> bool {
        // Resolve junctions and consistently compare Win32 paths, which normally
        // differ only in casing across Shell Link and process metadata.
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        running.iter().any(|candidate| {
            let candidate = fs::canonicalize(candidate).unwrap_or_else(|_| candidate.clone());
            candidate.as_os_str().eq_ignore_ascii_case(path.as_os_str())
        })
    }
}
