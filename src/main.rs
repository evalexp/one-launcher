#![windows_subsystem = "windows"]

use one_launcher::config::ConfigManager;
use one_launcher::indexer::AppIndexManager;
use one_launcher::single_instance::SingleInstance;
use one_launcher::ui::run_launcher;
use tracing::{error, info};

fn main() {
    // 确保单实例运行
    let _instance = match SingleInstance::acquire("OneLauncher_Global_Single_Instance_Mutex") {
        Some(instance) => instance,
        None => {
            eprintln!("OneLauncher 已在后台运行中。");
            return;
        }
    };

    // 初始化日志记录
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("OneLauncher 正在启动...");

    // 1. 加载配置管理器
    let config_mgr = match ConfigManager::init() {
        Ok(mgr) => mgr,
        Err(err) => {
            error!("初始化配置失败: {}", err);
            return;
        }
    };

    let cfg = config_mgr.get();
    info!("配置加载完毕，全局呼出热键: {}", cfg.general.hotkey);

    // 2. 初始化应用索引管理器 (后台扫描开始菜单与系统应用)
    let app_indexer = AppIndexManager::new(&cfg.apps);

    // 3. 运行原生无边框极速窗口 (插件系统由专属后台 Worker 线程驱动，UI 线程完全解耦)
    if let Err(err) = run_launcher(config_mgr, app_indexer) {
        error!("运行时发生错误: {}", err);
    }
}
