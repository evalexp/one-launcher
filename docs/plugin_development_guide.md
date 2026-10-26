# OneLauncher 插件开发规范与指南 (Plugin Development Guide)

> **欢迎加入 OneLauncher 插件生态！**
> OneLauncher 致力于打造 Windows 平台极速、轻量（常驻内存 8MB~15MB）、零后台 CPU 占用且高度解耦的类 macOS Spotlight / uTools 启动器。
> 本文档旨在指导第三方开发者快速掌握 OneLauncher 的插件开发流程与规范。

---

## 一、 核心架构与运行机制

1. **QuickJS 原生嵌入引擎**：微秒级启动，单次解释执行延迟 < 0.05ms，零重型 V8/Electron 开销。
2. **专用后台 Worker 线程隔离 (`OneLauncher-PluginWorker`)**：
   - 所有插件的初始化、ES Module 解析、Native FFI 调用及输入求值，均在专属的后台工作线程中运行；
   - 无论插件计算有多重或 FFI 是否跨进程阻塞，Win32 UI 线程始终保持 0ms 阻塞、丝滑响应。
3. **严格 Prefix 独占路由**：
   - 核心系统应用搜索为默认功能（无需前缀，直接在内存索引中检索）；
   - 插件功能必须绑定独立且独占的 `prefix`（如 `fs:`、`calc:` 等），**绝不将插件结果与系统本地应用混搜**。
4. **0 内存分配复用设计**：
   - 插件配置对象在加载期单次实例化，按键检索期间 0 额外堆内存分配。

---

## 二、 插件生命周期规范契约

一个标准的 OneLauncher 插件目录结构如下：
```text
plugins/my_plugin/
├── index.js          # 插件入口主文件 (默认 entry)
├── icon.ico          # (可选) 自定义图标文件
└── MyLibrary64.dll   # (可选) 声明式绑定的原生动态链接库
```

插件必须且仅需导出一个 `metadata` 元信息对象以及约定的生命周期函数：

### 1. `export const metadata` (必需)
定义插件的基础标识、前缀、默认配置项以及可选的动态链接库：
```javascript
export const metadata = {
  name: "我的扩展插件",      // 插件中文显示名称
  prefix: "demo:",          // 触发插件的专属前缀
  default_config: {         // 插件默认参数 (将被自动映射入主程序 TOML 配置中)
    max_results: 10,
    api_key: "",
  },
  // (可选) 声明式 Native 动态链接库绑定
  dynamic_library: {
    // 动态库声明...
  }
};
```

---

### 2. `export function on_setup(context)` (可选但推荐)
插件被主程序加载时触发单次初始化。`context` 提供宿主运行环境与辅助能力：

| context 成员方法 | 说明 | 示例 |
| :--- | :--- | :--- |
| `cache_icon(bytes \| path)` | 将图标二进制字节（Uint8Array/ArrayBuffer）或相对路径缓存至程序内部，返回唯一的 `ref_id` | `const iconId = context.cache_icon("./icon.ico");` |
| `get_plugin_config()` | 获取合并后的当前生效插件配置对象 | `const cfg = context.get_plugin_config();` |
| `get_logger()` | 获取带插件前缀标识的运行时日志输出器 | `const logger = context.get_logger(); logger.info("Ready");` |

---

### 3. `export function on_query(input)` (必需)
当用户在前缀后输入字符并满足防抖间隔后触发。

- **入参**：`input: string` —— 用户剥离前缀后的纯查询字符串（例如输入 `calc: 1+2`，`input` 为 `"1+2"`）。
- **返回值**：**必须返回对象数组**，每个条目必须满足规范字段：

```javascript
[
  {
    id: "唯一标识字符串",           // 必填 (string)
    title: "主标题文本",            // 必填 (string)，列表中以高亮醒目字体展示
    subtitle: "副标题说明或详情",    // 必填 (string)，列表中以低饱和灰度字体展示
    icon: "图标引用ID",            // 必填 (string)，支持内置预设或 cache_icon 返回的 ref_id
    payload: { ... }               // 必填 (object 或任意值)，回车选中时传递给 on_select
  }
]
```

#### 内置预设图标引用 (无需打包图片即可开箱即用)：
| 图标 ref_id | 视觉展示 |
| :--- | :--- |
| `"file"` / `"doc"` | 系统原生文件图标 |
| `"folder"` / `"dir"` | 系统原生文件夹图标 |
| `"calc"` / `"calculator"` | 系统计算器应用图标 |
| `"terminal"` / `"cmd"` | 系统命令提示符 / 终端图标 |
| `"settings"` / `"config"` | 系统控制面板 / 设置图标 |
| `"notepad"` / `"text"` | 系统记事本图标 |
| `"info"` | Windows 标准对话框信息蓝标 (ℹ) |
| `"warn"` / `"warning"` | Windows 标准警告黄标 (⚠) |
| `"error"` | Windows 标准错误红标 (❌) |
| `"shield"` | Windows UAC 安全盾牌图标 |

