# LagScope 卡顿透视

> 面向 Windows 桌面用户的卡顿诊断工具：实时仪表盘、自动卡顿事件检测归因、事件时间线复盘与悬浮窗常驻，四合一。

[![CI](https://github.com/StarAsh042/LagScope/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/StarAsh042/LagScope/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## 目录

- [简介](#简介)
- [功能](#功能)
- [卡顿检测机制](#卡顿检测机制)
- [截图](#截图)
- [前置条件](#前置条件)
- [安装](#安装)
- [快速开始](#快速开始)
- [配置](#配置)
- [兼容性](#兼容性)
- [项目结构](#项目结构)
- [文档导航](#文档导航)
- [参与开发](#参与开发)
- [许可证](#许可证)
- [致谢](#致谢)

## 简介

LagScope 卡顿透视是一个 Windows 桌面应用，用于回答「电脑刚才为什么卡」。它在后台以 1 Hz 采集系统指标，持续评估多种卡顿信号，命中阈值时自动生成带中文归因描述的事件，并把触发时刻的计数器快照、Top 进程与窗口状态存入本地 SQLite，供事后复盘。

采集与判定全部在本机完成，数据不离开设备。适用人群：需要定位间歇性卡顿原因的 Windows 用户、游戏玩家与桌面运维人员。不适用的场景：服务器集群监控、跨平台（macOS / Linux）使用、需要远程集中收集指标的环境。

本项目采用 Tauri 2 + Svelte 5 + TypeScript + Rust。当前版本 `1.1.1`，仅面向 Windows，维护状态为活跃。

## 功能

- **实时仪表盘**：CPU（总 / 每核）、内存 / 提交内存、磁盘（速率 / 活跃 / 延迟 / 队列）、GPU（3D 引擎 / 显存）、网络、DPC 与硬缺页、帧率、热区温度的 1 Hz 实时曲线；进程表支持排序、过滤、结束进程与高耗标记。
- **卡顿自动归因**：后台滚动窗口持续评估以下信号，命中阈值即生成事件（带中文归因描述），存入 SQLite。
  - CPU 满载持续（默认 85% × 10s）。
  - 提交内存逼近上限（默认 90%）。
  - 硬缺页风暴（默认 500 页/秒）。
  - 磁盘平均延迟超标（默认 25 ms，持续 5s）。
  - 中断时间占比过高（默认 20%）。
  - 无响应（挂起）窗口检测（与任务管理器「未响应」同判定）。
  - 游戏帧间隔尖峰（PresentMon，fps ≥ 45 时最大间隔 ≥ 100 ms）。
  - DPC 驱动风暴（ETW 内核跟踪，归因到具体驱动文件）。
- **事件时间线复盘**：每个事件记录触发时刻的完整计数器快照、CPU / 内存 / I/O Top5 进程、挂起窗口列表、帧统计与 DPC 驱动分布；支持手动「立即诊断」。
- **悬浮迷你窗**：置顶无边框小窗，六项核心指标迷你条，顶部拖动、双击回主窗、右上角关闭；托盘与主窗口均可开关。
- **常驻后台**：托盘图标（左键切窗口，右键菜单含悬浮窗 / 立即诊断 / 退出），关闭窗口隐藏到托盘继续采集，单实例，可选开机自启。
- **启动项分析**：注册表 Run 键（HKCU / HKLM / WOW64）与启动文件夹扫描，禁用 / 启用与任务管理器同机制（StartupApproved），残留项检测。
- **设置页**：全部阈值可调并热生效；帧监测与 DPC 归因开关；设置与事件数据持久化于本机。

## 卡顿检测机制

| 信号 | 机制 |
|---|---|
| CPU / 内存 / 进程 / 网络 | [`sysinfo`](https://github.com/GuillaumeGomez/sysinfo) 1 Hz 轮询 |
| 提交内存 / 硬缺页 / 磁盘延迟 / 队列 / DPC / 中断 / GPU 引擎 | Windows PDH 性能计数器（`PdhAddEnglishCounterW`，兼容本地化系统与新版计数器名） |
| 挂起窗口 | `EnumWindows` + `SendMessageTimeout(SMTO_ABORTIFHUNG)`，独立线程检测避免拖慢采样 |
| 进程 I/O 归因 | `sysinfo` `disk_usage`（含全部 I/O，而非纯磁盘） |

阈值引擎为滚动时间窗口 + 持续判定 + 同类事件冷却（默认 60s），按时间覆盖率判定而非固定样本数，对采集抖动鲁棒。各默认阈值见「配置」。

## 截图

暂无截图。仓库内没有当前版本的真实窗口截图；按规范不放占位图或应用图标冒充界面截图。2.4 要求的真实窗口截图标记为【未验证】。

## 前置条件

| 依赖 | 版本 | 必需 | 说明 |
|---|---|---|---|
| Windows | 10 / 11，x86-64 | 是 | 采集层使用 Windows PDH、`EnumWindows` 与 ETW，仅 Windows 可用 |
| WebView2 Runtime | 随 Windows 11 / 新版 Windows 10 预装 | 是 | Tauri 2 的 WebView 运行时；缺失时应用无法启动 |
| Node.js | >= 20（CI 使用 22） | 从源码构建时必需 | 运行 `npm` 脚本与前端类型检查 |
| npm | 随 Node.js 提供 | 从源码构建时必需 | 本仓库使用 `package-lock.json`（lockfileVersion 3） |
| Rust | stable（本地实测 1.99.0） | 从源码构建时必需 | 编译 `src-tauri`；`edition = "2024"` 需要较新的 stable |
| 管理员权限 | 不适用 | 否 | ETW DPC 驱动归因需要管理员权限，非管理员时自动降级；其余功能不需要 |
| [Intel PresentMon](https://github.com/GameTechDev/PresentMon) | 随安装包提供 `extras/PresentMon.exe` | 否 | 关闭后失去帧率与帧间隔尖峰监测 |

## 安装

项目未配置自动更新通道。安装包可从 [GitHub Release 页面](https://github.com/StarAsh042/LagScope/releases) 下载 `.msi`，或从源码构建，或从 [Actions 工作流](https://github.com/StarAsh042/LagScope/actions/workflows/build.yml) 的 `windows-build` 产物 `LagScope-msi` 中获取 `.msi`。

从源码获取代码并安装依赖：

```bash
git clone https://github.com/StarAsh042/LagScope.git
cd LagScope
npm ci
```

构建 Windows 安装包（`.msi`）：

```bash
npm run tauri build
```

构建产物位于 `src-tauri/target/release/bundle/msi/`。CI 使用的等价命令为 `npm run tauri build -- --bundles msi`。

验证构建结果（预期输出为至少一个 `.msi` 文件名）：

```bash
ls src-tauri/target/release/bundle/msi/*.msi
```

安装：双击 `.msi` 按向导完成。卸载：通过 Windows「设置 → 应用 → 安装的应用」卸载 LagScope。签名 / 公证状态：安装包**未签名、未公证**；`src-tauri/tauri.conf.json` 与 CI 工作流中均无签名配置，Windows 可能显示 SmartScreen 提示。

## 快速开始

以下为开发模式，从源码启动，干净环境约 5–10 分钟（不含 Rust / Node 安装）：

```bash
npm ci
npm run tauri dev
```

入口：自动打开标题为「LagScope 卡顿透视」的桌面窗口（1280×800）。预期现象：仪表盘显示 1 Hz 刷新的 CPU / 内存 / 磁盘曲线与进程表；托盘出现 LagScope 图标。

停止方式：关闭窗口默认隐藏到托盘并继续采集；要完全退出，右键托盘图标选择「退出 LagScope」，或在设置中关闭「关闭窗口时隐藏到托盘」后再关闭窗口。

失败排查：确认已安装 WebView2 Runtime；查看终端中 `cargo` / `npm` 报错；到 [issue 列表](https://github.com/StarAsh042/LagScope/issues) 反馈。

## 配置

设置通过应用内「设置页」修改，保存后**热生效**，无需重启。配置由 `tauri-plugin-store` 持久化到 `settings.json`，键名为 `settings`；无环境变量、无命令行参数。默认值如下：

| 键 | 必需 | 默认值 | 示例 | 敏感 | 说明 |
|---|---|---|---|---|---|
| `cpuPct` | 否 | 85 | `85` | 否 | CPU 满载判定阈值，单位 %；取值 0–100 |
| `cpuSustainS` | 否 | 10 | `10` | 否 | CPU 满载持续时间，单位 s |
| `commitPct` | 否 | 90 | `90` | 否 | 提交内存占上限比例阈值，单位 %；取值 0–100 |
| `pageInput` | 否 | 500 | `500` | 否 | 硬缺页速率阈值，单位 页/s |
| `diskTransferMs` | 否 | 25 | `25` | 否 | 磁盘平均延迟阈值，单位 ms |
| `diskSustainS` | 否 | 5 | `5` | 否 | 磁盘延迟超标持续时间，单位 s |
| `interruptPct` | 否 | 20 | `20` | 否 | 中断时间占比阈值，单位 %；取值 0–100 |
| `cooldownS` | 否 | 60 | `60` | 否 | 同类事件冷却时间，单位 s |
| `closeToTray` | 否 | true | `true` | 否 | 关闭窗口时隐藏到托盘并继续采集 |
| `autostart` | 否 | false | `false` | 否 | 开机自启 |
| `enableFrameMonitor` | 否 | true | `true` | 否 | PresentMon 帧监测开关 |
| `enableDpcWatch` | 否 | true | `true` | 否 | ETW DPC 驱动归因开关（非管理员自动降级） |

完整最小配置示例（`settings.json` 中 `settings` 键对应的对象，字段与 `settings.rs` 的 `Settings` 结构体一致，序列化为 `camelCase`）：

```json
{
  "cpuPct": 85,
  "cpuSustainS": 10,
  "commitPct": 90,
  "pageInput": 500,
  "diskTransferMs": 25,
  "diskSustainS": 5,
  "interruptPct": 20,
  "cooldownS": 60,
  "closeToTray": true,
  "autostart": false,
  "enableFrameMonitor": true,
  "enableDpcWatch": true
}
```

取值优先级：设置页 > 默认值（无 CLI、无环境变量、无其他配置文件覆盖）。生效方式：保存设置后**热更新**检测器阈值、帧监测 / DPC 开关与开机自启注册，无需重启。校验失败时 `set_settings` 返回错误字符串（如「保存设置失败」），设置不落盘；可在终端查看 Rust 侧报错。以上键均为本地偏好，不含 secret / token / 连接串。

另有三个检测参数在 `settings.rs` 的 `to_detector_config()` 中固定，未暴露到设置页：`interrupt_sustain_s = 5`、`stutter_gap_ms = 100.0`、`dpc_total_threshold = 8000.0`。

### 数据与配置位置

| 文件 | 位置 | 说明 |
|---|---|---|
| `settings.json` | Tauri `app_data_dir()`，Windows 下位于 `%APPDATA%` 内 | 用户设置；文件损坏或缺失时回落默认值 |
| `lagscope.db` | 同上 | SQLite 事件库，保留最近 500 条事件 |

具体路径由 Tauri 的 `app_data_dir()` 在运行时决定（`identifier = com.lagscope.app`），代码未硬编码绝对路径。

### 权限与 capability

`src-tauri/capabilities/default.json` 把权限应用到 `main` 与 `mini` 两个窗口，权限集合为 `core:default`、`core:window:allow-start-dragging`、`opener:default`；未启用命令级细粒度授权。应用需要读取系统性能计数器、枚举窗口与进程信息。

### 隐私说明

依据静态代码检查（`src/` 与 `src-tauri/src/` 中未发现 `reqwest` / `hyper` / `fetch` / telemetry / analytics / sentry 等远程上报代码），采集到的指标与事件**仅写入本机 SQLite 与本地设置文件，未发现远程上报**；此项为静态推断，未做运行时网络抓包验证。`src-tauri/tauri.conf.json` 中 `app.security.csp = null`，即未启用 Content Security Policy。PresentMon 以 `--output_stdout --v1_metrics` 流式解析，不写磁盘，随应用退出终止。

## 兼容性

| 项目版本 | 平台 / 架构 | 运行时 | 状态 |
|---|---|---|---|
| 1.1.1 | Windows x86-64（CI `windows-latest`）| WebView2 + Node.js 22 | 已构建验证：CI `windows-build` 全绿，完成 `cargo test --lib`、`npm run check` 与 `.msi` 打包；此为编译与单元测试证据，非真实桌面 GUI 运行时验收 |
| 1.1.1 | Windows 10 / 11 x86-64 | WebView2 | 未测试（仓库无真实桌面运行截图与手工验收记录） |
| 1.1.1 | macOS / Linux | 不适用 | 未测试，且代码依赖 Windows 专用 API |
| 1.1.1 | Windows ARM64 | 不适用 | 未测试 |

依据规范，CI 编译通过不等于真实平台验收，故上表不把任何平台标为「已测试」；仅如实记录 CI 上的构建与测试证据。

## 项目结构

```text
src/                    SvelteKit 前端（SPA、uPlot 图表、主窗口与迷你窗页面）
src-tauri/src/         Rust 后端
  collectors/          sysinfo + PDH + 挂起窗口采集层
  engine/              卡顿检测引擎与阈值
  store/               SQLite 事件库
  settings.rs          设置持久化与热更新
src-tauri/capabilities/ Tauri 权限配置
src-tauri/extras/       PresentMon.exe
src-tauri/icons/        应用图标
.github/workflows/      build.yml（windows-build）
```

## 文档导航

- [CHANGELOG](CHANGELOG.md)：版本变更历史
- [参与开发](#参与开发)：贡献与 PR 流程
- [构建工作流](https://github.com/StarAsh042/LagScope/actions/workflows/build.yml)：CI 状态与 MSI 产物
- [Intel PresentMon](https://github.com/GameTechDev/PresentMon)：帧监测依赖的上游项目

仓库暂无 `CONTRIBUTING.md`、`SECURITY.md`、`docs/` 目录；贡献流程直接写在本节。

## 参与开发

> **给外部贡献者的重要说明**
>
> `main` 分支受保护，且仅维护者可直接推送。
> **请向 `dev` 分支提交 Pull Request；直接向 `main` 提交的 PR 大概率不会受理。**

推荐流程：

1. Fork 本仓库到你自己的账号。
2. 从 `dev` 分支切出特性分支（`git checkout -b feature/xxx`）。
3. 提交改动并推送到你的 Fork。
4. 向本仓库的 **`dev` 分支**开 Pull Request，等待 CI 通过与合并。

分支流向：`feature/xxx` → `dev`（日常集成）→ `main`（发布版本）。

本地验证命令（与 CI `windows-build` 一致）：

```bash
npm ci
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run check
npm run tauri build -- --bundles msi
```

## 许可证

MIT（SPDX: `MIT`），见 [LICENSE](LICENSE)。第三方素材的许可证见「致谢」。

## 致谢

- [NeoHtop](https://github.com/Abdenasser/neohtop)（MIT）：Tauri + Svelte 同栈轮询模式与进程表 UX。
- [bottom](https://github.com/ClementTsang/bottom)（MIT）：采集层与状态层解耦的架构范式。
- [Mission Center](https://gitlab.com/mission-center-devs/mission-center)、[Resources](https://github.com/nokyan/resources)（GPL-3.0）：仅借鉴「每资源一页 + 进程详情」布局思路。
- [System Informer](https://github.com/winsiderss/systeminformer)（MIT）：指标口径参考。
- [TrafficMonitor](https://github.com/zhongyang219/TrafficMonitor)（GPL-3.0）：计数器口径参考（`GetSystemTimes` 与 PDH）。
- [Intel PresentMon](https://github.com/GameTechDev/PresentMon)（MIT）：帧监测子进程，随安装包提供 `extras/PresentMon.exe`，通过 `--output_stdout --v1_metrics` 流式解析。
- [`sysinfo`](https://github.com/GuillaumeGomez/sysinfo)（MIT）：进程与系统指标采集。

以上均只借鉴思路与口径或作为依赖引入，未复制其源码；本项目原创代码采用 MIT 许可证。

[![](https://img.shields.io/badge/powered_by-dsh-4D6BFE?style=flat-square&logo=deepseek&logoColor=white)](https://github.com/deepseek-ai/deepseek-harness)
