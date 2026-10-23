use crate::indexer::AppItem;
use crate::plugin::model::PluginResultItem;

#[derive(Debug, Clone)]
pub enum SearchResultKind {
    App(AppItem),
    Plugin {
        plugin_id: String,
        item_id: String,
        payload: String,
    },
}

#[derive(Debug, Clone)]
pub struct SearchResultItem {
    pub title: String,
    pub subtitle: String,
    pub badge: String,
    pub icon_path: String,
    pub is_folder: bool,
    pub kind: SearchResultKind,
}

impl SearchResultItem {
    pub fn from_app(app: AppItem) -> Self {
        let badge = if app.is_uwp {
            "UWP".to_string()
        } else {
            "应用".to_string()
        };
        let subtitle = app.path.to_string_lossy().to_string();
        let icon_path = subtitle.clone();
        Self {
            title: app.name.clone(),
            subtitle,
            badge,
            icon_path,
            is_folder: false,
            kind: SearchResultKind::App(app),
        }
    }

    pub fn from_plugin(plugin_name: &str, plugin_id: &str, item: PluginResultItem) -> Self {
        let badge = if item.is_folder {
            "文件夹".to_string()
        } else {
            plugin_name.to_string()
        };
        Self {
            title: item.title,
            subtitle: item.subtitle,
            badge,
            icon_path: String::new(),
            is_folder: item.is_folder,
            kind: SearchResultKind::Plugin {
                plugin_id: plugin_id.to_string(),
                item_id: item.id,
                payload: item.payload,
            },
        }
    }
}
