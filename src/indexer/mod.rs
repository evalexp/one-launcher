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
    /// 优先通过 Windows 桌面资源管理器 (IShellDispatch2) 跨进程创建；降级使用 ShellExecuteExW。
    pub fn launch(item: &AppItem) -> Result<(), String> {
        info!("启动程序: {} ({:?})", item.name, item.path);

        #[cfg(windows)]
        {
            use windows::core::{BSTR, HSTRING, PCWSTR, VARIANT};
            use windows::Win32::Foundation::HWND;
            use windows::Win32::System::Com::{
                CoCreateInstance, CoInitializeEx, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
            };
            use windows::Win32::System::Diagnostics::Debug::{
                SetErrorMode, SEM_FAILCRITICALERRORS, SEM_NOOPENFILEERRORBOX,
            };
            use windows::Win32::UI::Shell::{
                IShellDispatch2, Shell, ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SHELLEXECUTEINFOW,
            };
            use windows::Win32::UI::WindowsAndMessaging::{
                AllowSetForegroundWindow, ASFW_ANY, SW_SHOWNORMAL,
            };

            unsafe {
                let _ = SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX);
                let _ = AllowSetForegroundWindow(ASFW_ANY);
            }

            let path_str = item.path.to_str().ok_or("路径包含无效字符")?;
            let dir_str = item
                .path
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("");

            // 1. 通过 Explorer COM 接口 (IShellDispatch2) 委托创建进程
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

                if let Ok(dispatch) =
                    CoCreateInstance::<_, IShellDispatch2>(&Shell, None, CLSCTX_LOCAL_SERVER)
                {
                    let file_bstr = BSTR::from(path_str);
                    let op_var = VARIANT::from("open");
                    let dir_var = if !dir_str.is_empty() {
                        Some(VARIANT::from(dir_str))
                    } else {
                        None
                    };
                    let show_var = VARIANT::from(SW_SHOWNORMAL.0 as i32);

                    if dispatch
                        .ShellExecute(
                            &file_bstr,
                            None,
                            dir_var.as_ref(),
                            Some(&op_var),
                            Some(&show_var),
                        )
                        .is_ok()
                    {
                        return Ok(());
                    }
                }
            }

            // 2. 降级使用 ShellExecuteExW
            unsafe {
                let path_h = HSTRING::from(path_str);
                let verb_open = HSTRING::from("open");
                let dir_h = if !dir_str.is_empty() {
                    Some(HSTRING::from(dir_str))
                } else {
                    None
                };

                let mut sei = SHELLEXECUTEINFOW {
                    cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                    fMask: SEE_MASK_FLAG_NO_UI,
                    hwnd: HWND::default(),
                    lpVerb: PCWSTR(verb_open.as_ptr()),
                    lpFile: PCWSTR(path_h.as_ptr()),
                    lpParameters: PCWSTR::null(),
                    lpDirectory: dir_h.as_ref().map(|d| PCWSTR(d.as_ptr())).unwrap_or(PCWSTR::null()),
                    nShow: SW_SHOWNORMAL.0,
                    ..Default::default()
                };

                if ShellExecuteExW(&mut sei).is_err() {
                    return Err("通过 ShellExecuteExW 唤起应用程序失败".to_string());
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
