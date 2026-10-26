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

    // 1. 加载配置管理器
    let config_mgr = match ConfigManager::init() {
        Ok(mgr) => mgr,
        Err(err) => {
            eprintln!("初始化配置失败: {}", err);
            return;
        }
    };

    let cfg = config_mgr.get();

    // 2. 初始化全局日志系统
    if let Err(err) = one_launcher::logger::init_logger(&cfg.log, config_mgr.path()) {
        eprintln!("初始化日志警告: {}", err);
    }

    info!("OneLauncher 正在启动...");
    info!("配置文件: {:?}", config_mgr.path());
    info!("全局热键: {}", cfg.general.hotkey);

    // 3. 初始化应用索引管理器
    let app_indexer = AppIndexManager::new(&cfg.apps);

    // 4. 运行启动器主窗口
    if let Err(err) = run_launcher(config_mgr, app_indexer) {
        error!("运行时发生错误: {}", err);
    }
}