---

### 4. `export function on_select(item)` (必需)
当用户在候选列表中按下 `Enter` 回车或鼠标左键点击确认某项条目时触发。

- **入参**：`item: { id: string, payload: any }` —— 对应 `on_query` 返回条目中的 `id` 与 `payload`。
- **典型操作**：调用 `launcher.shell.open` 打开系统应用/网页，或使用 `launcher.clipboard.write_text` 写入剪贴板。

---

## 三、 全局宿主 API (`launcher`)

在插件执行上下文中，OneLauncher 原生注入了全局宿主对象：

### 1. `launcher.shell.open(targetPath)`
调用 Windows 系统默认关联程序打开目标路径、启动 `.exe`、打开目录或在默认浏览器中打开网址：
```javascript
launcher.shell.open("https://github.com");
launcher.shell.open("C:\\Windows\\notepad.exe");
```

### 2. `launcher.clipboard.write_text(text)` / `launcher.clipboard.write(text)`
以 `CF_UNICODETEXT` 原生格式安全写入字符串至 Windows 系统剪贴板：
```javascript
launcher.clipboard.write_text("1024");
```

### 3. `launcher.config`
获取该插件当前生效的配置快照。

### 4. 控制台与日志输出
支持标准 `console.log(...)`、`console.warn(...)`、`console.error(...)`，日志将根据主配置自动流向控制台或本地日志文件。

---

## 四、 原生动态链接库 (FFI) 声明式绑定

如果你的插件需要调用 Windows C/C++ 动态链接库（`.dll`），只需在 `metadata.dynamic_library` 中以声明式 JSON 配置：

```javascript
export const metadata = {
  name: "FFI 示例",
  prefix: "ffi:",
  dynamic_library: {
    MyLib: {
      path: "./MyLib64.dll",
      symbols: [
        { name: "AddNumbers", params: ["u32", "u32"], returns: "u32" },
        { name: "SetKeywordW", params: ["string"], returns: "void" },
        { name: "QueryData", params: ["bool"], returns: "bool" },
        { name: "GetResultPathW", params: ["u32", "buffer", "u32"], returns: "u32" },
      ]
    }
  }
};
```

- **加载期 Fail-Fast**：Rust 在启动插件时一次性校验 DLL 路径与所有导出符号。若符号不存在或签名不匹配，启动时即告警并拒绝加载，绝不在运行时产生段错误崩溃；
- **微秒级挂载**：通过校验后，所有符号自动挂载至全局 `native.<LibName>.<SymbolName>` 下，在 JS 中可直接同步调用：
  ```javascript
  const sum = native.MyLib.AddNumbers(10, 20);
  native.MyLib.SetKeywordW("keyword");
  ```

---

## 五、 官方完整示例 1：Everything 全盘极速文件搜索

