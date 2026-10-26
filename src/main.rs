#![windows_subsystem = "windows"]

use one_launcher::cli::{self, CliArgs};
use one_launcher::config::ConfigManager;
use one_launcher::indexer::AppIndexManager;
use one_launcher::single_instance::SingleInstance;
use one_launcher::ui::run_launcher;
use tracing::{error, info};

fn main() {
    let cli_args = CliArgs::parse();

    #[cfg(windows)]
    if cli_args.is_cli_mode() || cli_args.debug {
        unsafe {
            use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS, GetStdHandle, SetStdHandle, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE};
            use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES, GetFileType, FILE_TYPE_PIPE, FILE_TYPE_DISK};
            use windows::core::w;

            let current_out = GetStdHandle(STD_OUTPUT_HANDLE).unwrap_or_default();
            let file_type = if !current_out.is_invalid() && !current_out.0.is_null() {
                GetFileType(current_out)
            } else {
                windows::Win32::Storage::FileSystem::FILE_TYPE(0)
            };

            // 若当前标准输出尚未重定向至管道或文件，则依附父进程控制台
            if file_type != FILE_TYPE_PIPE
                && file_type != FILE_TYPE_DISK
                && AttachConsole(ATTACH_PARENT_PROCESS).is_ok()
            {
                if let Ok(conout) = CreateFileW(
                    w!("CONOUT$"),
                    0x40000000, // GENERIC_WRITE
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                ) {
                    let _ = SetStdHandle(STD_OUTPUT_HANDLE, conout);
                    let _ = SetStdHandle(STD_ERROR_HANDLE, conout);
                }
            }
        }
    }

    // 1. 若处于纯命令行调试模式（例如传入 -input, -plugin, -help 且未显式指定 -gui），直接在控制台执行并退出，不绘制 UI
    if cli_args.is_cli_mode() {
        let code = cli::run_cli(&cli_args);
        std::process::exit(code);
    }

    // 2. GUI 模式：确保单实例运行
    let _instance = match SingleInstance::acquire("OneLauncher_Global_Single_Instance_Mutex") {
        Some(instance) => instance,
        None => {
            eprintln!("OneLauncher 已在后台运行中。");
            return;
        }
    };

    // 3. 加载配置管理器
    let config_mgr = match ConfigManager::init() {
        Ok(mgr) => mgr,
        Err(err) => {
            eprintln!("初始化配置失败: {}", err);
            return;
        }
    };

    let mut cfg = config_mgr.get();

    // 命令行参数覆盖
    if cli_args.debug {
        cfg.log.level = "debug".to_string();
        if cli_args.log_out.is_none() {
            cfg.log.path = "std".to_string();
        }
    }
    if let Some(ref lvl) = cli_args.log_level {
        cfg.log.level = lvl.clone();
    }
    if let Some(ref out) = cli_args.log_out {
        cfg.log.path = out.clone();
    }

    // 4. 初始化全局日志系统
    if let Err(err) = one_launcher::logger::init_logger(&cfg.log, config_mgr.path()) {
        eprintln!("初始化日志警告: {}", err);
    }

    info!("OneLauncher 正在启动...");
    info!("配置文件: {:?}", config_mgr.path());
    info!("全局热键: {}", cfg.general.hotkey);

    // 5. 初始化应用索引管理器
    let app_indexer = AppIndexManager::new(&cfg.apps);

    // 6. 运行启动器主窗口
    if let Err(err) = run_launcher(config_mgr, app_indexer) {
        error!("运行时发生错误: {}", err);
    }
}
