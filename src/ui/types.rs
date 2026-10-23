use crate::everything::EverythingItem;
use crate::indexer::AppItem;

#[derive(Debug, Clone)]
pub enum SearchResultKind {
    App(AppItem),
    Everything(EverythingItem),
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
}
