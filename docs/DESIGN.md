# OneLauncher 架构与设计规范

本文档记录 OneLauncher 的核心技术选型、设计准则与模块规范。

## 1. 核心设计原则
1. **极致轻量 (Zero Bloat)**：杜绝 Electron/Chromium 等重量级运行时，基于 Rust 原生开发，常驻内存控制在 10MB~15MB 级别。
2. **零后台开销 (0% CPU Idle)**：热键与文件监听基于 Windows 系统内核事件通知驱动，拒绝无意义的 Sleep 轮询。
3. **无 UI 配置哲学 (Config as Code)**：所有设置均收敛在 `config.toml` 中，通过外部编辑器修改，保存即自动热重载生效。
4. **Everything 原生无缝集成**：通过 Windows 消息 IPC (`WM_COPYDATA`) 直接与 Everything 内存通信，毫秒级响应，无需依赖外部 C SDK dll。

## 2. 模块架构图

- `src/config/`: 配置加载、预设模板生成、文件热监听与更新
- `src/hotkey/`: 全局热键注册与管理 (基于 Win32 `RegisterHotKey`)
- `src/indexer/`: 系统快捷方式扫描、UWP 枚举、拼音转换与模糊评分
- `src/everything/`: Everything IPC 客户端，查询结构体封包与解包
- `src/ui/`: 极轻量无边框原生窗口渲染与交互
- `src/tray/`: 系统托盘图标与上下文菜单
