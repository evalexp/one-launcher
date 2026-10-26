export const metadata = {
  name: "插件管理器",
  prefix: "pm:",
  default_config: {}
};

/**
 * 提取文件名的 basename
 */
function getBasename(pathStr) {
  const norm = (pathStr || "").replace(/\\/g, "/");
  const idx = norm.lastIndexOf("/");
  return idx !== -1 ? norm.substring(idx + 1) : norm;
}

/**
 * 将相对路径解析为基于远程基准地址的完整 URL
 */
function resolveUrl(baseUrl, relPath) {
  const trimmed = (relPath || "").replace(/^\.?\//, "");
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
    return trimmed;
  }
  return baseUrl + trimmed;
}

/**
 * 获取插件存放的基础目录
 */
function getPluginsBaseDir() {
  if (typeof launcher !== "undefined" && launcher.fs && launcher.fs.exists("plugins")) {
    return "plugins";
  }
  if (typeof launcher !== "undefined" && launcher.app && launcher.app.config_path) {
    const p = launcher.app.config_path.replace(/\\/g, "/");
    const idx = p.lastIndexOf("/");
    if (idx !== -1) {
      const dir = p.substring(0, idx) + "/plugins";
      return dir;
    }
  }
  return "plugins";
}

/**
 * 获取所有已安装/已声明的插件列表
 */
