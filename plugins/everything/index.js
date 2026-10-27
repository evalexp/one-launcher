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

/**
 * 插件初始化生命周期 (无参)
 */
export function on_setup() {
  fileIconRef = launcher.cache_icon("file");
  folderIconRef = launcher.cache_icon("folder");
  console.info("Everything 插件已就绪");
}

/**
 * 响应用户输入查询
 * @param {string} input 用户在 fs: 之后输入的检索词
 * @returns {Array<{id: string, title: string, subtitle: string, icon: string, payload: Object}>}
 */
export function on_query(input) {
  if (!input || input.trim() === "") {
    return [];
  }

  // 1. 设置搜索关键词并执行同步检索
  native.Everything.Everything_SetSearchW(input.trim());
  const ok = native.Everything.Everything_QueryW(true);
  if (!ok) {
    const err = native.Everything.Everything_GetLastError();
    console.warn(`Everything_QueryW 返回失败, GetLastError = ${err}`);
    if (err === 2) {
      return [{
        id: "everything_not_running",
        title: "未检测到 Everything 正在运行",
        subtitle: "Everything 检索服务依赖客户端主程序提供支持，请先启动 Everything",
        icon: "error", // 使用内置醒目红色错误图标
        badge: "提示",
        payload: { action: "start_everything" },
      }];
    }
    return [];
  }

  // 2. 获取总结果数并读取配置项 max_results
  const total = native.Everything.Everything_GetNumResults();
  if (total === 0) {
    const err = native.Everything.Everything_GetLastError();
    if (err === 2) {
      console.warn("结果数为 0 且 GetLastError = 2 (IPC 错误，主程序未运行)");
      return [{
        id: "everything_not_running",
        title: "未检测到 Everything 正在运行",
        subtitle: "Everything 检索服务依赖客户端主程序提供支持，请先启动 Everything",
        icon: "error", // 使用内置醒目红色错误图标
        badge: "提示",
        payload: { action: "start_everything" },
      }];
    }
    return [];
  }

  const cfg = self.get_config();
  const maxResults = (cfg && cfg.max_results) || 9;
  const count = Math.min(total, maxResults);
  const items = [];

  for (let i = 0; i < count; i++) {
    // 自动通过原生缓冲区读取 UTF-16 完整绝对路径
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

  console.info(`搜索 '${input.trim()}' 命中 ${total} 条结果，当前展示 ${items.length} 条`);

  return items;
}

/**
 * 用户在候选列表中选中并确认某项条目时触发
 * @param {Object} item 被选中的条目对象 { id, payload }
 */
export function on_select(item) {
  if (!item) return;

  if (item.id === "everything_not_running" || (item.payload && item.payload.action === "start_everything")) {
    console.info("用户触发了未运行提示，尝试拉起 Everything 客户端程序...");
    launcher.shell.open("everything.exe");
    return;
  }

  if (item.payload && item.payload.path) {
    launcher.shell.open(item.payload.path);
  } else if (typeof item.payload === "string" && item.payload) {
    launcher.shell.open(item.payload);
  }
}
