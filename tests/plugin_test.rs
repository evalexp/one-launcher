use one_launcher::config::model::PluginConfig;
use one_launcher::plugin::PluginManager;
use std::collections::HashMap;
use std::fs;

#[test]
fn test_plugin_strict_lifecycle_and_default_config() {
    let temp_dir = std::env::temp_dir().join(format!("one_launcher_plugin_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);

    // 1. 创建合规插件 (包含 metadata, default_config, on_query, on_select)
    let valid_plugin_dir = temp_dir.join("valid_plugin");
    fs::create_dir_all(&valid_plugin_dir).unwrap();
    let valid_code = r#"
        export const metadata = {
            name: "有效测试插件",
            prefix: "test:",
            default_config: {
                multiplier: 10,
                author: "evalexp",
            }
        };

        export function on_query(input) {
            const mult = (launcher.config && launcher.config.multiplier) || 1;
            return [
                { id: "1", title: "Result: " + (Number(input) * mult), subtitle: "计算完成", is_folder: false, payload: "res" }
            ];
        }

        export function on_select(item) {
            // 选择条目
        }
    "#;
    fs::write(valid_plugin_dir.join("index.js"), valid_code).unwrap();

    // 2. 创建不合规插件 (缺少必需导出的 on_select)
    let invalid_plugin_dir = temp_dir.join("invalid_plugin");
    fs::create_dir_all(&invalid_plugin_dir).unwrap();
    let invalid_code = r#"
        export const metadata = {
            name: "缺少函数的不合规插件",
            prefix: "bad:",
        };
        export function on_query(input) {
            return [];
        }
        // 故意不导出 on_select
    "#;
    fs::write(invalid_plugin_dir.join("index.js"), invalid_code).unwrap();

    let mut plugins = HashMap::new();
    plugins.insert(
        "valid".to_string(),
        PluginConfig {
            enabled: true,
            prefix: "test:".to_string(),
            name: "测试插件".to_string(),
            path: valid_plugin_dir,
            entry: "index.js".to_string(),
            config: toml::Table::new(),
        },
    );
    plugins.insert(
        "invalid".to_string(),
        PluginConfig {
            enabled: true,
            prefix: "bad:".to_string(),
            name: "非法插件".to_string(),
            path: invalid_plugin_dir,
            entry: "index.js".to_string(),
            config: toml::Table::new(),
        },
    );

    let mgr = PluginManager::init(&plugins);

    // 验证不合规插件没有被加载
    assert!(mgr.match_prefix("bad: hello").is_none(), "缺少 on_select 的插件必须拒绝加载");

    // 验证合规插件正常加载并响应
    let matched = mgr.match_prefix("test: 5");
    assert!(matched.is_some(), "合规插件必须成功加载并匹配 prefix");
    let (p, query) = matched.unwrap();
    assert_eq!(p.id, "valid");
    assert_eq!(query, "5");

    let results = mgr.query(&p.id, query);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Result: 50"); // 5 * 10 (来自 default_config)

    let _ = fs::remove_dir_all(&temp_dir);
}
