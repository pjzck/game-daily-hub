# 探索报告

调研日期：2026-11-10。目标：判断 Windows 上用 Tauri 统一管理游戏账号、MuMu、MAA / M9A 和每日状态的可行性。

## 结论

可采用独立的 Tauri 管理端，加上工具执行、设备管理和社区数据三个适配层。最接近完整需求的 AUTO-MAS 使用 Electron + Vue + Python；最接近目标技术栈的 MXU 使用 Tauri + React + TypeScript，但服务于 MaaFramework PI V2 项目。先验证现有工具的最小接入路径，再实现自己的账号总览和状态管理。

仓库目前没有可运行的 Tauri 应用。以下明确区分本机验证、源码核对和设计建议。

## 已验证的本机能力

这里只发布通用结论；完整路径、实例名称、连接地址和配置位置仅保存在本地被忽略的目录中。

| 项目 | 观察结果 | 影响 |
| --- | --- | --- |
| MuMu | `MuMuManager.exe info -v all` 在沙箱外成功返回多个实例，包含实例索引、运行状态和已运行实例的 ADB 地址 | 可以直接枚举；实例数量与当前运行数量需分别展示 |
| MuMu 查询环境 | 沙箱内首次查询异常退出；沙箱外、以安装目录为工作目录的查询成功 | 环境和工作目录均需纳入诊断，尚不能归因为单一权限问题 |
| MAA | 程序文件版本与旧目录名不一致；新版配置文件包含多个配置组和连接信息 | 不从目录名判断版本；导入适配必须识别配置结构 |
| M9A | `interface.json` 声明 PI V2、项目版本、控制器和导入任务文件；MFAA 程序文件版本不代表资源版本 | 分别保存外壳、框架和项目资源版本 |
| PC 游戏 | 快捷方式可指向统一启动器；存在旧安装或渠道目录 | 快捷方式、注册表、游戏配置与可执行文件要交叉核对 |
| 开发环境 | Node.js、npm、pnpm 和 Git 可发现；Rust 工具链未在 PATH 或常见用户安装位置发现 | 桌面原型编译前需准备 Rust，并核实 Windows C++ 构建工具 |

没有启动或关闭游戏、模拟器，没有修改现有工具配置，没有执行日常任务。没有读取社区凭据，也没有使用个人登录态请求体力数据。

## 现有项目与代码证据

### AUTO-MAS

