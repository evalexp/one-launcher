use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;

pub struct SingleInstance {
    handle: HANDLE,
}

impl SingleInstance {
    /// 尝试获取全局互斥锁以确保单实例运行
    pub fn acquire(name: &str) -> Option<Self> {
        let name_h = HSTRING::from(name);
        unsafe {
            let handle = CreateMutexW(None, true, PCWSTR(name_h.as_ptr())).ok()?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return None;
            }
            Some(Self { handle })
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
