export const metadata = {
  name: "插件管理器",
  prefix: "pm:",
  default_config: {}
};

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
 * 解析 GitHub 仓库或子目录链接
 */
function parseGithubTarget(raw) {
  let cleaned = (raw || "").trim();
  cleaned = cleaned.replace(/^https?:\/\//i, "");
  cleaned = cleaned.replace(/^github\.com\//i, "");
  cleaned = cleaned.replace(/\.git$/i, "");
  cleaned = cleaned.replace(/^\/+|\/+$/g, "");

  const parts = cleaned.split("/").filter(Boolean);
  if (parts.length < 2) {
    return null;
  }
  const owner = parts[0];
  const repo = parts[1];
  const subfolder = parts.slice(2).join("/");

  // 提取插件 ID
  const pluginId = (parts.length >= 3 ? parts[parts.length - 1] : repo)
    .toLowerCase()
    .replace(/[^a-z0-9_\-]/g, "_");

  return {
    owner,
    repo,
    subfolder,
    pluginId,
    cloneUrl: `https://github.com/${owner}/${repo}.git`
  };
}

/**
 * 递归复制文件夹
 */
function copyDirRecursive(src, dst) {
  launcher.fs.mkdir(dst);
  const items = launcher.fs.list(src) || [];
  for (const item of items) {
    const s = src + "/" + item.name;
    const d = dst + "/" + item.name;
    if (item.is_dir) {
      if (item.name !== ".git") {
        copyDirRecursive(s, d);
      }
    } else {
      launcher.fs.copy(s, d, true);
    }
  }
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
 * 执行插件下载与安装
 */
function executeInstall(info) {
  const pluginsBase = getPluginsBaseDir();
  const destDir = (pluginsBase + "/" + info.pluginId).replace(/\\/g, "/");
  console.info(`准备安装插件 [${info.pluginId}] 至: ${destDir}`);

  const tempDir = (pluginsBase + "/.temp_install_" + Date.now()).replace(/\\/g, "/");
  launcher.fs.mkdir(tempDir);

  let success = false;

  // 1. 优先尝试系统 git 命令
  console.info(`正在通过 Git 克隆: ${info.cloneUrl}`);
  const gitRes = launcher.shell.exec("git", {
    args: ["clone", "--depth", "1", info.cloneUrl, tempDir]
  });

  if (gitRes.code === 0 && launcher.fs.exists(tempDir)) {
    console.info("Git 克隆成功！");
    const srcDir = info.subfolder
      ? (tempDir + "/" + info.subfolder).replace(/\\/g, "/")
      : tempDir;

    if (launcher.fs.exists(srcDir)) {
      copyDirRecursive(srcDir, destDir);
      success = true;
    } else {
      console.warn(`未在仓库中找到子目录: ${info.subfolder}，直接采用仓库根目录`);
      copyDirRecursive(tempDir, destDir);
      success = true;
    }
  } else {
    // 2. 回退机制：通过原生 HTTP 直接下载仓库 Zip
    console.info("Git 不可用或克隆失败，回退为原生 HTTP 下载源码包...");
    let zipUrl = `https://github.com/${info.owner}/${info.repo}/archive/refs/heads/main.zip`;
    let zipRes = launcher.http.get(zipUrl, { timeout_ms: 30000 });
    if (!zipRes.ok) {
      zipUrl = `https://github.com/${info.owner}/${info.repo}/archive/refs/heads/master.zip`;
      zipRes = launcher.http.get(zipUrl, { timeout_ms: 30000 });
    }

    if (zipRes.ok && zipRes.buffer) {
      const zipPath = tempDir + "/archive.zip";
      launcher.fs.write(zipPath, zipRes.buffer);
      console.info("下载完成，正在解压...");
      const tarRes = launcher.shell.exec(`tar -xf "${zipPath}" -C "${tempDir}"`);
      if (tarRes.code === 0) {
        const entries = launcher.fs.list(tempDir) || [];
        const extractedRoot = entries.find(e => e.is_dir && e.name.startsWith(info.repo));
        if (extractedRoot) {
          const candidateSrc = info.subfolder
            ? (extractedRoot.path.replace(/\\/g, "/") + "/" + info.subfolder)
            : extractedRoot.path.replace(/\\/g, "/");

          if (launcher.fs.exists(candidateSrc)) {
            copyDirRecursive(candidateSrc, destDir);
            success = true;
          } else {
            copyDirRecursive(extractedRoot.path.replace(/\\/g, "/"), destDir);
            success = true;
          }
        }
      }
    }
  }

  // 清理临时目录
  launcher.fs.remove(tempDir);

  if (!success || !launcher.fs.exists(destDir)) {
    console.error(`下载安装插件 [${info.pluginId}] 失败`);
    return false;
  }

  // 3. 读取 index.js 获取 metadata 声明
  let pluginName = info.pluginId;
  let pluginPrefix = info.pluginId + ":";
  const indexJs = destDir + "/index.js";
  if (launcher.fs.exists(indexJs)) {
    const code = launcher.fs.read_text(indexJs) || "";
    const nm = code.match(/name:\s*['"](.*?)['"]/);
    const pr = code.match(/prefix:\s*['"](.*?)['"]/);
    if (nm) pluginName = nm[1];
    if (pr) pluginPrefix = pr[1];
  }

  // 4. 更新 config.toml
  addPluginToConfig(info.pluginId, pluginName, pluginPrefix, `plugins/${info.pluginId}`);

  // 5. 触发配置热重载
  if (launcher.app && launcher.app.reload) {
    launcher.app.reload();
  }
  console.info(`插件 [${pluginName}] (${info.pluginId}) 安装完成！`);
  return true;
}

/**
 * 执行插件卸载
 */
function executeUninstall(pluginId, pluginPath) {
  console.info(`准备卸载插件: [${pluginId}] 路径: ${pluginPath}`);

  // 1. 从 config.toml 中移除
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
  console.info(`插件 [${pluginId}] 已成功卸载并重载生效！`);
  return true;
}

/**
 * 查询输入处理 (响应 install 与 uninstall)
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
        subtitle: "输入: install <github.com/xxx/yyy[/plugin_name]>",
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
          title: "请输入 GitHub 仓库地址",
          subtitle: "例如: install github.com/owner/repo/my_plugin",
          icon: "info",
          badge: "安装",
          payload: { action: "guide" }
        }
      ];
    }

    const info = parseGithubTarget(target);
    if (!info) {
      return [
        {
          id: "invalid-url",
          title: "无效的 GitHub 地址格式",
          subtitle: `已输入: "${target}"，标准格式: github.com/owner/repo[/folder]`,
          icon: "warn",
          badge: "错误",
          payload: { action: "none" }
        }
      ];
    }

    return [
      {
        id: `install-${info.pluginId}`,
        title: `下载并安装插件: ${info.pluginId}`,
        subtitle: `来源: ${info.owner}/${info.repo}${info.subfolder ? "/" + info.subfolder : ""} (回车执行安装)`,
        icon: "info",
        badge: "安装",
        payload: { action: "install", info }
      }
    ];
  }

  // 4. 输入前缀匹配顶层选项 (如仅输入 "i" 或 "u")
  const options = [
    {
      id: "guide-install",
      title: "安装插件 (Install Plugin)",
      subtitle: "输入: install <github.com/xxx/yyy[/plugin_name]>",
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

  if (payload.action === "install" && payload.info) {
    executeInstall(payload.info);
  } else if (payload.action === "uninstall" && payload.pluginId) {
    executeUninstall(payload.pluginId, payload.pluginPath);
  }
}
