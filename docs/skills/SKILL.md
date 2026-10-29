---
name: one-launcher-plugin
description: Specification and standard guide for creating, maintaining, and testing OneLauncher JavaScript plugins. Use when implementing plugins, querying capabilities, file operations, native WinHttp requests, process execution with UAC elevation, or dynamic FFI bindings for OneLauncher.
---

# OneLauncher 插件开发规范与指南 (SKILL)

本文档定义 OneLauncher 插件系统的接口契约、安全准则、宿主能力与调试规范。编写插件时需遵守本规范中的生命周期、返回结构与性能边界。

---

## 1. 核心架构与设计原则

OneLauncher 是基于 Rust + Windows 原生 Win32 API + QuickJS 打造的超轻量启动器（主程序体积 < 3MB，内存占用控制在 8MB ~ 15MB 预算内）。

- **运行环境**：内嵌 **QuickJS ES2020** 引擎，支持标准 ES 模块（`export` / `import`）。
- **线程隔离**：所有插件生命周期（`on_setup`、`on_query`、`on_select` 及 FFI 调用）均在**后台专用 Worker 线程**中串行执行，绝不阻塞 UI 渲染与用户输入。
- **前缀路由 (Prefix Routing)**：每个插件必须通过 `metadata.prefix` 声明独占触发前缀（如 `clash:`、`sys:`、`pm:`、`=`、`fs:`）。
- **零膨胀原则**：严禁引入庞大的前端第三方 npm 包。充分利用宿主注入的原生能力（`launcher.fs`、`launcher.http`、`launcher.shell` 等）。

---

## 2. 插件目录结构与配置

### 2.1 目录组织

标准插件应放置在项目根目录 `plugins/<plugin-id>/` 或系统 `%APPDATA%\OneLauncher\plugins\<plugin-id>\` 下：

```text
plugins/<plugin-id>/
├── index.js              # 插件入口文件（默认 entry）
├── icon.png              # (可选) 本地图标资产
└── <library>64.dll       # (可选) 声明式 FFI 原生动态链接库
```

### 2.2 宿主配置绑定 (`config.toml`)

插件在 `config.toml` 中按如下结构注册：

```toml
[plugins.<plugin-id>]
enabled = true
# path 为可选字段：默认指向 "plugins/<plugin-id>"；仅当存放于自定义目录时需显式指定
# path = "plugins/<plugin-id>"
# prefix 与 name 为可选字段：若未指定则自动从插件 index.js 的 metadata 声明中读取回退
prefix = "myprefix:"
name = "我的插件名称"
# entry 为可选字段：默认为 "index.js"
entry = "index.js"

[plugins.<plugin-id>.config]
custom_field = "custom_value"
```

因此，对于标准插件目录，最简注册只需：
```toml
[plugins.my_plugin]
enabled = true
```

---

## 3. 插件生命周期契约

插件脚本必须使用 ES 模块导出规范，核心导出项如下：

### 3.1 `export const metadata` (必需)

声明插件的基础标识、前缀、默认配置与可选的 FFI 原生库定义：

```javascript
export const metadata = {
  name: "插件显示名称",       // string, 必需: 插件显示名
  prefix: "demo:",            // string, 必需: 触发前缀（建议以半角冒号结尾）
  default_config: {           // object, 必需: 默认配置对象
    timeout: 5000,
  },
  // (可选) 声明式 FFI 绑定
  dynamic_library: {
    lib_path: "MyLib64.dll",
    functions: [
      { name: "MyNativeFunc", args: ["string", "i32"], returns: "bool" }
    ]
  }
};
```

---

### 3.2 `export function on_setup()` (可选，无参)

插件首次加载或重新载入时触发一次。

> [!NOTE]
> `on_setup` 为**无参调用**。自定义图标直接通过全局共享网关 `launcher.cache_icon(...)` 进行缓存注册。

```javascript
let myIconId = "gear";

