# Changelog

All notable changes to this project will be documented in this file.

## [v0.1.1-pre] - 2026-10-30

### Features
- **脱壳非子进程启动机制**：
  - 应用程序检索与启动模块（`AppIndexManager::launch`）接入 Windows 桌面外壳 Explorer COM 接口（`IShellDispatch2`），委托 `explorer.exe` 创建进程，新进程父进程为 `explorer.exe`。
  - 启动目标程序时将程序所在父目录注入为当前工作目录（CWD）。
  - Explorer COM 不可用时降级使用 `ShellExecuteExW` 并配置 `SEE_MASK_FLAG_NO_UI` 掩码。
- **系统错误模式配置**：
  - 主程序入口配置全局 `SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX)`。
- **前台焦点交接机制**：
  - 在唤起程序与调用 `open_system_path` 时调用 `AllowSetForegroundWindow(ASFW_ANY)` 并指定 `SW_SHOWNORMAL` 状态。
- **依赖更新**：
  - `Cargo.toml` 中 `windows` 依赖启用 `Win32_System_Diagnostics_Debug` 特性。

## [v0.1.0-pre] - 2026-10-29

### Features
- 基础启动器架构与 Win32 原生 GDI 界面。
- 多源应用程序索引（开始菜单快捷方式、UWP 现代应用）。
- QuickJS 插件系统与沙箱隔离。
- 内置插件：极速计算器、Clash 控制器、系统管理、TOTP 动态令牌、插件管理器。
- GitHub Actions 双发行包构建流水线。
