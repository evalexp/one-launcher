# OneLauncher 插件目录 (Plugins)

本目录包含 OneLauncher 官方插件与示例插件。

## 官方示例插件

| 插件目录 | 绑定前缀 | 说明 | 特性展示 |
| :--- | :--- | :--- | :--- |
| [`everything/`](./everything/) | `fs:` | Everything 全盘极速文件搜索 | 声明式 Native C DLL FFI 绑定、`on_setup` 生命周期、`Everything_GetLastError` IPC 状态诊断 |
| [`calculator/`](./calculator/) | `calc:` | 极速数学表达式计算器 | 纯 JavaScript 轻量交互、安全求值、内置预设图标引用、`launcher.clipboard.write_text` 剪贴板写入 |

## 插件开发指南

详细开发文档、规范约束、生命周期说明及完整 API 请参考：
👉 [OneLauncher 插件开发规范与指南 (docs/plugin_development_guide.md)](../docs/plugin_development_guide.md)
