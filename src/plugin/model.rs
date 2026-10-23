use serde::{Deserialize, Serialize};

/// 插件向启动器返回的标准搜索条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResultItem {
    /// 唯一标识
    pub id: String,
    /// 主标题 (例如文件名或应用名)
    pub title: String,
    /// 副标题或详情路径 (低饱和度低色调显示)
    pub subtitle: String,
    /// 是否为文件夹
    #[serde(default)]
    pub is_folder: bool,
    /// 回车执行时传给插件 execute 的载荷 (例如绝对路径)
    #[serde(default)]
    pub payload: String,
}
