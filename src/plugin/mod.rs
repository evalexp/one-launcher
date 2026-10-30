pub mod dynamic_lib;
pub mod host_app;
pub mod host_env;
pub mod host_fs;
pub mod host_http;
pub mod host_process;
pub mod host_progress;
pub mod host_vm;
pub mod model;

use crate::config::PluginConfig;
use dynamic_lib::{bind_dynamic_libraries, LoadedDynamicLibraries};
pub use model::PluginResultItem;
use rquickjs::{Context, Function, Module, Object, Runtime};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{error, info, warn};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY, SW_SHOWNORMAL};

thread_local! {
    static CURRENT_PLUGIN_NAME: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub fn set_current_plugin(name: Option<String>) {
    CURRENT_PLUGIN_NAME.with(|cell| {
        *cell.borrow_mut() = name;
    });
}

pub fn get_current_plugin() -> Option<String> {
    CURRENT_PLUGIN_NAME.with(|cell| cell.borrow().clone())
}

pub struct PluginContextGuard;
impl Drop for PluginContextGuard {
    fn drop(&mut self) {
        set_current_plugin(None);
    }
}

pub fn enter_plugin_context(name: &str) -> PluginContextGuard {
    set_current_plugin(Some(name.to_string()));
    PluginContextGuard
}

pub fn format_plugin_msg(msg: &str) -> String {
    if let Some(name) = get_current_plugin() {
        format!("-[{}] {}", name, msg)
    } else {
        msg.to_string()
    }
}

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
    /// 插件专属完全隔离的 QuickJS Context
    pub ctx: Context,
    pub merged_config: HashMap<String, toml::Value>,
    pub _dyn_libs: LoadedDynamicLibraries,
}

pub struct PluginManager {
    pub plugins: Vec<LoadedPlugin>,
    _rt: Runtime,
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        self.plugins.clear();
    }
}

/// 为插件 Context 注册全局 Rust Core 共享能力 (launcher API 网关与 console)
pub fn setup_shared_launcher_apis<'js>(
    ctx: &rquickjs::Ctx<'js>,
    hwnd: isize,
    config_path: PathBuf,
    plugin_dir: &std::path::Path,
) -> Result<Object<'js>, rquickjs::Error> {
    let launcher = Object::new(ctx.clone())?;
    let shell = Object::new(ctx.clone())?;
    let clipboard = Object::new(ctx.clone())?;

    // launcher.shell.open(path)
    let open_fn = Function::new(ctx.clone(), |path: String| {
        open_system_path(&path);
        Ok::<_, rquickjs::Error>(())
    })?;
    shell.set("open", open_fn)?;

    // 注册命令行与外部进程能力 (launcher.shell.exec / spawn 与 launcher.process，支持 sudo 提权与 batch_exec)
    let _ = host_process::register_process(ctx, &launcher, &shell, hwnd);
    launcher.set("shell", shell)?;

    // 注册原生文件系统操作能力 (launcher.fs)
    let _ = host_fs::register_fs(ctx, &launcher);

    // 注册 HTTP 网络请求能力 (launcher.http 与 fetch，基于 reqwest)
    let _ = host_http::register_http(ctx, &launcher);

    // 注册环境变量操作能力 (launcher.env)
    let _ = host_env::register_env(ctx, &launcher);

    // 注册应用生命周期与配置管理能力 (launcher.app)
    let _ = host_app::register_app(ctx, &launcher, hwnd, config_path);

    // 注册进度展示能力 (launcher.progress 与 launcher.app.progress)
    if let Ok(app_obj) = launcher.get::<_, Object>("app") {
        let _ = host_progress::register_progress(ctx, &launcher, &app_obj);
    }

    // 注册虚拟机与隔离上下文能力 (launcher.create_context, launcher.eval_module)
    let _ = host_vm::register_vm(ctx, &launcher);

    // launcher.clipboard.write_text(text) / launcher.clipboard.write(text)
    let copy_fn = Function::new(ctx.clone(), |text: String| {
        set_clipboard_text(&text);
        Ok::<_, rquickjs::Error>(())
    })?;
    clipboard.set("write_text", copy_fn.clone())?;
    clipboard.set("write", copy_fn)?;
    launcher.set("clipboard", clipboard)?;

    // 注入核心 launcher.cache_icon(path_or_bytes)
    let p_dir = plugin_dir.to_path_buf();
    let cache_icon_fn = Function::new(ctx.clone(), move |val: rquickjs::Value| {
        // 1. 如果传入的是字符串，优先匹配内置预设矢量图标 ID
        if let Some(s) = val.as_string() {
            if let Ok(st) = s.to_string() {
                let lower = st.to_lowercase();
                match lower.as_str() {
                    "folder" | "dir" | "file" | "doc" | "error" | "success" | "warn"
                    | "warning" | "info" | "calc" | "terminal" | "cmd" | "settings"
                    | "gear" | "notepad" | "shield" | "key" => return Ok::<_, rquickjs::Error>(st),
                    _ => {}
                }
            }
        }

        // 2. 尝试从本地相对/绝对文件路径或内存字节提取并缓存
        let ref_id = match extract_icon_bytes(&val, &p_dir) {
            Some(bytes) => crate::ui::icon_extractor::cache_icon_from_bytes(&bytes)
                .unwrap_or_else(|| "file".to_string()),
            None => "file".to_string(),
        };
        Ok::<_, rquickjs::Error>(ref_id)
    })?;
    launcher.set("cache_icon", cache_icon_fn)?;

    // 注入全局 console 与 launcher.log
    let console = Object::new(ctx.clone())?;
    let log_obj = Object::new(ctx.clone())?;

    let log_info = Function::new(ctx.clone(), |msg: String| {
        tracing::info!("{}", format_plugin_msg(&msg));
        Ok::<_, rquickjs::Error>(())
    })?;
    let log_warn = Function::new(ctx.clone(), |msg: String| {
        tracing::warn!("{}", format_plugin_msg(&msg));
        Ok::<_, rquickjs::Error>(())
    })?;
    let log_error = Function::new(ctx.clone(), |msg: String| {
        tracing::error!("{}", format_plugin_msg(&msg));
        Ok::<_, rquickjs::Error>(())
    })?;
    let log_debug = Function::new(ctx.clone(), |msg: String| {
        tracing::debug!("{}", format_plugin_msg(&msg));
        Ok::<_, rquickjs::Error>(())
    })?;

    console.set("log", log_info.clone())?;
    console.set("info", log_info.clone())?;
    console.set("warn", log_warn.clone())?;
    console.set("error", log_error.clone())?;
    console.set("debug", log_debug.clone())?;

    log_obj.set("info", log_info)?;
    log_obj.set("warn", log_warn)?;
    log_obj.set("error", log_error)?;
    log_obj.set("debug", log_debug)?;

    launcher.set("log", log_obj)?;
    ctx.globals().set("launcher", launcher.clone())?;
    ctx.globals().set("console", console)?;

    Ok(launcher)
}

