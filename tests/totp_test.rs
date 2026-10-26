use one_launcher::config::model::PluginConfig;
use one_launcher::plugin::PluginManager;
use std::collections::HashMap;

#[test]
fn test_totp_plugin_queries_and_evaluations() {
    let mut plugins = HashMap::new();
    let mut totp_config = toml::Table::new();
    totp_config.insert("interval".to_string(), toml::Value::Integer(30));
    totp_config.insert("algorithm".to_string(), toml::Value::String("SHA1".to_string()));

    plugins.insert(
        "totp".to_string(),
        PluginConfig {
            enabled: true,
            prefix: "otp:".to_string(),
            name: "动态验证码".to_string(),
            path: None,
            entry: "index.js".to_string(),
            config: totp_config,
        },
    );

    let mgr = PluginManager::init(&plugins);

    // 1. 验证默认直接输入 Secret 模式 (otp:XXXX)
    let matched = mgr.match_prefix("otp: JBSWY3DPEHPK3PXP");
    assert!(matched.is_some(), "应成功匹配 otp: 前缀");
    let (plugin, query) = matched.unwrap();
    assert_eq!(plugin.id, "totp");
    assert_eq!(query, "JBSWY3DPEHPK3PXP");

    let results = mgr.query(&plugin.id, query);
    assert_eq!(results.len(), 1);
    let item = &results[0];
    assert_eq!(item.id, "totp_result");
    assert_eq!(item.title.len(), 6, "验证码必须为 6 位数字");
    assert!(item.title.chars().all(|c| c.is_ascii_digit()), "必须全部为数字");
    assert_eq!(item.icon, "shield");
    assert_eq!(item.badge, "TOTP");

    // 2. 验证多元参数模式 (s=xxx&a=sha256&i=60) 与大小写忽略
    let matched_kv = mgr.match_prefix("otp: s=JBSWY3DPEHPK3PXP&a=sha256&i=60").unwrap();
    let results_kv = mgr.query(&matched_kv.0.id, matched_kv.1);
    assert_eq!(results_kv.len(), 1);
    assert_eq!(results_kv[0].title.len(), 6);
    assert!(results_kv[0].subtitle.contains("SHA256"));
    assert!(results_kv[0].subtitle.contains("60s"));

    // 3. 验证全拼单词与大写参数 (SECRET=xxx&ALGORITHM=SHA512&INTERVAL=45)
    let matched_upper = mgr.match_prefix("otp: SECRET=JBSWY3DPEHPK3PXP&ALGORITHM=sha512&INTERVAL=45").unwrap();
    let results_upper = mgr.query(&matched_upper.0.id, matched_upper.1);
    assert_eq!(results_upper.len(), 1);
    assert_eq!(results_upper[0].title.len(), 6);
    assert!(results_upper[0].subtitle.contains("SHA512"));
    assert!(results_upper[0].subtitle.contains("45s"));

    // 4. 验证无效 Base32 错误提示 (包含非法字符如 8, 9, 0, 1)
    let matched_invalid = mgr.match_prefix("otp: 189INVALID").unwrap();
    let results_invalid = mgr.query(&matched_invalid.0.id, matched_invalid.1);
    assert_eq!(results_invalid.len(), 1);
    assert_eq!(results_invalid[0].id, "totp_invalid_secret");
    assert_eq!(results_invalid[0].icon, "warn");

    // 5. 验证 on_select 选择动作
    mgr.select(&plugin.id, &item.id, &item.payload);
}