export function on_setup() {
  try {
    myIconId = launcher.cache_icon("./icon.png");
    const cfg = self.get_config();
    console.info("插件初始化成功，当前配置: ", cfg);
  } catch (err) {
    console.warn("缓存图标失败，使用后备图标: " + err);
  }
}
```

---

### 3.3 插件隔离上下文 (`self`)

每个插件均运行于宿主为其创建的**独立隔离 QuickJS Context** 中，彻底避免多插件全局作用域与变量污染。

插件环境中全局变量仅提供 `launcher`（共享核心 API 网关）与 `self`（插件自有上下文实例）。生命周期函数中只有 `on_query(input)` 与 `on_select(item)` 接收参数，`on_setup()` 严格无参。

插件通过 `self` 访问专属实例上下文信息与私有配置：

| 属性 / 方法 | 返回类型 | 说明 |
| :--- | :--- | :--- |
| `self.get_config()` | `object` | 获取插件最终合并配置（`metadata.default_config` + `config.toml`） |
| `self.config` | `object` | 配置对象的直接属性引用（等价于 `self.get_config()`） |
| `self.get_id()` / `self.id` | `string` | 插件在宿主系统中的唯一标识 ID（如 `"clash"`） |
| `self.get_name()` / `self.name` | `string` | 插件显示名称 |
| `self.get_prefix()` / `self.prefix` | `string` | 插件独占触发前缀（如 `"clash:"`） |
| `self.get_dir()` / `self.dir` / `self.path` | `string` | 插件本地所在目录绝对/相对路径 |

```javascript
// 在插件任何函数中均可直接安全访问自己的配置
const cfg = self.get_config();
console.info("插件 ID: " + self.id + "，超时设置: " + cfg.timeout);
```

---

### 3.4 `export function on_query(input)` (必需)

当用户输入匹配前缀且满足防抖间隔时由宿主调用。

> [!IMPORTANT]
> 1. `input: string` 是**剥离前缀后的纯查询内容**（例如用户输入 `clash: sub`，传入的 `input` 为 `" sub"`）。
> 2. `on_query` 必须保持**纯同步且极速响应**（< 5ms）。严禁在 `on_query` 中发起阻塞式网络请求或重型外部进程调用。
> 3. 当 `input` 为空字符串或纯空白时，必须返回初始推荐操作或全部候选指令列表。
> 4. 当 `input` 非空时，建议进行**忽略大小写的前缀匹配**（`startsWith`）或包含匹配。

#### 返回值规范：`Array<SearchResultItem>`

必须返回数组，每个条目必须满足如下字段契约：

```typescript
interface SearchResultItem {
  id: string;        // 必需: 结果唯一标识
  title: string;     // 必需: 主标题，列表高亮显示
  subtitle: string;  // 必需: 副标题，详细说明与状态文本
  icon: string;      // 必需: 图标引用 ID（内置预设或 cache_icon 返回的 ID）
  badge: string;     // 必需: 列表右侧角标文字 (如 "Clash"、"系统"、"设置")，严禁为空
  payload: any;      // 必需: 确认选中时原样传递给 on_select 的有效载荷
}
```

#### 内置图标库 (`icon`)

无需额外引入图片资产，可直接使用宿主内置的矢量/预设图标 ID：

| 图标 ID | 适用场景 | 视觉形态 |
| :--- | :--- | :--- |
| `"file"` / `"doc"` | 文件、日志、文本、文档 | 白底折角文档图标 |
| `"folder"` / `"dir"` | 目录、文件夹路径 | 琥珀黄文件夹图标 |
| `"error"` | 失败状态、异常提示、错误诊断 | 红色圆环错误叉号 ❌ |
| `"success"` | 成功状态、完成提示、健康正常 | 绿色圆环成功对勾 ✔ |
| `"warn"` / `"warning"` | 警示信息、权限提示、缺少配置 | 黄色警示三角 ⚠ |
| `"info"` | 常规信息、说明、提示 | 蓝色圆环感叹号 ℹ |
| `"gear"` / `"settings"` | 系统管理、设置、配置操作 | 齿轮设置图标 ⚙ |
| `"calc"` | 数学计算、科学运算 | 计算器按键图标 🧮 |
| `"terminal"` / `"cmd"` | 命令行、控制台、脚本执行 | 黑色终端窗口图标 💻 |
| `"key"` | TOTP、密钥、密码、令牌 | 金色钥匙图标 🔑 |
| `"shield"` | 代理、安全、权限控制 | 护盾防御图标 🛡 |

---

### 3.4 `export function on_select(item)` (必需)

用户在界面中按回车键或点击选中某条结果时触发。在此处执行实际动作（网络通信、文件变更、系统管理、剪贴板写入等）。

- **入参**：`item: SearchResultItem` —— 即 `on_query` 返回的条目对象本身。

```javascript
export function on_select(item) {
  const action = item && item.payload && item.payload.action;
  switch (action) {
    case "run":
      // 执行动作...
      break;
    default:
      console.warn("未知动作: " + action);
  }
}
```

---

## 4. 宿主能力矩阵 (`launcher.*`)

宿主在 QuickJS 顶层预先注入了全局共享 API 网关 `launcher`。`launcher` 保持为纯净的共享 API 接入点，不携带任何插件私有配置（插件配置统一通过 `self.get_config()` 获取）。

### 4.0 能力总览与速查表

| 能力模块 | 挂载路径 | 核心方法 / 特性 | 主要应用场景 |
| :--- | :--- | :--- | :--- |
| **图标缓存** | `launcher.cache_icon` | `cache_icon(preset \| path \| buffer)` | 预设矢量图标、本地图片注册与 ArrayBuffer 图像入池 |
| **文件系统** | `launcher.fs` | `exists`, `read`, `read_text`, `write`, `append`, `copy`, `move`, `remove`, `mkdir`, `list`, `stat` | Win32 原生高性能文件与目录读写、二进制流及元信息查询 |
| **网络通信** | `launcher.http` / 全局 `fetch` | `get`, `post`, `request` / `fetch()` | 基于成熟 reqwest 引擎的高可靠 HTTPS 请求，**底层透明应用系统代理** |
| **进程与提权**| `launcher.shell` / `launcher.process` | `exec`, `batch_exec`, `spawn`, `open` | 控制台静默调用、`auto_sudo` 智能单次提权与批处理、外部程序/网址打开 |
| **环境变量** | `launcher.env` | `get`, `set`, `has`, `remove`, `all` | 系统与进程级环境变量读写探测 |
| **宿主与弹窗**| `launcher.app` / `launcher.message_box` | `quit`, `reload`, `edit_config`, `message_box`, `alert` | 原生 MessageBox 弹窗、主配置热重载与生命周期管理 |
| **隔离沙箱** | `launcher.create_context` / `eval_module` | `create_context()`, `eval_module(code)` | 创建独立 QuickJS 隔离虚拟机，支持安全动态模块评估与依赖预解析 |
| **原生进度条**| `launcher.progress` | `create()`, `show()`, `update()`, `close()` | 独立 UI 消息线程与 GDI 双缓冲原生 Win32 进度条（支持面向对象与单例） |
| **系统剪贴板**| `launcher.clipboard` | `write_text(str)`, `write(str)` | 系统剪贴板内容写入与同步 |
| **控制台日志**| `console` | `log`, `info`, `warn`, `error` | 自动绑定插件名称前缀（如 `-[插件名]`），并入 Rust `tracing` 引擎 |

### 4.1 图标注册 (`launcher.cache_icon`)

```javascript
// 1. 注册预设矢量图标（直接返回预设 ID，如 "calc", "shield", "file", "folder", "gear" 等）
const iconId = launcher.cache_icon("shield");

