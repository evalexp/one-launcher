use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 应用程序全局主配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub appearance: AppearanceConfig,
    pub apps: AppsConfig,
    pub everything: EverythingConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            appearance: AppearanceConfig::default(),
            apps: AppsConfig::default(),
            everything: EverythingConfig::default(),
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
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            hotkey: "Alt+Space".to_string(),
            hide_on_blur: true,
            clear_on_show: true,
            max_results: 9,
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

/// Everything 搜索联动配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EverythingConfig {
    /// 是否开启 Everything 原生 IPC 联动
    pub enabled: bool,
    /// 专用触发前缀；留空 "" 表示混合搜索，若填 "f " 或 "e:" 则输入该前缀时激活专属搜索
    pub prefix: String,
    /// Everything 实例名称 (空字符串 "" 为默认实例，若使用多开实例可指定如 "1.5a")
    pub instance_name: String,
    /// Everything 返回结果的最大数量上限
    pub max_results: usize,
}

impl Default for EverythingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            prefix: "".to_string(),
            instance_name: "".to_string(),
            max_results: 20,
        }
    }
}
