use one_launcher::config::model::PluginConfig;
use one_launcher::plugin::PluginManager;
use std::collections::HashMap;

#[test]
fn test_system_plugin_queries_and_prefix_matching() {
    let mut configs = HashMap::new();
    configs.insert(
        "system".to_string(),
        PluginConfig {
            enabled: true,
            prefix: "sys:".to_string(),
            name: "系统控制".to_string(),
            path: None,
            entry: "index.js".to_string(),
            config: toml::Table::new(),
        },
    );

    let pm = PluginManager::init(&configs);

    // 1. 测试空输入查询: 返回全部 3 个系统控制指令 (quit, reload, edit)
    let all_items = pm.query("system", "");
    assert_eq!(all_items.len(), 3, "sys: 空输入时应展示全部 3 个候选动作");
    let ids: Vec<String> = all_items.iter().map(|item| item.id.clone()).collect();
    assert_eq!(ids, vec!["quit", "reload", "edit"]);

    // 验证所有条目的 badge 为必需字段
    for item in &all_items {
        assert_eq!(item.badge, "系统");
    }

    // 2. 测试前缀匹配 "q" -> 仅返回 quit
    let q_items = pm.query("system", "Q");
    assert_eq!(q_items.len(), 1);
    assert_eq!(q_items[0].id, "quit");
    assert!(q_items[0].title.contains("退出"));

    // 3. 测试前缀匹配 "re" -> 仅返回 reload
    let re_items = pm.query("system", "re");
    assert_eq!(re_items.len(), 1);
    assert_eq!(re_items[0].id, "reload");
    assert!(re_items[0].title.contains("重载"));

    // 4. 测试前缀匹配 "ed" -> 仅返回 edit
    let ed_items = pm.query("system", "ED");
    assert_eq!(ed_items.len(), 1);
    assert_eq!(ed_items[0].id, "edit");
    assert!(ed_items[0].title.contains("编辑"));

    // 5. 测试不存在的前缀 -> 返回空列表
    let none_items = pm.query("system", "xyz");
    assert_eq!(none_items.len(), 0);
}
