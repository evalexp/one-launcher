pub mod sdk;

pub use sdk::{EverythingItem, EverythingSdk};

use std::path::Path;
use std::sync::Arc;
use tracing::warn;

#[cfg(windows)]
use windows::core::{HSTRING, PCWSTR};
#[cfg(windows)]
use windows::Win32::Foundation::HWND;
#[cfg(windows)]
use windows::Win32::UI::Shell::ShellExecuteW;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

/// Everything 客户端
#[derive(Clone)]
pub struct EverythingClient {
    sdk: Option<Arc<EverythingSdk>>,
}

impl Default for EverythingClient {
    fn default() -> Self {
        Self::new()
    }
}

impl EverythingClient {
    pub fn new() -> Self {
        match EverythingSdk::load() {
            Ok(sdk) => Self {
                sdk: Some(Arc::new(sdk)),
            },
            Err(err) => {
                warn!("未能初始化 Everything SDK: {}", err);
                Self { sdk: None }
            }
        }
    }

    /// 检查 Everything 进程/服务是否在线且可通信
    pub fn is_available(&self) -> bool {
        if let Some(sdk) = &self.sdk {
            sdk.is_available()
        } else {
            false
        }
    }

    /// 执行文件秒级搜索
    pub fn search(&self, query: &str, max_results: usize) -> Result<Vec<EverythingItem>, String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        if let Some(sdk) = &self.sdk {
            sdk.search(trimmed, max_results)
        } else {
            Err("Everything SDK 尚未就绪".to_string())
        }
    }

    /// 打开搜索到的文件或文件夹
    pub fn open_file(path: &Path) -> Result<(), String> {
        #[cfg(windows)]
        {
            let path_str = path
                .to_str()
                .ok_or_else(|| "文件路径包含无效的 UTF-8 字符".to_string())?;
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
                    return Err(format!("打开文件失败，ShellExecute 错误码: {}", code));
                }
            }
            Ok(())
        }

        #[cfg(not(windows))]
        {
            Err("非 Windows 平台不支持该操作".to_string())
        }
    }

    /// 在资源管理器中定位并选中文件
    pub fn open_containing_folder(path: &Path) -> Result<(), String> {
        #[cfg(windows)]
        {
            let path_str = path
                .to_str()
                .ok_or_else(|| "文件路径包含无效的 UTF-8 字符".to_string())?;

            let args = format!("/select,\"{}\"", path_str);
            let exe_h = HSTRING::from("explorer.exe");
            let args_h = HSTRING::from(args.as_str());

            unsafe {
                let res = ShellExecuteW(
                    HWND::default(),
                    PCWSTR::null(),
                    PCWSTR(exe_h.as_ptr()),
                    PCWSTR(args_h.as_ptr()),
                    PCWSTR::null(),
                    SW_SHOW,
                );
                let code = res.0 as usize;
                if code <= 32 {
                    return Err(format!("定位文件夹失败，ShellExecute 错误码: {}", code));
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
