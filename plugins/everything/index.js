// ==============================================================================
// OneLauncher 官方插件: Everything 全盘极速文件搜索
// 说明: 采用声明式 Native 动态链接绑定，Rust 宿主在加载期自动解析并校验符号，
//      将原生 C 函数映射为 native.Everything.* 供 JS 直接微秒级调用。
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
      ]
    }
  }
};

/**
 * 响应用户输入查询
 * @param {string} input 用户在 fs: 之后输入的检索词
 * @returns {Array} 候选条目列表
 */
export function on_query(input) {
  if (!input || input.trim() === "") {
    return [];
  }

  // 1. 设置搜索关键词并执行同步检索
  native.Everything.Everything_SetSearchW(input.trim());
  const ok = native.Everything.Everything_QueryW(true);
  if (!ok) {
    return [];
  }

  // 2. 获取总结果数并读取配置项 max_results
  const total = native.Everything.Everything_GetNumResults();
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
      is_folder: isFolder,
      payload: fullPath,
    });
  }

  return items;
}

/**
 * 用户在候选列表中选中并确认某项条目时触发
 * @param {Object} item 被选中的条目对象
 */
export function on_select(item) {
  if (item && item.payload) {
    launcher.shell.open(item.payload);
  }
}
