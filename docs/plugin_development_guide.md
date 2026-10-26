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
    badge: "右侧角标文字",    // string，必需，列表中右侧展示（如 "文件"、"文件夹"、"计算器" 等）
    payload: { ... },         // any，必需，回车确认时传递给 on_select
  }
]
```

#### 角标 (Badge) 说明：
列表条目右侧展示的徽章文字完全由条目的 `badge` 字段指定。例如：
- Everything 检索到文件夹时指定 `badge: "文件夹"`，检索到文件时指定 `badge: "文件"`；
- 计算器插件指定 `badge: "计算器"`；
- 状态或异常提醒条目可指定 `badge: "提示"`。

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

### 1. `launcher.shell` 与 `launcher.process`
提供系统外壳操作、控制台命令执行与外部子进程管理接口。

- **`launcher.shell.open(targetPath: string): void`**
  - 使用 Windows 默认关联程序打开目标路径、启动可执行程序、打开目录或在默认浏览器中打开网址。
  - 示例：
    ```javascript
    launcher.shell.open("https://github.com");
    launcher.shell.open("C:\\Windows\\notepad.exe");
    launcher.shell.open("D:\\Documents");
    ```

- **`launcher.shell.exec(command: string, options?: ExecOptions): ExecResult`**（别名：`launcher.process.exec`）
  - 同步执行外部命令或程序，默认启用 `CREATE_NO_WINDOW`（绝无控制台黑框弹窗闪烁），并具备 UTF-8 与中文系统 GBK/ANSI 智能自动解码兼容。
  - **参数**：
    - `command: string`：待执行的命令或程序路径；
    - `options?: { args?: string[], cwd?: string, env?: Record<string, string>, shell?: boolean }`
      - `args`：当指定时直接启动程序并传入参数列表（无需经过 cmd 解析）；
      - `cwd`：工作目录路径；
      - `env`：自定义环境变量；
      - `shell`：是否通过 `cmd.exe /C` 执行（默认如果未指定 `args` 则为 `true`）。
  - **返回值 `ExecResult`**：
    ```javascript
    {
      code: 0,              // 进程退出码
      stdout: "输出文本",    // 标准输出（已解码为字符串）
      stderr: "",           // 标准错误
      success: true         // code === 0
    }
    ```
  - 示例：
    ```javascript
    // 方式 A：直接运行系统命令行命令
    const res = launcher.shell.exec("ipconfig /all");
    if (res.success) {
      console.log(res.stdout);
    }

    // 方式 B：传参执行外部工具程序
    const gitRes = launcher.shell.exec("git.exe", {
      args: ["status", "--porcelain"],
      cwd: "D:\\projects\\repo"
    });
    ```

- **`launcher.shell.spawn(command: string, args?: string[]): boolean`**（别名：`launcher.process.spawn`）
  - 脱机在后台启动外部子进程（无需等待其执行结束退出），返回是否成功拉起。
  - 示例：
    ```javascript
    launcher.shell.spawn("calc.exe");
    ```

---

### 2. `launcher.fs` (原生文件系统操作)
提供丰富、高性能的底层系统级文件与目录操作接口，读写支持原始二进制缓冲区与文本，复制移动直接调用 Windows Win32 原生 API。

- **`launcher.fs.read(path: string): ArrayBuffer | null`**
  - 读取指定文件的原始二进制字节流，返回标准 `ArrayBuffer`。文件不存在或读取失败时返回 `null`。
  - 示例：
    ```javascript
    const buffer = launcher.fs.read("D:\\data.bin");
    if (buffer) {
      const bytes = new Uint8Array(buffer);
      console.log("第一个字节:", bytes[0]);
    }
    ```

- **`launcher.fs.read_text(path: string): string | null`**
  - 读取文件并按 UTF-8 解码为文本字符串。
  - 示例：
    ```javascript
    const content = launcher.fs.read_text("D:\\notes.txt");
    ```

- **`launcher.fs.write(path: string, data: string | ArrayBuffer | Uint8Array): boolean`**（别名：`write_file`, `write_text`）
  - 写入数据至指定文件，支持传入字符串、`ArrayBuffer` 或 `Uint8Array` 等类型化数组；若父目录不存在会自动递归创建。
  - 示例：
    ```javascript
    // 写入文本
    launcher.fs.write("D:\\temp\\log.txt", "Hello World");

    // 写入二进制字节
    const binData = new Uint8Array([0x89, 0x50, 0x4E, 0x47]);
    launcher.fs.write("D:\\temp\\icon.png", binData);
    ```

- **`launcher.fs.append(path: string, data: string | ArrayBuffer | Uint8Array): boolean`**（别名：`append_text`）
  - 向文件末尾追加文本或二进制内容。

- **`launcher.fs.copy(src: string, dst: string, overwrite: boolean = true): boolean`**
  - 调用 Windows 原生 `CopyFileW` 快速复制文件，自动创建目标父目录。

- **`launcher.fs.move(src: string, dst: string, overwrite: boolean = true): boolean`**
  - 调用 Windows 原生 `MoveFileExW` 快速重命名/跨卷移动文件。

- **`launcher.fs.remove(path: string): boolean`**（别名：`delete`）
  - 删除文件或递归清空删除文件夹。

- **`launcher.fs.mkdir(path: string): boolean`**
  - 递归创建目录（等同于 `mkdir -p`）。

- **`launcher.fs.exists(path: string): boolean`**
  - 校验路径或文件是否存在。

- **`launcher.fs.list(path: string): Array<{ name, path, is_dir, is_file, size }> | null`**（别名：`read_dir`）
  - 列举目标目录下所有的直接子条目元数据。

- **`launcher.fs.stat(path: string): { is_dir: boolean, is_file: boolean, size: number, modified_ms: number } | null`**
  - 获取指定路径的属性与修改时间戳。

---

### 3. `launcher.http` 与全局 `fetch` (Windows 原生 WinHttp 网络操作)
基于 Windows 原生系统级 `WinHttp` 网络栈构建，零额外打包体积膨胀，原生继承 Windows 系统的受信任根证书库与系统代理设置，完美支持 HTTP/HTTPS。

- **`launcher.http.get(url: string, options?: RequestOptions): Response`**
- **`launcher.http.post(url: string, body?: string | ArrayBuffer | Uint8Array, options?: RequestOptions): Response`**
- **`launcher.http.request(options: FullRequestOptions): Response`**
- **`launcher.fetch(url: string, options?: RequestOptions): Response`**（全局亦挂载标准别名 `fetch`）

#### `Response` 响应对象字段：
```javascript
{
  status: 200,            // HTTP 状态码 (number)
  status_text: "OK",      // 状态文本说明 (string)
  ok: true,               // 是否成功 (status >= 200 && status < 300)
  headers: {              // 响应头映射字典 (全部小写键名)
    "content-type": "application/json",
    ...
  },
  body: "...",            // 响应文本正文 (UTF-8 string)
  buffer: ArrayBuffer,    // 原始响应二进制字节流
  json: () => any         // 便捷方法：以 JSON 格式解析 body 并返回 JS 对象
}
```

#### 示例：
```javascript
// 发起 GET 请求并解析 JSON
const res = launcher.http.get("https://api.github.com/zen");
if (res.ok) {
  console.log("GitHub Zen:", res.body);
}

