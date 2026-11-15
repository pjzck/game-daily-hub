use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{State, WebviewWindow};

#[derive(Clone, Copy, Ord, PartialOrd, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Maa,
    M9a,
    Genshin,
    Starrail,
    Mumu,
}

impl Tool {
    fn executable(self) -> &'static str {
        match self {
            Tool::Maa => "MAA.exe",
            Tool::M9a => "M9A.exe",
            Tool::Genshin => "YuanShen.exe",
            Tool::Starrail => "StarRail.exe",
            Tool::Mumu => "MuMuManager.exe",
        }
    }

    fn path_variable(self) -> &'static str {
        match self {
            Tool::Maa => "GAME_DAILY_HUB_MAA_EXE",
            Tool::M9a => "GAME_DAILY_HUB_M9A_EXE",
            Tool::Genshin => "GAME_DAILY_HUB_GENSHIN_EXE",
            Tool::Starrail => "GAME_DAILY_HUB_STARRAIL_EXE",
            Tool::Mumu => "GAME_DAILY_HUB_MUMU_MANAGER",
        }
    }

    fn matches(self, path: &Path) -> bool {
        path.file_name().is_some_and(|name| {
            name.eq_ignore_ascii_case(self.executable())
                || (self == Tool::Genshin && name.eq_ignore_ascii_case("GenshinImpact.exe"))
        })
    }

    pub(crate) fn valid_path(self, path: &Path) -> bool {
        path.is_absolute() && self.matches(path) && path.is_file()
    }
}

pub struct LaunchSettings {
    file: PathBuf,
    access: Mutex<()>,
}

impl LaunchSettings {
    pub fn new(directory: &Path) -> Self {
        Self {
            file: directory.join("launchers.json"),
            access: Mutex::new(()),
        }
    }

    fn load(&self) -> Result<BTreeMap<Tool, PathBuf>, String> {
        match fs::read(&self.file) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| "本地启动路径配置无法读取。".into())
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(_) => Err("无法访问本地启动路径配置。".into()),
        }
    }

    fn paths(&self) -> Result<BTreeMap<Tool, PathBuf>, String> {
        let _access = self.access.lock().map_err(|_| "无法读取启动路径配置。")?;
        self.load()
    }

    pub(crate) fn path(&self, tool: Tool) -> Result<Option<PathBuf>, String> {
        Ok(self.paths()?.remove(&tool))
    }

    fn set(&self, tool: Tool, path: Option<PathBuf>) -> Result<(), String> {
        let _access = self.access.lock().map_err(|_| "无法保存启动路径配置。")?;
        let mut paths = self.load()?;
        if let Some(path) = path {
            if !tool.valid_path(&path) {
                let expected = if tool == Tool::Genshin {
                    "YuanShen.exe 或 GenshinImpact.exe"
                } else {
                    tool.executable()
                };
                return Err(format!("请选择 {expected}。"));
            }
            paths.insert(tool, path);
        } else if paths.remove(&tool).is_none() {
            return Ok(());
        }
        let directory = self.file.parent().ok_or("启动路径配置目录无效。")?;
        let error = "无法保存本地启动路径配置。";
        fs::create_dir_all(directory).map_err(|_| error)?;
        let mut temp = tempfile::NamedTempFile::new_in(directory).map_err(|_| error)?;
        serde_json::to_writer(&mut temp, &paths).map_err(|_| error)?;
        temp.flush().map_err(|_| error)?;
        temp.as_file().sync_all().map_err(|_| error)?;
        temp.persist(&self.file).map_err(|_| error)?;
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn overrides_survive_reopen_and_can_be_reset_independently() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("settings");
        let game = temp.path().join("YuanShen.exe");
        let manager = temp.path().join("MuMuManager.exe");
        let launcher = temp.path().join("launcher.exe");
        for path in [&game, &manager, &launcher] {
            fs::write(path, []).unwrap();
        }
        let settings = LaunchSettings::new(&directory);
        assert!(settings.path(Tool::Genshin).unwrap().is_none());
        settings.set(Tool::Genshin, Some(game.clone())).unwrap();
        assert!(settings.set(Tool::Genshin, Some(launcher)).is_err());
        settings.set(Tool::Mumu, Some(manager.clone())).unwrap();
        let settings = LaunchSettings::new(&directory);
        assert_eq!(settings.path(Tool::Genshin).unwrap(), Some(game));
        assert_eq!(settings.path(Tool::Mumu).unwrap(), Some(manager.clone()));
        settings.set(Tool::Genshin, None).unwrap();
        assert!(settings.path(Tool::Genshin).unwrap().is_none());
        assert_eq!(settings.path(Tool::Mumu).unwrap(), Some(manager));
    }
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
    path: Option<PathBuf>,
    source: Source,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Source {
    Manual,
    Environment,
    Auto,
}

