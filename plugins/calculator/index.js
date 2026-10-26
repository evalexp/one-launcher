// ==============================================================================
// OneLauncher 官方插件: 极速数学计算器 (Calculator)
// 说明: 纯 JavaScript 编写的轻量插件示例，展示 on_setup 生命周期、内置图标引用、
//      自定义配置读取、规范结果对象以及系统剪贴板原生操作。
// ==============================================================================

export const metadata = {
  name: "极速计算器",
  prefix: "calc:",
  default_config: {
    precision: 6,
    auto_copy: true,
  },
};

let logger = null;
let pluginConfig = null;
let calcIcon = "calc";

/**
 * 插件初始化生命周期
 * @param {Object} context 宿主初始化环境对象
 * @param {Function} context.cache_icon (bytes|path) => ref_id 缓存自定义图标
 * @param {Function} context.get_plugin_config () => config 读取当前生效配置
 * @param {Function} context.get_logger () => logger 获取运行时日志输出器
 */
export function on_setup(context) {
  logger = context.get_logger();
  pluginConfig = context.get_plugin_config();

  logger.info("Calculator 计算器插件正在初始化 (on_setup)...");

  // 使用宿主提供的内置预设图标 "calc" (亦支持 "terminal"、"settings"、"info"、"file" 等)
  calcIcon = "calc";

  logger.info(`Calculator 计算器插件初始化就绪，计算精度: ${pluginConfig.precision || 6}`);
}

/**
 * 响应计算输入
 * @param {string} input 用户在 calc: 之后输入的数学表达式 (如 2^10, sqrt(144), 3*4+5)
 * @returns {Array<{id: string, title: string, subtitle: string, icon: string, payload: Object}>}
 */
export function on_query(input) {
  if (!input || input.trim() === "") {
    return [];
  }

  const raw = input.trim();
  const precision = (pluginConfig && pluginConfig.precision) || 6;
  const result = evaluateMath(raw, precision);

  if (result === null) {
    return [];
  }

  return [
    {
      id: "calculator_result",
      title: `${result}`,
      subtitle: `= ${raw}  (回车即可复制计算结果到剪贴板)`,
      icon: calcIcon,
      payload: {
        expression: raw,
        result: result,
      },
    },
  ];
}

/**
 * 用户在候选列表中选中并确认条目时触发
 * @param {Object} item 选中的条目对象 { id, payload }
 */
export function on_select(item) {
  if (!item || !item.payload || !item.payload.result) {
    return;
  }

  const val = String(item.payload.result);
  if (launcher && launcher.clipboard) {
    launcher.clipboard.write_text(val);
    if (logger) {
      logger.info(`已成功复制计算结果 '${val}' 到剪贴板`);
    }
  }
}

/**
 * 安全解析与求值数学表达式
 */
function evaluateMath(rawExpr, precision) {
  let expr = rawExpr.trim();
  if (!expr) return null;

  // 1. 语法预处理：^ 转为 **, % 转为 /100
  expr = expr.replace(/\^/g, "**");
  expr = expr.replace(/(\d+(\.\d+)?)%/g, "($1/100)");

  // 2. 数学函数与常量作用域定义
  const scope = {
    pi: Math.PI,
    PI: Math.PI,
    e: Math.E,
    E: Math.E,
    sin: Math.sin,
    cos: Math.cos,
    tan: Math.tan,
    asin: Math.asin,
    acos: Math.acos,
    atan: Math.atan,
    sqrt: Math.sqrt,
    cbrt: Math.cbrt,
    abs: Math.abs,
    round: Math.round,
    floor: Math.floor,
    ceil: Math.ceil,
    log: Math.log,
    log10: Math.log10,
    log2: Math.log2,
    pow: Math.pow,
    exp: Math.exp,
  };

  // 3. 严格白名单校验
  const allowed = /^[0-9+\-*/%().,\s a-zA-Z_]+$/;
  if (!allowed.test(expr)) {
    return null;
  }

  // 4. 禁用关键字拦截
  const forbidden = [
    "eval",
    "Function",
    "globalThis",
    "window",
    "process",
    "require",
    "import",
    "constructor",
    "prototype",
    "__proto__",
    "native",
    "launcher",
    "console",
  ];
  for (const kw of forbidden) {
    if (expr.includes(kw)) {
      return null;
    }
  }

  try {
    const keys = Object.keys(scope);
    const vals = Object.values(scope);
    const fn = new Function(...keys, `"use strict"; return (${expr});`);
    const num = fn(...vals);

    if (typeof num === "number" && Number.isFinite(num)) {
      const prec = typeof precision === "number" ? precision : 6;
      return Number(num.toFixed(prec)).toString();
    }
  } catch (err) {
    // 语法尚未输入完整时静默忽略
  }
  return null;
}