// 使用 fetch 别名与 POST 数据
const apiRes = launcher.fetch("https://httpbin.org/post", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ query: input })
});
if (apiRes.ok) {
  const data = apiRes.json();
  console.log("响应数据:", data);
}
```

---

### 4. `launcher.clipboard`
提供系统剪贴板原生操作接口。
- **`launcher.clipboard.write_text(text: string): boolean`**（别名：`launcher.clipboard.write`）
  - 以 UTF-16 原生格式将文本安全写入 Windows 系统剪贴板。
  - 示例：
    ```javascript
    launcher.clipboard.write_text("42");
    ```

---

### 5. `launcher.env` (系统环境变量访问)
提供读写宿主操作系统环境变量的原生接口，支持函数调用与直接属性代理访问。

- **`launcher.env.get(name: string): string | null`**：获取环境变量字符串值，不存在时返回 `null`；
- **`launcher.env.set(name: string, value: string): void`**：设置环境变量；
- **`launcher.env.has(name: string): boolean`**：判断环境变量是否存在；
- **`launcher.env.remove(name: string): void`**：删除指定环境变量；
- **`launcher.env.all(): Record<string, string>`**：获取全部环境变量的键值对象字典；
- **动态属性代理**：亦支持直接通过 `launcher.env.<KEY>` 或 `launcher.env[KEY]` 读取或赋值环境变量。
- 示例：
  ```javascript
  const mihomoConfig = launcher.env.get("MIHOMO_CONFIG") || launcher.env.MIHOMO_CONFIG;
  const subUrl = launcher.env.SUBSCRIPTION_URL;
  ```

---

### 6. `launcher.app` (宿主程序与配置管理)
提供与 OneLauncher 核心主程序的交互接口，支持退出、重载配置以及原生弹窗提示。

- **`launcher.app.config_path: string`**：当前生效的 `config.toml` 文件在磁盘上的绝对路径；
- **`launcher.app.quit(): void`**（别名：`launcher.app.exit`、`launcher.exit`）：安全退出启动器主程序；
- **`launcher.app.reload(): void`**：通知主程序重新读取 `config.toml` 并热更新所有插件；
- **`launcher.app.edit_config(): void`**：调用系统默认文本编辑器打开当前配置文件；
- **`launcher.app.message_box(text: string, title?: string, type?: "info" | "warn" | "error"): void`**（别名：`launcher.message_box`、`launcher.alert`）：
  - 调用 Windows 原生 `MessageBoxW` 弹出置顶系统对话框。
  - 示例：
    ```javascript
    launcher.message_box("Mihomo 订阅更新成功！", "Clash 提示", "info");
    ```

---

### 7. `launcher.config`
当前插件生效的配置快照对象。
- 在插件加载时，主程序将 `metadata.default_config` 与用户 `config.toml` 中 `[plugins.<id>.config]` 对应项合并后注入该对象；
- 同时在全局作用域注入别名 `config`，支持 `launcher.config.xxx` 或 `config.xxx` 访问。
- 示例：
  ```javascript
  const precision = (launcher.config && launcher.config.precision) || 6;
  ```

---

### 8. `launcher.log` 与全局 `console`
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

---

### 9. 全局 `native` 对象
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
        badge: "提示",
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
      badge: isFolder ? "文件夹" : "文件",
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
      badge: "计算器",
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

## 八、 示例 3：Clash 控制器 (综合运用 env, fs, http, shell, process)

源码位置：[`plugins/clash/index.js`](file:///D:/src/one-launcher/plugins/clash/index.js)

```javascript
export const metadata = {
  name: "Clash 控制器",
  prefix: "clash:",
  default_config: {}
};

// 综合使用 launcher.env 读取环境变量，launcher.fs 读取/备份配置文件，
// launcher.http 发起网络请求下载订阅并通知热重载，以及 launcher.shell.exec 执行 reg 命令开启/关闭 Windows 系统代理。
```

---

## 九、 插件配置与调试

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
