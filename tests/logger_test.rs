use one_launcher::config::model::LogConfig;
use one_launcher::logger::init_logger;
use std::fs;
use tracing::{debug, info, warn};

#[test]
fn test_logger_file_output_and_level() {
    let temp_dir = std::env::temp_dir().join(format!("one_launcher_log_test_{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let log_file = temp_dir.join("sub_dir").join("test-launcher.log");
    let dummy_config_path = temp_dir.join("config.toml");

    let log_config = LogConfig {
        level: "debug".to_string(),
        path: log_file.clone(),
    };

    let resolved_path = init_logger(&log_config, &dummy_config_path).expect("初始化日志应成功");
    assert_eq!(resolved_path, log_file);

    info!("这是测试信息日志: Info Message 12345");
    warn!("这是测试警告日志: Warn Message 67890");
    debug!("这是测试调试日志: Debug Message ABCDE");

    // 检查日志文件是否已生成并包含内容
    assert!(log_file.exists(), "日志文件必须被正确创建: {:?}", log_file);
    let log_content = fs::read_to_string(&log_file).unwrap_or_default();
    assert!(log_content.contains("Info Message 12345"), "日志文件必须包含 info 记录");
    assert!(log_content.contains("Warn Message 67890"), "日志文件必须包含 warn 记录");
    assert!(log_content.contains("Debug Message ABCDE"), "日志文件必须包含 debug 记录");
}
