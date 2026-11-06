# LagScope 卡顿透视

监测 Windows 电脑卡顿的原因:实时仪表盘 + 自动卡顿事件检测归因 + 事件时间线复盘,三合一。

技术栈:**Tauri 2 + Svelte 5 + TypeScript + Rust**(仅支持 Windows)。

## 功能

- **实时仪表盘**:CPU(总/每核)、内存/提交内存、磁盘(速率/活跃/延迟/队列)、GPU(3D 引擎/显存)、网络、DPC 与硬缺页的 1Hz 实时曲线;进程表支持排序/过滤/结束进程/高耗标记。
- **卡顿自动归因**:后台滚动窗口持续评估以下信号,命中阈值即生成事件(带中文归因描述),存入 SQLite:
  - CPU 满载持续(默认 85% × 10s)
  - 提交内存逼近上限(默认 90%)
  - 硬缺页风暴(内存不足,默认 500 页/秒)
  - 磁盘平均延迟超标(默认 25ms,持续 5s)
  - 中断时间占比过高(驱动/外设,默认 20%)
  - 无响应(挂起)窗口检测(与任务管理器"未响应"同判定)
- **事件时间线复盘**:每个事件记录触发时刻的完整计数器快照 + CPU/内存/I/O Top5 进程 + 挂起窗口列表;支持手动"立即诊断"。
- **常驻后台**:托盘图标(左键切窗口,右键菜单含立即诊断/退出),关闭窗口隐藏到托盘继续采集,单实例,可选开机自启。
- **设置页**:全部阈值可调并热生效;持久化于 `%APPDATA%/com.lagscope.app/settings.json`,事件库同目录 `lagscope.db`。

## 卡顿检测机制(为什么能找到原因)

| 信号 | 机制 |
|---|---|
| CPU/内存/进程/网络 | [`sysinfo`](https://crates.io/crates/sysinfo) 1Hz 轮询 |
| 提交内存/硬缺页/磁盘延迟/队列/DPC/中断/GPU 引擎 | Windows PDH 性能计数器(`PdhAddEnglishCounterW`,兼容本地化系统与新版计数器名) |
| 挂起窗口 | `EnumWindows` + `SendMessageTimeout(SMTO_ABORTIFHUNG)`,独立线程检测避免拖慢采样 |
| 进程 I/O 归因 | sysinfo `disk_usage`(注意:含全部 I/O 而非纯磁盘) |

阈值引擎为滚动时间窗口 + 持续判定 + 同类事件冷却(默认 60s),按时间覆盖率判定而非固定样本数,对采集抖动鲁棒。

## 开发

```bash
npm install
npm run tauri dev      # 开发调试
npm run tauri build    # 打包安装程序(release)
```

- 前端:`src/`(SvelteKit SPA 模式 + uPlot 图表)
- 后端:`src-tauri/src/`
  - `collectors/` 采集层(sysinfo + PDH + 挂起窗口独立线程)
  - `engine/lag_detector.rs` 卡顿检测引擎(阈值/滚动窗口/归因快照)
  - `store/db.rs` SQLite 事件库
  - `settings.rs` 设置持久化与热更新
- 单元测试:`cargo test --lib`(PDH 冒烟、检测器触发/冷却/慢 tick 回归、kill 通路)

## 借鉴的开源项目(致谢)

- [NeoHtop](https://github.com/Abdenasser/neohtop)(MIT)— Tauri+Svelte 同栈轮询模式与进程表 UX
- [bottom](https://github.com/ClementTsang/bottom)(MIT)— 采集层与状态层解耦的架构范式
- [Mission Center](https://gitlab.com/mission-center-devs/mission-center)、[Resources](https://github.com/nokyan/resources)(GPL-3.0)— 仅借鉴"每资源一页 + 进程详情"布局思路
- [System Informer](https://github.com/winsiderss/systeminformer)(MIT)— 指标口径参考
- [TrafficMonitor](https://github.com/zhongyang219/TrafficMonitor)(GPL-3.0)— 计数器口径参考(GetSystemTimes vs PDH)

以上均只借鉴思路与口径,未复制代码;本项目原创代码采用 MIT 许可证。

## v2 路线图

- [x] PresentMon(2.6.0,MIT)子进程集成:帧率/最大帧间隔曲线,渲染大户卡帧归因(v0.2.0)
- [x] 启动项分析与清理:注册表 Run 键 + 启动文件夹扫描,禁用/启用(StartupApproved,与任务管理器同机制),残留项检测(v0.4.0)
- [ ] ETW 按驱动归因 DPC/延迟(定位具体驱动)
- [x] 悬浮迷你窗:置顶无边框小窗,CPU/内存/GPU/磁盘/网络/帧率迷你条,托盘与主窗口均可开关(v0.3.0)
- [ ] 硬件温度(LibreHardwareMonitor 思路)

> 帧监测说明:集成 [Intel PresentMon](https://github.com/GameTechDev/PresentMon)(MIT)CLI,`src-tauri/extras/PresentMon.exe` 经 `--output_stdout --v1_metrics` 流式解析(无磁盘写入);2.3.1 起 ETW 采集无需管理员权限;可在设置中开关,随应用退出终止。
