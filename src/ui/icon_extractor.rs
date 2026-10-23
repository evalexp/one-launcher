use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::OnceLock;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::HICON;

static ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<isize>>>> = OnceLock::new();

fn get_cache() -> &'static Mutex<HashMap<String, Option<isize>>> {
    ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 获取指定路径文件的 32x32 原生 Windows HICON 图标（支持 .exe, .lnk, 目录等）
pub fn get_icon_for_path(path: &str) -> Option<HICON> {
    if path.is_empty() {
        return None;
    }

    let cache = get_cache();
    {
        let lock = cache.lock();
        if let Some(val) = lock.get(path) {
            return val.map(|h| HICON(h as *mut _));
        }
    }

    let extracted = extract_hicon(path);
    let mut lock = cache.lock();
    lock.insert(path.to_string(), extracted.map(|h| h.0 as isize));
    extracted
}

fn extract_hicon(path: &str) -> Option<HICON> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut sfi = SHFILEINFOW::default();

    unsafe {
        let res = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut sfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );

        if res != 0 && !sfi.hIcon.is_invalid() {
            Some(sfi.hIcon)
        } else {
            None
        }
    }
}
