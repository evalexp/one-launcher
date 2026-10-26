export const metadata = {
  name: "极速计算器",
  prefix: "=",
  default_config: {
    precision: 6,
    auto_copy: true,
  },
};

let calcIcon = "calc";

/**
 * 插件初始化
 * @param {Object} context 包含 context.cache_icon 用于图标缓存
 */
export function on_setup(context) {
  calcIcon = "calc";
  console.info(`Calculator 插件已就绪 (精度: ${(launcher.config && launcher.config.precision) || 6})`);
}

/**
 * 响应计算输入
 * @param {string} input 用户在 = 之后输入的数学表达式 (如 2^10, sqrt(144), 3*4+5)
 * @returns {Array<{id: string, title: string, subtitle: string, icon: string, payload: Object}>}
 */
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
      subtitle: `= ${raw}  (回车即可复制计算结果到剪贴板)`,
      icon: calcIcon,
      badge: "计算器",
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
    console.info(`已成功复制计算结果 '${val}' 到剪贴板`);
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
