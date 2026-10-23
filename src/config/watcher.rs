use crate::config::model::AppConfig;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// 配置文件监听器
pub struct ConfigWatcher {
    // 保持 debouncer 句柄存活
    _debouncer: notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>,
}

impl ConfigWatcher {
    /// 启动对配置文件的防抖监听 (默认防抖时间 250ms)
    pub fn watch<F>(config_path: PathBuf, on_reload: F) -> Result<Self, String>
    where
        F: Fn(AppConfig) + Send + Sync + 'static,
    {
        let parent_dir = config_path
            .parent()
            .ok_or_else(|| "无法获取配置文件所在的父目录".to_string())?
            .to_path_buf();

        let target_file_name = config_path
            .file_name()
            .ok_or_else(|| "无法获取配置文件文件名".to_string())?
            .to_os_string();

        let on_reload = Arc::new(on_reload);
        let target_path = config_path.clone();

        let mut debouncer = new_debouncer(
            Duration::from_millis(250),
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    let mut should_reload = false;
                    for event in events {
                        if event.path.file_name() == Some(&target_file_name) {
                            should_reload = true;
                            break;
                        }
                    }

                    if should_reload {
                        info!("检测到配置文件发生变动，正在重新加载...");
                        match Self::read_config_from_file(&target_path) {
                            Ok(new_config) => {
                                info!("配置文件重载成功！");
                                on_reload(new_config);
                            }
                            Err(err) => {
                                error!("配置文件语法错误，重载失败，已保持上一次配置: {}", err);
                            }
                        }
                    }
                }
                Err(err) => {
                    warn!("配置文件监听告警: {:?}", err);
                }
            },
        )
        .map_err(|e| format!("初始化配置文件监听器失败: {}", e))?;

        debouncer
            .watcher()
            .watch(&parent_dir, RecursiveMode::NonRecursive)
            .map_err(|e| format!("注册目录监听失败 ({:?}): {}", parent_dir, e))?;

        info!("配置文件热重载监听已就绪: {:?}", config_path);

        Ok(Self {
            _debouncer: debouncer,
        })
    }

    /// 从磁盘读取并解析配置文件
    pub fn read_config_from_file(path: &Path) -> Result<AppConfig, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("读取文件失败 ({:?}): {}", path, e))?;

        toml::from_str::<AppConfig>(&content)
            .map_err(|e| format!("解析 TOML 语法失败: {}", e))
    }
}
