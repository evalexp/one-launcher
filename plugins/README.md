# OneLauncher 插件目录 (Plugins)

本目录包含 OneLauncher 示例插件。

## 示例插件

| 插件目录 | 前缀 | 说明 | 特性 |
| :--- | :--- | :--- | :--- |
| [`everything/`](./everything/) | `fs:` | Everything 全盘文件搜索 | 声明式 Native C DLL FFI 绑定、`on_setup`、IPC 状态诊断 |
| [`calculator/`](./calculator/) | `=` | 数学表达式计算器 | 纯 JavaScript 编写、数学求值、内置图标、`launcher.clipboard.write_text` |
| [`totp/`](./totp/) | `otp:` | 动态验证码 (RFC 6238) | 纯 JavaScript 编写、RFC Base32、HMAC-SHA1/SHA256/SHA512、多元化参数解析 |
| [`clash/`](./clash/) | `clash:` | Clash 控制器 | 环境变量读取、配置备份更新、原生 WinHttp 请求、系统代理注册表切换 |

## 插件开发指南

详细开发文档、生命周期规范及完整 API 请参考：
👉 [OneLauncher 插件开发指南 (docs/plugin_development_guide.md)](../docs/plugin_development_guide.md)
