use one_launcher::config::model::PluginConfig;
use one_launcher::plugin::PluginManager;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::thread;

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
            path: Some(PathBuf::from("plugins/plugin-manager")),
            entry: "index.js".to_string(),
            config: toml::Table::new(),
        },
    );

    // 绑定 mock 配置文件路径初始化
    let pm = PluginManager::init_with_host(&configs, 0, Some(mock_config.clone()));

    // 1. 空输入测试: 返回四大管理向导 (install, enable, disable, uninstall)
    let empty_items = pm.query("pm", "");
    assert_eq!(empty_items.len(), 4);
    let ids: Vec<String> = empty_items.iter().map(|i| i.id.clone()).collect();
    assert!(ids.contains(&"guide-install".to_string()));
    assert!(ids.contains(&"guide-enable".to_string()));
    assert!(ids.contains(&"guide-disable".to_string()));
    assert!(ids.contains(&"guide-uninstall".to_string()));

    // 2. 测试 install 解析 GitHub index.js 地址
    let install_items = pm.query("pm", "install github.com/evalexp/one-launcher-plugins/weather/index.js");
    assert_eq!(install_items.len(), 1);
    assert_eq!(install_items[0].id, "install-weather");
    assert_eq!(install_items[0].badge, "安装");
    assert!(install_items[0].subtitle.contains("weather/index.js"));

    // 3. 测试 uninstall 列出所有可卸载插件
    let un_all = pm.query("pm", "uninstall");
    assert!(un_all.len() >= 2);
    let un_ids: Vec<String> = un_all.iter().map(|i| i.id.clone()).collect();
    assert!(un_ids.contains(&"uninstall-calc_mock".to_string()));
    assert!(un_ids.contains(&"uninstall-demo_plugin".to_string()));

    // 4. 测试 uninstall 按前缀匹配名字与 ID
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

    // 7. 测试 disable 与 enable
    let dis_payload = r#"{"action":"disable","pluginId":"demo_plugin","pluginName":"演示插件"}"#;
    pm.select("pm", "disable-demo_plugin", dis_payload);
    let disabled_toml = fs::read_to_string(&mock_config).unwrap();
    assert!(disabled_toml.contains("enabled = false"));

    let en_payload = r#"{"action":"enable","pluginId":"demo_plugin","pluginName":"演示插件"}"#;
    pm.select("pm", "enable-demo_plugin", en_payload);
    let enabled_toml = fs::read_to_string(&mock_config).unwrap();
    assert!(enabled_toml.contains("enabled = true"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_plugin_manager_install_with_dependencies_and_on_setup() {
    // 搭建测试本地 HTTP 服务器，模拟 GitHub 托管的远程插件仓库
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);

            if req.contains("index.js") {
                let code = r#"
export const metadata = {
  name: "测试天气插件",
  prefix: "wt:",
  dynamic_library: {
    WeatherLib: { path: "./weather.dll" }
  }
};

export function on_setup() {
  launcher.cache_icon("./weather_icon.png");
}

export function on_query(input) { return []; }
export function on_select(item) {}
"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    code.len(),
                    code
                );
                let _ = stream.write_all(resp.as_bytes());
            } else if req.contains("weather.dll") {
                let dll_data = b"MOCK_DLL_BINARY_DATA";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    dll_data.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.write_all(dll_data);
            } else if req.contains("weather_icon.png") {
                let png_data = b"MOCK_PNG_IMAGE_DATA";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    png_data.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.write_all(png_data);
            }
            let _ = stream.flush();
        }
    });

    let temp_dir = std::env::temp_dir().join("one_launcher_pm_install_test");
    let _ = fs::create_dir_all(&temp_dir);
    let mock_config = temp_dir.join("config.toml");
    fs::write(&mock_config, "[general]\nhotkey = \"Alt+Space\"\n").unwrap();

    let mut configs = HashMap::new();
    configs.insert(
        "pm".to_string(),
        PluginConfig {
            enabled: true,
            prefix: "pm:".to_string(),
            name: "插件管理器".to_string(),
            path: Some(PathBuf::from("plugins/plugin-manager")),
            entry: "index.js".to_string(),
            config: toml::Table::new(),
        },
    );

    let pm = PluginManager::init_with_host(&configs, 0, Some(mock_config.clone()));

    // 触发安装流程，传入由本地测试服务器托管的 index.js
    let raw_url = format!("http://127.0.0.1:{}/repo/weather/index.js", port);
    let base_url = format!("http://127.0.0.1:{}/repo/weather/", port);

    let payload = format!(
        r#"{{
        "action": "install",
        "parsed": {{
            "owner": "test_owner",
            "repo": "test_repo",
            "pluginId": "weather_test",
            "rawUrl": "{}",
            "baseUrl": "{}"
        }}
    }}"#,
        raw_url, base_url
    );

    pm.select("pm", "install-weather_test", &payload);

    // 验证安装结果:
    // 1. config.toml 中成功注册 [plugins.weather_test]
    let updated_toml = fs::read_to_string(&mock_config).unwrap();
    assert!(updated_toml.contains("[plugins.weather_test]"));
    assert!(updated_toml.contains(r#"prefix = "wt:""#));
    assert!(updated_toml.contains(r#"name = "测试天气插件""#));

    // 2. 本地 plugins/weather_test 目录下载完整：index.js, weather.dll, weather_icon.png
    let dest_dir = PathBuf::from("plugins/weather_test");
    assert!(dest_dir.join("index.js").exists(), "index.js 必须存在");
    assert!(dest_dir.join("weather.dll").exists(), "metadata.dynamic_library 中的 dll 必须成功下载");
    assert!(dest_dir.join("weather_icon.png").exists(), "on_setup(context.cache_icon) 下载的图标必须成功保存");

    // 验证文件内容
    assert_eq!(fs::read(dest_dir.join("weather.dll")).unwrap(), b"MOCK_DLL_BINARY_DATA");
    assert_eq!(fs::read(dest_dir.join("weather_icon.png")).unwrap(), b"MOCK_PNG_IMAGE_DATA");

    // 清理创建的测试产物
    let _ = fs::remove_dir_all(&dest_dir);
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_plugin_manager_url_resolver_and_native_metadata_download() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);

            if req.contains("index.js") {
                let code = r#"
export const metadata = {
  name: "测试原生插件",
  prefix: "np:",
  native: {
    Core: { path: "./native_core.dll" }
  }
};

export function on_setup() {
  launcher.cache_icon("./logo.png");
}

export function on_query(input) { return []; }
export function on_select(item) {}
"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/javascript\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    code.len(),
                    code
                );
                let _ = stream.write_all(resp.as_bytes());
            } else if req.contains("native_core.dll") {
                let dll_data = b"MOCK_NATIVE_DLL_DATA";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    dll_data.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.write_all(dll_data);
            } else if req.contains("logo.png") {
                let logo_data = b"MOCK_LOGO_PNG_DATA";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    logo_data.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.write_all(logo_data);
            }
            let _ = stream.flush();
        }
    });

    let temp_dir = std::env::temp_dir().join("one_launcher_pm_resolver_test");
    let _ = fs::create_dir_all(&temp_dir);
    let mock_config = temp_dir.join("config.toml");
    fs::write(&mock_config, "[general]\nhotkey = \"Alt+Space\"\n").unwrap();

    let mut configs = HashMap::new();
    configs.insert(
        "pm".to_string(),
        PluginConfig {
            enabled: true,
            prefix: "pm:".to_string(),
            name: "插件管理器".to_string(),
            path: Some(PathBuf::from("plugins/plugin-manager")),
            entry: "index.js".to_string(),
            config: toml::Table::new(),
        },
    );

    let pm = PluginManager::init_with_host(&configs, 0, Some(mock_config.clone()));

    // 1. 测试 Gitee URL 解析查询
    let gitee_items = pm.query("pm", "install gitee.com/evalexp/my-tool/raw/master/index.js");
    assert_eq!(gitee_items.len(), 1);
    assert_eq!(gitee_items[0].id, "install-my-tool");
    assert!(gitee_items[0].subtitle.contains("gitee.com/evalexp/my-tool/raw/master/index.js"));

    // 2. 测试 Direct HTTP URL 解析查询
    let direct_url = format!("http://127.0.0.1:{}/native_plugin/index.js", port);
    let direct_items = pm.query("pm", &format!("install {}", direct_url));
    assert_eq!(direct_items.len(), 1);
    assert_eq!(direct_items[0].id, "install-native_plugin");

    // 3. 执行安装Direct HTTP URL插件（包含 metadata.native 与 parameterless launcher.cache_icon）
    let raw_url = format!("http://127.0.0.1:{}/native_plugin/index.js", port);
    let base_url = format!("http://127.0.0.1:{}/native_plugin/", port);
    let payload = format!(
        r#"{{
        "action": "install",
        "parsed": {{
            "pluginId": "native_test",
            "rawUrl": "{}",
            "baseUrl": "{}"
        }}
    }}"#,
        raw_url, base_url
    );

    pm.select("pm", "install-native_test", &payload);

    // 4. 验证安装产物与配置
    let updated_toml = fs::read_to_string(&mock_config).unwrap();
    assert!(updated_toml.contains("[plugins.native_test]"));
    assert!(updated_toml.contains(r#"prefix = "np:""#));
    assert!(updated_toml.contains(r#"name = "测试原生插件""#));

    let dest_dir = PathBuf::from("plugins/native_test");
    assert!(dest_dir.join("index.js").exists(), "index.js 必须存在");
    assert!(dest_dir.join("native_core.dll").exists(), "metadata.native 中的 dll 必须成功下载");
    assert!(dest_dir.join("logo.png").exists(), "on_setup(launcher.cache_icon) 拦截的图标必须成功保存");

    assert_eq!(fs::read(dest_dir.join("native_core.dll")).unwrap(), b"MOCK_NATIVE_DLL_DATA");
    assert_eq!(fs::read(dest_dir.join("logo.png")).unwrap(), b"MOCK_LOGO_PNG_DATA");

    let _ = fs::remove_dir_all(&dest_dir);
    let _ = fs::remove_dir_all(&temp_dir);
}