> 源码位置：[`plugins/everything/index.js`](file:///D:/src/one-launcher/plugins/everything/index.js)  
> 特性演示：**声明式 Native C DLL 绑定** + **on_setup 生命周期** + **错误码异常诊断** + **系统程序联动**

```javascript
// ==============================================================================
// OneLauncher 官方插件: Everything 全盘极速文件搜索
// ==============================================================================

export const metadata = {
  name: "Everything 全盘文件搜索",
  prefix: "fs:",
  default_config: {
    max_results: 9,
  },
  dynamic_library: {
    Everything: {
      path: "./Everything64.dll",
      symbols: [
        { name: "Everything_SetSearchW", params: ["string"], returns: "void" },
        { name: "Everything_QueryW", params: ["bool"], returns: "bool" },
        { name: "Everything_GetNumResults", params: [], returns: "u32" },
        { name: "Everything_GetResultFullPathNameW", params: ["u32", "buffer", "u32"], returns: "u32" },
        { name: "Everything_IsFolderResult", params: ["u32"], returns: "bool" },
        { name: "Everything_GetLastError", params: [], returns: "u32" },
      ]
    }
  }
};

let logger = null;
let pluginConfig = null;
let fileIconRef = "file";
let folderIconRef = "folder";

export function on_setup(context) {
  logger = context.get_logger();
  pluginConfig = context.get_plugin_config();

  logger.info("Everything 插件正在初始化 (on_setup)...");

  // 使用系统内置预设图标引用
  fileIconRef = "file";
  folderIconRef = "folder";

  logger.info("Everything 插件初始化就绪");
}

export function on_query(input) {
  if (!input || input.trim() === "") {
    return [];
  }

  // 1. 设置搜索词并同步检索
  native.Everything.Everything_SetSearchW(input.trim());
  const ok = native.Everything.Everything_QueryW(true);

  if (!ok) {
    const err = native.Everything.Everything_GetLastError();
    if (err === 2) { // EVERYTHING_ERROR_IPC (主程序未开启)
      return [{
        id: "everything_not_running",
        title: "未检测到 Everything 正在运行",
        subtitle: "Everything 检索服务依赖客户端主程序提供支持，请先启动 Everything",
        icon: fileIconRef,
        payload: { action: "start_everything" },
      }];
    }
    return [];
  }

  // 2. 获取总结果数并读取配置项
  const total = native.Everything.Everything_GetNumResults();
  const maxResults = (pluginConfig && pluginConfig.max_results) || 9;
  const count = Math.min(total, maxResults);
  const items = [];

  for (let i = 0; i < count; i++) {
    const fullPath = native.Everything.Everything_GetResultFullPathNameW(i);
    const isFolder = native.Everything.Everything_IsFolderResult(i);
    const fileName = fullPath.split("\\").pop();

    items.push({
      id: fullPath,
      title: fileName || fullPath,
      subtitle: fullPath,
      icon: isFolder ? folderIconRef : fileIconRef,
      payload: {
        path: fullPath,
        is_folder: isFolder,
      },
    });
  }

  return items;
}

export function on_select(item) {
  if (!item) return;

  if (item.id === "everything_not_running" || (item.payload && item.payload.action === "start_everything")) {
    launcher.shell.open("everything.exe");
    return;
  }

  if (item.payload && item.payload.path) {
    launcher.shell.open(item.payload.path);
  }
}
```

---

## 六、 官方完整示例 2：Calculator 极速数学计算器

> 源码位置：[`plugins/calculator/index.js`](file:///D:/src/one-launcher/plugins/calculator/index.js)  
> 特性演示：**纯 JavaScript 轻量交互** + **安全数学求值** + **内置图标** + **剪贴板操作**

```javascript
// ==============================================================================
// OneLauncher 官方插件: 极速数学计算器 (Calculator)
// ==============================================================================

export const metadata = {
  name: "极速计算器",
  prefix: "calc:",
  default_config: {
    precision: 6,
    auto_copy: true,
  },
};

let logger = null;
let pluginConfig = null;
let calcIcon = "calc";

export function on_setup(context) {
  logger = context.get_logger();
  pluginConfig = context.get_plugin_config();

  // 使用内置计算器预设图标
  calcIcon = "calc";
  logger.info(`Calculator 计算器初始化就绪，精度: ${pluginConfig.precision || 6}`);
}

export function on_query(input) {
  if (!input || input.trim() === "") {
    return [];
  }

  const raw = input.trim();
  const precision = (pluginConfig && pluginConfig.precision) || 6;
  const result = evaluateMath(raw, precision);

  if (result === null) {
    return [];
  }

  return [
    {
      id: "calculator_result",
      title: `${result}`,
      subtitle: `= ${raw}  (回车即可复制计算结果到剪贴板)`,
      icon: calcIcon,
      payload: {
        expression: raw,
        result: result,
      },
    },
  ];
}

export function on_select(item) {
  if (!item || !item.payload || !item.payload.result) {
    return;
  }

  const val = String(item.payload.result);
  launcher.clipboard.write_text(val);
  if (logger) {
    logger.info(`已成功复制计算结果 '${val}' 到剪贴板`);
  }
}

function evaluateMath(rawExpr, precision) {
  let expr = rawExpr.trim().replace(/\^/g, "**").replace(/(\d+(\.\d+)?)%/g, "($1/100)");
  const scope = {
    pi: Math.PI, e: Math.E, sin: Math.sin, cos: Math.cos, tan: Math.tan,
    sqrt: Math.sqrt, abs: Math.abs, round: Math.round, log: Math.log, pow: Math.pow,
  };

  if (!/^[0-9+\-*/%().,\s a-zA-Z_]+$/.test(expr)) return null;

  try {
    const fn = new Function(...Object.keys(scope), `"use strict"; return (${expr});`);
    const num = fn(...Object.values(scope));
    if (typeof num === "number" && Number.isFinite(num)) {
      return Number(num.toFixed(precision)).toString();
    }
  } catch (err) {}
  return null;
}
```

---

## 七、 插件注册与调试方式

1. **在 `config.toml` 中注册**：
   在配置文件中添加插件子块：
   ```toml
   [plugins.my_plugin]
   enabled = true
   prefix = "demo:"
   name = "我的扩展插件"
   path = "plugins/my_plugin"
   # entry = "index.js" (可选，默认 index.js)

   # 覆写插件在 metadata.default_config 中声明的专属配置项
   [plugins.my_plugin.config]
   max_results = 8
   ```

2. **热重载调试**：
   修改 `index.js` 脚本或 `config.toml` 配置后，右键任务栏系统托盘图标点击 **“重载配置”**，插件系统即刻自动全量重载生效，无需重启主程序。

3. **查看排错日志**：
   在 `config.toml` 中配置：
   ```toml
   [log]
   level = "debug"
   path = "fs:logs/one-launcher.log"
   ```
   即可实时查阅插件 `on_setup`、`on_query`、`on_select` 执行详情与报错调用栈。