// 2. 从本地相对/绝对路径注册自定义图片
const customIcon = launcher.cache_icon("./icon.png");

// 3. 从内存二进制 Buffer 注册
const bufIcon = launcher.cache_icon(imageBuffer);
```

---

### 4.2 文件系统操作 (`launcher.fs`)

基于 Windows 原生 Win32 API（`CopyFileW`、`MoveFileExW`、`DeleteFileW`）封装，原生支持二进制 Buffer：

```javascript
// 1. 判断路径存在
launcher.fs.exists("C:/path/to/file.txt"); // -> boolean

// 2. 读取文本与二进制
const text = launcher.fs.read_text("C:/config.json", "utf-8"); // -> string
const buffer = launcher.fs.read("C:/image.png");               // -> ArrayBuffer
const bytes = new Uint8Array(buffer);

// 3. 写入文件（支持 string, ArrayBuffer, Uint8Array）
launcher.fs.write("C:/output.txt", "Hello World");
launcher.fs.write("C:/output.bin", bytes);

// 4. 追加写入
launcher.fs.append("C:/app.log", "New log line\n");

// 5. 原生极速复制与移动 (调用 Win32 CopyFileW / MoveFileExW)
launcher.fs.copy("C:/src.txt", "C:/dest.txt", true); // overwrite = true
launcher.fs.move("C:/old.txt", "C:/new.txt", true);

