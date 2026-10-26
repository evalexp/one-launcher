# OneLauncher

Windows 原生启动器。

## 特性
- **轻量原生**：基于 Rust 与 Win32 GDI 开发。
- **全局唤出**：默认快捷键 `Alt + Space` 唤出与隐藏。
- **应用搜索**：自动索引开始菜单、UWP 应用与自定义目录，支持中文拼音与英文缩写模糊检索。
- **插件体系**：内嵌 QuickJS 引擎，支持编写 JavaScript 插件，支持声明式 Native C DLL 动态链接绑定。
- **配置驱动**：基于 `config.toml` 配置，支持托盘重载。

## 插件开发
请查阅：[插件开发指南](docs/plugin_development_guide.md)
