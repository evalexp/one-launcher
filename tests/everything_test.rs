use one_launcher::config::model::PluginConfig;
use one_launcher::plugin::PluginManager;
use std::collections::HashMap;

#[test]
fn test_everything_plugin_query() {
    let mut plugins = HashMap::new();
    plugins.insert(
        "everything".to_string(),
        PluginConfig {
            enabled: true,
            prefix: "fs:".to_string(),
            name: "Everything 全盘搜索".to_string(),
            path: None,
            entry: "index.js".to_string(),
            config: toml::Table::new(),
        },
    );

    let mgr = PluginManager::init(&plugins);

    // 1. 验证 prefix 路由严格匹配
    let matched = mgr.match_prefix("fs: cargo.exe");
    assert!(matched.is_some(), "应能成功匹配 fs: 前缀");
    let (plugin, pure_query) = matched.unwrap();
    assert_eq!(plugin.id, "everything");
    assert_eq!(pure_query, "cargo.exe");

    // 2. 验证非前缀输入不会命中该插件
    assert!(mgr.match_prefix("cargo.exe").is_none());

    // 3. 执行插件检索
    let results = mgr.query(&plugin.id, pure_query);
    println!("Everything 插件通过 QuickJS + Native 动态库查询返回条数: {}", results.len());
    for item in results.iter().take(3) {
        println!("  - [{}] {} ({})", item.badge, item.title, item.subtitle);
    }
}