// 6. 删除文件或递归删除目录
launcher.fs.remove("C:/temp_file.txt");
launcher.fs.remove("C:/temp_dir"); // 若为目录自动递归删除

// 7. 创建目录
launcher.fs.mkdir("C:/a/b/c", true); // recursive = true

// 8. 遍历目录
const files = launcher.fs.list("C:/Windows"); // -> string[] (文件名列表)

// 9. 获取文件元数据
const stat = launcher.fs.stat("C:/boot.ini");
// -> { size: number, is_dir: boolean, is_file: boolean, readonly: boolean, modified_ms: number }
```

---

### 4.3 网络请求 (`launcher.http` 与全局 `fetch`)

基于 Rust 成熟网络库 `reqwest` 实现，支持完整 HTTPS、系统证书与智能重定向：

```javascript
// 方式一：全局标准 fetch API
const res = await fetch("https://api.github.com/repos/user/repo", {
  method: "GET",
  headers: { "User-Agent": "OneLauncher" }
});
if (res.ok) {
  const json = await res.json();
  const text = await res.text();
}

// 方式二：同步/直接 launcher.http 接口
const getRes = launcher.http.get("https://my-domain.com/sub", {
  headers: { "Authorization": "Bearer xxx" },
  timeout_ms: 10000
});
// getRes -> { ok: boolean, status: 200, status_text: "OK", headers: {...}, body: string, buffer: ArrayBuffer, json(): any }

const postRes = launcher.http.post("http://127.0.0.1:9090/configs?force=true", "{}", {
  headers: { "Content-Type": "application/json" }
});
```

> [!NOTE]
> 网络代理统一在宿主配置层（`config.toml` 中的 `[network]` 节点）集中接管控制。插件发起网络请求时无需关心是否走代理，宿主会自动根据系统配置适配直连或指定代理（支持 HTTP/HTTPS/SOCKS5）。

---

### 4.4 命令与外部进程 (`launcher.shell` & `launcher.process`)

支持静默控制台执行、批处理、UAC 单次提权以及失败自动重试机制。

#### A. 单命令执行：`launcher.shell.exec(command, options?)`

```javascript
const res = launcher.shell.exec("reg query \"HKCU\\Software\"", {
  args: ["..."],            // (可选) 纯命令行参数数组
  cwd: "C:/",               // (可选) 工作目录
  env: { FOO: "BAR" },      // (可选) 环境变量
  shell: true,              // (可选) 默认 true，通过 cmd.exe /C 执行
  sudo: false,              // (可选) 默认 false，设为 true 时弹出单次 UAC 提权执行
  auto_sudo: false,         // (可选) 默认 false，普通执行失败时自动提权重试！
  timeout_ms: 30000,        // (可选) 超时毫秒数
  show_window: false        // (可选) 默认 false，无黑框静默运行
});

