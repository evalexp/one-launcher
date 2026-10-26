use crate::config::model::LogConfig;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::info;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// 日志输出目标类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogDestination {
    /// 终端标准输出
    Std,
    /// 磁盘文件存储
    File(PathBuf),
}

/// 线程安全的文件写入器
#[derive(Clone)]
pub struct FileLogWriter {
    file: Arc<Mutex<fs::File>>,
}

impl Write for FileLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut f) = self.file.lock() {
            let _ = f.write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Ok(mut f) = self.file.lock() {
            let _ = f.flush();
        }
        Ok(())
    }
}

/// 解析日志目标配置（支持 "std" 终端输出与 "fs:<path>" 磁盘文件输出）
pub fn parse_log_destination(config_path: &Path, raw_path: &str) -> LogDestination {
    let trimmed = raw_path.trim();
    if trimmed.eq_ignore_ascii_case("std") || trimmed.eq_ignore_ascii_case("stdout") {
        LogDestination::Std
    } else {
        let file_path_str = if let Some(stripped) = trimmed.strip_prefix("fs:") {
            stripped.trim()
        } else {
            trimmed
        };

        let p = Path::new(file_path_str);
        let mut resolved = if p.is_absolute() {
            p.to_path_buf()
        } else if let Some(config_dir) = config_path.parent() {
            config_dir.join(p)
        } else {
            p.to_path_buf()
        };

        // 若给定的路径为目录或未携带扩展名，自动追加默认文件名
        if resolved.is_dir() || resolved.extension().is_none() {
            resolved = resolved.join("one-launcher.log");
        }

        LogDestination::File(resolved)
    }
}

/// 初始化全局日志记录器
pub fn init_logger(config: &LogConfig, config_file_path: &Path) -> Result<LogDestination, String> {
    let destination = parse_log_destination(config_file_path, &config.path);
    let is_silent = config.level.trim().eq_ignore_ascii_case("silent")
        || config.level.trim().eq_ignore_ascii_case("off");

    if is_silent {
        // 静默模式：设置日志过滤为 off，完全不输出任何事件
        let subscriber = tracing_subscriber::registry().with(EnvFilter::new("off"));
        let _ = subscriber.try_init();
        return Ok(destination);
    }

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&config.level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    match destination {
        LogDestination::Std => {
            let subscriber = tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .with_writer(std::io::stdout)
                        .with_ansi(true)
                        .with_target(true)
                        .with_thread_ids(true),
                );

            let _ = subscriber.try_init();
            info!(
                "日志系统初始化就绪，输出目标: 终端控制台 (std)，过滤级别: '{}'",
                config.level
            );
        }
        LogDestination::File(ref log_file_path) => {
            if let Some(parent) = log_file_path.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("创建日志目录失败 ({:?}): {}", parent, e))?;
                }
            }

            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(log_file_path)
                .map_err(|e| format!("打开或创建日志文件失败 ({:?}): {}", log_file_path, e))?;

            let file_writer = FileLogWriter {
                file: Arc::new(Mutex::new(file)),
            };

            let subscriber = tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .with_writer(move || file_writer.clone())
                        .with_ansi(false)
                        .with_target(true)
                        .with_thread_ids(true),
                );

            let _ = subscriber.try_init();
            info!(
                "日志系统初始化就绪，输出目标: {:?}，过滤级别: '{}'",
                log_file_path, config.level
            );
        }
    }

    Ok(destination)
}
