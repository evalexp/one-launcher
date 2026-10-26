export const metadata = {
  name: "Clash 控制器",
  prefix: "clash:",
  default_config: {}
};

/**
 * 获取 MIHOMO_CONFIG 环境变量
 */
function getMihomoConfigPath() {
  if (typeof launcher !== "undefined" && launcher.env) {
    const val = launcher.env.get("MIHOMO_CONFIG") || launcher.env.MIHOMO_CONFIG;
    if (val && typeof val === "string") return val.trim();
  }
  return null;
}

/**
 * 获取 SUBSCRIPTION_URL 环境变量
 */
function getSubscriptionUrl() {
  if (typeof launcher !== "undefined" && launcher.env) {
    const val = launcher.env.get("SUBSCRIPTION_URL") || launcher.env.SUBSCRIPTION_URL;
    if (val && typeof val === "string") return val.trim();
  }
  return null;
}

/**
 * 从 MIHOMO_CONFIG 文件解析 external-controller 端口与 secret
 */
function getClashPortAndSecret() {
  const cfgPath = getMihomoConfigPath();
  if (!cfgPath || !launcher.fs.exists(cfgPath)) {
    return { port: "9090", secret: "" };
  }
  const content = launcher.fs.read_text(cfgPath) || "";
  let port = "9090";
  let secret = "";

  const portMatch = content.match(/^external-controller:\s*['"]?.*?:(\d+)['"]?/m);
  if (portMatch) {
    port = portMatch[1];
  }

  const secretMatch = content.match(/^secret:\s*['"]?(.*?)['"]?\s*$/m);
  if (secretMatch) {
    secret = secretMatch[1];
  }

  return { port, secret };
}

/**
 * 从 MIHOMO_CONFIG 解析 mixed-port 混合代理端口
 */
function getMixedPort() {
  const cfgPath = getMihomoConfigPath();
  if (!cfgPath || !launcher.fs.exists(cfgPath)) {
    return null;
  }
  const content = launcher.fs.read_text(cfgPath) || "";
  const match = content.match(/^mixed-port:\s*(\d+)/m);
  return match ? match[1] : null;
}

/**
 * 计算配置文件的最后更新时间差字符串
 */
function getSubscriptionTimeInfo() {
  const cfgPath = getMihomoConfigPath();
  if (!cfgPath) {
    return "环境变量 MIHOMO_CONFIG 未设置";
  }
  if (!launcher.fs.exists(cfgPath)) {
    return "配置文件不存在: " + cfgPath;
  }
  const stat = launcher.fs.stat(cfgPath);
  if (!stat || !stat.modified_ms) {
    return "未知时间";
  }
  const diffSec = Math.max(0, Math.floor((Date.now() - stat.modified_ms) / 1000));
  if (diffSec < 60) return `${diffSec} 秒前更新`;
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin} 分钟前更新`;
  const diffHours = Math.floor(diffMin / 60);
  const remMin = diffMin % 60;
  if (diffHours < 24) return `${diffHours} 小时 ${remMin} 分钟前更新`;
  const diffDays = Math.floor(diffHours / 24);
  const remHours = diffHours % 24;
  return `${diffDays} 天 ${remHours} 小时前更新`;
}

/**
 * 生成候选动作列表
 */
function getAllActions() {
  const cfgPath = getMihomoConfigPath();
  const timeText = getSubscriptionTimeInfo();
  const { port } = getClashPortAndSecret();
  const mixed = getMixedPort();

  return [
    {
      id: "update-subscription",
      title: "Update Subscription",
      subtitle: `备份并下载最新订阅 (当前: ${timeText})`,
      icon: "info",
      badge: "Clash",
      payload: { action: "update-subscription" }
    },
    {
      id: "dash",
      title: "Open Dashboard",
      subtitle: `在默认浏览器中打开 MetaCubeXD 控制台 (端口: ${port})`,
      icon: "terminal",
      badge: "Clash",
      payload: { action: "dash" }
    },
    {
      id: "enable-system-proxy",
      title: "Enable System Proxy",
      subtitle: `开启 Windows 全局系统代理 (${mixed ? "127.0.0.1:" + mixed : "从配置读取 mixed-port"})`,
      icon: "shield",
      badge: "Clash",
      payload: { action: "enable-system-proxy" }
    },
    {
      id: "disable-system-proxy",
      title: "Disable System Proxy",
      subtitle: "关闭 Windows 全局系统代理 (ProxyEnable = 0)",
      icon: "shield",
      badge: "Clash",
      payload: { action: "disable-system-proxy" }
    },
    {
      id: "subscription-time",
      title: "Subscription Time",
      subtitle: `当前配置文件修改状态: ${timeText}`,
      icon: "file",
      badge: "Clash",
      payload: { action: "subscription-time" }
    }
  ];
}

/**
 * 查询输入处理 (满足 input 为空时返回全部，不为空时忽略大小写前缀匹配)
 */
export function on_query(input) {
  const all = getAllActions();
  const trimmed = (input || "").trim().toLowerCase();

  if (!trimmed) {
    return all;
  }

  // 忽略大小写匹配前缀
  const matched = all.filter(item => item.id.toLowerCase().startsWith(trimmed));
  return matched;
}

/**
 * 用户回车选择时执行对应动作
 */
export function on_select(item) {
  const action = item && item.payload && item.payload.action;
  const cfgPath = getMihomoConfigPath();

  switch (action) {
    case "update-subscription": {
      try {
        const subUrl = getSubscriptionUrl();
        if (!cfgPath) {
          console.error("未找到环境变量 MIHOMO_CONFIG");
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box("未找到环境变量 MIHOMO_CONFIG，请先在系统环境变量中设置配置文件路径。", "Clash 提示", "warn");
          }
          return;
        }
        if (!subUrl) {
          console.error("未找到环境变量 SUBSCRIPTION_URL");
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box("未找到环境变量 SUBSCRIPTION_URL，请先在系统环境变量中设置订阅地址。", "Clash 提示", "warn");
          }
          return;
        }

        // 1. 备份现有配置
        if (launcher.fs.exists(cfgPath)) {
          const bakPath = cfgPath + ".bak";
          launcher.fs.copy(cfgPath, bakPath, true);
          console.info(`已备份配置文件至: ${bakPath}`);
        }

        // 2. 发起 HTTP 请求下载订阅
        console.info(`正在请求订阅: ${subUrl}`);
        const res = launcher.http.get(subUrl, { timeout_ms: 20000 });
        if (!res.ok || !res.body) {
          console.error(`下载订阅失败: HTTP ${res.status} ${res.status_text}`);
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box(`下载订阅配置失败: HTTP ${res.status} ${res.status_text}`, "Clash 订阅更新失败", "error");
          }
          return;
        }
        launcher.fs.write(cfgPath, res.body);
        console.info(`订阅内容已更新写入: ${cfgPath}`);

        // 3. 通知 Mihomo 重载配置
        const { port, secret } = getClashPortAndSecret();
        const reloadUrl = `http://127.0.0.1:${port}/configs?force=true`;
        console.info(`通知 Mihomo 重新载入配置: ${reloadUrl}`);
        const reloadRes = launcher.http.request({
          url: reloadUrl,
          method: "PUT",
          headers: {
            "Authorization": "Bearer " + secret,
            "Content-Type": "application/json"
          },
          body: "{}",
          timeout_ms: 6000
        });

        if (reloadRes.ok) {
          console.info("Mihomo 订阅更新并重载成功！");
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box("Mihomo 订阅更新并重载成功！", "Clash 订阅更新", "info");
          }
        } else {
          console.warn(`Mihomo 重载请求完成: 状态 ${reloadRes.status} ${reloadRes.status_text}`);
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box(`订阅已更新保存，但 Mihomo 重载返回状态: ${reloadRes.status} ${reloadRes.status_text}`, "Clash 订阅提示", "warn");
          }
        }
      } catch (err) {
        console.error("更新订阅发生异常: " + err);
        if (typeof launcher !== "undefined" && launcher.message_box) {
          launcher.message_box(`更新订阅发生异常:\n${err}`, "Clash 订阅更新错误", "error");
        }
      }
      break;
    }

    case "dash": {
      const { port, secret } = getClashPortAndSecret();
      const url = `http://127.0.0.1:${port}/ui/metacubexd/#/setup?host=127.0.0.1&hostname=127.0.0.1&port=${port}&secret=${secret}`;
      console.info("正在打开控制台: " + url);
      launcher.shell.open(url);
      break;
    }

    case "enable-system-proxy": {
      try {
        const currentCfg = getMihomoConfigPath();
        if (!currentCfg || !launcher.fs.exists(currentCfg)) {
          const msg = "未找到 Mihomo 配置文件，无法读取代理端口。";
          console.error(msg);
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box(msg, "系统代理设置", "warn");
          }
          return;
        }

        const mixed = getMixedPort();
        if (!mixed) {
          const msg = "未能从配置文件中解析出 mixed-port，请检查 Mihomo 配置。";
          console.error(msg);
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box(msg, "系统代理设置", "warn");
          }
          return;
        }

        const proxyServer = `127.0.0.1:${mixed}`;
        console.info("正在开启系统代理: " + proxyServer);

        const r1 = launcher.shell.exec('reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyEnable /t REG_DWORD /d 1 /f');
        const r2 = launcher.shell.exec(`reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyServer /t REG_SZ /d "${proxyServer}" /f`);
        const r3 = launcher.shell.exec('reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyOverride /t REG_SZ /d "<local>" /f');

        const ok1 = r1 && (r1.code === 0 || r1.success);
        const ok2 = r2 && (r2.code === 0 || r2.success);
        const ok3 = r3 && (r3.code === 0 || r3.success);

        if (ok1 && ok2 && ok3) {
          console.info("全局系统代理已开启！");
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box(`全局系统代理已开启！\n代理地址: ${proxyServer}`, "系统代理设置", "info");
          }
        } else {
          const errMsg = [r1 && r1.stderr, r2 && r2.stderr, r3 && r3.stderr].filter(Boolean).join("\n") || "注册表写入失败";
          console.error("开启系统代理失败: " + errMsg);
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box(`开启系统代理失败:\n${errMsg}`, "系统代理设置", "error");
          }
        }
      } catch (err) {
        console.error("开启系统代理发生异常: " + err);
        if (typeof launcher !== "undefined" && launcher.message_box) {
          launcher.message_box(`开启系统代理发生异常:\n${err}`, "系统代理设置", "error");
        }
      }
      break;
    }

    case "disable-system-proxy": {
      try {
        console.info("正在关闭系统代理...");
        const res = launcher.shell.exec('reg add "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings" /v ProxyEnable /t REG_DWORD /d 0 /f');
        const ok = res && (res.code === 0 || res.success);

        if (ok) {
          console.info("全局系统代理已关闭！");
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box("全局系统代理已成功关闭！", "系统代理设置", "info");
          }
        } else {
          const errMsg = (res && res.stderr) || "注册表修改失败";
          console.error("关闭系统代理失败: " + errMsg);
          if (typeof launcher !== "undefined" && launcher.message_box) {
            launcher.message_box(`关闭系统代理失败:\n${errMsg}`, "系统代理设置", "error");
          }
        }
      } catch (err) {
        console.error("关闭系统代理发生异常: " + err);
        if (typeof launcher !== "undefined" && launcher.message_box) {
          launcher.message_box(`关闭系统代理发生异常:\n${err}`, "系统代理设置", "error");
        }
      }
      break;
    }

    case "subscription-time": {
      const info = getSubscriptionTimeInfo();
      console.info(`订阅文件时间: ${info}`);
      if (typeof launcher !== "undefined" && launcher.clipboard) {
        launcher.clipboard.write_text(info);
      }
      break;
    }

    default:
      console.warn("未知的 Clash 指令操作: " + action);
  }
}
