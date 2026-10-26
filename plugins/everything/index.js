// ==============================================================================
// OneLauncher 官方插件: Everything 全盘极速文件搜索
// 说明: 采用声明式 Native 动态链接绑定与 on_setup 规范生命周期
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

let fileIconRef = "file";
let folderIconRef = "folder";

/**
 * 插件初始化生命周期
 * @param {Object} context 宿主初始化对象 (提供 context.cache_icon 用于一次性完成文件/图片IO缓存)
 * @param {Function} context.cache_icon (bytes|path) => ref_id 将图标缓存至程序内部并返回引用 ID
 */
export function on_setup(context) {
  // 全局已注入 launcher.config 与 launcher.log / console，context 专用于高效预存图标等重型IO
  fileIconRef = "file";
  folderIconRef = "folder";

  console.info("Everything 插件初始化就绪 (on_setup)");
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
        payload: { action: "start_everything" },
      }];
    }
    return [];
  }

  const maxResults = (launcher.config && launcher.config.max_results) || 9;
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
