# Game Daily Hub

使用 **Tauri 2、React、TypeScript 和 Rust** 的 Windows 游戏账号管理桌面端。

## 原型功能

- **账号总览**支持米游社扫码登录，读取绑定的原神和崩铁国服角色及实时便笺。账号卡片显示 UID、区服、等级、体力和更新时间。点「刷新全部」可重新读取，失败时保留上次读到的便笺。
- **手动账号**支持原神、崩铁、明日方舟和重返未来：1999，可填写体力并编辑本地卡片。
- **抽卡记录**支持导入原神 UIGF v3、原神／崩铁 UIGF v4 和崩铁 SRGF v1 的 JSON 文件。按游戏、UID 和记录 ID 合并，可按卡池筛选。
- **MuMu 实例**通过 Windows 安装信息发现管理程序，打开页面后执行只读查询，显示运行状态和 ADB 地址。

在「添加账号 → 米游社扫码」生成二维码，用米游社 App 扫码并在手机上确认。扫码会话可以读取米游社小组件的体力数据。同一游戏绑定多个角色时，程序按各角色的 UID 和区服请求便笺。若米游社要求安全验证或启用实时便笺，按官方 App 中的提示处理后再刷新。连接窗口也支持导入已有 Cookie，并选填同一会话的设备 ID 和设备指纹。

账号与抽卡文件存放在当前 Windows 用户的应用数据目录。米游社凭据通过 Windows DPAPI 加密。国服接口参考 [genshin.py](https://github.com/seriaati/genshin.py) 和 [AUTO-MAS 的扫码协议实现](https://github.com/AUTO-MAS-Project/AUTO-MAS/blob/18567cf1234ee1de9e0b83b8b677f27e423e7d2d/app/tools/miyoushe_qr.py)。

## 本地运行

需要 Node.js 22、Windows x64 MSVC Rust 工具链、Visual Studio C++ 构建工具、Windows SDK 和 WebView2。

```powershell
npm ci
npm run tauri dev
```

在 Windows 本机编译可直接运行的程序。

```powershell
npm run tauri build
```

可执行文件位于项目的 `src-tauri/target/release/game-daily-hub.exe`。可用 `GAME_DAILY_HUB_DATA_DIR` 指定独立的数据目录，或用 `GAME_DAILY_HUB_MUMU_MANAGER` 指定 MuMu 管理程序的绝对路径。

## 目标

- 按账号展示体力、预计回满时间、每日待办和最近运行结果。
- 发现 MuMu 安装及实例，将工具、账号与设备明确关联。
- 通过独立适配器连接现有 MAA / M9A，保留用户原有配置。
- 扩展游戏数据源和可同步的状态字段。
- 记录任务运行、失败原因和需要人工处理的情况。

## 文档

- [探索报告](docs/research.md)：现有项目、代码证据、验证范围和未决问题。
- [架构草案](docs/architecture.md)：数据模型、适配器与任务状态。
- [实施路线](docs/roadmap.md)：阶段目标和验收条件。
- [配置示例](examples/workspace.example.json)：不含凭据的占位数据。

## 运行本机探测

需要 PowerShell 7.2 或更新版本。默认只检查注册表安装项、快捷方式目标及文件版本，不读取浏览器 Cookie、游戏账号凭据或工具的任务配置。

```powershell
pwsh -NoProfile -File scripts/Probe-Environment.ps1
```

显式查询 MuMu 已有实例：

```powershell
pwsh -NoProfile -File scripts/Probe-Environment.ps1 -QueryMumu
```

也可以提供 `-MumuManagerPath`，或用 `-GameRoots` 指定需要检查的游戏目录。实例查询只执行 `info -v all`，不会启动模拟器或游戏；查询程序可能写入自身运行日志。部分环境需要在沙箱外运行，失败会记录为未验证。

默认报告保存在被 Git 忽略的 `.local/environment.json`。报告可能包含本机路径、模拟器名称和 ADB 地址，请在分享前脱敏。

## 检查仓库

安装前端依赖后运行本地检查。

```powershell
npm run check
```

项目使用本地检查，不配置 GitHub Actions。

## 参与和开源范围

欢迎围绕已列出的验证项提交 issue。报告问题时请使用脱敏配置，不要上传 Cookie、Token、账号数据库或完整本机探测报告。

本仓库原创内容采用 MIT 协议。第三方依赖与参考项目保留各自许可。
