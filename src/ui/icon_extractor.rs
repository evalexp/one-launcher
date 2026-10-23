use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::OnceLock;
use windows::core::{Interface, HSTRING, PCWSTR};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    SHCreateItemFromParsingName, IShellItem, IShellItemImageFactory, SHGetFileInfoW, SHFILEINFOW,
    SHGFI_ICON, SHGFI_LARGEICON, SIIGBF_RESIZETOFIT,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

static ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<isize>>>> = OnceLock::new();

fn get_cache() -> &'static Mutex<HashMap<String, Option<isize>>> {
    ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 获取指定路径或 UWP 应用名称的 32x32 原生 Windows HICON 图标（支持 .exe, .lnk, 目录及 UWP 应用）
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

    let extracted = if path.starts_with("shell:") {
        extract_uwp_hicon(path)
    } else {
        extract_hicon(path).or_else(|| extract_uwp_hicon(path))
    };

    let mut lock = cache.lock();
    lock.insert(path.to_string(), extracted.map(|h| h.0 as isize));
    extracted
}

/// 针对标准文件系统路径 (.exe, .lnk, 目录等) 通过 SHGetFileInfoW 极速抽取
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

/// 针对 UWP 应用 (shell:AppsFolder\<AUMID>) 通过 Shell 图像工厂提取 32x32 原生图标
fn extract_uwp_hicon(path: &str) -> Option<HICON> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let hpath = HSTRING::from(path);
        let item: IShellItem = SHCreateItemFromParsingName(&hpath, None).ok()?;
        let factory: IShellItemImageFactory = item.cast().ok()?;
        let hbitmap = factory.GetImage(SIZE { cx: 32, cy: 32 }, SIIGBF_RESIZETOFIT).ok()?;

        let mask = CreateBitmap(32, 32, 1, 1, None);
        let icon_info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: hbitmap,
        };

        let hicon = CreateIconIndirect(&icon_info).ok();
        let _ = DeleteObject(mask);
        let _ = DeleteObject(hbitmap);
        hicon
    }
}
