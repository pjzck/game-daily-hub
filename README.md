# Game Daily Hub

一个计划使用 **Tauri 2 + React + TypeScript + Rust** 构建的手游账号与每日任务桌面管理端。首期目标平台为 Windows，集中管理 MuMu、MAA、M9A，以及原神、崩坏：星穹铁道和鸣潮的 PC 客户端。

**当前阶段：探索与设计。尚未实现桌面界面、游戏任务执行器或个人数据登录。** 本仓库目前提供调研报告、架构草案、配置示例和本机只读探测脚本。

## 目标

- 按账号展示体力、预计回满时间、每日待办和最近运行结果。
- 发现 MuMu 安装及实例，将工具、账号与设备明确关联。
- 通过独立适配器连接现有 MAA / M9A，保留用户原有配置。
- 接入米游社 / HoYoLAB 与库街区的数据查询；显示来源、更新时间及过期状态。
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

需要 Node.js 22 或更新版本，无须安装 npm 依赖：

```powershell
npm run check
```

项目使用本地检查，不配置 GitHub Actions。

## 参与和开源范围

欢迎围绕已列出的验证项提交 issue。报告问题时请使用脱敏配置，不要上传 Cookie、Token、账号数据库或完整本机探测报告。

本仓库原创内容采用 MIT 协议。外部项目均单独保留其许可；当前只引用公开文档和代码位置，未复制其实现、打包其程序或采用其美术资源。未来引入第三方代码时逐项记录来源和许可。
