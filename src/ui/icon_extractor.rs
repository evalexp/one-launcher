use parking_lot::Mutex;
use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tracing::debug;
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

// 缓存 RGBA 原始字节流 (4096 字节)，Arc<[u8]> 是线程安全的 Send + Sync
static ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<[u8]>>>>> = OnceLock::new();

fn get_cache() -> &'static Mutex<HashMap<String, Option<Arc<[u8]>>>> {
    ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 从给定的文件或应用路径中提取 32x32 程序图标（支持 .exe, .lnk, 文件夹与系统关联文件）
pub fn get_icon_for_path(path: &str) -> Option<Image> {
    if path.is_empty() {
        return None;
    }

    let cache = get_cache();
    let maybe_bytes = {
        let lock = cache.lock();
        lock.get(path).cloned()
    };

    let raw_bytes = match maybe_bytes {
        Some(b) => b,
        None => {
            let extracted = extract_icon_raw_bytes(path);
            let mut lock = cache.lock();
            lock.insert(path.to_string(), extracted.clone());
            extracted
        }
    };

    raw_bytes.map(|bytes| {
        let pixel_buffer = SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(&bytes, 32, 32);
        Image::from_rgba8(pixel_buffer)
    })
}

fn extract_icon_raw_bytes(path: &str) -> Option<Arc<[u8]>> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut sfi = SHFILEINFOW::default();

    unsafe {
        let result = SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut sfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );

        if result == 0 || sfi.hIcon.is_invalid() {
            return None;
        }

        let mut icon_info = ICONINFO::default();
        if GetIconInfo(sfi.hIcon, &mut icon_info).is_err() {
            let _ = DestroyIcon(sfi.hIcon);
            return None;
        }

        let hdc = CreateCompatibleDC(None);
        let width = 32i32;
        let height = 32i32;

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // 负数表示自顶向下的 DIB 位图
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bgra_buf = vec![0u8; (width * height * 4) as usize];
        let hbm = if !icon_info.hbmColor.is_invalid() {
            icon_info.hbmColor
        } else {
            icon_info.hbmMask
        };

        let scan_lines = GetDIBits(
            hdc,
            hbm,
            0,
            height as u32,
            Some(bgra_buf.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = DeleteDC(hdc);
        if !icon_info.hbmColor.is_invalid() {
            let _ = DeleteObject(icon_info.hbmColor);
        }
        if !icon_info.hbmMask.is_invalid() {
            let _ = DeleteObject(icon_info.hbmMask);
        }
        let _ = DestroyIcon(sfi.hIcon);

        if scan_lines == 0 {
            return None;
        }

        // 检测 alpha 通道是否存在非 0 值（部分旧式 32 位图标没有写入 alpha 通道，全为 0）
        let mut has_non_zero_alpha = false;
        for i in 0..(width * height) as usize {
            if bgra_buf[i * 4 + 3] > 0 {
                has_non_zero_alpha = true;
                break;
            }
        }

        let mut rgba_buf = vec![0u8; (width * height * 4) as usize];
        for i in 0..(width * height) as usize {
            let b = bgra_buf[i * 4];
            let g = bgra_buf[i * 4 + 1];
            let r = bgra_buf[i * 4 + 2];
            let a = if has_non_zero_alpha {
                bgra_buf[i * 4 + 3]
            } else {
                255
            };
            rgba_buf[i * 4] = r;
            rgba_buf[i * 4 + 1] = g;
            rgba_buf[i * 4 + 2] = b;
            rgba_buf[i * 4 + 3] = a;
        }

        debug!("已成功为 '{}' 提取 32x32 图标", path);
        Some(Arc::from(rgba_buf.into_boxed_slice()))
    }
}