function getInstalledPlugins() {
  const list = [];
  const seenIds = new Set();

  // 1. 从 config.toml 中解析声明的插件
  const configPath = typeof launcher !== "undefined" && launcher.app && launcher.app.config_path;
  if (configPath && launcher.fs.exists(configPath)) {
    const tomlContent = launcher.fs.read_text(configPath) || "";
    const pluginHeaderRegex = /\[plugins\.([a-zA-Z0-9_\-]+)\]/g;
    let match;
    while ((match = pluginHeaderRegex.exec(tomlContent)) !== null) {
      const id = match[1];
      if (seenIds.has(id)) continue;
      seenIds.add(id);

      const startIdx = match.index;
      const nextMatch = tomlContent.slice(startIdx + match[0].length).search(/\n\[plugins\./);
      const sectionText = nextMatch !== -1
        ? tomlContent.slice(startIdx, startIdx + match[0].length + nextMatch)
        : tomlContent.slice(startIdx);

      const nameM = sectionText.match(/name\s*=\s*['"](.*?)['"]/);
      const prefixM = sectionText.match(/prefix\s*=\s*['"](.*?)['"]/);
      const pathM = sectionText.match(/path\s*=\s*['"](.*?)['"]/);

      list.push({
        id,
        name: nameM ? nameM[1] : id,
        prefix: prefixM ? prefixM[1] : "",
        path: pathM ? pathM[1] : ("plugins/" + id)
      });
    }
  }

  // 2. 从本地 plugins 目录扫描已存在且包含 index.js 的文件夹
  const pluginsDir = getPluginsBaseDir();
  if (typeof launcher !== "undefined" && launcher.fs && launcher.fs.exists(pluginsDir)) {
    const dirEntries = launcher.fs.list(pluginsDir) || [];
    for (const entry of dirEntries) {
      if (entry.is_dir && !entry.name.startsWith(".")) {
        const id = entry.name;
        if (!seenIds.has(id)) {
          const indexFile = (entry.path.replace(/\\/g, "/") + "/index.js");
          let name = id;
          let prefix = "";
          if (launcher.fs.exists(indexFile)) {
            const code = launcher.fs.read_text(indexFile) || "";
            const nm = code.match(/name:\s*['"](.*?)['"]/);
            const pr = code.match(/prefix:\s*['"](.*?)['"]/);
            if (nm) name = nm[1];
            if (pr) prefix = pr[1];
          }
          list.push({
            id,
            name,
            prefix,
            path: entry.path
          });
          seenIds.add(id);
        }
      }
    }
  }

  return list;
}

/**
 * 解析 GitHub 链接，提取目标仓库、分支、路径及推导的插件 ID
 */
export function parseGithubUrl(raw) {
  let cleaned = (raw || "").trim();
  cleaned = cleaned.replace(/^https?:\/\//i, "");
  cleaned = cleaned.replace(/^\/+|\/+$/g, "");

  // 1. 直连 raw.githubusercontent.com
  if (cleaned.toLowerCase().startsWith("raw.githubusercontent.com/")) {
    const rest = cleaned.substring("raw.githubusercontent.com/".length);
    const parts = rest.split("/").filter(Boolean);
    if (parts.len < 3) return null;
    const owner = parts[0];
    const repo = parts[1];
    const branch = parts[2];
    const pathParts = parts.slice(3);
    const fullPath = pathParts.join("/");
    const idx = fullPath.lastIndexOf("/");
    const baseDir = idx !== -1 ? fullPath.substring(0, idx) : "";
    const pluginId = baseDir ? baseDir.split("/").pop() : repo;

    const rawUrl = `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${fullPath}`;
    const baseUrl = `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${baseDir ? baseDir + "/" : ""}`;

    return {
      owner,
      repo,
      branch,
      pluginId: pluginId.toLowerCase().replace(/[^a-z0-9_\-]/g, "_"),
      rawUrl,
      baseUrl
    };
  }

  // 2. 标准 github.com 格式 (例如 github.com/owner/repo/index.js 或 github.com/owner/repo/plugins/calc/index.js)
  cleaned = cleaned.replace(/^github\.com\//i, "");
  const parts = cleaned.split("/").filter(Boolean);
  if (parts.length < 2) return null;

  const owner = parts[0];
  const repo = parts[1].replace(/\.git$/i, "");

  let branch = "main";
  let pathParts = parts.slice(2);

  if (pathParts.length >= 2 && pathParts[0] === "blob") {
    branch = pathParts[1];
    pathParts = pathParts.slice(2);
  }

  // 若未指定 .js，默认指向 index.js
  if (pathParts.length === 0 || !pathParts[pathParts.length - 1].endsWith(".js")) {
    pathParts.push("index.js");
  }

  const fullPath = pathParts.join("/");
  const idx = fullPath.lastIndexOf("/");
  const baseDir = idx !== -1 ? fullPath.substring(0, idx) : "";
  const pluginId = baseDir ? baseDir.split("/").pop() : repo;

  const rawUrl = `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${fullPath}`;
  const baseUrl = `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${baseDir ? baseDir + "/" : ""}`;

  return {
    owner,
    repo,
    branch,
    fullPath,
    pluginId: pluginId.toLowerCase().replace(/[^a-z0-9_\-]/g, "_"),
    rawUrl,
    baseUrl
  };
}

/**
 * 下载远程 index.js（带 main / master 分支回退）
 */
function downloadIndexJs(parsed) {
  console.info(`正在请求下载: ${parsed.rawUrl}`);
  let res = launcher.http.get(parsed.rawUrl, { timeout_ms: 15000 });
  if (res.ok && res.body) {
    return { code: res.body, baseUrl: parsed.baseUrl };
  }

  // 回退尝试 master 分支
  if (parsed.branch === "main" && parsed.fullPath) {
    const masterUrl = `https://raw.githubusercontent.com/${parsed.owner}/${parsed.repo}/master/${parsed.fullPath}`;
    console.info(`main 分支未命中，尝试 master 分支: ${masterUrl}`);
    res = launcher.http.get(masterUrl, { timeout_ms: 15000 });
    if (res.ok && res.body) {
      const idx = parsed.fullPath.lastIndexOf("/");
      const baseDir = idx !== -1 ? parsed.fullPath.substring(0, idx) : "";
      const masterBaseUrl = `https://raw.githubusercontent.com/${parsed.owner}/${parsed.repo}/master/${baseDir ? baseDir + "/" : ""}`;
      return { code: res.body, baseUrl: masterBaseUrl };
    }
  }

  return null;
}

/**
 * 在受控环境安全解析 ES Module 代码，提取导出的 metadata 与 on_setup
 */
function evaluatePluginModule(code) {
  const transformed = code
    .replace(/^\s*export\s+const\s+/gm, "const ")
    .replace(/^\s*export\s+let\s+/gm, "let ")
    .replace(/^\s*export\s+var\s+/gm, "var ")
    .replace(/^\s*export\s+function\s+/gm, "function ")
    .replace(/^\s*export\s+class\s+/gm, "class ")
    .replace(/^\s*export\s+default\s+/gm, "const __default__ = ");

  const runner = new Function(`
    "use strict";
    ${transformed}
    return {
      metadata: typeof metadata !== "undefined" ? metadata : null,
      on_setup: typeof on_setup === "function" ? on_setup : null
    };
  `);
  return runner();
}

/**
 * 在 config.toml 中写入或更新插件配置段
 */
function addPluginToConfig(id, name, prefix, relPath) {
  const configPath = launcher.app && launcher.app.config_path;
  if (!configPath || !launcher.fs.exists(configPath)) {
    console.error("无法定位配置文件: " + configPath);
    return;
  }

  let content = launcher.fs.read_text(configPath) || "";
  const sectionHeader = `[plugins.${id}]`;
  const newSection = `\n[plugins.${id}]\nenabled = true\nprefix = "${prefix}"\nname = "${name}"\npath = "${relPath}"\n`;

  const regex = new RegExp(`\\[plugins\\.${id}\\][\\s\\S]*?(?=(\\n\\[|$))`);
  if (regex.test(content)) {
    content = content.replace(regex, `[plugins.${id}]\nenabled = true\nprefix = "${prefix}"\nname = "${name}"\npath = "${relPath}"\n`);
  } else {
    if (!content.endsWith("\n")) {
      content += "\n";
    }
    content += newSection;
  }

  launcher.fs.write(configPath, content);
  console.info(`已更新配置文件中的 ${sectionHeader} 配置段`);
}

/**
 * 从 config.toml 中移除插件配置段
 */
function removePluginFromConfig(id) {
  const configPath = launcher.app && launcher.app.config_path;
  if (!configPath || !launcher.fs.exists(configPath)) {
    return;
  }

  let content = launcher.fs.read_text(configPath) || "";
  const blockRegex = new RegExp(`\\n*\\[plugins\\.${id}(\\.[^\\]]+)?\\][\\s\\S]*?(?=(\\n\\[plugins\\.[a-zA-Z0-9_\\]]|\\n\\[[a-zA-Z0-9_\\]]|$))`, "g");
  content = content.replace(blockRegex, "");
  content = content.replace(/\n{3,}/g, "\n\n");
  launcher.fs.write(configPath, content);
  console.info(`已从配置文件中移除 [plugins.${id}] 声明`);
}

/**
 * 安装单文件驱动插件 (下载 index.js -> 解析 metadata -> 下载 dynamic library -> 执行 on_setup(context) 下载资源)
 */
function executeInstall(parsed) {
  const pluginsBase = getPluginsBaseDir();
  const targetDir = (pluginsBase + "/" + parsed.pluginId).replace(/\\/g, "/");

  console.info(`准备安装插件 [${parsed.pluginId}] 至: ${targetDir}`);
  launcher.fs.mkdir(targetDir);

  // 1. 下载 index.js
  const downloadRes = downloadIndexJs(parsed);
  if (!downloadRes || !downloadRes.code) {
    const errMsg = `下载插件脚本失败: ${parsed.rawUrl}`;
    console.error(errMsg);
    if (launcher.message_box) {
      launcher.message_box(errMsg, "安装失败", "error");
    }
    return false;
  }

  const { code: indexCode, baseUrl } = downloadRes;

  // 2. 解析插件模块
  let mod;
  try {
    mod = evaluatePluginModule(indexCode);
  } catch (err) {
    const errMsg = "解析插件脚本模块失败: " + err;
    console.error(errMsg);
    if (launcher.message_box) {
      launcher.message_box(errMsg, "语法解析错误", "error");
    }
    return false;
  }

  const meta = mod.metadata || {};
  const pluginName = meta.name || parsed.pluginId;
  const pluginPrefix = meta.prefix || (parsed.pluginId + ":");

  // 3. 检查 metadata.dynamic_library，若有则下载对应的 DLL 文件
  if (meta.dynamic_library) {
    for (const libKey in meta.dynamic_library) {
      const entry = meta.dynamic_library[libKey];
      if (entry && entry.path) {
        const filename = getBasename(entry.path);
        const dllUrl = resolveUrl(baseUrl, entry.path);
        console.info(`正在下载目标插件依赖的原生动态库 [${libKey}]: ${dllUrl}`);
        const dllRes = launcher.http.get(dllUrl, { timeout_ms: 30000 });
        if (dllRes.ok && dllRes.buffer) {
          launcher.fs.write(targetDir + "/" + filename, dllRes.buffer);
          console.info(`动态库已落盘: ${targetDir}/${filename}`);
        } else {
          console.warn(`下载动态库失败: HTTP ${dllRes.status} ${dllRes.status_text}`);
        }
      }
    }
  }

  // 4. 准备隔离安装 context，指向资源下载接口，并调用目标插件的 on_setup(context)
  if (typeof mod.on_setup === "function") {
    console.info("在安装环境中准备 context 并执行目标插件 on_setup(context)...");
    const installContext = {
      cache_icon: function(iconPath) {
        if (typeof iconPath === "string") {
          const trimmed = iconPath.trim();
          const filename = getBasename(trimmed);
          const iconUrl = resolveUrl(baseUrl, trimmed);
          console.info("on_setup 请求下载图标资源: " + iconUrl);
          const iconRes = launcher.http.get(iconUrl, { timeout_ms: 15000 });
          if (iconRes.ok && iconRes.buffer) {
            launcher.fs.write(targetDir + "/" + filename, iconRes.buffer);
            console.info(`图标资源已落盘: ${targetDir}/${filename}`);
            return filename;
          }
          return iconPath;
        }
        return "file";
      }
    };

    try {
      mod.on_setup(installContext);
    } catch (err) {
      console.warn("调用 on_setup 产生警告: " + err);
    }
  }

  // 5. 保存 index.js 脚本至插件目录
  launcher.fs.write(targetDir + "/index.js", indexCode);
  console.info(`插件入口脚本已写入: ${targetDir}/index.js`);

  // 6. 更新 config.toml
  addPluginToConfig(parsed.pluginId, pluginName, pluginPrefix, `plugins/${parsed.pluginId}`);

  // 7. 触发主程序热重载
  if (launcher.app && launcher.app.reload) {
    launcher.app.reload();
  }

  // 8. 弹窗提醒安装成功
  if (launcher.message_box) {
    launcher.message_box(
      `插件 [${pluginName}] (${parsed.pluginId})\n前缀: ${pluginPrefix}\n已成功安装并热重载生效！`,
      "插件安装成功",
      "info"
    );
  }

  console.info(`插件 [${pluginName}] (${parsed.pluginId}) 安装完成！`);
  return true;
}

/**
 * 卸载插件
 */
function executeUninstall(pluginId, pluginPath) {
  console.info(`准备卸载插件: [${pluginId}] 路径: ${pluginPath}`);

  // 1. 从 config.toml 中移除配置
  removePluginFromConfig(pluginId);

  // 2. 删除磁盘对应文件夹
  const pluginsBase = getPluginsBaseDir();
  const candidate1 = (pluginsBase + "/" + pluginId).replace(/\\/g, "/");
  const candidate2 = pluginPath ? pluginPath.replace(/\\/g, "/") : "";

  if (launcher.fs.exists(candidate1)) {
    launcher.fs.remove(candidate1);
  }
  if (candidate2 && candidate2 !== candidate1 && launcher.fs.exists(candidate2)) {
    launcher.fs.remove(candidate2);
  }

  // 3. 触发配置热重载
  if (launcher.app && launcher.app.reload) {
    launcher.app.reload();
  }

  // 4. 弹窗提醒卸载成功
  if (launcher.message_box) {
    launcher.message_box(`插件 [${pluginId}] 已成功彻底卸载！`, "插件管理", "info");
  }

  console.info(`插件 [${pluginId}] 已成功卸载！`);
  return true;
}

/**
 * 查询输入处理 (响应 pm: install 与 pm: uninstall)
 */
export function on_query(input) {
  const raw = (input || "").trim();
  const lower = raw.toLowerCase();

  // 1. 空输入：展示顶层指令选择
  if (!raw) {
    return [
      {
        id: "guide-install",
        title: "安装插件 (Install Plugin)",
        subtitle: "输入: install <github.com/xxx/yyy/index.js>",
        icon: "info",
        badge: "管理",
        payload: { action: "guide" }
      },
      {
        id: "guide-uninstall",
        title: "卸载插件 (Uninstall Plugin)",
        subtitle: "输入: uninstall 浏览已安装列表或指定插件名前缀",
        icon: "error",
        badge: "管理",
        payload: { action: "guide" }
      }
    ];
  }

  // 2. 用户正在输入 uninstall 指令
  if (lower.startsWith("uninstall") || lower.startsWith("un")) {
    const afterUninstall = lower.startsWith("uninstall")
      ? raw.substring(9).trim()
      : raw.substring(2).trim();

    const plugins = getInstalledPlugins();
    const query = afterUninstall.toLowerCase();

    // 当输入 uninstall 或 uninstall 空格时，列出全部可卸载插件
    // 当输入 uninstall xxx 时，按插件 id 或名字前缀匹配
    const filtered = query
      ? plugins.filter(p => p.id.toLowerCase().startsWith(query) || p.name.toLowerCase().startsWith(query))
      : plugins;

    if (filtered.length === 0) {
      return [
        {
          id: "no-plugins-found",
          title: "未找到匹配的可卸载插件",
          subtitle: query ? `没有以前缀 "${query}" 开头的插件` : "当前没有已安装或声明的插件",
          icon: "warn",
          badge: "提示",
          payload: { action: "none" }
        }
      ];
    }

    return filtered.map(p => ({
      id: `uninstall-${p.id}`,
      title: `卸载插件: ${p.name} (${p.id})`,
      subtitle: `前缀: ${p.prefix || "无"} | 路径: ${p.path} (回车彻底卸载)`,
      icon: "error",
      badge: "卸载",
      payload: { action: "uninstall", pluginId: p.id, pluginPath: p.path }
    }));
  }

  // 3. 用户正在输入 install 指令
  if (lower.startsWith("install") || lower.startsWith("in")) {
    const target = lower.startsWith("install")
      ? raw.substring(7).trim()
      : raw.substring(2).trim();

    if (!target) {
      return [
        {
          id: "prompt-install-url",
          title: "请输入 GitHub 插件 index.js 地址",
          subtitle: "例如: install github.com/owner/repo/index.js",
          icon: "info",
          badge: "安装",
          payload: { action: "guide" }
        }
      ];
    }

    const parsed = parseGithubUrl(target);
    if (!parsed) {
      return [
        {
          id: "invalid-url",
          title: "无效的 GitHub 地址格式",
          subtitle: `已输入: "${target}"，示例: github.com/owner/repo/index.js`,
          icon: "warn",
          badge: "错误",
          payload: { action: "none" }
        }
      ];
    }

    return [
      {
        id: `install-${parsed.pluginId}`,
        title: `下载并安装插件: ${parsed.pluginId}`,
        subtitle: `来源: ${parsed.rawUrl} (回车解析 metadata、依赖并安装)`,
        icon: "info",
        badge: "安装",
        payload: { action: "install", parsed }
      }
    ];
  }

  // 4. 输入前缀匹配顶层选项 (如仅输入 "i" 或 "u")
  const options = [
    {
      id: "guide-install",
      title: "安装插件 (Install Plugin)",
      subtitle: "输入: install <github.com/xxx/yyy/index.js>",
      icon: "info",
      badge: "管理",
      payload: { action: "guide" }
    },
    {
      id: "guide-uninstall",
      title: "卸载插件 (Uninstall Plugin)",
      subtitle: "输入: uninstall 浏览已安装列表或指定插件名前缀",
      icon: "error",
      badge: "管理",
      payload: { action: "guide" }
    }
  ];

  return options.filter(opt => opt.id.toLowerCase().includes(lower));
}

/**
 * 回车选择动作处理
 */
export function on_select(item) {
  const payload = item && item.payload;
  if (!payload) return;

  if (payload.action === "install" && payload.parsed) {
    executeInstall(payload.parsed);
  } else if (payload.action === "uninstall" && payload.pluginId) {
    executeUninstall(payload.pluginId, payload.pluginPath);
  }
}