- [仓库](https://github.com/AUTO-MAS-Project/AUTO-MAS)、[MAA 配置](https://doc.auto-mas.top/docs/script-guide/maa.html)、[M9A 配置](https://doc.auto-mas.top/docs/script-guide/m9a.html)。
- 参考源码快照：`18567cf1234ee1de9e0b83b8b677f27e423e7d2d`。
- [frontend/package.json](https://github.com/AUTO-MAS-Project/AUTO-MAS/blob/18567cf1234ee1de9e0b83b8b677f27e423e7d2d/frontend/package.json) 确认 Electron + Vue；后端为 Python。
- [community_activity_roles.py](https://github.com/AUTO-MAS-Project/AUTO-MAS/blob/18567cf1234ee1de9e0b83b8b677f27e423e7d2d/app/tools/community_activity_roles.py) 登记的只读活动数据源为森空岛和米游社，其中米游社覆盖原神、星穹铁道、绝区零。
- [community_activity_provider.py](https://github.com/AUTO-MAS-Project/AUTO-MAS/blob/18567cf1234ee1de9e0b83b8b677f27e423e7d2d/app/tools/community_activity_provider.py) 包含米游社设备信息、凭据能力、请求间隔及风控状态处理。
- [community_activity_parser.py](https://github.com/AUTO-MAS-Project/AUTO-MAS/blob/18567cf1234ee1de9e0b83b8b677f27e423e7d2d/app/tools/community_activity_parser.py) 包含原粹树脂、洞天宝钱、开拓力、储备开拓力和部分每日任务字段。
- 此快照的库街区模块提供签到相关流程；发现外部鸣潮能力引用，不能因此认定 AUTO-MAS 已接入鸣潮体力便笺。
- 仓库标注 AGPL-3.0，视觉资源另有说明。当前只研究其设计，不复制实现。

### MXU

- [仓库](https://github.com/MaaXYZ/MXU)。参考源码快照：`c3215325461b79533f2cf1269ffbc548499dc89c`。
- [README](https://github.com/MaaXYZ/MXU/blob/c3215325461b79533f2cf1269ffbc548499dc89c/README.md) 确认 Tauri + React + TypeScript、PI V2、多实例、任务日志和定时能力。
- [system.rs](https://github.com/MaaXYZ/MXU/blob/c3215325461b79533f2cf1269ffbc548499dc89c/src-tauri/src/commands/system.rs) 定义 `--autostart`、`--instance`、`--quit-after-run`。
- 命令行支持属于该快照的 MXU 外壳，不能直接套用于本机 M9A 的 MFAA 外壳。需要验证用户版本，或提供独立 MaaFramework 运行器。
- [M9A 的 MaaPiCli 文档](https://1999.fan/zh_cn/manual/cli.html) 另行说明了 CLI 下载入口和 `-d` 跳过交互运行任务；这是候选接入方式，尚未在本机安装或执行。
- 仓库标注 AGPL-3.0。作为技术参考，暂不 fork。

### MAA / MaaCore

- [官方文档](https://docs.maa.plus/zh-cn/)、[集成文档](https://github.com/MaaAssistantArknights/MaaAssistantArknights/blob/dev-v2/docs/zh-cn/protocol/integration.md)。本轮核对的默认分支为 `dev-v2`。
- MAA 的 MaaCore 与 M9A 的 MaaFramework 是不同接入目标。设计独立 RunnerAdapter，不能共享一套底层 API 假设。
- [maa-cli](https://github.com/MaaAssistantArknights/maa-cli) 提供 Rust 命令行接入，但 README 提示 Windows 支持有限，不能直接作为 Windows 首版唯一执行路径。
- 推荐先评估现有 GUI 的受支持启动方式；若状态和停止接口不足，再以独立进程封装 MaaCore。

### 原神 / 崩铁数据

- [genshin.py](https://github.com/seriaati/genshin.py) 是米游社 / HoYoLAB 非官方封装，仓库标注 MIT。
- [原神查询源码](https://github.com/seriaati/genshin.py/blob/master/genshin/client/components/chronicle/genshin.py) 提供 `get_genshin_notes`；[崩铁查询源码](https://github.com/seriaati/genshin.py/blob/master/genshin/client/components/chronicle/starrail.py) 提供 `get_starrail_notes`。
- [认证说明](https://gh.seria.moe/genshin.py/authentication/) 可作为凭据能力研究入口。国服与国际服需分开验证；某种 Cookie 可以签到，不代表足以查询完整便笺。
- 这些实现证明存在读取官方社区数据的路径，不能据此承诺有稳定的第三方开放 API。

### 鸣潮数据

- [wuthering-waves-platform](https://github.com/yangfeng20/wuthering-waves-platform) 声明支持库街区登录、体力、活跃度、周任务等。仓库标注 AGPL-3.0。
- [game.js](https://github.com/yangfeng20/wuthering-waves-platform/blob/main/src/http/game.js) 的 `getGameData` 调用 `/gamer/widget/game3/refresh`。
- [account.js](https://github.com/yangfeng20/wuthering-waves-platform/blob/main/src/services/account.js) 将 Token、设备标识、角色绑定与接口响应分开诊断。
- 需要验证 Token + did 的实际组合、登录互斥、展示开关和数据延迟。当前只确认候选协议路径，没有验证个人账号查询结果。

### 启动器参考

[Starward](https://github.com/Scighost/Starward) 提供米家 PC 游戏启动、账号切换和游戏时长等能力，采用 C#。适合参考启动入口与安装识别；不把它视为所有游戏通用的执行器。

## 数据来源和状态的处理

1. 数据快照保存 `fetchedAt`、`source` 和明确的角色标识；上游若提供数据时间，另存 `observedAt`。
2. 官方查询结果、时间推算、执行器日志和人工确认分开标记。
3. 接口失败时保留上次成功快照，并标记过期、凭据失效、受限或不可用。
4. 不把未知体力显示为零，不把进程退出直接记为日常完成。
5. 每日重置按游戏、服务器及对应任务类型配置；社区签到与游戏日常可以有不同边界。
6. 凭据存入系统安全存储，数据库只持有引用；日志和导出不携带凭据。

## 尚未完成的验证

- 本机 MAA / M9A 支持哪些外部启动、停止和完成通知方式。
- 两个运行实例分别安装的目标游戏，以及 M9A 当前实际使用的控制器和配置组。
- 原神、崩铁、鸣潮通过用户主动登录后的实时数据、刷新延迟和凭据生命周期。
- 工具失败后的可重试边界，停止是否能及时生效，更新是否会影响运行。
- Windows 原生构建依赖与安装包验证。

下一步以一个模拟器和一个只读数据源做最小验证，再进入完整界面实现。上述未验证项不会在产品中标为已支持。

## 补充：个人云主机构建（2026-11-11）

已通过只读 SSH 检查个人 Linux x86_64 云主机，具备 Docker，当前资源可作为构建候选；机器地址、容量与连接信息仅记入本地文件。未安装 Rust 或修改云端服务。

- Rust 按 target 编译产物，源码可移植不代表同一个可执行文件能在 Windows、Linux 和 macOS 通用。[Rust 平台说明](https://doc.rust-lang.org/rustc/platform-support.html)
- Tauri 官方给出了 Linux / macOS 使用 `cargo-xwin`、Windows MSVC target、LLVM / LLD 和 NSIS 构建 Windows 安装包的路径，同时明确指出该路径比 Windows 原生构建限制更多、测试更少。MSI 安装包仍需 Windows 构建。[Tauri Windows 构建文档](https://v2.tauri.app/distribute/windows-installer/#build-windows-apps-on-linux-and-macos)
- 建议先以隔离的构建环境验证最小 Windows 应用，采用手动 SSH 构建和下载产物，不使用 GitHub Actions。
- 桌面程序和本地设备适配器继续在 Windows 运行。用户运行预编译安装包不需要安装 Rust；云端构建不能替代本机的 MuMu、MAA / M9A 与窗口交互验收。
- 首版按 Windows 支持，其他系统的安装发现、凭据存储和工具调用仍需各自适配。

尚未验证：云端工具链安装、完整编译耗时和内存峰值、Windows 产物运行及安装包。暂不将远程构建标记为已支持。
