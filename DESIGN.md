# OneLauncher 架构设计与开发进度文档 (DESIGN.md)

本文档系统性记录 **OneLauncher** 的核心架构、设计规范、模块实现与完整开发进度。

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
│   ├─ launcher.http & fetch: 基于第三方成熟库 reqwest 的完整网络客户端   │
│   ├─ launcher.shell: exec, batch_exec, spawn, open, UAC 提权/auto_sudo │
│   ├─ launcher.env: 进程环境变量读写与代理                              │
│   ├─ launcher.app: quit, reload, edit_config, message_box 原生弹窗     │
│   ├─ launcher.progress: create, show, update, close 原生双缓冲进度条   │
│   ├─ launcher.clipboard: 系统剪贴板读写                                │
│   └─ launcher.cache_icon: 图标提取与注册入池                           │
│ - 插件私有上下文 (self):                                               │
│   ├─ self.get_config() / self.config: 插件私有合并配置                 │
│   ├─ self.id, self.name, self.prefix, self.dir                         │
│   └─ self.cache_icon: 相对路径自动解析                                 │
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
   - **插件私有上下文 (`self`)**：每个插件在初始化阶段都会在 QuickJS Runtime 内获得专属独立的 `Context`。在此 Context 中注入专属于该插件的 `self` 对象，提供 `self.get_config()`、`self.config`、`self.id`、`self.name`、`self.prefix`、`self.dir`。全局变量严格限定为 `launcher` 与 `self`（以及宿主 `console`），各插件的全局变量与模块状态 100% 物理隔离，杜绝全局作用域污染。
2. **生命周期规范**：
   - `metadata`: 声明插件名、前缀（`prefix`）、默认配置（`default_config`）与 FFI 声明。
   - `on_setup()`: **无参设计**，插件首次加载时执行一次。图标直接通过 `launcher.cache_icon(...)` 或 `self.cache_icon(...)` 注册。
   - `on_query(input)`: 同步极速过滤，必须返回 `SearchResultItem` 数组，每个条目必需包含 `badge` 标识。
   - `on_select(item)`: 确认触发动作，执行网络、文件或系统管理。
3. **HTTP 网络请求能力与系统级代理控制**：
   - 采用成熟三方库 `reqwest`（阻塞式 Blocking 客户端），原生支持 HTTPS、系统根证书信任链、智能重定向与代理穿透，提供 `launcher.http.get/post/request` 以及 `launcher.fetch` 全局别名。
   - 代理统一在系统配置层（`config.toml` -> `[network]`）集中控制（支持 HTTP/HTTPS/SOCKS5），未启用或留空时严格强制直连（`.no_proxy()`）。插件纯净发起请求，无需在业务代码中关心代理。
4. **Win32 原生进度条系统 (`src/ui/progress.rs` & `src/plugin/host_progress.rs`)**：
   - 独立 UI 消息循环线程驱动，暗黑风格 `#1E1E1E` + `#0078D4` 强调色与 DWM 圆角窗口。
   - GDI 内存双缓冲防闪烁绘制，支持实时百分比、状态文本、可拖拽移动与 ESC 快速退出，插件管理器等长耗时操作已全流程接入。
5. **UAC 提权与批处理执行**：
   - `launcher.shell.exec` 支持 `sudo: true` 单次弹出 UAC 提权窗口；支持 `auto_sudo: true` 在非提权失败后自动提权重试。
   - `launcher.shell.batch_exec` 将多个指令（如批量注册表写入）打包合并为单次提权批处理会话，杜绝多次 UAC 弹窗打扰。
6. **日志前缀自动识别**：
   - 通过线程局部 RAII 上下文 `enter_plugin_context`，插件脚本内的 `console.log` 及宿主代理日志自动附带当前插件名标签（如 `-[Clash 控制器] ...`）。

### 3.2 命令行调试与隔离诊断系统 (`src/cli/`)
专用于终端调试与验证插件逻辑，无需打开 GUI 界面：
- **参数支持**：`-plugin`, `-input`, `-select`, `-debug`, `-log-out`, `-log-level`, `-format`, `-config`, `-help`。
- **无窗体保证**：完全绕过 Win32 窗口注册与绘制流程，以纯控制台方式执行。
- **控制台依附**：在 Windows GUI 子系统模式下，安全检测标准输出类型，未重定向时通过 `AttachConsole(ATTACH_PARENT_PROCESS)` 依附父进程控制台，不破坏重定向管道。
- **结果隔离输出**：使用清晰的横幅块（`=== ONELAUNCHER RESULTS ===`）分隔业务结果与追踪日志，并提供易读纯文本与结构化 JSON 两种格式。

### 3.3 配置管理 (`src/config/`)
- 基于 `config.toml`，支持托盘一键热重载与 `sys: reload` 插件重载。
- **路径缺省回退**：`[plugins.<id>]` 下的 `path` 字段为可选配置，默认自动解析至 `"plugins/<id>"`，支持极简注册。
- **精简原则**：清理了无用的外观无效配置（`theme`、`font_size`、`backdrop_effect`），严格保持配置项与运行时逻辑的一一映射。

