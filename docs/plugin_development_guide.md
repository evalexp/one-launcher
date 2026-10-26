# OneLauncher 插件开发指南

本文档介绍 OneLauncher 插件的运行机制、接口契约、全局 API 与开发流程。

---

## 一、 架构与运行机制

1. **JavaScript 引擎**：内嵌 QuickJS 运行时，支持 ES 模块（`export` / `import`）。
2. **后台 Worker 线程隔离**：所有插件的生命周期方法（`on_setup`、`on_query`、`on_select` 及 Native FFI 调用）均在专用的后台工作线程中执行，不阻塞主界面渲染与输入交互。
3. **前缀路由机制 (Prefix Routing)**：
   - 本地应用程序检索为默认功能（无需前缀）；
   - 每个插件需声明专属的 `prefix`（例如 `fs:`、`=` 等），当用户输入的字符匹配此前缀时，由对应插件独占处理查询。

---

## 二、 目录结构与接口契约

标准插件目录结构：
```text
plugins/my_plugin/
├── index.js          # 插件入口文件 (默认 entry)
├── icon.png          # (可选) 自定义图标文件
└── MyLibrary64.dll   # (可选) 原生动态链接库
```

插件必须导出一个 `metadata` 元信息对象，并按需实现约定的生命周期函数：

### 1. `export const metadata` (必需)
定义插件的基础标识、前缀、默认配置及可选的原生动态库绑定：
```javascript
export const metadata = {
  name: "示例插件",        // 插件显示名称
  prefix: "demo:",          // 触发前缀
  default_config: {         // 默认配置 (可在主配置文件 config.toml 中覆写)
    max_results: 10,
    api_key: "",
  },
  // (可选) 声明式原生动态链接库绑定
  dynamic_library: {
    // 详见后文 FFI 章节
  }
};
```

---

### 2. `export function on_setup(context)` (可选)
插件加载时触发一次初始化。

- **入参**：`context` 对象，专用于执行一次性重型初始化与图标缓存：
  - `context.cache_icon(bytes | path): string`：将图标文件路径（相对路径或绝对路径）或二进制字节（`Uint8Array` / `ArrayBuffer` / 数字数组）存入主程序图标池，返回唯一的 `ref_id`。可在 `on_query` 中作为 `icon` 字段使用。
- **示例**：
```javascript
let myIconRef = "file";

export function on_setup(context) {
  // 缓存本地图标文件并获取引用 ID
  myIconRef = context.cache_icon("./icon.png");
  console.info("插件初始化完成");
}
```

---

### 3. `export function on_query(input)` (必需)
当用户输入前缀后的查询内容且满足防抖间隔时触发。

- **入参**：`input: string` —— 剥离前缀后的查询字符串（例如输入 `= 1+2` 时，`input` 为 `"1+2"`）。
- **返回值**：必须返回对象数组，每个元素需包含以下字段：

```javascript
[
  {
    id: "唯一标识",           // string，必需
    title: "主标题文本",       // string，必需，列表中突出显示
    subtitle: "副标题说明",    // string，必需，列表中灰度显示
    icon: "图标引用ID",       // string，必需，支持内置预设或 cache_icon 返回的 ref_id
    payload: { ... },         // any，必需，回车确认时传递给 on_select
    is_folder: false,         // boolean，可选，显式标记是否为目录
  }
]
```

#### 角标 (Badge) 显示规则：
启动器列表条目右侧的角标根据条目类型自动判定：
- 当条目为目录（`is_folder: true` 或 `icon: "folder"` / `"dir"`）时，角标显示为 `[文件夹]`；
- 当条目为普通文件（`icon: "file"` / `"doc"`）时，角标显示为 `[文件]`；
- 其余情况，角标显示为当前插件的 `name`（例如 `[极速计算器]`）。

#### 内置预设图标列表：
插件可直接在 `icon` 字段填入以下内置引用名，无需额外提供图片资源：

| 图标 ref_id | 视觉样式 |
| :--- | :--- |
| `"file"` / `"doc"` | 白底折角文档图标 |
| `"folder"` / `"dir"` | 琥珀黄文件夹图标 |
| `"error"` | 红色圆环错误叉号 ❌ (故障诊断状态) |
| `"success"` | 绿色圆环成功对勾 ✔ (操作完成状态) |
| `"warn"` / `"warning"` | 标准警示三角黄标 ⚠ |
| `"info"` | 标准信息对话框蓝标 ℹ |
| `"calc"` / `"calculator"` | 计算器图标 |
| `"terminal"` / `"cmd"` | 终端控制台图标 |
| `"settings"` / `"config"` | 系统齿轮设置图标 |
| `"notepad"` / `"text"` | 记事本文档图标 |
| `"shield"` | Windows UAC 盾牌图标 |

---

### 4. `export function on_select(item)` (必需)
当用户在候选列表中按 `Enter` 回车或鼠标点击条目时触发。

- **入参**：`item: { id: string, payload: any }` —— 对应 `on_query` 返回条目中的 `id` 与 `payload`。
- **典型操作**：调用 `launcher.shell.open` 打开文件或链接，或调用 `launcher.clipboard.write_text` 复制到剪贴板。

---

## 三、 全局宿主 API

OneLauncher 在插件上下文中注入了全局对象，供插件随时调用。

### 1. `launcher.shell`
提供系统外壳操作接口。
- **`launcher.shell.open(targetPath: string): void`**
  - 使用 Windows 默认关联程序打开目标路径、启动 `.exe`、打开目录或在默认浏览器中打开网址。
  - 示例：
    ```javascript
    launcher.shell.open("https://github.com");
    launcher.shell.open("C:\\Windows\\notepad.exe");
    launcher.shell.open("D:\\Documents");
    ```

