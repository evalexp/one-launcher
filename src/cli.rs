use crate::config::model::PluginConfig;
use crate::config::ConfigManager;
use crate::plugin::{PluginResultItem, PluginManager};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct CliArgs {
    pub debug: bool,
    pub plugin: Option<String>,
    pub input: Option<String>,
    pub select: Option<String>,
    pub log_out: Option<String>,
    pub log_level: Option<String>,
    pub config_path: Option<PathBuf>,
    pub format: String,
    pub gui: bool,
    pub help: bool,
}

impl CliArgs {
    pub fn parse() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        Self::parse_from(&args)
    }

    pub fn parse_from(args: &[String]) -> Self {
        let mut cli = CliArgs {
            format: "text".to_string(),
            ..Default::default()
        };

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            let lower = arg.to_lowercase();
            match lower.as_str() {
                "-debug" | "--debug" | "-d" => {
                    cli.debug = true;
                }
                "-help" | "--help" | "-h" | "/?" => {
                    cli.help = true;
                }
                "-gui" | "--gui" => {
                    cli.gui = true;
                }
                "-plugin" | "--plugin" | "-p" => {
                    if i + 1 < args.len() {
                        cli.plugin = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "-input" | "--input" | "-i" | "-q" | "--query" => {
                    if i + 1 < args.len() {
                        cli.input = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "-select" | "--select" | "-s" => {
                    if i + 1 < args.len() {
                        cli.select = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "-log-out" | "--log-out" | "-log-dest" | "--log-dest" | "-l" => {
                    if i + 1 < args.len() {
                        cli.log_out = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "-log-level" | "--log-level" => {
                    if i + 1 < args.len() {
                        cli.log_level = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "-config" | "--config" | "-c" => {
                    if i + 1 < args.len() {
                        cli.config_path = Some(PathBuf::from(&args[i + 1]));
                        i += 1;
                    }
                }
                "-format" | "--format" | "-f" => {
                    if i + 1 < args.len() {
                        cli.format = args[i + 1].to_lowercase();
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }

        cli
    }

    /// 判断当前是否处于纯命令行调试模式（无需绘制 GUI 界面）
    pub fn is_cli_mode(&self) -> bool {
        self.help || (self.input.is_some() && !self.gui) || (self.plugin.is_some() && !self.gui)
    }
}

/// 解析单独指定的插件目标（支持配置名、插件目录、入口脚本文件等）
pub fn resolve_single_plugin(
    target: &str,
    existing: &HashMap<String, PluginConfig>,
) -> HashMap<String, PluginConfig> {
    let mut map = HashMap::new();

    // 1. 如果已在 config.toml 中配置
    if let Some(cfg) = existing.get(target) {
        let mut c = cfg.clone();
        c.enabled = true;
        map.insert(target.to_string(), c);
        return map;
    }

    // 2. 检查是否为磁盘上的绝对路径或相对路径
    let target_path = PathBuf::from(target);
    if target_path.exists() {
        if target_path.is_file() {
            let parent = target_path.parent().unwrap_or(Path::new("."));
            let entry = target_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("index.js")
                .to_string();
            let id = parent
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("isolated_plugin")
                .to_string();
            map.insert(
                id,
                PluginConfig {
                    enabled: true,
                    prefix: String::new(),
                    name: String::new(),
                    path: parent.to_path_buf(),
                    entry,
                    config: toml::Table::new(),
                },
            );
            return map;
        } else if target_path.is_dir() {
            let id = target_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("isolated_plugin")
                .to_string();
            map.insert(
                id,
                PluginConfig {
                    enabled: true,
                    prefix: String::new(),
                    name: String::new(),
                    path: target_path.clone(),
                    entry: "index.js".to_string(),
                    config: toml::Table::new(),
                },
            );
            return map;
        }
    }

    // 3. 检查当前工作目录下的 plugins/<target>
    let candidate = PathBuf::from("plugins").join(target);
    if candidate.exists() {
        map.insert(
            target.to_string(),
            PluginConfig {
                enabled: true,
                prefix: String::new(),
                name: String::new(),
                path: candidate,
                entry: "index.js".to_string(),
                config: toml::Table::new(),
            },
        );
        return map;
    }

    // 4. 检查 %APPDATA%/OneLauncher/plugins/<target>
    if let Some(app_data) = dirs::config_dir() {
        let candidate2 = app_data.join("OneLauncher").join("plugins").join(target);
        if candidate2.exists() {
            map.insert(
                target.to_string(),
                PluginConfig {
                    enabled: true,
                    prefix: String::new(),
                    name: String::new(),
                    path: candidate2,
                    entry: "index.js".to_string(),
                    config: toml::Table::new(),
                },
            );
            return map;
        }
    }

    map
}

/// 打印命令行使用帮助指南
pub fn print_help() {
    println!(
        r#"OneLauncher - Developer & LLM Diagnostic CLI

Usage:
  one-launcher [options]

Options:
  -plugin, --plugin, -p <target>      指定单独加载的插件路径或名称（完全隔离调试）
  -input, --input, -i, -q <query>     指定查询指令字符串（如 "sys:"、"clash: update"）
  -select, --select, -s <id|index>    查询后自动触发执行指定条目的 on_select 动作
  -debug, --debug, -d                 开启调试模式（日志级别设为 debug，输出至控制台）
  -log-out, --log-out, -l <dest>      指定日志输出目标: 'std' (终端) 或 'fs:<path>' (文件)
  -log-level, --log-level <level>     指定日志级别: trace, debug, info, warn, error, silent
  -format, --format, -f <text|json>   结果输出格式: text (默认易读文本) 或 json (结构化)
  -config, --config, -c <path>        指定自定义配置文件路径
  -gui, --gui                         强制以 GUI 窗口模式启动（在界面中展示结果）
  -help, --help, -h                   显示本帮助信息并退出

Examples:
  # 单独测试系统控制插件空输入查询 (返回 quit, reload, edit)
  one-launcher -plugin system -input ""

  # 单独测试 Clash 控制器插件更新订阅并查看日志
  one-launcher -debug -plugin clash -input "update"

  # 查询并直接执行选择动作 (测试 on_select)
  one-launcher -plugin clash -input "clash:" -select update-subscription

  # 输出结构化 JSON 供脚本/大模型自动化解析
  one-launcher -plugin system -input "sys:" -format json
"#
    );
}

/// 打印格式化后的查询结果（与日志明确区分）
pub fn print_results(
    results: &[PluginResultItem],
    plugin_id: &str,
    plugin_name: &str,
    prefix: &str,
    input: &str,
    format: &str,
) {
    if format == "json" {
        println!("======================== ONELAUNCHER RESULTS (JSON) ========================");
        let mut json_str = String::new();
        json_str.push_str("{\n");
        json_str.push_str(&format!(
            "  \"plugin\": {{ \"id\": \"{}\", \"name\": \"{}\", \"prefix\": \"{}\" }},\n",
            plugin_id, plugin_name, prefix
        ));
        json_str.push_str(&format!("  \"input\": \"{}\",\n", input.replace('"', "\\\"")));
        json_str.push_str(&format!("  \"count\": {},\n", results.len()));
        json_str.push_str("  \"items\": [\n");
        for (i, item) in results.iter().enumerate() {
            json_str.push_str("    {\n");
            json_str.push_str(&format!("      \"id\": \"{}\",\n", item.id.replace('"', "\\\"")));
            json_str.push_str(&format!("      \"title\": \"{}\",\n", item.title.replace('"', "\\\"")));
            json_str.push_str(&format!("      \"subtitle\": \"{}\",\n", item.subtitle.replace('"', "\\\"")));
            json_str.push_str(&format!("      \"badge\": \"{}\",\n", item.badge.replace('"', "\\\"")));
            json_str.push_str(&format!("      \"icon\": \"{}\",\n", item.icon.replace('"', "\\\"")));
            let payload_fmt = if item.payload.starts_with('{') || item.payload.starts_with('[') {
                item.payload.as_str()
            } else {
                "\"\""
            };
            json_str.push_str(&format!("      \"payload\": {}\n", payload_fmt));
            json_str.push_str("    }");
            if i + 1 < results.len() {
                json_str.push(',');
            }
            json_str.push('\n');
        }
        json_str.push_str("  ]\n}\n");
        println!("{}", json_str);
        println!("============================================================================");
    } else {
        println!("======================== ONELAUNCHER RESULTS ========================");
        println!("Plugin:   {} (id: {}, prefix: '{}')", plugin_name, plugin_id, prefix);
        println!("Input:    \"{}\"", input);
        println!("Count:    {} item(s)", results.len());
        println!();
        if results.is_empty() {
            println!("  (No matching results)");
        } else {
            for (i, item) in results.iter().enumerate() {
                println!("  [{}] {}", i, item.title);
                println!("      Subtitle: {}", item.subtitle);
                println!("      Badge:    {}", item.badge);
                println!("      Icon:     {}", item.icon);
                println!("      Id:       {}", item.id);
                println!("      Payload:  {}", item.payload);
                println!();
            }
        }
        println!("=====================================================================");
    }
}

/// 执行纯命令行诊断模式（无 UI 渲染）
pub fn run_cli(cli: &CliArgs) -> i32 {
    if cli.help {
        print_help();
        return 0;
    }

    // 1. 读取配置文件（支持自定义路径覆盖）
    let config_path = cli
        .config_path
        .clone()
        .unwrap_or_else(ConfigManager::detect_config_path);
    let mut cfg = ConfigManager::read_config_from_file(&config_path).unwrap_or_default();

    // 2. 配置日志级别与输出目标
    if let Some(ref lvl) = cli.log_level {
        cfg.log.level = lvl.clone();
    } else if cli.debug {
        cfg.log.level = "debug".to_string();
    }

    if let Some(ref out) = cli.log_out {
        cfg.log.path = out.clone();
    } else {
        // CLI 模式默认输出至终端控制台
        cfg.log.path = "std".to_string();
    }

    let _ = crate::logger::init_logger(&cfg.log, &config_path);

    // 3. 确定需要加载的插件集合（若指定了 -plugin 则完全隔离单独加载）
    let target_plugins = if let Some(ref plugin_target) = cli.plugin {
        resolve_single_plugin(plugin_target, &cfg.plugins)
    } else {
        cfg.plugins.clone()
    };

    if target_plugins.is_empty() {
        eprintln!("======================== ONELAUNCHER ERROR ========================");
        eprintln!("未找到可加载的插件。请通过 -plugin 指定插件路径或检查配置文件。");
        eprintln!("===================================================================");
        return 1;
    }

    // 4. 初始化插件运行时环境
    let pm = PluginManager::init(&target_plugins);

    if pm.plugins.is_empty() {
        eprintln!("======================== ONELAUNCHER ERROR ========================");
        eprintln!("指定的目标插件初始化失败或未找到有效入口文件。");
        eprintln!("===================================================================");
        return 1;
    }

    // 5. 确定查询内容与目标插件
    let raw_input = cli.input.as_deref().unwrap_or("");

    let (target_plugin_id, query_str) = if let Some(_) = cli.plugin {
        // 单独加载该插件时，优先直接将 input 派发给它
        let p = &pm.plugins[0];
        let stripped = if !p.prefix.is_empty() && raw_input.starts_with(&p.prefix) {
            raw_input[p.prefix.len()..].trim_start()
        } else {
            raw_input
        };
        (p.id.clone(), stripped)
    } else if let Some((plugin, remainder)) = pm.match_prefix(raw_input) {
        (plugin.id.clone(), remainder)
    } else if pm.plugins.len() == 1 {
        (pm.plugins[0].id.clone(), raw_input)
    } else {
        eprintln!("======================== ONELAUNCHER ERROR ========================");
        eprintln!("输入 '{}' 未匹配到任何已加载插件的前缀。", raw_input);
        eprintln!("已加载的插件列表:");
        for p in &pm.plugins {
            eprintln!("  - {} (id: {}, prefix: '{}')", p.name, p.id, p.prefix);
        }
        eprintln!("===================================================================");
        return 1;
    };

    let target_plugin_name = pm.get_plugin_name(&target_plugin_id);
    let target_prefix = pm
        .plugins
        .iter()
        .find(|p| p.id == target_plugin_id)
        .map(|p| p.prefix.as_str())
        .unwrap_or("");

    // 6. 执行 on_query
    let results = pm.query(&target_plugin_id, query_str);

    // 7. 打印查询结果
    print_results(
        &results,
        &target_plugin_id,
        &target_plugin_name,
        target_prefix,
        query_str,
        &cli.format,
    );

    // 8. 若指定了 -select，触发执行 on_select
    if let Some(ref sel_target) = cli.select {
        let selected_item = if let Ok(idx) = sel_target.parse::<usize>() {
            results.get(idx).cloned()
        } else {
            results
                .iter()
                .find(|item| item.id.eq_ignore_ascii_case(sel_target))
                .cloned()
        };

        if let Some(item) = selected_item {
            println!("\n======================== ONELAUNCHER SELECT ========================");
            println!("Target Item:  [{}] \"{}\"", item.id, item.title);
            println!("Payload:      {}", item.payload);
            println!("--------------------------------------------------------------------");
            pm.select(&target_plugin_id, &item.id, &item.payload);
            println!("--------------------------------------------------------------------");
            println!("Action execution completed.");
            println!("====================================================================");
        } else {
            eprintln!("\n======================== ONELAUNCHER SELECT ERROR ==================");
            eprintln!(
                "未能在查询结果中找到匹配 '{}' 的条目 (可选序号 0..{})。",
                sel_target,
                results.len().saturating_sub(1)
            );
            eprintln!("====================================================================");
            return 1;
        }
    }

    0
}