// 返回值结构:
// res -> {
//   code: number,               // 退出码 (0 为成功)
//   stdout: string,             // 智能解码输出 (优先 UTF-8，自动兼容 GBK/ANSI)
//   stderr: string,             // 错误输出
//   success: boolean,           // code === 0
//   cancelled: boolean,         // 若为提权执行，用户是否在 UAC 弹窗中点击了“否”
//   retried_with_sudo: boolean  // 是否由于 auto_sudo 触发了二次提权执行
// }
```

#### B. 批量执行：`launcher.shell.batch_exec(commands, options?)`

> [!TIP]
> 当需要执行多条命令（如修改多项注册表、配置网络代理）时，**必须使用 `batch_exec` 并传入 `auto_sudo: true`**。
> 这样所有命令会打包至单个会话中执行，**全局仅触发一次 UAC 授权弹窗**，彻底免去多次弹窗打扰。

```javascript
const commands = [
  'reg add "HKCU\\Software\\...\\Internet Settings" /v ProxyEnable /t REG_DWORD /d 1 /f',
  'reg add "HKCU\\Software\\...\\Internet Settings" /v ProxyServer /t REG_SZ /d "127.0.0.1:7890" /f',
  'reg add "HKCU\\Software\\...\\Internet Settings" /v ProxyOverride /t REG_SZ /d "<local>" /f'
];

const res = launcher.shell.batch_exec(commands, {
  auto_sudo: true,     // 优先静默普通执行，若权限受限失败则自动单次提权重试
  stop_on_error: true  // 遇错立即终止后续指令
});

if (res.cancelled) {
  launcher.message_box("操作取消：需要管理员权限以写入配置。", "权限提示", "warn");
  return;
}

if (res.success) {
  launcher.message_box("配置已成功应用！", "提示", "info");
} else {
  launcher.message_box("配置失败: " + res.stderr, "错误", "error");
}
```

#### C. 后台程序启动与外链打开：

```javascript
// 启动脱机后台外部进程（不等待退出）
launcher.shell.spawn("notepad.exe", ["C:/test.txt"]);

// 调用系统默认关联程序打开文件、目录或网址
launcher.shell.open("https://github.com");
launcher.shell.open("C:/Users");
```

---

### 4.5 环境变量操作 (`launcher.env`)

```javascript
// 读取环境变量
const mihomoCfg = launcher.env.get("MIHOMO_CONFIG") || launcher.env.MIHOMO_CONFIG;

// 设置环境变量（进程级）
launcher.env.set("MY_FLAG", "1");

// 检查与删除
if (launcher.env.has("TEMP_VAR")) {
  launcher.env.remove("TEMP_VAR");
}

// 获取全部环境变量字典
const allEnv = launcher.env.all();
```

---

### 4.6 宿主应用控制与消息弹窗 (`launcher.app`)

```javascript
// 1. 弹出系统原生 MessageBoxW 弹窗
// launcher.message_box(text, title?, level?)
// level 支持: "info" (信息), "warn" (警示), "error" (错误), "ok" (普通确认)
launcher.message_box("订阅已更新完成并重新载入！", "Clash 控制器", "info");

// 2. 快捷别名
launcher.alert("操作异常！", "系统提示", "error");

