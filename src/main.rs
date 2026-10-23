mod config;

use config::ConfigManager;
use tracing::info;

fn main() {
    // 初始化日志跟踪系统
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("OneLauncher 启动中...");

    // 初始化配置管理器
    let config_mgr = match ConfigManager::init() {
        Ok(mgr) => mgr,
        Err(err) => {
            eprintln!("初始化配置失败: {}", err);
            return;
        }
    };

    let cfg = config_mgr.get();
    info!("当前全局呼出热键: {}", cfg.general.hotkey);
    info!("Everything 联动状态: {}", cfg.everything.enabled);
    info!("应用扫描是否包含 UWP: {}", cfg.apps.scan_uwp);
}
