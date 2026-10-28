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

        export function on_setup() {
            setupCalled = true;
            const cfg = self.get_config();
            console.info("on_setup 测试通过, 当前 multiplier: " + (cfg && cfg.multiplier));
            testIconRef = "terminal";
        }

        export function on_query(input) {
            const mult = (self.get_config() && self.get_config().multiplier) || 1;
            return [
                { id: "1", title: "Result: " + (Number(input) * mult), subtitle: "计算完成", icon: testIconRef, badge: "乘法测试", payload: { res: Number(input) * mult } }
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
            path: Some(valid_plugin_dir),
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
            path: Some(invalid_plugin_dir),
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
    assert_eq!(results[0].badge, "乘法测试"); // 来自 on_query 返回的 badge 字段

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_plugin_context_isolation_and_shared_launcher_apis() {
    let temp_dir = std::env::temp_dir().join(format!("one_launcher_isolation_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);

    // 插件 A: 具有独立 config a_val = 111, 定义全局变量 leak_test = "plugin_A"
    let p_a_dir = temp_dir.join("plugin_a");
    fs::create_dir_all(&p_a_dir).unwrap();
    let code_a = r#"
        globalThis.leak_test = "plugin_A";
        export const metadata = {
            name: "插件A",
            prefix: "a:",
            default_config: { a_val: 111 }
        };
        export function on_setup() {
            // 验证 launcher 保持为纯净共享 API，不携带私有 config；验证 context 不存在，仅保留 launcher 与 self
            if (typeof launcher.config !== "undefined") {
                throw new Error("launcher.config 必须为 undefined");
            }
            if (typeof context !== "undefined") {
                throw new Error("context 必须为 undefined，全局仅保留 launcher 与 self");
            }
        }
        export function on_query(input) {
            const cfg = self.get_config();
            return [{
                id: "item_a",
                title: "A:" + cfg.a_val + ":" + globalThis.leak_test,
                subtitle: "self:" + self.get_id(),
                icon: "file",
                badge: "A",
                payload: {}
            }];
        }
        export function on_select(item) {}
    "#;
    fs::write(p_a_dir.join("index.js"), code_a).unwrap();

    // 插件 B: 具有独立 config b_val = 222, 验证 globalThis.leak_test 不存在
    let p_b_dir = temp_dir.join("plugin_b");
    fs::create_dir_all(&p_b_dir).unwrap();
    let code_b = r#"
        export const metadata = {
            name: "插件B",
            prefix: "b:",
            default_config: { b_val: 222 }
        };
        export function on_setup() {}
        export function on_query(input) {
            const cfg = self.get_config();
            const leaked = typeof globalThis.leak_test !== "undefined" ? globalThis.leak_test : "none";
            return [{
                id: "item_b",
                title: "B:" + cfg.b_val + ":" + leaked,
                subtitle: "self:" + self.get_id(),
                icon: "file",
                badge: "B",
                payload: {}
            }];
        }
        export function on_select(item) {}
    "#;
    fs::write(p_b_dir.join("index.js"), code_b).unwrap();

    let mut plugins = HashMap::new();
    plugins.insert("p_a".to_string(), PluginConfig {
        enabled: true,
        prefix: "a:".to_string(),
        name: "插件A".to_string(),
        path: Some(p_a_dir),
        entry: "index.js".to_string(),
        config: toml::Table::new(),
    });
    plugins.insert("p_b".to_string(), PluginConfig {
        enabled: true,
        prefix: "b:".to_string(),
        name: "插件B".to_string(),
        path: Some(p_b_dir),
        entry: "index.js".to_string(),
        config: toml::Table::new(),
    });

    let mgr = PluginManager::init(&plugins);

    // 查询 A
    let res_a = mgr.query("p_a", "");
    assert_eq!(res_a.len(), 1);
    assert_eq!(res_a[0].title, "A:111:plugin_A");
    assert_eq!(res_a[0].subtitle, "self:p_a");

    // 查询 B: 验证完全隔离，未被 A 的全局变量污染，且获取自己的 config 222
    let res_b = mgr.query("p_b", "");
    assert_eq!(res_b.len(), 1);
    assert_eq!(res_b[0].title, "B:222:none");
    assert_eq!(res_b[0].subtitle, "self:p_b");

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
        badge: "文件夹".to_string(),
        payload: String::new(),
    };
    let res_folder = SearchResultItem::from_plugin("Everything 全盘搜索", "everything", folder_item);
    assert_eq!(res_folder.badge, "文件夹");

    // 2. 文件结果
    let file_item = PluginResultItem {
        id: "file_1".to_string(),
        title: "test.txt".to_string(),
        subtitle: r"C:\test.txt".to_string(),
        icon: "file".to_string(),
        badge: "文件".to_string(),
        payload: String::new(),
    };
    let res_file = SearchResultItem::from_plugin("Everything 全盘搜索", "everything", file_item);
    assert_eq!(res_file.badge, "文件");

    // 3. 通用插件条目 (如未提供 badge，回退展示插件名称)
    let calc_item = PluginResultItem {
        id: "calc_1".to_string(),
        title: "42".to_string(),
        subtitle: "= 6 * 7".to_string(),
        icon: "calc".to_string(),
        badge: String::new(),
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
        badge: "公式求值".to_string(),
        payload: String::new(),
    };
    let res_custom = SearchResultItem::from_plugin("极速计算器", "calculator", custom_item);
    assert_eq!(res_custom.badge, "公式求值");
}
