pub mod dynamic_lib;
pub mod model;

use crate::config::PluginConfig;
use dynamic_lib::{bind_dynamic_libraries, LoadedDynamicLibraries};
use model::PluginResultItem;
use rquickjs::{Context, Function, Module, Object, Runtime};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{error, info, warn};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

/// 解析插件目录在磁盘上的实际路径（支持当前工作目录、可执行文件目录、系统 APPDATA 目录）
pub fn resolve_plugin_dir(path: &std::path::Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    // 1. 优先尝试当前工作目录
    if path.exists() {
        return path.to_path_buf();
    }
    // 2. 尝试可执行程序同级目录 (例如发布包结构)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let candidate = exe_dir.join(path);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    // 3. 尝试系统数据目录 (%APPDATA%/OneLauncher)
    if let Some(app_data) = dirs::config_dir() {
        let candidate = app_data.join("OneLauncher").join(path);
        if candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

pub struct LoadedPlugin {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub dir: PathBuf,
    pub merged_config: HashMap<String, toml::Value>,
    pub _dyn_libs: LoadedDynamicLibraries,
}

pub struct PluginManager {
    _rt: Runtime,
    ctx: Context,
    pub plugins: Vec<LoadedPlugin>,
}

impl PluginManager {
    pub fn get_plugin_name(&self, plugin_id: &str) -> String {
        self.plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| plugin_id.to_string())
    }
    /// 扫描并初始化所有在配置文件中启用的插件
    pub fn init(configs: &HashMap<String, PluginConfig>) -> Self {
        let rt = Runtime::new().expect("初始化 QuickJS Runtime 失败");
        let ctx = Context::full(&rt).expect("初始化 QuickJS Context 失败");

        // 注入全局基础能力对象 (launcher 与 console)
        ctx.with(|ctx| {
            let launcher = Object::new(ctx.clone()).unwrap();
            let shell = Object::new(ctx.clone()).unwrap();

            // launcher.shell.open(path)
            let open_fn = Function::new(ctx.clone(), |path: String| {
                open_system_path(&path);
                Ok::<_, rquickjs::Error>(())
            })
            .unwrap();
            shell.set("open", open_fn).unwrap();
            launcher.set("shell", shell).unwrap();

            // 注入全局 console 与 launcher.log
            let console = Object::new(ctx.clone()).unwrap();
            let log_obj = Object::new(ctx.clone()).unwrap();

            let log_info = Function::new(ctx.clone(), |msg: String| {
                tracing::info!("{}", msg);
                Ok::<_, rquickjs::Error>(())
            })
            .unwrap();
            let log_warn = Function::new(ctx.clone(), |msg: String| {
                tracing::warn!("{}", msg);
                Ok::<_, rquickjs::Error>(())
            })
            .unwrap();
            let log_error = Function::new(ctx.clone(), |msg: String| {
                tracing::error!("{}", msg);
                Ok::<_, rquickjs::Error>(())
            })
            .unwrap();
            let log_debug = Function::new(ctx.clone(), |msg: String| {
                tracing::debug!("{}", msg);
                Ok::<_, rquickjs::Error>(())
            })
            .unwrap();

            console.set("log", log_info.clone()).unwrap();
            console.set("info", log_info.clone()).unwrap();
            console.set("warn", log_warn.clone()).unwrap();
            console.set("error", log_error.clone()).unwrap();
            console.set("debug", log_debug.clone()).unwrap();

            log_obj.set("info", log_info).unwrap();
            log_obj.set("warn", log_warn).unwrap();
            log_obj.set("error", log_error).unwrap();
            log_obj.set("debug", log_debug).unwrap();

            launcher.set("log", log_obj).unwrap();
            ctx.globals().set("launcher", launcher).unwrap();
            ctx.globals().set("console", console).unwrap();
        });

        let mut loaded = Vec::new();

        for (id, cfg) in configs {
            if !cfg.enabled {
                continue;
            }

            let plugin_dir = resolve_plugin_dir(&cfg.path);
            let entry_file = plugin_dir.join(&cfg.entry);

            if !entry_file.exists() {
                warn!(
                    "插件 [{}] 入口文件不存在: {}，跳过加载",
                    id,
                    entry_file.display()
                );
                continue;
            }

            let code = match std::fs::read_to_string(&entry_file) {
                Ok(c) => c,
                Err(e) => {
                    warn!("读取插件脚本 {} 失败: {}", entry_file.display(), e);
                    continue;
                }
            };

            let plugin_id = id.clone();
            let cfg_prefix = cfg.prefix.clone();
            let cfg_name = cfg.name.clone();

            let load_res: Result<LoadedPlugin, String> = ctx.with(|ctx| {
                // 1. 声明并评估 ES 模块
                let module = match Module::declare(ctx.clone(), plugin_id.clone(), code) {
                    Ok(m) => match m.eval() {
                        Ok((mod_eval, _)) => mod_eval,
                        Err(e) => return Err(format!("插件模块 eval 失败: {}", e)),
                    },
                    Err(e) => return Err(format!("插件模块 declare 失败: {}", e)),
                };

                // 2. 读取 metadata 对象并解析绑定原生动态库
                let (dyn_libs, default_cfg_opt) = match module.get::<_, Object>("metadata") {
                    Ok(meta) => {
                        let libs = bind_dynamic_libraries(&ctx, &meta, &plugin_dir)?;
                        let def_cfg = meta.get::<_, Object>("default_config").ok();
                        (libs, def_cfg)
                    }
                    Err(_) => {
                        return Err(format!("插件 [{}] 必须导出 metadata 元信息对象", id));
                    }
                };

                // 3. 构建该插件的合并配置并在加载期单次实例化 JS Object（后续查询 0 内存分配）
                let config_obj = Object::new(ctx.clone()).unwrap();
                let mut merged_config = HashMap::new();
                if let Some(def_cfg) = default_cfg_opt {
                    for key_res in def_cfg.keys::<String>() {
                        if let Ok(k) = key_res {
                            if let Ok(val) = def_cfg.get::<_, rquickjs::Value>(&k) {
                                if let Some(i) = val.as_int() {
                                    merged_config.insert(k.clone(), toml::Value::Integer(i as i64));
                                    let _ = config_obj.set(k.as_str(), i);
                                } else if let Some(b) = val.as_bool() {
                                    merged_config.insert(k.clone(), toml::Value::Boolean(b));
                                    let _ = config_obj.set(k.as_str(), b);
                                } else if let Some(f) = val.as_float() {
                                    merged_config.insert(k.clone(), toml::Value::Float(f));
                                    let _ = config_obj.set(k.as_str(), f);
                                } else if let Some(s) = val.as_string() {
                                    if let Ok(st) = s.to_string() {
                                        merged_config.insert(k.clone(), toml::Value::String(st.clone()));
                                        let _ = config_obj.set(k.as_str(), st);
                                    }
                                }
                            }
                        }
                    }
                }
                for (k, v) in &cfg.config {
                    merged_config.insert(k.clone(), v.clone());
                    match v {
                        toml::Value::Integer(i) => { let _ = config_obj.set(k.as_str(), *i); }
                        toml::Value::Boolean(b) => { let _ = config_obj.set(k.as_str(), *b); }
                        toml::Value::Float(f) => { let _ = config_obj.set(k.as_str(), *f); }
                        toml::Value::String(s) => { let _ = config_obj.set(k.as_str(), s.as_str()); }
                        _ => {}
                    }
                }
                let _ = ctx.globals().set(format!("__plugin_{}_config", plugin_id), config_obj);

                // 4. 严格校验并常驻注册 on_query 与 on_select 函数 (不符合规范直接拒绝加载)
                let query_fn: Function = module
                    .get("on_query")
                    .map_err(|_| format!("插件 [{}] 缺少必需导出的 on_query(input) 函数", id))?;
                let _ = ctx.globals().set(format!("__plugin_{}_query", plugin_id), query_fn);

                let select_fn: Function = module
                    .get("on_select")
                    .map_err(|_| format!("插件 [{}] 缺少必需导出的 on_select(item) 函数", id))?;
                let _ = ctx.globals().set(format!("__plugin_{}_select", plugin_id), select_fn);

                info!("插件 [{}] ({}) 加载就绪，绑定前缀 '{}'", id, cfg_name, cfg_prefix);

                Ok(LoadedPlugin {
                    id: plugin_id,
                    name: cfg_name,
                    prefix: cfg_prefix,
                    dir: plugin_dir.clone(),
                    merged_config,
                    _dyn_libs: dyn_libs,
                })
            });

            match load_res {
                Ok(plugin) => loaded.push(plugin),
                Err(err) => {
                    error!("加载插件 [{}] 失败: {}", id, err);
                }
            }
        }

        Self {
            _rt: rt,
            ctx,
            plugins: loaded,
        }
    }

    /// 匹配输入前缀，按最长前缀优先匹配已注册的插件
    pub fn match_prefix<'a>(&'a self, query: &'a str) -> Option<(&'a LoadedPlugin, &'a str)> {
        let trimmed = query.trim_start();
        let mut matched: Option<(&LoadedPlugin, &str)> = None;

        for plugin in &self.plugins {
            if !plugin.prefix.is_empty() && trimmed.starts_with(&plugin.prefix) {
                let remainder = trimmed[plugin.prefix.len()..].trim();
                if let Some((prev, _)) = matched {
                    if plugin.prefix.len() > prev.prefix.len() {
                        matched = Some((plugin, remainder));
                    }
                } else {
                    matched = Some((plugin, remainder));
                }
            }
        }

        matched
    }

    /// 向指定插件发起用户输入查询 (on_query)
    pub fn query(&self, plugin_id: &str, input: &str) -> Vec<PluginResultItem> {
        self.ctx.with(|ctx| {
            tracing::debug!("向插件 [{}] 派发查询参数: '{}'", plugin_id, input);
            // 复用加载期预先分配的静态配置对象引用，0 次 Object::new()，0 堆内存分配
            let cfg_key = format!("__plugin_{}_config", plugin_id);
            if let Ok(cfg_obj) = ctx.globals().get::<_, Object>(&cfg_key) {
                if let Ok(launcher) = ctx.globals().get::<_, Object>("launcher") {
                    let _ = launcher.set("config", cfg_obj.clone());
                }
                let _ = ctx.globals().set("config", cfg_obj);
            }

            let fn_name = format!("__plugin_{}_query", plugin_id);
            let query_fn: Function = match ctx.globals().get(&fn_name) {
                Ok(f) => f,
                Err(e) => {
                    warn!("插件 [{}] 未找到 on_query 函数: {}", plugin_id, e);
                    return Vec::new();
                }
            };

            let res_val: rquickjs::Value = match query_fn.call((input,)) {
                Ok(v) => v,
                Err(e) => {
                    warn!("插件 [{}] 执行 on_query 出错: {}", plugin_id, e);
                    return Vec::new();
                }
            };

            if !res_val.is_array() {
                return Vec::new();
            }

            let array = res_val.into_array().unwrap();
            let mut items = Vec::new();

            for i in 0..array.len() {
                if let Ok(obj) = array.get::<Object>(i) {
                    let id: String = obj.get("id").unwrap_or_default();
                    let title: String = obj.get("title").unwrap_or_default();
                    let subtitle: String = obj.get("subtitle").unwrap_or_default();
                    let is_folder: bool = obj.get("is_folder").unwrap_or(false);
                    let payload: String = obj.get("payload").unwrap_or_else(|_| id.clone());

                    items.push(PluginResultItem {
                        id,
                        title,
                        subtitle,
                        is_folder,
                        payload,
                    });
                }
            }

            tracing::debug!("插件 [{}] 查询完成，返回 {} 项结果", plugin_id, items.len());
            items
        })
    }

    /// 用户在列表中选中并确认某项条目时触发选择动作 (on_select)
    pub fn select(&self, plugin_id: &str, item_id: &str, payload: &str) {
        self.ctx.with(|ctx| {
            // 复用预置配置引用
            let cfg_key = format!("__plugin_{}_config", plugin_id);
            if let Ok(cfg_obj) = ctx.globals().get::<_, Object>(&cfg_key) {
                if let Ok(launcher) = ctx.globals().get::<_, Object>("launcher") {
                    let _ = launcher.set("config", cfg_obj.clone());
                }
                let _ = ctx.globals().set("config", cfg_obj);
            }

            let fn_name = format!("__plugin_{}_select", plugin_id);
            if let Ok(select_fn) = ctx.globals().get::<_, Function>(&fn_name) {
                let item_obj = Object::new(ctx.clone()).unwrap();
                let _ = item_obj.set("id", item_id);
                let _ = item_obj.set("payload", payload);
                if let Err(e) = select_fn.call::<_, ()>((item_obj,)) {
                    warn!("插件 [{}] on_select 执行失败: {}", plugin_id, e);
                }
                return;
            }
            // 兜底默认执行动作 (打开路径)
            open_system_path(payload);
        });
    }
}

/// 打开系统文件或调用默认关联程序
pub fn open_system_path(path: &str) {
    if path.is_empty() {
        return;
    }
    unsafe {
        let op = HSTRING::from("open");
        let target = HSTRING::from(path);
        let _ = ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOW,
        );
    }
}
