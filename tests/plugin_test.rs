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

        let setupCalled = false;
        let testIconRef = "file";

        export function on_setup(context) {
            setupCalled = true;
            console.info("on_setup 测试通过, 当前 multiplier: " + (launcher.config && launcher.config.multiplier));
            testIconRef = "terminal";
        }

        export function on_query(input) {
            const mult = (launcher.config && launcher.config.multiplier) || 1;
            return [
                { id: "1", title: "Result: " + (Number(input) * mult), subtitle: "计算完成", icon: testIconRef, payload: { res: Number(input) * mult } }
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
    assert_eq!(results[0].icon, "terminal"); // 来自 on_setup 中的初始化赋值

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_search_result_item_badge() {
    use one_launcher::plugin::model::PluginResultItem;
    use one_launcher::ui::SearchResultItem;

    // 1. 文件夹结果
    let folder_item = PluginResultItem {
        id: "dir_1".to_string(),
        title: "MyFolder".to_string(),
        subtitle: r"C:\MyFolder".to_string(),
        icon: "folder".to_string(),
        badge: None,
        is_folder: true,
        payload: String::new(),
    };
    let res_folder = SearchResultItem::from_plugin("Everything 全盘搜索", "everything", folder_item);
    assert_eq!(res_folder.badge, "文件夹");

    // 2. 文件结果 (即使来自 Everything 插件，也应显示为“文件”)
    let file_item = PluginResultItem {
        id: "file_1".to_string(),
        title: "test.txt".to_string(),
        subtitle: r"C:\test.txt".to_string(),
        icon: "file".to_string(),
        badge: None,
        is_folder: false,
        payload: String::new(),
    };
    let res_file = SearchResultItem::from_plugin("Everything 全盘搜索", "everything", file_item);
    assert_eq!(res_file.badge, "文件");

    // 3. 通用插件条目 (如计算器，未传 badge 时回退展示插件名称)
    let calc_item = PluginResultItem {
        id: "calc_1".to_string(),
        title: "42".to_string(),
        subtitle: "= 6 * 7".to_string(),
        icon: "calc".to_string(),
        badge: None,
        is_folder: false,
        payload: "42".to_string(),
    };
    let res_calc = SearchResultItem::from_plugin("极速计算器", "calculator", calc_item);
    assert_eq!(res_calc.badge, "极速计算器");

    // 4. 自定义 badge
    let custom_item = PluginResultItem {
        id: "cust_1".to_string(),
        title: "自定义".to_string(),
        subtitle: "描述".to_string(),
        icon: "calc".to_string(),
        badge: Some("公式求值".to_string()),
        is_folder: false,
        payload: String::new(),
    };
    let res_custom = SearchResultItem::from_plugin("极速计算器", "calculator", custom_item);
    assert_eq!(res_custom.badge, "公式求值");
}
