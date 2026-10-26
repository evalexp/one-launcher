use one_launcher::config::model::PluginConfig;
use one_launcher::plugin::PluginManager;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_plugin_manager_queries_and_uninstall() {
    let temp_dir = std::env::temp_dir().join("one_launcher_pm_test");
    let _ = fs::create_dir_all(&temp_dir);
    let mock_config = temp_dir.join("config.toml");

    let initial_toml = r#"
[general]
hotkey = "Alt+Space"

[plugins.calc_mock]
enabled = true
prefix = "="
name = "计算器模拟"
path = "plugins/calc_mock"

[plugins.demo_plugin]
enabled = true
prefix = "demo:"
name = "演示插件"
path = "plugins/demo_plugin"
"#;
    fs::write(&mock_config, initial_toml).unwrap();

    let mut configs = HashMap::new();
    configs.insert(
        "pm".to_string(),
        PluginConfig {
            enabled: true,
            prefix: "pm:".to_string(),
            name: "插件管理器".to_string(),
            path: PathBuf::from("plugins/plugin-manager"),
            entry: "index.js".to_string(),
            config: toml::Table::new(),
        },
    );

    // 绑定 mock 配置文件路径初始化
    let pm = PluginManager::init_with_host(&configs, 0, Some(mock_config.clone()));

    // 1. 空输入测试: 返回管理向导
    let empty_items = pm.query("pm", "");
    assert_eq!(empty_items.len(), 2);
    let ids: Vec<String> = empty_items.iter().map(|i| i.id.clone()).collect();
    assert!(ids.contains(&"guide-install".to_string()));
    assert!(ids.contains(&"guide-uninstall".to_string()));

    // 2. 测试 install 解析 GitHub 仓库与子目录
    let install_items = pm.query("pm", "install github.com/evalexp/one-launcher-plugins/weather");
    assert_eq!(install_items.len(), 1);
    assert_eq!(install_items[0].id, "install-weather");
    assert_eq!(install_items[0].badge, "安装");
    assert!(install_items[0].subtitle.contains("evalexp/one-launcher-plugins/weather"));

    // 3. 测试 uninstall 列出所有可卸载插件
    let un_all = pm.query("pm", "uninstall");
    assert!(un_all.len() >= 2);
    let un_ids: Vec<String> = un_all.iter().map(|i| i.id.clone()).collect();
    assert!(un_ids.contains(&"uninstall-calc_mock".to_string()));
    assert!(un_ids.contains(&"uninstall-demo_plugin".to_string()));

    // 4. 测试 uninstall 按前缀匹配名字与 ID (如输入 "uninstall calc_mock" 或 "uninstall 演示")
    let un_calc = pm.query("pm", "uninstall calc_mock");
    assert_eq!(un_calc.len(), 1);
    assert_eq!(un_calc[0].id, "uninstall-calc_mock");

    let un_demo = pm.query("pm", "uninstall 演示");
    assert_eq!(un_demo.len(), 1);
    assert_eq!(un_demo[0].id, "uninstall-demo_plugin");

    // 5. 测试 uninstall 命中不存在的前缀
    let un_none = pm.query("pm", "uninstall non_existent_prefix");
    assert_eq!(un_none.len(), 1);
    assert_eq!(un_none[0].id, "no-plugins-found");

    // 6. 测试执行卸载动作 (模拟回车选中 uninstall-calc_mock)
    let payload = r#"{"action":"uninstall","pluginId":"calc_mock","pluginPath":"plugins/calc_mock"}"#;
    pm.select("pm", "uninstall-calc_mock", payload);

    // 验证 config.toml 中的 [plugins.calc_mock] 已被移除
    let updated_toml = fs::read_to_string(&mock_config).unwrap();
    assert!(!updated_toml.contains("[plugins.calc_mock]"), "卸载后配置文件中不应再包含该插件段");
    assert!(updated_toml.contains("[plugins.demo_plugin]"), "未卸载的插件仍应保留");

    let _ = fs::remove_dir_all(&temp_dir);
}
