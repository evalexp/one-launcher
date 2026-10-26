pub mod model;
pub mod template;

pub use model::{AppearanceConfig, AppConfig, AppsConfig, GeneralConfig, LogConfig, PluginConfig};

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

/// 配置管理器，管理全局配置的生命周期与手动重载
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

        let config = Self::read_config_from_file(&path)?;
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

    /// 手动从磁盘重新加载最新配置文件并替换当前状态
    pub fn reload(&self) -> Result<AppConfig, String> {
        info!("正在从磁盘重新读取配置文件: {:?}", self.path);
        let new_config = Self::read_config_from_file(&self.path)?;
        {
            let mut lock = self.current.write();
            *lock = new_config.clone();
        }
        info!("配置重载成功！");
        Ok(new_config)
    }

    /// 从指定磁盘路径读取并解析 TOML 配置
    pub fn read_config_from_file(path: &Path) -> Result<AppConfig, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("读取文件失败 ({:?}): {}", path, e))?;

        toml::from_str::<AppConfig>(&content)
            .map_err(|e| format!("解析 TOML 语法失败: {}", e))
    }

    /// 使用系统外部默认编辑器打开配置文件供用户编辑
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