// 3. 应用生命周期控制
launcher.app.quit();         // 退出 OneLauncher 宿主程序
launcher.app.reload();       // 重新载入主配置文件并刷新插件池
launcher.app.edit_config();  // 在系统默认编辑器中打开 config.toml
const cfgPath = launcher.app.config_path(); // 获取 config.toml 绝对路径
```

---

### 4.7 虚拟机与完全隔离上下文 (`launcher.create_context`, `launcher.eval_module`)

宿主提供了创建完全隔离的 QuickJS 独立运行时（独立堆与垃圾回收器）以及安全评估 ES 模块的能力。此能力特别适合在插件管理器中预解析第三方脚本、在沙盒环境中拦截资源注册，或执行动态安全评估：

```javascript
// 1. 创建全新的完全隔离 QuickJS 上下文
const vm = launcher.create_context();

// 2. 在隔离环境中安全执行代码 (支持捕获或模拟环境变量与钩子)
vm.eval(`
  globalThis.__captured = [];
  globalThis.launcher = {
    cache_icon: (p) => { globalThis.__captured.push(p); return p; }
  };
`);

// 3. 评估 ES Module 脚本，提取导出的 metadata 与导出符号列表
const mod = vm.eval_module(`
  export const metadata = { name: "我的工具", prefix: "tool:" };
  export function on_setup() { launcher.cache_icon("./icon.png"); }
`);
console.info(mod.metadata.name); // -> "我的工具"
console.info(mod.exports);       // -> ["metadata", "on_setup"]

// 4. 调用隔离上下文中的函数
vm.call("on_setup");
const capturedIcons = vm.eval("globalThis.__captured"); // -> ["./icon.png"]

// 5. 便捷静态 ES 模块评估方法
const result = launcher.eval_module(moduleCodeString);
```

---

### 4.8 原生进度条展示 (`launcher.progress`)

用于长时间耗时任务（如插件管理器下载插件、依赖资源拉取、耗时批处理等）的原生进度展示，采用 Win32 原生 GDI 双缓冲渲染与独立 UI 消息线程驱动，平滑不卡顿：

```javascript
// 方式一：面向对象实例控制 (推荐)
const pb = launcher.progress.create("插件安装", "正在连接下载源...", 10);
pb.update(45, "正在下载依赖资源...");
pb.update(100, "安装完成！");
pb.close();

// 方式二：过程式全局单例调用
launcher.progress.show("任务标题", "正在处理...", 0);
launcher.progress.update(50, "处理中...");
launcher.progress.close();

