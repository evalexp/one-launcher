# OneLauncher 架构与系统设计规范 (DESIGN.md)

本文档系统性阐述 **OneLauncher** 的核心架构、子系统设计、模块交互契约与分发规范。

---

## 1. 项目定位与核心指标

OneLauncher 是一款面向 Windows 平台打造的原生轻量级快速启动器与插件宿主环境。

| 指标维度 | 目标设计值 | 当前实际测量值 | 架构保障措施 |
| :--- | :--- | :--- | :--- |
| **可执行文件体积** | < 4.0 MB | **~3.2 MB** (Release 构建) | 剔除重型 Web/UI 运行时，基于 Windows 原生 Win32 API + QuickJS + Reqwest |
| **运行时内存占用** | 8 MB ~ 15 MB | **~10.5 MB** (常驻就绪) | 纯 GDI 内存双缓冲渲染，零 WebView / Electron / Direct2D 额外运行层 |
| **全局呼出延迟** | < 16 ms (瞬时显示) | **< 5 ms** | 原生 `RegisterHotKey` 配合无边框 Win32 窗口快速位图贴图 |
| **插件沙箱隔离** | 独立 Context 隔离 | **完全隔离** | 每个插件分配专属独立 QuickJS Context，互不污染全局变量与状态 |

---

## 2. 系统整体架构

```text
┌────────────────────────────────────────────────────────────────────────┐
│                              OneLauncher                               │
├──────────────────────────────────┬─────────────────────────────────────┤
│            GUI 模式              │               CLI 模式              │
│  Win32 窗口 + 消息循环 + GDI 渲染  │  完全无窗体 + 终端隔离运行 + 结果分隔  │
└─────────────────┬────────────────┴──────────────────┬──────────────────┘
                  │                                   │
                  ▼                                   ▼
┌──────────────────────────────────┐ ┌───────────────────────────────────┐
│        src/indexer (检索层)       │ │          src/cli (调试层)        │
│ - 开始菜单快捷方式 (.lnk)         │ │ - -plugin / -input / -select     │
│ - 现代应用 (shell:AppsFolder)     │ │ - -debug / -log-out / -format    │
│ - 拼音全拼 / 首字母模糊加权匹配   │ │ - 结果与日志分界横幅隔离输出     │
└──────────────────────────────────┘ └───────────────────────────────────┘
                  │                                   │
                  └─────────────────┬─────────────────┘
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        src/plugin (插件宿主系统)                       │
│ - QuickJS (ES2020) 引擎 + 专用 Worker 线程                             │
│ - 每插件独立 Context 隔离: 专属 globalThis，杜绝变量泄漏与跨插件污染   │
│ - 共享 Rust Core API 网关 (launcher.*，无状态纯 API):                  │
│   ├─ launcher.fs: 原生 Win32 同步/二进制文件系统 API                   │
│   ├─ launcher.http & fetch: 基于成熟库 reqwest 的完整网络客户端         │
│   ├─ launcher.shell: exec, batch_exec, spawn, open, UAC 提权/auto_sudo │
│   ├─ launcher.env: 进程环境变量读写与代理                              │
│   ├─ launcher.app: quit, reload, edit_config, message_box 原生弹窗     │
│   ├─ launcher.progress: create, show, update, close 原生双缓冲进度条   │
│   ├─ launcher.clipboard: 系统剪贴板读写                                │
│   └─ launcher.cache_icon: 图标提取与注册入池                           │
│ - 插件私有上下文 (self):                                               │
│   ├─ self.get_config() / self.config: 插件私有合并配置                 │
│   └─ self.id, self.name, self.prefix, self.dir, self.path              │
└───────────────────────────────────┬────────────────────────────────────┘
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                          底层支撑与基础模块                            │
│ - src/config: TOML 配置读写、模板生成、动态重载、path 默认回退         │
│ - src/hotkey: 全局热键注册与解析 (Alt+Space 等)                        │
│ - src/tray: 系统托盘图标、右键菜单与状态联动                           │
│ - src/logger: tracing 封装，支持控制台 (std) 与文件 (fs:<path>) 输出   │
│ - src/ui: Win32 窗口管理、GDI 双缓冲绘制、矢量预设与外置图标池缓存     │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. 核心子系统设计

### 3.1 插件系统与上下文隔离架构 (`src/plugin/`)
1. **多 Context 完全隔离与共享 API 分离**：
   - **共享 API 网关 (`launcher`)**：`launcher` 作为纯净的程序能力接口接入点（API Gateway），绑定至共享 Rust 核心，提供文件操作、网络请求、进程与提权命令、剪贴板、系统管理及图标缓存能力。`launcher` 不包含任何插件私有状态或私有 `config`。
   - **插件私有上下文 (`self`)**：每个插件在初始化阶段在 QuickJS Runtime 内获得专属独立的 `Context`。在此 Context 中注入专属于该插件的 `self` 对象，提供 `self.get_config()`、`self.config`、`self.id`、`self.name`、`self.prefix`、`self.dir` 与 `self.path`。全局变量严格限定为 `launcher` 与 `self`（以及宿主 `console`），各插件的全局变量与模块状态物理隔离。
2. **生命周期规范**：
   - `metadata`: 声明插件名、前缀（`prefix`）、默认配置（`default_config`）与动态库 FFI 声明。
   - `on_setup()`: **无参调用**，插件首次加载时执行一次。图标通过 `launcher.cache_icon(...)` 注册。
   - `on_query(input)`: 同步响应，剥离前缀匹配过滤，返回包含 `id`, `title`, `subtitle`, `icon`, `badge`, `payload` 的 `SearchResultItem` 数组。
   - `on_select(item)`: 确认触发动作，执行网络、文件或系统管理。
3. **HTTP 网络请求能力与系统级代理控制**：
   - 基于 `reqwest` 客户端，支持完整 HTTPS、系统根证书信任链、智能重定向与代理穿透，提供 `launcher.http.get/post/request` 以及 `launcher.fetch` 全局别名。
   - 代理统一在系统配置层（`config.toml` -> `[network]`）集中控制（支持 HTTP/HTTPS/SOCKS5），未启用或留空时强制直连。插件发起请求无需关心代理，底层自动完成透明路由。
4. **Win32 原生进度条系统 (`src/ui/progress.rs` & `src/plugin/host_progress.rs`)**：
   - 独立 UI 消息循环线程驱动，暗黑风格 `#1E1E1E` + `#0078D4` 强调色与 DWM 圆角窗口。
   - GDI 内存双缓冲防闪烁绘制，支持实时百分比、状态文本、可拖拽移动与 ESC 快速退出，提供面向对象实例（`launcher.progress.create`）与全局单例接口。
