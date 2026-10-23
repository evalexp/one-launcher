use std::path::PathBuf;

/// 已索引的应用程序项
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppItem {
    /// 应用程序展示名称，例如 "Google Chrome" 或 "网易云音乐"
    pub name: String,
    /// 快捷方式 (.lnk/.url) 或可执行文件本体的路径
    pub path: PathBuf,
    /// 描述或辅助说明（如所属目录或分类）
    pub description: String,
    /// 中文全拼，用于拼音模糊匹配（如 "wangyiyunyinyue"）
    pub pinyin_full: String,
    /// 中文简拼首字母，用于极速拼音匹配（如 "wyy"）
    pub pinyin_abbr: String,
    /// 是否为 Windows UWP (应用商店/系统内置) 应用
    pub is_uwp: bool,
    /// 若为 UWP 应用，其 AppUserModelId（例如 "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"）
    pub app_user_model_id: Option<String>,
}

impl AppItem {
    pub fn new(
        name: String,
        path: PathBuf,
        description: String,
        pinyin_full: String,
        pinyin_abbr: String,
    ) -> Self {
        Self {
            name,
            path,
            description,
            pinyin_full,
            pinyin_abbr,
            is_uwp: false,
            app_user_model_id: None,
        }
    }

    pub fn new_uwp(
        name: String,
        app_user_model_id: String,
        description: String,
        pinyin_full: String,
        pinyin_abbr: String,
    ) -> Self {
        Self {
            name,
            path: PathBuf::from(format!("shell:AppsFolder\\{}", app_user_model_id)),
            description,
            pinyin_full,
            pinyin_abbr,
            is_uwp: true,
            app_user_model_id: Some(app_user_model_id),
        }
    }
}
