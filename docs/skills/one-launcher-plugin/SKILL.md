---
name: one-launcher-plugin
description: Specification and standard guide for creating, maintaining, and testing OneLauncher JavaScript plugins. Use when implementing plugins, querying capabilities, file operations, native WinHttp requests, process execution with UAC elevation, or dynamic FFI bindings for OneLauncher.
---

# OneLauncher 插件开发规范与 AI 智能体指令 (SKILL)

本规范专为编写、生成与重构 **OneLauncher** 插件的大语言模型（LLM）及开发者设计。编写 OneLauncher 插件时，必须严格遵守本规范中的接口契约、安全准则与性能边界。

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
prefix = "myprefix:"
name = "我的插件名称"
path = "plugins/<plugin-id>"
entry = "index.js"

[plugins.<plugin-id>.config]
custom_field = "custom_value"
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

### 3.2 `export function on_setup(context)` (可选)

插件首次加载或重新载入时触发一次。

- **入参**：`context` 初始化上下文对象：
  - `context.cache_icon(path_or_bytes): string`：将本地相对/绝对路径图片文件，或内存二进制数据（`ArrayBuffer` / `Uint8Array`）存入宿主图标池，返回唯一的图标 `ref_id`。

```javascript
let myIconId = "gear";

export function on_setup(context) {
  try {
    myIconId = context.cache_icon("./icon.png");
    console.info("插件初始化成功，已缓存自定义图标");
  } catch (err) {
    console.warn("缓存图标失败，使用后备图标: " + err);
  }
}
```

---

### 3.3 `export function on_query(input)` (必需)

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

宿主在 QuickJS 顶层预先注入了完整的 Windows 原生能力对象 `launcher`：

### 4.1 文件系统操作 (`launcher.fs`)

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

### 4.2 网络请求 (`launcher.http` 与全局 `fetch`)

基于 Windows 原生 `WinHttp`（`winhttp.dll`）实现，零外部依赖，支持 HTTPS 与重定向：

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

---

### 4.3 命令与外部进程 (`launcher.shell` & `launcher.process`)

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

### 4.4 环境变量操作 (`launcher.env`)

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

### 4.5 宿主应用控制与消息弹窗 (`launcher.app`)

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

### 4.6 剪贴板操作 (`launcher.clipboard`)

```javascript
launcher.clipboard.write_text("需要拷贝的字符串");
launcher.clipboard.write("别名方法");
```

---

### 4.7 控制台日志 (`console`)

QuickJS 中的 `console` 已桥接至 Rust 原生 `tracing` 日志引擎，支持日志级别输出：

```javascript
console.log("普通调试信息");
console.info("操作信息追踪");
console.warn("警示与非致命异常");
console.error("致命错误记录");
```

---

## 5. 大模型开发插件黄金法则 (LLM Checklist)

在为 OneLauncher 生成插件代码时，AI 智能体必须逐项核对以下要求：

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
