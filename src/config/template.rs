/// 内嵌的默认 config.toml 模板资源文件
const DEFAULT_CONFIG_STR: &str = include_str!("../../assets/default_config.toml");

/// 获取内嵌的包含详细中文注释的默认 config.toml 文本内容
pub fn default_config_content() -> &'static str {
    DEFAULT_CONFIG_STR
}
