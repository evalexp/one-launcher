export const metadata = {
  name: "系统控制",
  prefix: "sys:",
  default_config: {}
};

const ACTIONS = [
  {
    id: "quit",
    title: "退出 OneLauncher",
    subtitle: "关闭并彻底退出启动器进程 (sys: quit)",
    icon: "error",
    badge: "系统",
    payload: { action: "quit" }
  },
  {
    id: "reload",
    title: "重载配置",
    subtitle: "即时重新读取配置文件并热更新插件 (sys: reload)",
    icon: "settings",
    badge: "系统",
    payload: { action: "reload" }
  },
  {
    id: "edit",
    title: "编辑系统配置",
    subtitle: "在系统默认文本编辑器中打开 config.toml (sys: edit)",
    icon: "notepad",
    badge: "系统",
    payload: { action: "edit" }
  }
];

/**
 * 查询输入处理 (满足 input 为空时返回全部 3 个选项，不为空时忽略大小写按前缀匹配)
 */
export function on_query(input) {
  const trimmed = (input || "").trim().toLowerCase();
  if (!trimmed) {
    return ACTIONS;
  }
  return ACTIONS.filter(item => item.id.toLowerCase().startsWith(trimmed));
}

/**
 * 执行选中的系统动作
 */
export function on_select(item) {
  const action = item && item.payload && item.payload.action;
  switch (action) {
    case "quit": {
      console.info("正在执行系统控制指令: 退出 OneLauncher");
      if (typeof launcher !== "undefined" && launcher.app && launcher.app.quit) {
        launcher.app.quit();
      } else if (typeof launcher !== "undefined" && launcher.exit) {
        launcher.exit();
      }
      break;
    }
    case "reload": {
      console.info("正在执行系统控制指令: 重载配置与插件");
      if (typeof launcher !== "undefined" && launcher.app && launcher.app.reload) {
        launcher.app.reload();
      }
      break;
    }
    case "edit": {
      console.info("正在执行系统控制指令: 编辑配置文件");
      if (typeof launcher !== "undefined" && launcher.app && launcher.app.edit_config) {
        launcher.app.edit_config();
      } else if (typeof launcher !== "undefined" && launcher.app && launcher.app.config_path) {
        launcher.shell.open(launcher.app.config_path);
      }
      break;
    }
    default:
      console.warn("未知系统动作: " + action);
  }
}
