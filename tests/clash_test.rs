use one_launcher::config::model::PluginConfig;
use one_launcher::plugin::PluginManager;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_clash_plugin_queries_and_env() {
    let temp_dir = std::env::temp_dir().join("one_launcher_clash_test");
    let _ = fs::create_dir_all(&temp_dir);
    let mock_config = temp_dir.join("config.yaml");

    let yaml_content = r#"
mixed-port: 7890
external-controller: 127.0.0.1:9090
secret: "test_secret_123"
"#;
    fs::write(&mock_config, yaml_content).unwrap();

    // 设置临时环境变量
    unsafe {
        std::env::set_var("MIHOMO_CONFIG", mock_config.to_string_lossy().to_string());
        std::env::set_var("SUBSCRIPTION_URL", "http://127.0.0.1:18080/sub");
    }

    let mut configs = HashMap::new();
    let mut clash_cfg = toml::Table::new();
    clash_cfg.insert("foo".to_string(), toml::Value::String("bar".to_string()));

    configs.insert(
        "clash".to_string(),
        PluginConfig {
            enabled: true,
            prefix: "clash:".to_string(),
            name: "Clash 控制器".to_string(),
            path: PathBuf::from("plugins/clash"),
            entry: "index.js".to_string(),
            config: clash_cfg,
        },
    );

    let pm = PluginManager::init(&configs);

    // 1. 测试空输入查询: 返回全部选项 (update-subscription, dash, enable-system-proxy, disable-system-proxy, subscription-time)
    let all_items = pm.query("clash", "");
    assert_eq!(all_items.len(), 5, "空输入时应返回全部 5 个候选动作");
    let ids: Vec<String> = all_items.iter().map(|item| item.id.clone()).collect();
    assert!(ids.contains(&"update-subscription".to_string()));
    assert!(ids.contains(&"dash".to_string()));
    assert!(ids.contains(&"enable-system-proxy".to_string()));
    assert!(ids.contains(&"disable-system-proxy".to_string()));
    assert!(ids.contains(&"subscription-time".to_string()));

    // 验证所有条目的 badge 为必需字段
    for item in &all_items {
        assert_eq!(item.badge, "Clash");
    }

    // 2. 测试前缀忽略大小写匹配: "u" -> 仅匹配 update-subscription
    let u_items = pm.query("clash", "U");
    assert_eq!(u_items.len(), 1);
    assert_eq!(u_items[0].id, "update-subscription");
    assert!(u_items[0].subtitle.contains("当前:"));

    // 3. 测试前缀匹配: "dash" -> 仅匹配 dash (包含端口 9090)
    let dash_items = pm.query("clash", "dash");
    assert_eq!(dash_items.len(), 1);
    assert_eq!(dash_items[0].id, "dash");
    assert!(dash_items[0].subtitle.contains("9090"));

    // 4. 测试前缀匹配: "en" -> 仅匹配 enable-system-proxy (包含 127.0.0.1:7890)
    let en_items = pm.query("clash", "EN");
    assert_eq!(en_items.len(), 1);
    assert_eq!(en_items[0].id, "enable-system-proxy");
    assert!(en_items[0].subtitle.contains("127.0.0.1:7890"));

    // 5. 测试前缀匹配: "dis" -> 仅匹配 disable-system-proxy
    let dis_items = pm.query("clash", "dis");
    assert_eq!(dis_items.len(), 1);
    assert_eq!(dis_items[0].id, "disable-system-proxy");

    // 6. 测试前缀匹配: "sub" -> 仅匹配 subscription-time
    let sub_items = pm.query("clash", "Sub");
    assert_eq!(sub_items.len(), 1);
    assert_eq!(sub_items[0].id, "subscription-time");

    // 7. 测试 on_select 触发 (例如 subscription-time, enable-system-proxy, disable-system-proxy)
    pm.select("clash", "subscription-time", r#"{"action":"subscription-time"}"#);
    pm.select("clash", "enable-system-proxy", r#"{"action":"enable-system-proxy"}"#);
    pm.select("clash", "disable-system-proxy", r#"{"action":"disable-system-proxy"}"#);

    // 清理临时文件与环境变量
    unsafe {
        std::env::remove_var("MIHOMO_CONFIG");
        std::env::remove_var("SUBSCRIPTION_URL");
    }
    let _ = fs::remove_dir_all(&temp_dir);
}
