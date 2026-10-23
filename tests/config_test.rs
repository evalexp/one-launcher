use one_launcher::config::model::AppConfig;
use one_launcher::config::template::default_config_content;

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
    assert!(config.everything.enabled);
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
    assert!(parsed.everything.enabled);
}

#[test]
fn test_config_watcher_hot_reload() {
    use one_launcher::config::watcher::ConfigWatcher;
    use std::fs;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    let temp_dir = std::env::temp_dir().join(format!("one_launcher_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let config_path = temp_dir.join("config.toml");

    // 初始写入
    fs::write(&config_path, default_config_content()).unwrap();

    let (tx, rx) = channel();
    let _watcher = ConfigWatcher::watch(config_path.clone(), move |new_cfg| {
        let _ = tx.send(new_cfg);
    }).expect("监听器应成功启动");

    // 修改内容并保存
    let modified = r#"
    [general]
    hotkey = "Win+Space"
    max_results = 15
    "#;
    std::thread::sleep(Duration::from_millis(50));
    fs::write(&config_path, modified).unwrap();

    // 验证防抖后收到变更通知
    let reloaded = rx.recv_timeout(Duration::from_secs(3))
        .expect("应该在 3 秒内收到配置文件热重载通知");

    assert_eq!(reloaded.general.hotkey, "Win+Space");
    assert_eq!(reloaded.general.max_results, 15);

    // 清理临时文件
    let _ = fs::remove_dir_all(&temp_dir);
}
