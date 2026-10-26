pub mod matcher;
pub mod model;
pub mod pinyin_helper;
pub mod scanner;

pub use matcher::AppMatcher;
pub use model::AppItem;
pub use scanner::AppScanner;

use crate::config::AppsConfig;
use parking_lot::RwLock;
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

/// 应用程序索引管理器
#[derive(Clone)]
pub struct AppIndexManager {
    apps: Arc<RwLock<Vec<AppItem>>>,
    matcher: Arc<AppMatcher>,
}

impl AppIndexManager {
    /// 创建并异步开始后台初始扫描
    pub fn new(config: &AppsConfig) -> Self {
        let initial_apps = AppScanner::scan_all(config);
        Self {
            apps: Arc::new(RwLock::new(initial_apps)),
            matcher: Arc::new(AppMatcher::new()),
        }
    }

    /// 重新扫描应用程序
    pub fn reload(&self, config: &AppsConfig) {
        let new_apps = AppScanner::scan_all(config);
        let mut lock = self.apps.write();
        *lock = new_apps;
    }

    /// 搜索匹配应用程序
    pub fn search(&self, query: &str, limit: usize) -> Vec<AppItem> {
        let apps = self.apps.read();
        self.matcher.match_apps(&apps, query, limit)
    }

    /// 启动选定的应用程序
    pub fn launch(item: &AppItem) -> Result<(), String> {
        info!("启动程序: {} ({:?})", item.name, item.path);

        #[cfg(windows)]
        {
            let path_str = item.path.to_str().ok_or("路径包含无效字符")?;
            let path_h = HSTRING::from(path_str);
            let verb_open = HSTRING::from("open");

            unsafe {
                let res = ShellExecuteW(
                    HWND::default(),
                    PCWSTR(verb_open.as_ptr()),
                    PCWSTR(path_h.as_ptr()),
                    PCWSTR::null(),
                    PCWSTR::null(),
                    SW_SHOW,
                );
                let code = res.0 as usize;
                if code <= 32 {
                    return Err(format!("启动失败，ShellExecute 错误码: {}", code));
                }
            }
            Ok(())
        }

        #[cfg(not(windows))]
        {
            Err("非 Windows 平台不支持该操作".to_string())
        }
    }
}