### 2. `launcher.clipboard`
提供系统剪贴板原生操作接口。
- **`launcher.clipboard.write_text(text: string): boolean`**（别名：`launcher.clipboard.write`）
  - 以 UTF-16 原生格式将文本安全写入 Windows 系统剪贴板。
  - 示例：
    ```javascript
    launcher.clipboard.write_text("42");
    ```

### 3. `launcher.config`
当前插件生效的配置快照对象。
- 在插件加载时，主程序将 `metadata.default_config` 与用户 `config.toml` 中 `[plugins.<id>.config]` 对应项合并后注入该对象；
- 同时在全局作用域注入别名 `config`，支持 `launcher.config.xxx` 或 `config.xxx` 访问。
- 示例：
  ```javascript
  const precision = (launcher.config && launcher.config.precision) || 6;
  ```

### 4. `launcher.log` 与全局 `console`
提供运行时日志输出，日志输出目标根据主程序 `config.toml` 中的 `[log]` 设置（控制台或本地日志文件）：
- `launcher.log.info(msg: string)`
- `launcher.log.warn(msg: string)`
- `launcher.log.error(msg: string)`
- `launcher.log.debug(msg: string)`

亦支持标准 `console` 对象调用：
- `console.log(...)` / `console.info(...)`（映射为 INFO 等级）
- `console.warn(...)`（映射为 WARN 等级）
- `console.error(...)`（映射为 ERROR 等级）
- `console.debug(...)`（映射为 DEBUG 等级）

### 5. 全局 `native` 对象
当插件声明了 `metadata.dynamic_library` 时，各动态库导出的 C 函数会自动挂载至全局 `native.<LibName>.<SymbolName>` 下，支持在 JS 中直接同步调用。

---

## 四、 原生动态链接库 (FFI) 声明式绑定

如插件需要调用 Windows C/C++ 动态链接库（`.dll`），可在 `metadata.dynamic_library` 中声明：

```javascript
export const metadata = {
  name: "FFI 示例",
  prefix: "ffi:",
  dynamic_library: {
    MyLib: {
      path: "./MyLib64.dll", // 相对于插件目录的相对路径，或绝对路径
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

### 支持的参数与返回值类型：
- **`"u32"` / `"i32"`**：32 位整数；
- **`"bool"`**：布尔值；
- **`"string"`**：UTF-16 字符串指针 (`*const u16`)；
- **`"buffer"`**：UTF-16 输出缓冲区，绑定后在 JS 中调用自动解码为 JavaScript 字符串返回；
- **`"void"`**：无返回值。

若 DLL 不存在或导出符号缺失，启动加载时将直接提示报错并跳过该插件。

---

## 五、 示例 1：Everything 全盘文件搜索 (Native C DLL 插件)

源码位置：[`plugins/everything/index.js`](file:///D:/src/one-launcher/plugins/everything/index.js)

```javascript
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

let fileIconRef = "file";
let folderIconRef = "folder";

export function on_setup(context) {
  fileIconRef = "file";
  folderIconRef = "folder";
  console.info("Everything 插件已就绪");
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
    if (err === 2) { // EVERYTHING_ERROR_IPC (主程序未运行)
      return [{
        id: "everything_not_running",
        title: "未检测到 Everything 正在运行",
        subtitle: "检索服务依赖客户端主程序，请先启动 Everything",
        icon: "error", // 展示错误红标
        payload: { action: "start_everything" },
      }];
    }
    return [];
  }

  // 2. 获取结果并读取配置项
  const total = native.Everything.Everything_GetNumResults();
  const maxResults = (launcher.config && launcher.config.max_results) || 9;
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
      is_folder: isFolder,
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

## 六、 示例 2：Calculator 数学计算器 (纯 JavaScript 插件)

源码位置：[`plugins/calculator/index.js`](file:///D:/src/one-launcher/plugins/calculator/index.js)

```javascript
export const metadata = {
  name: "极速计算器",
  prefix: "=",
  default_config: {
    precision: 6,
    auto_copy: true,
  },
};

let calcIcon = "calc";

export function on_setup(context) {
  calcIcon = "calc";
  console.info(`Calculator 插件已就绪 (精度: ${(launcher.config && launcher.config.precision) || 6})`);
}

export function on_query(input) {
  if (!input || input.trim() === "") {
    return [];
  }

  const raw = input.trim();
  const precision = (launcher.config && launcher.config.precision) || 6;
  const result = evaluateMath(raw, precision);

  if (result === null) {
    return [];
  }

  return [
    {
      id: "calculator_result",
      title: `${result}`,
      subtitle: `= ${raw}  (回车复制到剪贴板)`,
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
  console.info(`已复制计算结果 '${val}' 到剪贴板`);
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

## 七、 插件配置与调试

### 1. 在 `config.toml` 中声明插件
在主配置文件中添加插件条目：
```toml
[plugins.calculator]
enabled = true
prefix = "="
name = "极速计算器"
path = "plugins/calculator"
# entry = "index.js" (可选，默认 index.js)

# 覆写插件在 metadata.default_config 中声明的默认配置项
[plugins.calculator.config]
precision = 4
```

### 2. 热重载
编辑插件脚本或修改 `config.toml` 后，右键任务栏系统托盘图标点击 **“重载配置”**，即可重新加载生效，无需重启启动器。

### 3. 日志排查
在 `config.toml` 中配置：
```toml
[log]
level = "debug"
path = "fs:logs/one-launcher.log"
```
即可查看插件加载、`on_setup`、`on_query`、`on_select` 执行日志与错误堆栈。