#[tauri::command]
pub async fn list_automation_tools(
    settings: State<'_, LaunchSettings>,
) -> Result<Vec<ToolStatus>, String> {
    #[cfg(windows)]
    {
        let paths = settings.paths()?;
        tauri::async_runtime::spawn_blocking(move || windows_launcher::list(&paths))
            .await
            .map_err(|_| "无法读取本机辅助工具。".into())
    }
    #[cfg(not(windows))]
    {
        let _ = settings;
        Err("当前本机启动功能支持 Windows。".into())
    }
}

#[tauri::command]
pub async fn launch_automation_tool(
    tool: Tool,
    settings: State<'_, LaunchSettings>,
) -> Result<ToolStatus, String> {
    #[cfg(windows)]
    {
        let path = settings.path(tool)?;
        tauri::async_runtime::spawn_blocking(move || windows_launcher::launch(tool, path))
            .await
            .map_err(|_| "无法启动辅助工具。")?
    }
    #[cfg(not(windows))]
    {
        let _ = (tool, settings);
        Err("当前本机启动功能支持 Windows。".into())
    }
}

#[tauri::command]
pub async fn choose_launch_path(
    tool: Tool,
    window: WebviewWindow,
    settings: State<'_, LaunchSettings>,
) -> Result<Option<ToolStatus>, String> {
    #[cfg(windows)]
    {
        let owner = window.hwnd().map_err(|_| "无法打开文件选择窗口。")?.0 as isize;
        let path =
            tauri::async_runtime::spawn_blocking(move || windows_launcher::pick(tool, owner))
                .await
                .map_err(|_| "文件选择已中断。")??;
        let Some(path) = path else { return Ok(None) };
        settings.set(tool, Some(path.clone()))?;
        refresh_tool(tool, Some(path)).await.map(Some)
    }
    #[cfg(not(windows))]
    {
        let _ = (tool, window, settings);
        Err("当前本机启动功能支持 Windows。".into())
    }
}

#[tauri::command]
pub async fn reset_launch_path(
    tool: Tool,
    settings: State<'_, LaunchSettings>,
) -> Result<ToolStatus, String> {
    #[cfg(windows)]
    {
        settings.set(tool, None)?;
        refresh_tool(tool, None).await
    }
    #[cfg(not(windows))]
    {
        let _ = (tool, settings);
        Err("当前本机启动功能支持 Windows。".into())
    }
}

#[cfg(windows)]
async fn refresh_tool(tool: Tool, path: Option<PathBuf>) -> Result<ToolStatus, String> {
    tauri::async_runtime::spawn_blocking(move || windows_launcher::inspect(tool, path))
        .await
        .map_err(|_| "无法读取本机程序。".into())
}

