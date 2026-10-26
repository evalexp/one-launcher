# OneLauncher 架构与设计规范

本文档记录 OneLauncher 的模块划分与设计准则。

## 1. 核心设计原则
1. **轻量与响应**：基于 Rust 与 Win32 GDI 原生开发。
2. **事件驱动**：热键与消息处理基于 Windows 事件循环驱动。
3. **配置驱动**：配置收敛在 `config.toml` 中，支持托盘手动重载。
4. **插件与隔离**：基于 QuickJS 运行时，并在独立 Worker 线程执行插件任务，与 UI 线程解耦。

## 2. 模块架构

- `src/config/`: 配置加载、模板生成与重载管理
- `src/hotkey/`: 全局热键注册与注销 (基于 Win32 `RegisterHotKey`)
- `src/indexer/`: 系统快捷方式扫描、`shell:AppsFolder` 现代应用枚举、拼音转换与模糊匹配
- `src/plugin/`: QuickJS 插件运行环境、原生 DLL FFI 绑定与任务线程隔离
- `src/ui/`: Win32 窗口管理、GDI 绘制、输入处理与图标提取
- `src/tray/`: 系统托盘图标与上下文菜单
- `src/logger/`: 日志系统 (支持控制台 std 与本地文件输出)