5. **UAC 提权与批处理执行**：
   - `launcher.shell.exec` 支持 `sudo: true` 单次弹出 UAC 提权窗口；支持 `auto_sudo: true` 在非提权失败后自动提权重试。
   - `launcher.shell.batch_exec` 将多个指令（如批量注册表写入）打包合并为单次提权批处理会话，全局仅触发一次 UAC 授权。
6. **日志前缀自动识别**：
   - 通过线程局部 RAII 上下文 `enter_plugin_context`，插件脚本内的 `console.log` 及宿主代理日志自动附带当前插件名标签（如 `-[Clash 控制器] ...`）。
7. **隔离沙箱与动态模块评估**：
   - 宿主提供 `launcher.create_context()` 创建全新独立 QuickJS 隔离虚拟机。
   - 宿主提供 `launcher.eval_module()`，以原生 ES Module 规范直接评估第三方脚本，安全提取 `metadata` 与导出列表。

### 3.2 命令行调试与隔离诊断系统 (`src/cli/`)
专用于终端调试与验证插件逻辑，无需打开 GUI 界面：
- **参数支持**：`-plugin`, `-input`, `-select`, `-debug`, `-log-out`, `-log-level`, `-format`, `-config`, `-help`。
- **无窗体保证**：绕过 Win32 窗口注册与绘制流程，以纯控制台方式执行。
- **控制台依附**：在 Windows GUI 子系统模式下，安全检测标准输出类型，未重定向时通过 `AttachConsole(ATTACH_PARENT_PROCESS)` 依附父进程控制台，不破坏重定向管道。
- **结果隔离输出**：使用清晰的横幅块（`=== ONELAUNCHER RESULTS ===`）分隔业务结果与追踪日志，并提供易读纯文本与结构化 JSON 两种格式。

### 3.3 配置管理 (`src/config/`)
- 基于 `config.toml`，支持托盘一键热重载与 `sys: reload` 插件重载。
- **路径缺省回退**：`[plugins.<id>]` 下的 `path` 字段为可选配置，默认自动解析至 `"plugins/<id>"`，支持极简注册。
- **严格映射原则**：严格保持配置项与运行时逻辑的一一映射，支持常规配置、外观尺寸、日志等级、网络代理与插件池。

### 3.4 原生 Win32 GDI 渲染与图标缓存 (`src/ui/`)
- 无重型 GPU/Web 渲染管线，采用 Win32 原生 GDI 双缓冲（CompatibleDC + CompatibleBitmap）消除画面闪烁。
- 采用深色极简风格设计（深灰背景 `#1E1E1E`、选中高亮 `#2D2D30`、柔和边框 `#3E3E42`）。
- 图标池支持内置矢量预设（`file`, `folder`, `error`, `success`, `warn`, `info`, `gear`, `calc`, `terminal`, `key`, `shield`）与插件本地图片动态转存。

