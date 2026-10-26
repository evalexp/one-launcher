use one_launcher::config::model::PluginConfig;
use one_launcher::plugin::PluginManager;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn test_calculator_plugin_lifecycle_and_evaluation() {
    let mut plugins = HashMap::new();
    let mut calc_config = toml::Table::new();
    calc_config.insert("precision".to_string(), toml::Value::Integer(4));

    plugins.insert(
        "calculator".to_string(),
        PluginConfig {
            enabled: true,
            prefix: "=".to_string(),
            name: "极速计算器".to_string(),
            path: PathBuf::from("plugins/calculator"),
            entry: "index.js".to_string(),
            config: calc_config,
        },
    );

    let mgr = PluginManager::init(&plugins);

    // 1. 前缀严格匹配
    let matched = mgr.match_prefix("= 12 * (3 + 5)");
    assert!(matched.is_some(), "应匹配 = 前缀");
    let (plugin, query) = matched.unwrap();
    assert_eq!(plugin.id, "calculator");
    assert_eq!(query, "12 * (3 + 5)");

    // 2. 执行计算求值
    let results = mgr.query(&plugin.id, query);
    assert_eq!(results.len(), 1, "应返回 1 个计算结果条目");
    let item = &results[0];
    assert_eq!(item.id, "calculator_result");
    assert_eq!(item.title, "96");
    assert_eq!(item.icon, "calc");
    assert_eq!(item.badge, "计算器");
    assert!(item.payload.contains("96"));

    // 3. 高级数学函数求值 (sqrt, 幂次 ^)
    let res_sqrt = mgr.query(&plugin.id, "sqrt(144) + 2^3");
    assert_eq!(res_sqrt.len(), 1);
    assert_eq!(res_sqrt[0].title, "20");

    // 4. 选择动作 (触发剪贴板复制)
    mgr.select(&plugin.id, &item.id, &item.payload);
}
