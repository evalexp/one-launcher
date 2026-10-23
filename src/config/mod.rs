pub mod model;
pub mod template;
pub mod watcher;

pub use model::AppConfig;
pub use watcher::ConfigWatcher;

use parking_lot::RwLock;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

#[cfg(windows)]
use windows::core::{HSTRING, PCWSTR};
#[cfg(windows)]
use windows::Win32::Foundation::HWND;
#[cfg(windows)]
use windows::Win32::UI::Shell::ShellExecuteW;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

/// 配置管理器，管理全局配置的生命周期、并发读写、文件持久化和热重载
#[derive(Clone)]
pub struct ConfigManager {
    path: PathBuf,
    current: Arc<RwLock<AppConfig>>,
}

impl ConfigManager {
    /// 初始化配置管理器：自动探测配置文件路径，必要时初始化默认模板，并加载配置
    pub fn init() -> Result<Self, String> {
        let path = Self::detect_config_path();
        Self::ensure_file_exists(&path)?;

        let config = watcher::ConfigWatcher::read_config_from_file(&path)?;
        info!("成功加载配置文件: {:?}", path);

        Ok(Self {
            path,
            current: Arc::new(RwLock::new(config)),
        })
    }

    /// 获取当前生效的配置快照
    pub fn get(&self) -> AppConfig {
        self.current.read().clone()
    }

    /// 获取配置文件在磁盘上的绝对路径
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 启动后台热重载监听
    pub fn start_watcher<F>(&self, on_change: F) -> Result<ConfigWatcher, String>
    where
        F: Fn(AppConfig) + Send + Sync + 'static,
    {
        let current_lock = self.current.clone();
        let callback = move |new_config: AppConfig| {
            {
                let mut lock = current_lock.write();
                *lock = new_config.clone();
            }
            on_change(new_config);
        };

        ConfigWatcher::watch(self.path.clone(), callback)
    }

    /// 使用外部默认编辑器打开配置文件供用户编辑
    pub fn open_in_editor(&self) -> Result<(), String> {
        info!("正在使用系统默认文本编辑器打开配置文件: {:?}", self.path);

        #[cfg(windows)]
        {
            let path_str = self
                .path
                .to_str()
                .ok_or_else(|| "配置文件路径包含非 UTF-8 字符".to_string())?;
            let path_hstring = HSTRING::from(path_str);
            let verb_open = HSTRING::from("open");

            unsafe {
                let result = ShellExecuteW(
                    HWND::default(),
                    PCWSTR(verb_open.as_ptr()),
                    PCWSTR(path_hstring.as_ptr()),
                    PCWSTR::null(),
                    PCWSTR::null(),
                    SW_SHOW,
                );

                // ShellExecute 返回值大于 32 表示成功，否则表示错误码
                let code = result.0 as usize;
                if code <= 32 {
                    return Err(format!("打开编辑器失败，ShellExecute 错误码: {}", code));
                }
            }
            Ok(())
        }

        #[cfg(not(windows))]
        {
            Err("非 Windows 平台不支持该操作".to_string())
        }
    }

    /// 检测配置文件存放路径（优先当前程序同级目录的便携配置，次选 %APPDATA%/OneLauncher）
    fn detect_config_path() -> PathBuf {
        // 1. 优先检查当前运行工作目录或 exe 同级目录是否存在 config.toml
        if let Ok(exe_path) = env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let portable_config = exe_dir.join("config.toml");
                if portable_config.exists() {
                    return portable_config;
                }
            }
        }

        // 2. 检查当前工作目录
        let local_config = PathBuf::from("config.toml");
        if local_config.exists() {
            return local_config;
        }

        // 3. 默认使用标准系统应用数据目录 (%APPDATA%\OneLauncher\config.toml)
        if let Some(app_data) = dirs::config_dir() {
            let app_dir = app_data.join("OneLauncher");
            return app_dir.join("config.toml");
        }

        // 4. 极端兜底
        PathBuf::from("config.toml")
    }

    /// 确保配置文件存在，如果不存在则自动创建并写入带注释的默认模板
    fn ensure_file_exists(path: &Path) -> Result<(), String> {
        if !path.exists() {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("创建配置目录失败 ({:?}): {}", parent, e))?;
                }
            }
            fs::write(path, template::default_config_content())
                .map_err(|e| format!("写入默认配置文件模板失败 ({:?}): {}", path, e))?;
            info!("已自动生成全量预设配置文件: {:?}", path);
        }
        Ok(())
    }
}