// 快捷别名 (同时挂载于 launcher 与 launcher.app)
launcher.show_progress("标题", "描述", 20);
launcher.update_progress(80, "即将完成");
launcher.close_progress();
```

---

### 4.9 剪贴板操作 (`launcher.clipboard`)

```javascript
launcher.clipboard.write_text("需要拷贝的字符串");
launcher.clipboard.write("别名方法");
```

---

### 4.10 控制台日志 (`console`)

QuickJS 中的 `console` 已桥接至 Rust 原生 `tracing` 日志引擎，支持日志级别输出：

```javascript
console.log("普通调试信息");
console.info("操作信息追踪");
console.warn("警示与非致命异常");
console.error("致命错误记录");
```

---

## 5. 插件开发合规自查表 (Checklist)

在编写或重构插件代码时，必须逐项核对以下规范要求：

1. [ ] **必须包含完整的 `metadata` 声明**：提供唯一的 `name`、`prefix` 及 `default_config`。
2. [ ] **`on_query` 必须返回规范的条目数组**：
   - 每一个 item 必须包含：`id`、`title`、`subtitle`、`icon`、`badge`、`payload`。
   - **`badge` 字段严禁缺失**（应准确表明功能分类，如 `"系统"`、`"网络"`、`"计算"`、`"代理"` 等）。
3. [ ] **空输入与前缀补全逻辑**：
   - 当 `input` 为空字符串或空格时，返回全部候选 action 列表；
   - 当 `input` 非空时，采用 `item.id.toLowerCase().startsWith(input.trim().toLowerCase())` 筛选。
4. [ ] **交互与静默分级策略**：
   - **只读查询（如获取时间、仪表盘外链打开、检索结果展示）**：保持**静默**，只在 `subtitle` 或剪贴板反馈，**切勿弹出 `message_box` 打扰用户**。
   - **写操作与状态变更（如更新配置、启停代理、安装/卸载插件、执行脚本）**：**必须通过 `launcher.message_box` 提示成功或失败**。
5. [ ] **异常与权限提权防御**：
   - 执行注册表或系统级修改时，使用 `batch_exec` 并配置 `auto_sudo: true`。
   - 检查 `res.cancelled`：如果用户在 UAC 提权界面中点击“否”，优雅提示用户“操作已取消”，不得崩溃。
   - 任何涉及 I/O 与网络的 `on_select` 逻辑必须用 `try { ... } catch (err) { ... }` 完整包裹。
6. [ ] **全局作用域与生命周期参数规范**：
   - 全局变量严格限制为共享网关 `launcher` 与自有上下文 `self`；
   - `on_setup()` 必须为无参定义；
   - 仅 `on_query(input)` 与 `on_select(item)` 允许接收参数。
7. [ ] **长耗时操作原生进度条反馈**：
   - 涉及网络批量下载、依赖拉取或耗时大于 1 秒的任务，建议调用 `launcher.progress.create(...)` 唤起原生独立线程进度条，保持用户体验平滑，并在任务结束或异常时务必调用 `close()`。
8. [ ] **系统网络代理透明解耦**：
   - 插件网络请求直接调用 `launcher.http` 或全局 `fetch`，严禁在插件业务代码中硬编码或手动判断代理规则；宿主在底层根据 `config.toml` 的 `[network]` 配置统一完成透明路由。

---

## 6. 经典插件参考实现模板

### 示例一：系统控制插件（前缀路由、空输入补全与生命周期）

```javascript
export const metadata = {
  name: "系统控制",
  prefix: "sys:",
  default_config: {}
};

const ACTIONS = [
  { id: "quit", title: "退出程序", subtitle: "彻底退出 OneLauncher", icon: "warn", badge: "系统", payload: { cmd: "quit" } },
  { id: "reload", title: "重载配置", subtitle: "重新加载 config.toml 与所有插件", icon: "gear", badge: "系统", payload: { cmd: "reload" } },
  { id: "edit", title: "编辑配置", subtitle: "使用系统默认编辑器打开配置文件", icon: "file", badge: "配置", payload: { cmd: "edit" } }
];

export function on_query(input) {
  const query = (input || "").trim().toLowerCase();
  if (!query) return ACTIONS;
  return ACTIONS.filter(a => a.id.toLowerCase().startsWith(query));
}

export function on_select(item) {
  const cmd = item?.payload?.cmd;
  if (cmd === "quit") launcher.app.quit();
  else if (cmd === "reload") launcher.app.reload();
  else if (cmd === "edit") launcher.app.edit_config();
}
```

---

### 示例二：网络与注册表提权插件（带原生 HTTP、`batch_exec` 与 `auto_sudo`）

```javascript
export const metadata = {
  name: "网络与代理控制器",
  prefix: "net:",
  default_config: { proxy_port: 7890 }
};

export function on_query(input) {
  const all = [
    { id: "enable", title: "开启全局代理", subtitle: "设置 Windows 系统代理", icon: "shield", badge: "网络", payload: { action: "enable" } },
    { id: "disable", title: "关闭全局代理", subtitle: "禁用 Windows 系统代理", icon: "shield", badge: "网络", payload: { action: "disable" } }
  ];
  const q = (input || "").trim().toLowerCase();
  return !q ? all : all.filter(item => item.id.startsWith(q));
}