#[cfg(windows)]
mod windows_launcher {
    use super::{Source, Status, Tool, ToolStatus};
    use std::{
        collections::BTreeMap,
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
            Foundation::{ERROR_CANCELLED, HANDLE, HWND, RPC_E_CHANGED_MODE},
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
                Common::COMDLG_FILTERSPEC, FOLDERID_CommonPrograms, FOLDERID_Desktop,
                FOLDERID_Programs, FOLDERID_PublicDesktop, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
                FOS_NOCHANGEDIR, FileOpenDialog, IFileOpenDialog, IShellLinkW, KF_FLAG_DEFAULT,
                SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, SHGetKnownFolderPath, SIGDN_FILESYSPATH,
                ShellExecuteExW, ShellLink,
            },
        },
        core::{GUID, Interface, PCWSTR, PWSTR, w},
    };

    const TOOLS: [Tool; 5] = [
        Tool::Maa,
        Tool::M9a,
        Tool::Genshin,
        Tool::Starrail,
        Tool::Mumu,
    ];
    static LAUNCHING: Mutex<()> = Mutex::new(());
    static PICKING: Mutex<()> = Mutex::new(());

    pub(super) fn list(paths: &BTreeMap<Tool, PathBuf>) -> Vec<ToolStatus> {
        let running = running_paths();
        TOOLS
            .into_iter()
            .map(|tool| status(tool, paths.get(&tool).cloned(), &running))
            .collect()
    }

    pub(super) fn inspect(tool: Tool, path: Option<PathBuf>) -> ToolStatus {
        status(tool, path, &running_paths())
    }

    fn status(tool: Tool, manual: Option<PathBuf>, running: &[PathBuf]) -> ToolStatus {
        let (path, source) = find_executable(tool, manual);
        let status = match path.as_deref().filter(|path| tool.valid_path(path)) {
            Some(path) if tool != Tool::Mumu && is_running(path, running) => Status::Running,
            Some(_) => Status::Ready,
            None => Status::Missing,
        };
        ToolStatus {
            tool,
            status,
            path,
            source,
        }
    }

    pub(super) fn launch(tool: Tool, manual: Option<PathBuf>) -> Result<ToolStatus, String> {
        if tool == Tool::Mumu {
            return Err("请在 MuMu 实例列表中选择要启动的实例。".into());
        }
        let (path, source) = find_executable(tool, manual);
        let path = path.filter(|path| tool.valid_path(path)).ok_or_else(|| {
            format!(
                "未找到 {}，请手动选择可执行文件或重新检测。",
                tool.executable()
            )
        })?;
        let _guard = LAUNCHING.lock().unwrap_or_else(|error| error.into_inner());
        if !is_running(&path, &running_paths()) {
            let directory = path.parent().ok_or("安装目录无效。")?;
            if matches!(tool, Tool::Genshin | Tool::Starrail) {
                open_game(&path, directory)?;
            } else {
                Command::new(&path)
                    .current_dir(directory)
                    .spawn()
                    .map_err(|error| format!("无法打开 {}（{}）。", tool.executable(), error))?;
            }
        }
        Ok(ToolStatus {
            tool,
            status: Status::Running,
            path: Some(path),
            source,
        })
    }

    fn find_executable(tool: Tool, manual: Option<PathBuf>) -> (Option<PathBuf>, Source) {
        if manual.is_some() {
            return (manual, Source::Manual);
        }
        if let Some(path) = env::var_os(tool.path_variable()).map(PathBuf::from) {
            return (Some(path), Source::Environment);
        }
        let detected = match tool {
            Tool::Mumu => crate::mumu::managers().into_iter().next(),
            Tool::Genshin | Tool::Starrail => hoyoplay_game(tool).or_else(|| find_shortcut(tool)),
            _ => find_shortcut(tool),
        };
        (detected, Source::Auto)
    }

    fn find_shortcut(tool: Tool) -> Option<PathBuf> {
        let _com = ComApartment::initialize()?;
        let resolve = |path: PathBuf| {
            if tool.valid_path(&path) {
                return Some(path);
            }
            if matches!(tool, Tool::Genshin | Tool::Starrail)
                && path
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("launcher.exe"))
            {
                let config = fs::read_to_string(path.parent()?.join("config.ini")).ok()?;
                let directory = config
                    .lines()
                    .filter_map(|line| line.split_once('='))
                    .find(|(key, _)| key.trim().eq_ignore_ascii_case("game_install_path"))?
                    .1;
                return game_in_directory(tool, Path::new(directory.trim().trim_matches('"')));
            }
            None
        };
        [
            (&FOLDERID_Desktop, 0),
            (&FOLDERID_PublicDesktop, 0),
            (&FOLDERID_Programs, 4),
            (&FOLDERID_CommonPrograms, 4),
        ]
        .into_iter()
        .find_map(|(folder, depth)| find_shortcut_target(&known_folder(folder)?, depth, &resolve))
    }

    fn game_in_directory(tool: Tool, directory: &Path) -> Option<PathBuf> {
        let path = directory.join(tool.executable());
        if tool.valid_path(&path) {
            return Some(path);
        }
        (tool == Tool::Genshin)
            .then(|| directory.join("GenshinImpact.exe"))
            .filter(|path| tool.valid_path(path))
    }

    fn hoyoplay_game(tool: Tool) -> Option<PathBuf> {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        let editions = match tool {
            Tool::Genshin => ["hk4e_cn", "hk4e_global"],
            Tool::Starrail => ["hkrpg_cn", "hkrpg_global"],
            _ => return None,
        };
        let user = RegKey::predef(HKEY_CURRENT_USER);
        for edition in editions {
            for root in [r"Software\miHoYo\HYP", r"Software\Cognosphere\HYP"] {
                let Ok(launcher) = user.open_subkey(root) else {
                    continue;
                };
                for channel in launcher.enum_keys().flatten() {
                    let Ok(game) = launcher.open_subkey(format!("{channel}\\{edition}")) else {
                        continue;
                    };
                    let Ok(directory) = game.get_value::<String, _>("GameInstallPath") else {
                        continue;
                    };
                    if let Some(path) = game_in_directory(tool, Path::new(&directory)) {
                        return Some(path);
                    }
                }
            }
        }
        None
    }

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain([0]).collect()
    }

    fn open_game(path: &Path, directory: &Path) -> Result<(), String> {
        let _com = ComApartment::initialize().ok_or("无法初始化 Windows 启动服务。")?;
        let executable = wide(path);
        let directory = wide(directory);
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC,
            lpVerb: w!("open"),
            lpFile: PCWSTR(executable.as_ptr()),
            lpDirectory: PCWSTR(directory.as_ptr()),
            nShow: 1, // SW_SHOWNORMAL
            ..Default::default()
        };
        unsafe { ShellExecuteExW(&mut info) }.map_err(|error| {
            if error.code() == ERROR_CANCELLED.to_hresult() {
                "已取消启动游戏。".into()
            } else {
                format!("无法打开游戏（{}）。", error.code())
            }
        })
    }

    pub(super) fn pick(tool: Tool, owner: isize) -> Result<Option<PathBuf>, String> {
        let _picking = PICKING.try_lock().map_err(|_| "已有文件选择窗口打开。")?;
        let _com = ComApartment::initialize().ok_or("无法打开文件选择窗口。")?;
        let mask = if tool == Tool::Genshin {
            w!("YuanShen.exe;GenshinImpact.exe")
        } else {
            w!("*.exe")
        };
        let title = format!("选择 {}", tool.executable());
        let title: Vec<u16> = title.encode_utf16().chain([0]).collect();
        let pick = || unsafe {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            dialog.SetTitle(PCWSTR(title.as_ptr()))?;
            dialog.SetOptions(
                dialog.GetOptions()? | FOS_FORCEFILESYSTEM | FOS_FILEMUSTEXIST | FOS_NOCHANGEDIR,
            )?;
            dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                pszName: w!("可执行文件"),
                pszSpec: mask,
            }])?;
            dialog.Show(Some(HWND(owner as *mut _)))?;
            let selected = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = PathBuf::from(OsString::from_wide(selected.as_wide()));
            CoTaskMemFree(Some(selected.0.cast()));
            Ok::<_, windows::core::Error>(path)
        };
        match pick() {
            Ok(path) => Ok(Some(path)),
            Err(error) if error.code() == ERROR_CANCELLED.to_hresult() => Ok(None),
            Err(_) => Err("无法打开或读取文件选择窗口。".into()),
        }
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
        resolve: &impl Fn(PathBuf) -> Option<PathBuf>,
    ) -> Option<PathBuf> {
        for entry in fs::read_dir(directory).ok()?.flatten() {
            let path = entry.path();
            if depth > 0 && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                if let Some(target) = find_shortcut_target(&path, depth - 1, resolve) {
                    return Some(target);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
                && let Some(target) = shortcut_target(&path).and_then(resolve)
            {
                return Some(target);
            }
        }
        None
    }

    fn shortcut_target(path: &Path) -> Option<PathBuf> {
        let filename = wide(path);
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
            if TOOLS
                .iter()
                .any(|tool| *tool != Tool::Mumu && tool.matches(&executable))
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
