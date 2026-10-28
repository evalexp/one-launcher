export const metadata = {
  name: "插件管理器",
  prefix: "pm:",
  default_config: {}
};

/**
 * 提取文件名的 basename
 */
function getBasename(pathStr) {
  const norm = (pathStr || "").replace(/\\/g, "/").replace(/[?#].*$/, "");
  const idx = norm.lastIndexOf("/");
  return idx !== -1 ? norm.substring(idx + 1) : norm;
}

/**
 * 将相对路径解析为基于远程基准地址的完整 URL
 */
function resolveUrl(baseUrl, relPath) {
  const trimmed = (relPath || "").trim().replace(/^\.?\//, "");
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
    return trimmed;
  }
  const base = (baseUrl || "").endsWith("/") ? baseUrl : baseUrl + "/";
  return base + trimmed;
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
  const configPath = typeof launcher !== "undefined" && launcher.app && launcher.app.config_path;

  // 1. 从 config.toml 中解析声明的插件配置段（跳过注释行）
  if (configPath && launcher.fs && launcher.fs.exists(configPath)) {
    const tomlContent = launcher.fs.read_text(configPath) || "";
    const lines = tomlContent.split("\n");
    let currentId = null;
    let currentSectionLines = [];

    const processCurrentSection = () => {
      if (!currentId || seenIds.has(currentId)) return;
      seenIds.add(currentId);
      const text = currentSectionLines.join("\n");
      const nameM = text.match(/^\s*name\s*=\s*['"](.*?)['"]/m);
      const prefixM = text.match(/^\s*prefix\s*=\s*['"](.*?)['"]/m);
      const pathM = text.match(/^\s*path\s*=\s*['"](.*?)['"]/m);
      const enabledM = text.match(/^\s*enabled\s*=\s*(true|false)/m);

      const relPath = pathM ? pathM[1] : ("plugins/" + currentId);
      const isEnabled = enabledM ? (enabledM[1] === "true") : true;

      // 验证在磁盘上是否存在
      const baseDir = getPluginsBaseDir();
      const checkPath1 = relPath;
      const checkPath2 = baseDir + "/" + currentId;
      const diskExists = (launcher.fs.exists(checkPath1) || launcher.fs.exists(checkPath2) || launcher.fs.exists(checkPath1 + "/index.js") || launcher.fs.exists(checkPath2 + "/index.js"));

      list.push({
        id: currentId,
        name: nameM ? nameM[1] : currentId,
        prefix: prefixM ? prefixM[1] : "",
        path: relPath,
        enabled: isEnabled,
        diskExists
      });
    };

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const trimmed = line.trim();
      // 严格跳过任何注释行，避免把注释模板当做实际声明
      if (trimmed.startsWith("#")) continue;

      const headerMatch = trimmed.match(/^\[plugins\.([a-zA-Z0-9_\-]+)\]$/);
      if (headerMatch) {
        processCurrentSection();
        currentId = headerMatch[1];
        currentSectionLines = [];
      } else if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
        processCurrentSection();
        currentId = null;
        currentSectionLines = [];
      } else if (currentId) {
        currentSectionLines.push(line);
      }
    }
    processCurrentSection();
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
          let hasIndex = false;
          if (launcher.fs.exists(indexFile)) {
            hasIndex = true;
            const code = launcher.fs.read_text(indexFile) || "";
            const nm = code.match(/name:\s*['"](.*?)['"]/);
            const pr = code.match(/prefix:\s*['"](.*?)['"]/);
            if (nm) name = nm[1];
            if (pr) prefix = pr[1];
          }
          if (hasIndex) {
            list.push({
              id,
              name,
              prefix,
              path: entry.path,
              enabled: false,
              diskExists: true
            });
            seenIds.add(id);
          }
        }
      }
    }
  }

  return list;
}

/**
 * 通用插件 URL 解析器 (URL Resolver)
 * 自动识别并映射主流代码托管源（GitHub, Gitee, GitLab）至 Raw 内容直链，
 * 并支持任意通用的 HTTP/HTTPS 直链。
 */
export function resolvePluginUrl(raw) {
  let cleaned = (raw || "").trim();
  if (!cleaned) return null;

  // 1. 直连 raw.githubusercontent.com
  const rawGhPrefix = /^https?:\/\/raw\.githubusercontent\.com\//i;
  if (rawGhPrefix.test(cleaned) || cleaned.toLowerCase().startsWith("raw.githubusercontent.com/")) {
    const withoutPrefix = cleaned.replace(/^https?:\/\//i, "").substring("raw.githubusercontent.com/".length);
    const parts = withoutPrefix.split("/").filter(Boolean);
    if (parts.length < 3) return null;
    const owner = parts[0];
    const repo = parts[1];
    const branch = parts[2];
    const pathParts = parts.slice(3);
    if (pathParts.length === 0 || !pathParts[pathParts.length - 1].endsWith(".js")) {
      pathParts.push("index.js");
    }
    const fullPath = pathParts.join("/");
    const idx = fullPath.lastIndexOf("/");
    const baseDir = idx !== -1 ? fullPath.substring(0, idx) : "";
    const pluginId = baseDir ? baseDir.split("/").pop() : repo;

    return {
      source: "github",
      owner,
      repo,
      branch,
      fullPath,
      pluginId: pluginId.toLowerCase().replace(/[^a-z0-9_\-]/g, "_"),
      rawUrl: `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${fullPath}`,
      baseUrl: `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${baseDir ? baseDir + "/" : ""}`
    };
  }

  // 2. 标准 github.com 格式 (例如 github.com/owner/repo/index.js 或 github.com/owner/repo/blob/main/calc/index.js)
  const ghPrefix = /^https?:\/\/github\.com\//i;
  if (ghPrefix.test(cleaned) || cleaned.toLowerCase().startsWith("github.com/")) {
    const withoutPrefix = cleaned.replace(/^https?:\/\//i, "").substring("github.com/".length);
    const parts = withoutPrefix.split("/").filter(Boolean);
    if (parts.length < 2) return null;

    const owner = parts[0];
    const repo = parts[1].replace(/\.git$/i, "");
    let branch = "main";
    let pathParts = parts.slice(2);

    if (pathParts.length >= 2 && (pathParts[0] === "blob" || pathParts[0] === "raw")) {
      branch = pathParts[1];
      pathParts = pathParts.slice(2);
    }

    if (pathParts.length === 0 || !pathParts[pathParts.length - 1].endsWith(".js")) {
      pathParts.push("index.js");
    }

    const fullPath = pathParts.join("/");
    const idx = fullPath.lastIndexOf("/");
    const baseDir = idx !== -1 ? fullPath.substring(0, idx) : "";
    const pluginId = baseDir ? baseDir.split("/").pop() : repo;

    return {
      source: "github",
      owner,
      repo,
      branch,
      fullPath,
      pluginId: pluginId.toLowerCase().replace(/[^a-z0-9_\-]/g, "_"),
      rawUrl: `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${fullPath}`,
      baseUrl: `https://raw.githubusercontent.com/${owner}/${repo}/${branch}/${baseDir ? baseDir + "/" : ""}`
    };
  }

  // 3. Gitee 格式
  const giteePrefix = /^https?:\/\/gitee\.com\//i;
  if (giteePrefix.test(cleaned) || cleaned.toLowerCase().startsWith("gitee.com/")) {
    const withoutPrefix = cleaned.replace(/^https?:\/\//i, "").substring("gitee.com/".length);
    const parts = withoutPrefix.split("/").filter(Boolean);
    if (parts.length >= 2) {
      const owner = parts[0];
      const repo = parts[1].replace(/\.git$/i, "");
      let branch = "master";
      let pathParts = parts.slice(2);
      if (pathParts.length >= 2 && (pathParts[0] === "raw" || pathParts[0] === "blob")) {
        branch = pathParts[1];
        pathParts = pathParts.slice(2);
      }
      if (pathParts.length === 0 || !pathParts[pathParts.length - 1].endsWith(".js")) {
        pathParts.push("index.js");
      }
      const fullPath = pathParts.join("/");
      const idx = fullPath.lastIndexOf("/");
      const baseDir = idx !== -1 ? fullPath.substring(0, idx) : "";
      const pluginId = baseDir ? baseDir.split("/").pop() : repo;

      return {
        source: "gitee",
        owner,
        repo,
        branch,
        fullPath,
        pluginId: pluginId.toLowerCase().replace(/[^a-z0-9_\-]/g, "_"),
        rawUrl: `https://gitee.com/${owner}/${repo}/raw/${branch}/${fullPath}`,
        baseUrl: `https://gitee.com/${owner}/${repo}/raw/${branch}/${baseDir ? baseDir + "/" : ""}`
      };
    }
  }

  // 4. GitLab 格式
  const gitlabPrefix = /^https?:\/\/gitlab\.com\//i;
  if (gitlabPrefix.test(cleaned) || cleaned.toLowerCase().startsWith("gitlab.com/")) {
    const withoutPrefix = cleaned.replace(/^https?:\/\//i, "").substring("gitlab.com/".length);
    const parts = withoutPrefix.split("/").filter(Boolean);
    if (parts.length >= 2) {
      const owner = parts[0];
      const repo = parts[1].replace(/\.git$/i, "");
      let branch = "main";
      let pathParts = parts.slice(2);
      if (pathParts[0] === "-") pathParts.shift();
      if (pathParts.length >= 2 && pathParts[0] === "raw") {
        branch = pathParts[1];
        pathParts = pathParts.slice(2);
      }
      if (pathParts.length === 0 || !pathParts[pathParts.length - 1].endsWith(".js")) {
        pathParts.push("index.js");
      }
      const fullPath = pathParts.join("/");
      const idx = fullPath.lastIndexOf("/");
      const baseDir = idx !== -1 ? fullPath.substring(0, idx) : "";
      const pluginId = baseDir ? baseDir.split("/").pop() : repo;

      return {
        source: "gitlab",
        owner,
        repo,
        branch,
        fullPath,
        pluginId: pluginId.toLowerCase().replace(/[^a-z0-9_\-]/g, "_"),
        rawUrl: `https://gitlab.com/${owner}/${repo}/-/raw/${branch}/${fullPath}`,
        baseUrl: `https://gitlab.com/${owner}/${repo}/-/raw/${branch}/${baseDir ? baseDir + "/" : ""}`
      };
    }
  }

  // 5. 通用 HTTP / HTTPS 直接下载链接
  if (cleaned.startsWith("http://") || cleaned.startsWith("https://")) {
    const lastSlash = cleaned.lastIndexOf("/");
    const baseUrl = lastSlash !== -1 ? cleaned.substring(0, lastSlash + 1) : cleaned;
    const pathWithoutSlash = cleaned.substring(0, lastSlash);
    const subIdx = pathWithoutSlash.lastIndexOf("/");
    const candidateName = subIdx !== -1 ? pathWithoutSlash.substring(subIdx + 1) : "plugin";
    const pluginId = candidateName.toLowerCase().replace(/[^a-z0-9_\-]/g, "_") || "plugin";

    return {
      source: "direct",
      pluginId,
      rawUrl: cleaned,
      baseUrl
    };
  }

  return null;
}

export function parseGithubUrl(raw) {
  return resolvePluginUrl(raw);
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

  if (parsed.source === "github" && parsed.branch === "main" && parsed.fullPath) {
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
 * 从 metadata (dynamic_library / native / icon / assets) 及 setup 拦截中收集所有需要下载的依赖资源
 */
function collectResourcePaths(meta, interceptedResources) {
  const fileSet = new Set();

  if (meta) {
    // 1. dynamic_library 声明的原生动态库
    if (meta.dynamic_library && typeof meta.dynamic_library === "object") {
      for (const key in meta.dynamic_library) {
        const item = meta.dynamic_library[key];
        if (typeof item === "string") {
          fileSet.add(item);
        } else if (item && typeof item === "object") {
          if (item.path) fileSet.add(item.path);
          if (item.lib_path) fileSet.add(item.lib_path);
        }
      }
      if (meta.dynamic_library.lib_path) {
        fileSet.add(meta.dynamic_library.lib_path);
      }
    }

    // 2. native 声明的原生资源
    if (meta.native) {
      if (typeof meta.native === "string") {
        fileSet.add(meta.native);
      } else if (Array.isArray(meta.native)) {
        for (const p of meta.native) {
          if (typeof p === "string") fileSet.add(p);
          else if (p && p.path) fileSet.add(p.path);
        }
      } else if (typeof meta.native === "object") {
        for (const k in meta.native) {
          const item = meta.native[k];
          if (typeof item === "string") fileSet.add(item);
          else if (item && item.path) fileSet.add(item.path);
        }
      }
    }

    // 3. icon 图标资产
    if (typeof meta.icon === "string" && !isPresetIcon(meta.icon)) {
      fileSet.add(meta.icon);
    }

    // 4. assets / resources 静态资源清单
    const staticAssets = meta.assets || meta.resources || meta.files;
    if (Array.isArray(staticAssets)) {
      for (const a of staticAssets) {
        if (typeof a === "string") fileSet.add(a);
      }
    }
  }

  // 5. on_setup 中动态拦截的图标/资产请求
  if (Array.isArray(interceptedResources)) {
    for (const r of interceptedResources) {
      if (typeof r === "string" && !isPresetIcon(r)) {
        fileSet.add(r);
      }
    }
  }

  return Array.from(fileSet);
}

function isPresetIcon(name) {
  if (!name || typeof name !== "string") return false;
  const lower = name.toLowerCase().trim();
  const presets = [
    "folder", "dir", "file", "doc", "error", "success", "warn",
    "warning", "info", "calc", "terminal", "cmd", "settings",
    "gear", "notepad", "shield", "key"
  ];
  return presets.includes(lower);
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
 * 设置插件的启用/禁用状态 (enabled = true / false)
 */
function setPluginEnabled(id, enabled) {
  const configPath = launcher.app && launcher.app.config_path;
  if (!configPath || !launcher.fs.exists(configPath)) {
    console.error("无法定位配置文件: " + configPath);
    return false;
  }

  let content = launcher.fs.read_text(configPath) || "";
  const lines = content.split("\n");
  const newLines = [];
  let inTargetSection = false;
  let sectionFound = false;
  let enabledLineModified = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    if (!trimmed.startsWith("#") && trimmed === `[plugins.${id}]`) {
      inTargetSection = true;
      sectionFound = true;
      newLines.push(line);
      continue;
    }

    if (inTargetSection) {
      if (!trimmed.startsWith("#") && trimmed.startsWith("[") && trimmed.endsWith("]")) {
        if (!enabledLineModified) {
          newLines.push(`enabled = ${enabled}`);
          enabledLineModified = true;
        }
        inTargetSection = false;
      } else if (trimmed.startsWith("enabled")) {
        newLines.push(`enabled = ${enabled}`);
        enabledLineModified = true;
        continue;
      }
    }
    newLines.push(line);
  }

  if (inTargetSection && !enabledLineModified) {
    newLines.push(`enabled = ${enabled}`);
    enabledLineModified = true;
  }

  if (!sectionFound) {
    if (newLines.length > 0 && newLines[newLines.length - 1].trim() !== "") {
      newLines.push("");
    }
    newLines.push(`[plugins.${id}]`);
    newLines.push(`enabled = ${enabled}`);
  }

  const newContent = newLines.join("\n").replace(/\n{3,}/g, "\n\n");
  launcher.fs.write(configPath, newContent);
  console.info(`已更新插件 [${id}] enabled = ${enabled}`);

  if (launcher.app && launcher.app.reload) {
    launcher.app.reload();
  }
  return true;
}

/**
 * 从 config.toml 中移除指定插件段
 */
function removePluginFromConfig(id) {
  const configPath = launcher.app && launcher.app.config_path;
  if (!configPath || !launcher.fs.exists(configPath)) {
    return;
  }

  let content = launcher.fs.read_text(configPath) || "";
  const lines = content.split("\n");
  const newLines = [];
  let inTargetSection = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    // 只有非注释行完全匹配 [plugins.<id>] 或 [plugins.<id>.xxx] 才进入目标段
    if (!trimmed.startsWith("#") && (trimmed === `[plugins.${id}]` || trimmed.startsWith(`[plugins.${id}.`))) {
      inTargetSection = true;
      continue;
    }

    if (inTargetSection) {
      if (!trimmed.startsWith("#") && trimmed.startsWith("[") && trimmed.endsWith("]")) {
        // 进入下一个非注释配置节，退出跳过状态
        inTargetSection = false;
      } else {
        continue; // 跳过目标配置节的内容
      }
    }

    newLines.push(line);
  }

  const newContent = newLines.join("\n").replace(/\n{3,}/g, "\n\n");
  launcher.fs.write(configPath, newContent);
  console.info(`已从配置文件中彻底移除 [plugins.${id}] 声明`);
}

/**
 * 安装单文件驱动插件
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

  // 2. 利用完全隔离的 QuickJS 虚拟机上下文安全评估 ES 模块
  let meta = null;
  const interceptedIcons = [];

  try {
    if (typeof launcher.create_context === "function") {
      const vm = launcher.create_context();

      // 在隔离虚拟机中严格只预设 launcher 与 self
      vm.eval(`
        globalThis.__captured_icons = [];
        globalThis.launcher = {
          cache_icon: function(p) {
            globalThis.__captured_icons.push(p);
            return p;
          }
        };
        globalThis.self = {
          get_config: function() { return {}; },
          cache_icon: function(p) {
            globalThis.__captured_icons.push(p);
            return p;
          }
        };
      `);

      // 评估目标 ES 模块
      const modRes = vm.eval_module(indexCode);
      meta = modRes.metadata || {};

      // 严格无参调用 on_setup()
      vm.eval(`
        if (typeof on_setup === "function") {
          try {
            on_setup();
          } catch (e) {
            // 忽略非致命设置警告
          }
        }
      `);

      const captured = vm.eval("globalThis.__captured_icons");
      if (Array.isArray(captured)) {
        for (const item of captured) interceptedIcons.push(item);
      }
    } else if (typeof launcher.eval_module === "function") {
      const modRes = launcher.eval_module(indexCode);
      meta = modRes.metadata || {};
    }
  } catch (err) {
    const errMsg = "解析插件脚本模块失败: " + err;
    console.error(errMsg);
    if (launcher.message_box) {
      launcher.message_box(errMsg, "语法解析错误", "error");
    }
    return false;
  }

  if (!meta) {
    meta = {};
  }

  const pluginName = meta.name || parsed.pluginId;
  const pluginPrefix = meta.prefix || (parsed.pluginId + ":");

  // 3. 收集并下载全部原生动态链接库 (native/dynamic_library) 与图标/资产
  const resourcesToDownload = collectResourcePaths(meta, interceptedIcons);
  console.info(`目标插件依赖资源列表: ${JSON.stringify(resourcesToDownload)}`);

  for (const relPath of resourcesToDownload) {
    const filename = getBasename(relPath);
    const fileUrl = resolveUrl(baseUrl, relPath);
    console.info(`正在下载插件依赖资源 [${filename}]: ${fileUrl}`);

    const fileRes = launcher.http.get(fileUrl, { timeout_ms: 30000 });
    if (fileRes.ok && fileRes.buffer) {
      launcher.fs.write(targetDir + "/" + filename, fileRes.buffer);
      console.info(`资源已落盘: ${targetDir}/${filename}`);
    } else {
      console.warn(`下载资源 [${filename}] 失败: HTTP ${fileRes.status} ${fileRes.status_text}`);
    }
  }

  // 4. 保存 index.js 脚本至插件目录
  launcher.fs.write(targetDir + "/index.js", indexCode);
  console.info(`插件入口脚本已写入: ${targetDir}/index.js`);

  // 5. 更新 config.toml
  addPluginToConfig(parsed.pluginId, pluginName, pluginPrefix, `plugins/${parsed.pluginId}`);

  // 6. 触发主程序热重载
  if (launcher.app && launcher.app.reload) {
    launcher.app.reload();
  }

  // 7. 弹窗提醒安装成功
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

  // 1. 从 config.toml 中彻底逐行移除配置
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
 * 查询输入处理 (响应 pm: install, pm: enable, pm: disable, pm: uninstall)
 */
export function on_query(input) {
  const raw = (input || "").trim();
  const lower = raw.toLowerCase();

  // 1. 空输入：展示顶层四大管理操作
  if (!raw) {
    return [
      {
        id: "guide-install",
        title: "安装插件 (Install Plugin)",
        subtitle: "输入: install <github/gitee/直链> 下载并安装新插件",
        icon: "info",
        badge: "管理",
        payload: { action: "guide" }
      },
      {
        id: "guide-enable",
        title: "启用插件 (Enable Plugin)",
        subtitle: "输入: enable 浏览当前已禁用的插件并一键启用",
        icon: "success",
        badge: "管理",
        payload: { action: "guide" }
      },
      {
        id: "guide-disable",
        title: "禁用插件 (Disable Plugin)",
        subtitle: "输入: disable 浏览当前已启用的插件并一键禁用",
        icon: "warn",
        badge: "管理",
        payload: { action: "guide" }
      },
      {
        id: "guide-uninstall",
        title: "卸载插件 (Uninstall Plugin)",
        subtitle: "输入: uninstall 浏览已安装列表或指定插件名回车彻底卸载",
        icon: "error",
        badge: "管理",
        payload: { action: "guide" }
      }
    ];
  }

  // 2. 启用插件 (enable / en)
  if (lower.startsWith("enable") || lower.startsWith("en")) {
    const afterEnable = lower.startsWith("enable")
      ? raw.substring(6).trim()
      : raw.substring(2).trim();

    const plugins = getInstalledPlugins();
    // 仅列出当前 enabled === false 的插件
    const disabledPlugins = plugins.filter(p => !p.enabled);
    const query = afterEnable.toLowerCase();

    const filtered = query
      ? disabledPlugins.filter(p => p.id.toLowerCase().includes(query) || p.name.toLowerCase().includes(query))
      : disabledPlugins;

    if (filtered.length === 0) {
      return [
        {
          id: "no-disabled-plugins",
          title: "当前没有已禁用的插件",
          subtitle: query ? `未找到匹配 "${query}" 的已禁用插件` : "所有已安装插件均处于启用状态",
          icon: "info",
          badge: "提示",
          payload: { action: "none" }
        }
      ];
    }

    return filtered.map(p => ({
      id: `enable-${p.id}`,
      title: `启用插件: ${p.name} (${p.id})`,
      subtitle: `前缀: ${p.prefix || "无"} | 路径: ${p.path} (回车启用并热重载)`,
      icon: "success",
      badge: "启用",
      payload: { action: "enable", pluginId: p.id, pluginName: p.name }
    }));
  }

  // 3. 禁用插件 (disable / dis)
  if (lower.startsWith("disable") || lower.startsWith("dis")) {
    const afterDisable = lower.startsWith("disable")
      ? raw.substring(7).trim()
      : raw.substring(3).trim();

    const plugins = getInstalledPlugins();
    // 仅列出当前 enabled === true 的插件（排除插件管理器自身）
    const enabledPlugins = plugins.filter(p => p.enabled && p.id !== "plugin-manager");
    const query = afterDisable.toLowerCase();

    const filtered = query
      ? enabledPlugins.filter(p => p.id.toLowerCase().includes(query) || p.name.toLowerCase().includes(query))
      : enabledPlugins;

    if (filtered.length === 0) {
      return [
        {
          id: "no-enabled-plugins",
          title: "当前没有可禁用的插件",
          subtitle: query ? `未找到匹配 "${query}" 的活跃插件` : "当前没有其他处于启用状态的插件",
          icon: "info",
          badge: "提示",
          payload: { action: "none" }
        }
      ];
    }

    return filtered.map(p => ({
      id: `disable-${p.id}`,
      title: `禁用插件: ${p.name} (${p.id})`,
      subtitle: `前缀: ${p.prefix || "无"} | 路径: ${p.path} (回车禁用并热重载)`,
      icon: "warn",
      badge: "禁用",
      payload: { action: "disable", pluginId: p.id, pluginName: p.name }
    }));
  }

  // 4. 卸载插件 (uninstall / un)
  if (lower.startsWith("uninstall") || lower.startsWith("un")) {
    const afterUninstall = lower.startsWith("uninstall")
      ? raw.substring(9).trim()
      : raw.substring(2).trim();

    const plugins = getInstalledPlugins();
    // 排除插件管理器自身
    const candidatePlugins = plugins.filter(p => p.id !== "plugin-manager");
    const query = afterUninstall.toLowerCase();

    const filtered = query
      ? candidatePlugins.filter(p => p.id.toLowerCase().includes(query) || p.name.toLowerCase().includes(query))
      : candidatePlugins;

    if (filtered.length === 0) {
      return [
        {
          id: "no-plugins-found",
          title: "未找到匹配的可卸载插件",
          subtitle: query ? `没有匹配 "${query}" 的插件` : "当前没有已安装或声明的第三方插件",
          icon: "warn",
          badge: "提示",
          payload: { action: "none" }
        }
      ];
    }

    return filtered.map(p => ({
      id: `uninstall-${p.id}`,
      title: `卸载插件: ${p.name} (${p.id})`,
      subtitle: `状态: ${p.enabled ? "已启用" : "已禁用"} | 路径: ${p.path} (回车彻底卸载)`,
      icon: "error",
      badge: "卸载",
      payload: { action: "uninstall", pluginId: p.id, pluginPath: p.path, pluginName: p.name }
    }));
  }

  // 5. 安装指令 (install / in)
  if (lower.startsWith("install") || lower.startsWith("in")) {
    const target = lower.startsWith("install")
      ? raw.substring(7).trim()
      : raw.substring(2).trim();

    if (!target) {
      return [
        {
          id: "prompt-install-url",
          title: "请输入插件 index.js 地址",
          subtitle: "例如: install github.com/owner/repo/index.js 或 direct URL",
          icon: "info",
          badge: "安装",
          payload: { action: "guide" }
        }
      ];
    }

    const parsed = resolvePluginUrl(target);
    if (!parsed) {
      return [
        {
          id: "invalid-url",
          title: "无效的插件地址格式",
          subtitle: `已输入: "${target}"，支持 GitHub, Gitee, GitLab 或 HTTP/HTTPS 直链`,
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

  // 6. 前缀缩写匹配导航 (如输入 i / e / d / u)
  const options = [
    {
      id: "guide-install",
      title: "安装插件 (Install Plugin)",
      subtitle: "输入: install <github/gitee/直链> 下载并安装新插件",
      icon: "info",
      badge: "管理",
      payload: { action: "guide" }
    },
    {
      id: "guide-enable",
      title: "启用插件 (Enable Plugin)",
      subtitle: "输入: enable 浏览当前已禁用的插件并一键启用",
      icon: "success",
      badge: "管理",
      payload: { action: "guide" }
    },
    {
      id: "guide-disable",
      title: "禁用插件 (Disable Plugin)",
      subtitle: "输入: disable 浏览当前已启用的插件并一键禁用",
      icon: "warn",
      badge: "管理",
      payload: { action: "guide" }
    },
    {
      id: "guide-uninstall",
      title: "卸载插件 (Uninstall Plugin)",
      subtitle: "输入: uninstall 浏览已安装列表或指定插件名回车彻底卸载",
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
  } else if (payload.action === "enable" && payload.pluginId) {
    if (setPluginEnabled(payload.pluginId, true)) {
      if (launcher.message_box) {
        launcher.message_box(`插件 [${payload.pluginName || payload.pluginId}] 已成功启用并热重载！`, "插件管理", "info");
      }
    }
  } else if (payload.action === "disable" && payload.pluginId) {
    if (setPluginEnabled(payload.pluginId, false)) {
      if (launcher.message_box) {
        launcher.message_box(`插件 [${payload.pluginName || payload.pluginId}] 已成功禁用并热重载！`, "插件管理", "info");
      }
    }
  } else if (payload.action === "uninstall" && payload.pluginId) {
    executeUninstall(payload.pluginId, payload.pluginPath);
  }
}