export function on_select(item) {
  const action = item?.payload?.action;
  try {
    if (action === "enable") {
      const port = 7890;
      const cmds = [
        'reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyEnable /t REG_DWORD /d 1 /f',
        `reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyServer /t REG_SZ /d "127.0.0.1:${port}" /f`
      ];
      // 一次性批处理提交，支持权限不足时自动单次提权
      const res = launcher.shell.batch_exec(cmds, { auto_sudo: true });
      if (res.cancelled) {
        launcher.message_box("用户取消了授权，设置未应用。", "代理提示", "warn");
        return;
      }
      if (res.success) {
        launcher.message_box(`系统代理已成功开启 (127.0.0.1:${port})`, "代理提示", "info");
      } else {
        launcher.message_box(`修改失败:\n${res.stderr}`, "代理错误", "error");
      }
    } else if (action === "disable") {
      const res = launcher.shell.batch_exec([
        'reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyEnable /t REG_DWORD /d 0 /f'
      ], { auto_sudo: true });
      if (res.success) {
        launcher.message_box("系统代理已成功关闭！", "代理提示", "info");
      }
    }
  } catch (err) {
    launcher.message_box(`执行异常:\n${err}`, "系统错误", "error");
  }
}
```

---

## 7. 命令行调试工具 (CLI Diagnostics)

OneLauncher 提供用于在终端中完全隔离调试插件的命令行模式。在调用时**不启动与绘制 GUI 窗口**，直接将查询结果与执行状态结构化返回至终端。

### 7.1 核心命令参数

| 参数 | 缩写 | 说明 |
| :--- | :--- | :--- |
| `-plugin <target>` | `-p` | **隔离加载单个插件**：支持指定插件名（如 `clash`、`system`）、插件目录（`plugins/clash`）或入口脚本绝对路径 |
| `-input "<query>"` | `-i`, `-q` | 指定查询内容（如 `"sys:"`、`"clash: update"`、`""`） |
| `-select <id\|index>`| `-s` | 查询完成后自动触发指定结果条目的 `on_select` 进行动作测试 |
| `-debug` | `-d` | 开启调试模式（日志级别自动提升为 `debug`，并输出至控制台） |
| `-log-out <dest>` | `-l` | 指定日志输出目标，默认为 `'std'`（控制台），可选 `'fs:<path>'` |
| `-log-level <level>`| 无 | 日志级别：`trace`、`debug`、`info`、`warn`、`error`、`silent` |
| `-format <text\|json>`| `-f` | 结果输出格式：`text`（默认易读文本）或 `json`（结构化 JSON，便于脚本解析） |
| `-config <path>` | `-c` | 指定自定义 `config.toml` 文件路径 |
| `-help` | `-h`, `/?` | 查看完整的命令行帮助指南 |

### 7.2 插件日志标识与结果输出规范

1. **自动前缀标识**：插件通过 `console.log`、`console.info`、`console.warn`、`console.error` 或 `launcher.log` 输出的每条日志，宿主都会自动加上当前插件名称的前缀（例如 `-[Clash 控制器] 正在请求订阅...` 或 `-[系统管理] 退出程序`）。
2. **结果与日志明确分隔**：宿主查询结果被严格置于专用的分界线块内，便于脚本解析与核验：

```text
======================== ONELAUNCHER RESULTS ========================
Plugin:   Clash 控制器 (id: clash, prefix: 'clash:')
Input:    "update"
Count:    1 item(s)

  [0] Update Subscription
      Subtitle: 备份并下载最新订阅 (当前: 10 秒前更新)
      Badge:    Clash
      Icon:     info
      Id:       update-subscription
      Payload:  {"action":"update-subscription"}

=====================================================================
```

### 7.3 常用调试命令示例

- **验证空输入菜单补全**：
  `one-launcher -plugin system -input "sys:"`
- **验证搜索过滤与结果内容**：
  `one-launcher -plugin clash -input "update"`
- **以 JSON 格式快速获取结果数据**：
  `one-launcher -plugin system -input "sys: q" -format json`
- **全链路测试回车执行 (on_select)**：
  `one-launcher -debug -plugin clash -input "clash: sub" -select subscription-time`

