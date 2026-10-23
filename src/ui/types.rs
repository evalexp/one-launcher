use crate::everything::EverythingItem;
use crate::indexer::AppItem;

#[derive(Debug, Clone)]
pub enum SearchResultKind {
    App(AppItem),
    Everything(EverythingItem),
    OpenConfig,
}

#[derive(Debug, Clone)]
pub struct SearchResultItem {
    pub title: String,
    pub subtitle: String,
    pub badge: String,
    pub kind: SearchResultKind,
}

impl SearchResultItem {
    pub fn from_app(app: AppItem) -> Self {
        let badge = if app.is_uwp { "[UWP]".to_string() } else { "[应用]".to_string() };
        let subtitle = app.path.to_string_lossy().to_string();
        Self {
            title: app.name.clone(),
            subtitle,
            badge,
            kind: SearchResultKind::App(app),
        }
    }

    pub fn from_everything(item: EverythingItem) -> Self {
        let badge = if item.is_folder { "[目录]".to_string() } else { "[文件]".to_string() };
        let subtitle = item.path.clone();
        Self {
            title: item.name.clone(),
            subtitle,
            badge,
            kind: SearchResultKind::Everything(item),
        }
    }

    pub fn open_config_action() -> Self {
        Self {
            title: "编辑配置文件 (config.toml)".to_string(),
            subtitle: "回车调用系统默认文本编辑器修改，保存后自动热重载".to_string(),
            badge: "[配置]".to_string(),
            kind: SearchResultKind::OpenConfig,
        }
    }
}
