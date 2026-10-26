use one_launcher::config::model::LogConfig;
use one_launcher::logger::{init_logger, parse_log_destination, LogDestination};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

#[test]
fn test_parse_log_destination_std_and_fs() {
    let base_config = Path::new("D:\\app\\config.toml");

    // 1. 测试 "std" 与大小写忽略
    assert_eq!(parse_log_destination(base_config, "std"), LogDestination::Std);
    assert_eq!(parse_log_destination(base_config, "STD"), LogDestination::Std);
    assert_eq!(parse_log_destination(base_config, "stdout"), LogDestination::Std);

    // 2. 测试 "fs:<path>" 相对路径映射至配置文件所在目录
    let dest_fs = parse_log_destination(base_config, "fs:log/one-launcher.log");
    assert_eq!(
        dest_fs,
        LogDestination::File(PathBuf::from("D:\\app\\log\\one-launcher.log"))
    );

    // 3. 测试带 fs: 前缀的无后缀目录自动追加默认文件名
    let dest_dir = parse_log_destination(base_config, "fs:logs");
    assert_eq!(
        dest_dir,
        LogDestination::File(PathBuf::from("D:\\app\\logs\\one-launcher.log"))
    );
}

#[test]
fn test_logger_file_output_and_silent_level() {
    let temp_dir = std::env::temp_dir().join(format!("one_launcher_log_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let log_file = temp_dir.join("sub_dir").join("test-launcher.log");
    let dummy_config_path = temp_dir.join("config.toml");

    // 1. 测试 fs: 目标输出与 debug 级别日志写入
    let log_config = LogConfig {
        level: "debug".to_string(),
        path: format!("fs:{}", log_file.display()),
    };

    let resolved_dest = init_logger(&log_config, &dummy_config_path).expect("初始化日志应成功");
    assert_eq!(resolved_dest, LogDestination::File(log_file.clone()));

    info!("这是测试信息日志: Info Message 12345");
    warn!("这是测试警告日志: Warn Message 67890");
    debug!("这是测试调试日志: Debug Message ABCDE");

    assert!(log_file.exists(), "日志文件必须被正确创建: {:?}", log_file);
    let log_content = fs::read_to_string(&log_file).unwrap_or_default();
    assert!(log_content.contains("Info Message 12345"), "日志文件必须包含 info 记录");
    assert!(log_content.contains("Warn Message 67890"), "日志文件必须包含 warn 记录");
    assert!(log_content.contains("Debug Message ABCDE"), "日志文件必须包含 debug 记录");

    // 2. 测试 silent 级别解析
    let silent_config = LogConfig {
        level: "silent".to_string(),
        path: "std".to_string(),
    };
    let silent_dest = init_logger(&silent_config, &dummy_config_path).expect("初始化静音模式应成功");
    assert_eq!(silent_dest, LogDestination::Std);
}