### 3.4 原生 Win32 GDI 渲染与图标缓存 (`src/ui/`)
- 无重型 GPU/Web 渲染管线，采用 Win32 原生 GDI 双缓冲（CompatibleDC + CompatibleBitmap）消除画面闪烁。
- 采用深色极简风格设计（深灰背景 `#1E1E1E`、选中高亮 `#2D2D30`、柔和边框 `#3E3E42`）。
- 图标池支持内置矢量预设（`file`, `folder`, `error`, `success`, `warn`, `info`, `gear`, `calc`, `terminal`, `key`, `shield`）与插件本地图片动态转存。

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

## 5. 当前开发进度与里程碑总结

- [x] **基础设施重构与体积优化**：
  - 核心架构稳定，单文件 Release 编译体积控制在 ~3.8 MB（包含 QuickJS 引擎与 reqwest HTTPS 协议栈）。
  - 内存占用控制在 10MB 左右。
- [x] **插件 HTTP 引擎第三方库重构**：
  - 引入成熟三方网络库 `reqwest`，替换原本基于 Windows 原生 WinHttp 的底层实现，提供高可靠的 HTTPS 连接、证书验证与重定向支持。
- [x] **插件上下文隔离与 Rust Core API 共享重构**：
  - `launcher` 保持为纯净的全局共享 API 接入点，完全剥离私有 `config`。
  - 插件分配独立隔离的 QuickJS Context，互相之间全局变量与状态彻底隔离。
  - 引入 `self` 插件私有上下文对象，通过 `self.get_config()` 获取自有配置。全局变量严格限定为 `launcher`、`self` 与 `console`。
  - `on_setup` 改为严格无参设计，仅 `on_query(input)` 与 `on_select(item)` 接收参数，`cache_icon` 整合至 `launcher` 与 `self`。
- [x] **完全隔离虚拟机与 ES 模块安全评估能力**：
  - 宿主提供 `launcher.create_context()`，按需生成独立 QuickJS 运行时与沙盒上下文。
  - 宿主提供 `launcher.eval_module()`，支持以原生 ES Module 规范直接评估第三方脚本，安全提取 `metadata` 与导出列表，无需脆弱的字符串正则转换。
- [x] **插件管理器全功能重写与多源 URL Resolver**：
  - 支持多源代码托管平台直链解析（GitHub 自动映射至 raw.githubusercontent.com、Gitee、GitLab 以及任意通用 HTTP/HTTPS 链接）。
  - 自动化依赖收集：自动扫描 `metadata.dynamic_library`、`metadata.native` 原生动态链接库、`metadata.icon` / `metadata.assets` 静态资源，并在沙盒环境中预执行 `on_setup` 拦截动态注册图标。
  - 插件生命周期管理闭环：
    - 在线安装 (`install`)：资源全量离线下载 -> 目录创建 -> 配置热注册 -> 宿主实时重载。
    - 热启用 (`enable`) / 热禁用 (`disable`)：全盘扫描磁盘插件与配置段（精准跳过注释），动态切换状态并热重载。
    - 完整卸载 (`uninstall`)：列出所有已安装与声明插件，同步抹除配置文件与本地磁盘目录。
- [x] **系统级代理配置与网络透明接管**：
  - 新增 `[network]` 配置段，支持 HTTP/HTTPS/SOCKS5 代理（引入 `socks` 协议栈支持）。
  - 代理配置统一在系统底层集中控制，插件纯净发起请求；未配置或显式禁用时严格强制直连（`.no_proxy()`）。
- [x] **Win32 原生双缓冲进度条系统 (`launcher.progress`)**：
  - 独立 UI 消息循环线程驱动，采用现代暗黑风格与 DWM 圆角窗口，双缓冲 GDI 贴图绝不闪烁。
  - 提供实例级（`create`）与全局级（`show`/`update`/`close`）接口，支持实时百分比、状态文本、鼠标按住拖拽与 ESC 键快速关闭。
  - 插件管理器在线安装、依赖下载与完整卸载全流程接入，告别盲等。
- [x] **配置清理与简化**：
  - 移除无用外观配置，精简 `AppearanceConfig`。
  - 插件 `PluginConfig.path` 设为可选，默认解析回退到 `plugins/<plugin-id>`。
- [x] **输入防抖与空前缀响应修复**：
  - 解决 `sys:`、`clash:` 等仅敲击前缀或带空格时无候选返回的问题。
  - 调整为空输入时立即返回全量动作列表。
- [x] **命令行隔离调试工具**：
  - 实现纯终端无窗口运行模式，支持单插件隔离加载、模拟查询、条目选中执行。
  - 增加结果分界横幅与 JSON 格式化输出。
  - 增加插件日志线程局部命名标签输出（`-[<plugin_name>]`）。
- [x] **规范与文档标准化**：
  - 统一建立 `docs/skills/SKILL.md`，规范化生命周期、能力矩阵、自查清单与调试指南。
  - 移除冗余的 `plugin_development_guide.md`，文档唯一维护。
- [x] **质量与自动化测试**：
  - 涵盖配置解析、热键、应用索引模糊匹配、QuickJS 插件生命周期与 Context 隔离、各官方插件业务逻辑、CLI 隔离执行、宿主文件/HTTP能力、独立虚拟机与隔离上下文、插件管理器多平台 URL 解析与 native 依赖下载。
  - 全套 14 组测试套件 20 个用例 100% 通过，0 编译警告。
