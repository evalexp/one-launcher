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

/// 线程安全的多路写入器：同时写入标准输出（若存在控制台终端）与目标日志文件
#[derive(Clone)]
pub struct DualLogWriter {
    file: Arc<Mutex<fs::File>>,
}

impl Write for DualLogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let _ = std::io::stdout().write(buf);
        if let Ok(mut f) = self.file.lock() {
            let _ = f.write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = std::io::stdout().flush();
        if let Ok(mut f) = self.file.lock() {
            let _ = f.flush();
        }
        Ok(())
    }
}

/// 解析日志文件的绝对物理路径
pub fn resolve_log_path(config_path: &Path, log_path_setting: &Path) -> PathBuf {
    let mut resolved = if log_path_setting.is_absolute() {
        log_path_setting.to_path_buf()
    } else if let Some(config_dir) = config_path.parent() {
        config_dir.join(log_path_setting)
    } else {
        log_path_setting.to_path_buf()
    };

    // 若配置的路径是目录或无后缀名，自动追加默认日志文件名
    if resolved.is_dir() || resolved.extension().is_none() {
        resolved = resolved.join("one-launcher.log");
    }

    resolved
}

/// 初始化全局日志记录器
pub fn init_logger(config: &LogConfig, config_file_path: &Path) -> Result<PathBuf, String> {
    let log_file_path = resolve_log_path(config_file_path, &config.path);

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
        .open(&log_file_path)
        .map_err(|e| format!("打开或创建日志文件失败 ({:?}): {}", log_file_path, e))?;

    let dual_writer = DualLogWriter {
        file: Arc::new(Mutex::new(file)),
    };

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&config.level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_writer(move || dual_writer.clone())
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true),
        );

    // 在多线程或单元测试环境下，允许安全地重复尝试注入 Subscriber
    let _ = subscriber.try_init();

    info!(
        "日志系统初始化就绪，日志级别: '{}', 日志输出路径: {:?}",
        config.level, log_file_path
    );

    Ok(log_file_path)
}
