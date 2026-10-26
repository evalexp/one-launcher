use one_launcher::config::model::AppConfig;
use one_launcher::config::template::default_config_content;
use one_launcher::config::ConfigManager;
use std::fs;

#[test]
fn test_default_config_template_is_valid() {
    let content = default_config_content();
    let parsed: Result<AppConfig, _> = toml::from_str(content);
    assert!(parsed.is_ok(), "默认模板必须能够正常解析为 AppConfig: {:?}", parsed.err());

    let config = parsed.unwrap();
    assert_eq!(config.general.hotkey, "Alt+Space");
    assert!(config.general.hide_on_blur);
    assert_eq!(config.appearance.width, 680);
    assert!(config.apps.scan_uwp);
    
    // 验证日志配置
    assert_eq!(config.log.level, "info");
    assert_eq!(config.log.path, "fs:logs/one-launcher.log");

    // 验证默认模板下发时不携带任何插件
    assert!(config.plugins.is_empty(), "默认配置模板应当不包含任何内置注册的插件");
}

#[test]
fn test_plugin_registration_via_toml() {
    let custom_toml = r#"
    [plugins.everything]
    enabled = true
    prefix = "fs:"
    name = "Everything 全盘搜索"
    path = "plugins/everything"

    [plugins.everything.config]
    max_results = 9

    [plugins.calculator]
    enabled = true
    prefix = "="
    name = "极速计算器"
    path = "plugins/calculator"

    [plugins.calculator.config]
    precision = 6
    "#;
    let config: AppConfig = toml::from_str(custom_toml).unwrap();
    assert!(config.plugins.contains_key("everything"));
    assert_eq!(config.plugins["everything"].prefix, "fs:");
    assert_eq!(config.plugins["everything"].config.get("max_results").and_then(|v| v.as_integer()), Some(9));

    assert!(config.plugins.contains_key("calculator"));
    assert_eq!(config.plugins["calculator"].prefix, "=");
    assert_eq!(config.plugins["calculator"].config.get("precision").and_then(|v| v.as_integer()), Some(6));
}

#[test]
fn test_partial_config_fallback_to_defaults() {
    let partial_toml = r#"
    [general]
    hotkey = "Ctrl+Shift+Space"
    "#;

    let parsed: AppConfig = toml::from_str(partial_toml).expect("应该支持缺省字段回退默认值");
    assert_eq!(parsed.general.hotkey, "Ctrl+Shift+Space");
    // 其他字段应该自动继承默认值
    assert_eq!(parsed.general.max_results, 9);
    assert_eq!(parsed.appearance.width, 680);
    assert!(parsed.plugins.is_empty());
}

#[test]
fn test_manual_reload_config() {
    let temp_dir = std::env::temp_dir().join(format!("one_launcher_reload_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let config_path = temp_dir.join("config.toml");

    // 初始写入
    fs::write(&config_path, default_config_content()).unwrap();

    let initial = ConfigManager::read_config_from_file(&config_path).unwrap();
    assert_eq!(initial.general.hotkey, "Alt+Space");

    // 用户在外部编辑器修改后保存
    let modified = r#"
    [general]
    hotkey = "Ctrl+Alt+Space"
    max_results = 12
    "#;
    fs::write(&config_path, modified).unwrap();

    // 手动调用重载
    let reloaded = ConfigManager::read_config_from_file(&config_path).unwrap();
    assert_eq!(reloaded.general.hotkey, "Ctrl+Alt+Space");
    assert_eq!(reloaded.general.max_results, 12);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_plugin_path_defaults_to_plugins_dir() {
    let toml_str = r#"
    [plugins.my_plugin]
    enabled = true

    [plugins.custom_plugin]
    enabled = true
    path = "custom/path/my_dir"
    "#;

    let config: AppConfig = toml::from_str(toml_str).expect("应当支持省略 path 字段");
    let my_plugin = &config.plugins["my_plugin"];
    assert_eq!(my_plugin.path, None);
    assert_eq!(my_plugin.get_path("my_plugin"), std::path::PathBuf::from("plugins/my_plugin"));

    let custom_plugin = &config.plugins["custom_plugin"];
    assert_eq!(custom_plugin.path, Some(std::path::PathBuf::from("custom/path/my_dir")));
    assert_eq!(custom_plugin.get_path("custom_plugin"), std::path::PathBuf::from("custom/path/my_dir"));
}
