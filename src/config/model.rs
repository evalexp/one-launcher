use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 应用程序全局主配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub appearance: AppearanceConfig,
    pub apps: AppsConfig,
    pub log: LogConfig,
    pub plugins: HashMap<String, PluginConfig>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut plugins = HashMap::new();
        let mut everything_config = toml::Table::new();
        everything_config.insert("max_results".to_string(), toml::Value::Integer(9));

        plugins.insert(
            "everything".to_string(),
            PluginConfig {
                enabled: true,
                prefix: "fs:".to_string(),
                name: "Everything 全盘搜索".to_string(),
                path: PathBuf::from("plugins/everything"),
                entry: "index.js".to_string(),
                config: everything_config,
            },
        );

        let mut calc_config = toml::Table::new();
        calc_config.insert("precision".to_string(), toml::Value::Integer(6));

        plugins.insert(
            "calculator".to_string(),
            PluginConfig {
                enabled: true,
                prefix: "calc:".to_string(),
                name: "极速计算器".to_string(),
                path: PathBuf::from("plugins/calculator"),
                entry: "index.js".to_string(),
                config: calc_config,
            },
        );

        Self {
            general: GeneralConfig::default(),
            appearance: AppearanceConfig::default(),
            apps: AppsConfig::default(),
            log: LogConfig::default(),
            plugins,
        }
    }
}

/// 常规基础配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    /// 全局呼出/隐藏热键，如 "Alt+Space", "Ctrl+Space", "Win+Space"
    pub hotkey: String,
    /// 窗口失去焦点时是否自动隐藏
    pub hide_on_blur: bool,
    /// 唤出窗口时是否清空上一次的输入框内容
    pub clear_on_show: bool,
    /// 启动器列表最大显示结果数
    pub max_results: usize,
    /// 插件输入防抖间隔时间 (毫秒)，默认 100ms；仅对外部插件生效，核心应用搜索 0ms 即时响应
    pub plugin_debounce_ms: u32,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            hotkey: "Alt+Space".to_string(),
            hide_on_blur: true,
            clear_on_show: true,
            max_results: 9,
            plugin_debounce_ms: 100,
        }
    }
}

/// 外观与视觉效果配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    /// 主题模式: "system" (跟随系统), "dark" (暗色), "light" (亮色)
    pub theme: String,
    /// 启动器窗口宽度 (像素)
    pub width: u32,
    /// 字体大小 (像素)
    pub font_size: u32,
    /// 背景特效: "mica" (Windows 11 云母), "acrylic" (亚克力), "solid" (纯色)
    pub backdrop_effect: String,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            width: 680,
            font_size: 16,
            backdrop_effect: "mica".to_string(),
        }
    }
}

/// 系统应用扫描与索引配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppsConfig {
    /// 索引的文件扩展名，通常为 lnk (快捷方式), exe (可执行文件), url (网页快捷方式)
    pub extensions: Vec<String>,
    /// 是否扫描 Windows 10/11 UWP (应用商店) 应用程序
    pub scan_uwp: bool,
    /// 额外扫描的自定义软件目录 (例如用户的绿色软件、便携软件目录)
    pub extra_directories: Vec<PathBuf>,
    /// 排除的黑名单关键词（命中名称或路径则不收录）
    pub exclude_patterns: Vec<String>,
}

impl Default for AppsConfig {
    fn default() -> Self {
        Self {
            extensions: vec!["lnk".to_string(), "exe".to_string(), "url".to_string()],
            scan_uwp: true,
            extra_directories: Vec::new(),
            exclude_patterns: vec![
                "卸载".to_string(),
                "Uninstall".to_string(),
                "Help".to_string(),
                "Readme".to_string(),
            ],
        }
    }
}

/// 插件拓展配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub prefix: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default = "default_entry")]
    pub entry: String,
    /// 插件自定义专属配置块 [plugins.<id>.config]
    #[serde(default)]
    pub config: toml::Table,
}

fn default_true() -> bool {
    true
}

fn default_entry() -> String {
    "index.js".to_string()
}

/// 日志记录与排错配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// 日志记录级别: "trace", "debug", "info", "warn", "error", "silent" (默认: "info")
    pub level: String,
    /// 日志输出目标: "std" (输出至终端) 或 "fs:<path>" (输出至指定文件，如 "fs:logs/one-launcher.log")
    pub path: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            path: "fs:logs/one-launcher.log".to_string(),
        }
    }
}
