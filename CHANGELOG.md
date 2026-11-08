# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

本项目为 Windows 桌面应用，条目以用户可见的功能、行为变化与修复为准；纯内部重构、CI-only 与测试-only 变更不记录。历史版本的变更内容依据各 tag 区间的真实提交整理。

## [Unreleased]

## [1.1.1] - 2026-11-08

### Added

- 新增 MIT 许可证文件 `LICENSE`，明确原创代码的授权条款（[`0ffe2eb`](https://github.com/StarAsh042/LagScope/commit/0ffe2ebbeb0dff425c5b982d1c95ba926eb0df24)）。

### Changed

- 变更贡献流程：在 README 增加「参与开发」章节，要求向 `dev` 分支提交 Pull Request，而非直接向 `main` 提交（[#1](https://github.com/StarAsh042/LagScope/pull/1)，[`b24bbdd`](https://github.com/StarAsh042/LagScope/commit/b24bbdd4f43487e0e970c41201102d287671f175)）。
- 变更文档：按书写规范重写 README 并新增 CHANGELOG，新增目录、配置示例列与完整最小配置示例，规范命令与版本事实源（[#7](https://github.com/StarAsh042/LagScope/pull/7)，[#8](https://github.com/StarAsh042/LagScope/pull/8)）。

## [1.1.0] - 2026-11-07

### Fixed

- 修复采集层与前端使用陈旧数据快照的问题，避免卡顿判定与界面显示引用过期采样（[`7afcffb`](https://github.com/StarAsh042/LagScope/commit/7afcffbf26159582e09241d92688a64476ce18cf)）。

## [1.0.1] - 2026-11-07

### Added

- 新增事件库保留上限（最近 500 条事件），避免 SQLite 事件库无限增长（[`d9e75b6`](https://github.com/StarAsh042/LagScope/commit/d9e75b6a2de06dfbf06080febfe45e97ac788e79)）。
- 新增设置页的监控状态徽章，可查看帧监测与 DPC 归因是否在运行（[`d9e75b6`](https://github.com/StarAsh042/LagScope/commit/d9e75b6a2de06dfbf06080febfe45e97ac788e79)）。

## [1.0.0] - 2026-11-07

首个稳定版。该版本为发布准备，无新增功能或行为变化；`0.2.0`–`0.6.0` 已提供的功能在本版 README 中统一列明。

### Changed

- 变更版本支持状态为首个稳定版（`0.x` 阶段结束），此后公共行为遵循 SemVer 2.0.0；本版不含新增功能（[`a896fb9`](https://github.com/StarAsh042/LagScope/commit/a896fb911c38562ff47a9f8325ba19ba19088224)）。

## [0.6.0] - 2026-11-07

### Added

- 新增基于 ETW 内核会话（`ferrisetw`）的 DPC 按驱动归因，高频 DPC 风暴可定位到具体驱动文件，非管理员时自动降级（[`6e9c40e`](https://github.com/StarAsh042/LagScope/commit/6e9c40e63204bce863946fb52f4edf068762d5d9)）。

## [0.5.0] - 2026-11-06

### Added

- 新增 ACPI 热区温度表盘，通过 PDH 的 `Thermal Zone Information` 读取温度并按阈值变色（[`b75cce1`](https://github.com/StarAsh042/LagScope/commit/b75cce14e438245ca08ce3db1d172f4bc4119849)）。

## [0.4.0] - 2026-11-06

### Added

- 新增启动项分析：扫描注册表 Run 键与启动文件夹，支持禁用 / 启用（`StartupApproved`，与任务管理器同机制）及残留项检测（[`a8957e3`](https://github.com/StarAsh042/LagScope/commit/a8957e3f5dce4427fd30eca470f5cd6872fbcdd7)）。

## [0.3.1] - 2026-11-06

### Fixed

- 修复仪表盘仪表行抖动与底部内容被裁切的问题，改为固定宽度与可滚动布局（[`80289e9`](https://github.com/StarAsh042/LagScope/commit/80289e9d9b7c79aa57058a8bdcced51714d67d2f)）。

## [0.3.0] - 2026-11-06

### Added

- 新增悬浮迷你窗：置顶无边框小窗显示六项核心指标，支持拖动、双击回主窗与托盘开关（[`228cc63`](https://github.com/StarAsh042/LagScope/commit/228cc6396af06275bdd15aff7e7d3cb4434c528c)）。

### Fixed

- 修复仪表盘旋转仪表数字显示方向不正的问题（[`228cc63`](https://github.com/StarAsh042/LagScope/commit/228cc6396af06275bdd15aff7e7d3cb4434c528c)）。

## [0.2.0] - 2026-11-06

### Added

- 新增基于 Intel PresentMon 的帧卡顿监测：帧率与最大帧间隔曲线，并把帧间隔尖峰归因到渲染进程（[`54a1b5b`](https://github.com/StarAsh042/LagScope/commit/54a1b5bb647a2101c576597aff850d1505c37351)）。

## [0.1.0] - 2026-11-05

首个版本，包含实时仪表盘、卡顿检测引擎、事件时间线与托盘常驻等基础能力。

### Added

- 新增实时仪表盘：CPU、内存、磁盘、GPU、网络等 1 Hz 曲线与可排序的进程表（[`22001e8`](https://github.com/StarAsh042/LagScope/commit/22001e8ace8f2b3188166d445ebc9a26e4f3748d)）。
- 新增卡顿检测引擎：滚动窗口阈值判定、挂起窗口检测与本地 SQLite 事件库（[`7c235cb`](https://github.com/StarAsh042/LagScope/commit/7c235cb3dfe27bd7343d2a98088e453fe4951c00)）。
- 新增事件时间线复盘与手动「立即诊断」，记录触发时刻的计数器快照与 Top 进程（[`a8c0a31`](https://github.com/StarAsh042/LagScope/commit/a8c0a318e5f3ef2b8b6b224bf06f1d4bc9a9605d)）。
- 新增托盘图标、关闭窗口隐藏到托盘、单实例、开机自启与设置页（[`204482b`](https://github.com/StarAsh042/LagScope/commit/204482b09c39b5a33c8f8961e427681f023cbee1)）。
- 新增 `sysinfo` 与 Windows PDH 指标采集层及进程相关命令（[`cac2270`](https://github.com/StarAsh042/LagScope/commit/cac2270b02976938281d5e5b50fb85f919408250)）。

[Unreleased]: https://github.com/StarAsh042/LagScope/compare/v1.1.1...HEAD
[1.1.1]: https://github.com/StarAsh042/LagScope/compare/v1.1.0...v1.1.1
[1.1.0]: https://github.com/StarAsh042/LagScope/compare/v1.0.1...v1.1.0
[1.0.1]: https://github.com/StarAsh042/LagScope/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/StarAsh042/LagScope/compare/v0.6.0...v1.0.0
[0.6.0]: https://github.com/StarAsh042/LagScope/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/StarAsh042/LagScope/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/StarAsh042/LagScope/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/StarAsh042/LagScope/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/StarAsh042/LagScope/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/StarAsh042/LagScope/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/StarAsh042/LagScope/compare/8d16d8a...v0.1.0