### 3.5 应用程序检索与脱壳启动体系 (`src/indexer/`)
- **多源索引**：扫描系统与用户开始菜单（`.lnk`、`.exe`、`.url`）以及通过 `shell:AppsFolder` 提取已安装 UWP/MSIX 应用，支持拼音全拼与首字母模糊匹配。
- **脱壳非子进程启动与前台焦点交接机制**：
  - 启动优先通过 Windows 桌面外壳 Explorer COM 接口（`IShellDispatch2`），由 `explorer.exe` 创建进程，新进程父进程为 `explorer.exe`。
  - 将目标程序所在父目录配置为当前工作目录（CWD）。
  - **前台焦点交接**：在唤起前调用 `AllowSetForegroundWindow(ASFW_ANY)` 并指定 `SW_SHOWNORMAL` 显示状态。
- **容错防护与静默兜底**：
  - 主程序全局配置 `SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX)`。
  - 在 COM 唤起不可用时降级调用 `ShellExecuteExW` 并配置 `SEE_MASK_FLAG_NO_UI` 掩码。

---

## 4. 插件生态就绪清单

| 插件名称 | 目录 | 触发前缀 | 核心功能 |
| :--- | :--- | :--- | :--- |
| **Clash 控制器** | `plugins/clash/` | `clash:` | 订阅自动备份与更新、MetaCubeXD 控制台跳转、系统代理一键启停（`batch_exec` + `auto_sudo`）、配置时间戳查看 |
| **系统管理** | `plugins/system/` | `sys:` | 快速退出程序、即时重载配置与插件、使用默认编辑器编辑 `config.toml` |
| **极速计算器** | `plugins/calculator/` | `=` | 高精度数学表达式快速解析与计算结果一键复制 |
| **Everything 全盘搜索** | `plugins/everything/` | `fs:` | 联动 Everything SDK / 原生接口快速检索系统文件与路径定位 |
| **TOTP 动态令牌** | `plugins/totp/` | `otp:` | 基于标准 HMAC-SHA1 的 2FA 验证码离线实时计算与剪贴板复制 |
| **插件管理器** | `plugins/plugin-manager/` | `pm:` | 插件在线安装、依赖自动检测、插件热启用/禁用、本地插件卸载与配置维护 |

---

## 5. 构建与分发交付体系

### 5.1 构建架构
- **目标平台**：Windows x64 (`x86_64-pc-windows-msvc`)。
- **CI 流水线**：基于 GitHub Actions (`.github/workflows/release.yml`)，严格配置仅在 Release Tag (`push.tags: ['v*']`) 触发自动编译与发布。

### 5.2 分发包体系
发布流程并行产出两个发行包与一份独立可执行文件：

1. **标准发布版 (`OneLauncher-${tag}-windows-x64.zip`)**：
   - 面向纯粹启动器需求的用户，纯净轻量。
   - 包含主程序 `one-launcher.exe`、默认纯净 `config.toml` 与使用说明，不携带任何预装插件。
2. **插件管理器发布版 (`OneLauncher-${tag}-with-plugin-manager-windows-x64.zip`)**：
   - 内置 `plugins/plugin-manager` 插件目录。
   - 预配置默认 `config.toml` 中开启 `[plugins.plugin-manager]` (`enabled = true`)。
   - 用户解压即具备在线安装与管理插件能力。
3. **单文件直接下载 (`one-launcher-${tag}-windows-x64.exe`)**：
   - 包含版本标签的独立 Release 可执行程序，便于单文件快捷更新或部署。

---

## 6. 版本更新记录

### v0.1.1-pre
- **应用程序启动架构升级**：
  - 接入 Windows 桌面外壳 Explorer COM 接口（`IShellDispatch2`），由 `explorer.exe` 代理创建应用程序进程。
  - 启动目标程序时注入父目录作为当前工作目录（CWD）。
  - 保留 `ShellExecuteExW`（携带 `SEE_MASK_FLAG_NO_UI`）作为降级兜底路径。
- **系统错误模式与焦点管理**：
  - 主程序全局启用 `SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX)`。
  - 在唤起程序与调用 `open_system_path` 时调用 `AllowSetForegroundWindow(ASFW_ANY)` 并指定 `SW_SHOWNORMAL` 状态。
- **依赖特性配置**：
  - `Cargo.toml` 中 `windows` 依赖引入 `Win32_System_Diagnostics_Debug` 特性。

### v0.1.0-pre
- 初始架构设计与双发布构建流水线建立。
- 极速计算器、Clash 控制器、系统管理、TOTP 与插件管理器就绪。