impl PluginManager {
    pub fn get_plugin_name(&self, plugin_id: &str) -> String {
        self.plugins
            .iter()
            .find(|p| p.id == plugin_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| plugin_id.to_string())
    }

    /// 扫描并初始化所有在配置文件中启用的插件（默认无宿主窗口绑定）
    pub fn init(configs: &HashMap<String, PluginConfig>) -> Self {
        let config_path = crate::config::ConfigManager::detect_config_path();
        Self::init_with_host(configs, 0, Some(config_path))
    }

    /// 扫描并初始化插件，每个插件拥有独立隔离的 QuickJS Context
    pub fn init_with_host(
        configs: &HashMap<String, PluginConfig>,
        hwnd: isize,
        config_path_opt: Option<PathBuf>,
    ) -> Self {
        let config_path = config_path_opt.unwrap_or_else(crate::config::ConfigManager::detect_config_path);
        if let Ok(app_cfg) = crate::config::ConfigManager::read_config_from_file(&config_path) {
            host_http::set_global_proxy(app_cfg.network.get_effective_proxy().map(str::to_string));
        }
        let rt = Runtime::new().expect("初始化 QuickJS Runtime 失败");
        let mut loaded = Vec::new();

        for (id, cfg) in configs {
            if !cfg.enabled {
                continue;
            }

            let plugin_dir = resolve_plugin_dir(&cfg.get_path(id));
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

            // 为每个插件初始化独立隔离的 QuickJS Context
            let plugin_ctx = match Context::full(&rt) {
                Ok(c) => c,
                Err(e) => {
                    error!("创建插件 [{}] 的独立 Context 失败: {}", id, e);
                    continue;
                }
            };

            let plugin_id = id.clone();
            let cfg_prefix = cfg.prefix.clone();
            let cfg_name = cfg.name.clone();
            let config_path_clone = config_path.clone();

            let load_res: Result<LoadedPlugin, String> = plugin_ctx.with(|ctx| {
                // 1. 注入全局共享的 Rust Core API (launcher)
                let launcher = setup_shared_launcher_apis(&ctx, hwnd, config_path_clone, &plugin_dir)
                    .map_err(|e| format!("初始化 launcher API 失败: {}", e))?;

                // 2. 声明并评估 ES 模块
                let module = match Module::declare(ctx.clone(), plugin_id.clone(), code) {
                    Ok(m) => match m.eval() {
                        Ok((mod_eval, _)) => mod_eval,
                        Err(e) => return Err(format!("插件模块 eval 失败: {}", e)),
                    },
                    Err(e) => return Err(format!("插件模块 declare 失败: {}", e)),
                };

                // 3. 读取 metadata 对象并解析绑定原生动态库
                let (dyn_libs, default_cfg_opt, meta_name_opt, meta_prefix_opt) =
                    match module.get::<_, Object>("metadata") {
                        Ok(meta) => {
                            let libs = bind_dynamic_libraries(&ctx, &meta, &plugin_dir)?;
                            let def_cfg = meta.get::<_, Object>("default_config").ok();
                            let m_name = meta.get::<_, String>("name").ok();
                            let m_prefix = meta.get::<_, String>("prefix").ok();
                            (libs, def_cfg, m_name, m_prefix)
                        }
                        Err(_) => {
                            return Err(format!("插件 [{}] 必须导出 metadata 元信息对象", id));
                        }
                    };

                let final_name = if !cfg_name.is_empty() {
                    cfg_name
                } else if let Some(m_name) = meta_name_opt {
                    m_name
                } else {
                    plugin_id.clone()
                };

                let final_prefix = if !cfg_prefix.is_empty() {
                    cfg_prefix
                } else if let Some(m_prefix) = meta_prefix_opt {
                    m_prefix
                } else {
                    String::new()
                };

                let _guard = enter_plugin_context(&final_name);

                // 4. 构建合并配置
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

                // 5. 构建插件专属上下文对象 (self / context / plugin)，完全隔离于该插件 Context
                let self_obj: Object = ctx
                    .eval(
                        r#"({
                            get_config() { return this.config; },
                            get_id() { return this.id; },
                            get_name() { return this.name; },
                            get_prefix() { return this.prefix; },
                            get_dir() { return this.dir; }
                        })"#,
                    )
                    .unwrap();

                let pdir_str = plugin_dir.to_string_lossy().to_string();
                self_obj.set("config", config_obj).unwrap();
                self_obj.set("id", plugin_id.clone()).unwrap();
                self_obj.set("name", final_name.clone()).unwrap();
                self_obj.set("prefix", final_prefix.clone()).unwrap();
                self_obj.set("dir", pdir_str.clone()).unwrap();
                self_obj.set("path", pdir_str).unwrap();

                // launcher 保持为纯净的共享 API 网关，不携带插件私有 config；插件自身私有环境仅挂载 self
                let _ = ctx.globals().set("launcher", launcher);
                let _ = ctx.globals().set("self", self_obj.clone());

                // 6. 执行插件 on_setup() 初始化生命周期 (严格无参调用)
                if let Ok(setup_fn) = module.get::<_, Function>("on_setup") {
                    if let Err(e) = setup_fn.call::<_, ()>(()) {
                        warn!("插件 [{}] 执行 on_setup 失败: {}", plugin_id, e);
                    } else {
                        info!("插件 [{}] 初始化就绪", plugin_id);
                    }
                }

                // 7. 校验并注册 on_query 与 on_select 函数
                let query_fn: Function = module
                    .get("on_query")
                    .map_err(|_| format!("插件 [{}] 缺少必需导出的 on_query(input) 函数", id))?;
                let _ = ctx.globals().set("__on_query", query_fn);

                let select_fn: Function = module
                    .get("on_select")
                    .map_err(|_| format!("插件 [{}] 缺少必需导出的 on_select(item) 函数", id))?;
                let _ = ctx.globals().set("__on_select", select_fn);

                info!("插件 [{}] 加载就绪，前缀 '{}'", id, final_prefix);

                Ok(LoadedPlugin {
                    id: plugin_id,
                    name: final_name,
                    prefix: final_prefix,
                    dir: plugin_dir.clone(),
                    ctx: plugin_ctx.clone(),
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
        let plugin = match self.plugins.iter().find(|p| p.id == plugin_id) {
            Some(p) => p,
            None => return Vec::new(),
        };

        let _guard = enter_plugin_context(&plugin.name);
        plugin.ctx.with(|ctx| {
            tracing::debug!("向插件 [{}] 派发查询参数: '{}'", plugin_id, input);

            let query_fn: Function = match ctx.globals().get("__on_query") {
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
                    let icon: String = obj.get("icon").unwrap_or_default();
                    let badge: String = obj.get("badge").unwrap_or_else(|_| plugin_id.to_string());

                    let payload_str = if let Ok(val) = obj.get::<_, rquickjs::Value>("payload") {
                        if let Some(s) = val.as_string() {
                            s.to_string().unwrap_or_default()
                        } else {
                            ctx.json_stringify(val)
                                .ok()
                                .flatten()
                                .and_then(|s| s.to_string().ok())
                                .unwrap_or_default()
                        }
                    } else {
                        String::new()
                    };

                    items.push(PluginResultItem {
                        id,
                        title,
                        subtitle,
                        icon,
                        badge,
                        payload: payload_str,
                    });
                }
            }

            tracing::debug!("插件 [{}] 查询完成，返回 {} 项结果", plugin_id, items.len());
            items
        })
    }

    /// 用户在列表中选中并确认某项条目时触发选择动作 (on_select)
    pub fn select(&self, plugin_id: &str, item_id: &str, payload: &str) {
        let plugin = match self.plugins.iter().find(|p| p.id == plugin_id) {
            Some(p) => p,
            None => {
                open_system_path(payload);
                return;
            }
        };

        let _guard = enter_plugin_context(&plugin.name);
        plugin.ctx.with(|ctx| {
            if let Ok(select_fn) = ctx.globals().get::<_, Function>("__on_select") {
                let item_obj = Object::new(ctx.clone()).unwrap();
                let _ = item_obj.set("id", item_id);
                let payload_val = if let Ok(parsed) = ctx.json_parse(payload) {
                    parsed
                } else if let Ok(s) = rquickjs::String::from_str(ctx.clone(), payload) {
                    rquickjs::Value::from_string(s)
                } else {
                    rquickjs::Value::new_null(ctx.clone())
                };
                let _ = item_obj.set("payload", payload_val);
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
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        let op = HSTRING::from("open");
        let target = HSTRING::from(path);
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// 写入纯文本内容到 Windows 系统剪贴板
#[cfg(windows)]
pub fn set_clipboard_text(text: &str) -> bool {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GHND};

    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let _ = EmptyClipboard();
        let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let byte_count = utf16.len() * 2;
        if let Ok(hmem) = GlobalAlloc(GHND, byte_count) {
            let ptr = GlobalLock(hmem);
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr as *mut u16, utf16.len());
                let _ = GlobalUnlock(hmem);
                let _ = SetClipboardData(13, windows::Win32::Foundation::HANDLE(hmem.0));
            }
        }
        let _ = CloseClipboard();
        true
    }
}

#[cfg(not(windows))]
pub fn set_clipboard_text(_text: &str) -> bool {
    false
}

/// 从 QuickJS 值中安全提取图标二进制数据（支持 TypedArray、ArrayBuffer、数字数组、本地文件路径及字符串）
pub fn extract_icon_bytes<'js>(
    val: &rquickjs::Value<'js>,
    plugin_dir: &std::path::Path,
) -> Option<Vec<u8>> {
    if let Ok(typed) = rquickjs::TypedArray::<u8>::from_value(val.clone()) {
        return unsafe { typed.as_bytes() }.map(|b| b.to_vec());
    }
    if let Some(buf) = rquickjs::ArrayBuffer::from_value(val.clone()) {
        return unsafe { buf.as_bytes() }.map(|b| b.to_vec());
    }
    if let Some(arr) = val.as_array() {
        let mut bytes = Vec::with_capacity(arr.len());
        for i in 0..arr.len() {
            if let Ok(b) = arr.get::<u32>(i) {
                bytes.push(b as u8);
            } else {
                return None;
            }
        }
        return Some(bytes);
    }
    if let Some(s) = val.as_string() {
        if let Ok(st) = s.to_string() {
            let p = std::path::Path::new(&st);
            let target_path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                plugin_dir.join(p)
            };
            if target_path.exists() && target_path.is_file() {
                if let Ok(file_bytes) = std::fs::read(&target_path) {
                    return Some(file_bytes);
                }
            }
            return None;
        }
    }
    None
}
