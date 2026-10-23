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
    assert_eq!(config.appearance.backdrop_effect, "mica");
    assert!(config.apps.scan_uwp);
    
    // 验证 Everything 插件通过 [plugins.everything] 注册
    assert!(config.plugins.contains_key("everything"));
    let ev = &config.plugins["everything"];
    assert!(ev.enabled);
    assert_eq!(ev.prefix, "fs:");
    assert_eq!(ev.config.get("max_results").and_then(|v| v.as_integer()), Some(9));
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
    assert!(parsed.plugins.contains_key("everything"));
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
