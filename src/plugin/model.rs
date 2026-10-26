use serde::{Deserialize, Serialize};

/// 插件向启动器返回的标准搜索条目规范
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResultItem {
    /// 唯一标识
    pub id: String,
    /// 主标题
    pub title: String,
    /// 副标题或详情
    pub subtitle: String,
    /// 图标引用 ID (ref_id)
    #[serde(default)]
    pub icon: String,
    /// 右侧角标文字 (如 "文件" / "文件夹" / "计算器")
    #[serde(default)]
    pub badge: String,
    /// 回车执行时传给插件 on_select 的载荷 (支持对象或字符串序列化)
    #[serde(default)]
    pub payload: String,
}
