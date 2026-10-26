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
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, CreateIconIndirect, LoadIconW, HICON, ICONINFO, IDI_SHIELD,
    LR_DEFAULTCOLOR,
};

const ICON_PNG_FOLDER: &[u8] = include_bytes!("../../assets/icons/folder.png");
const ICON_PNG_FILE: &[u8] = include_bytes!("../../assets/icons/file.png");
const ICON_PNG_ERROR: &[u8] = include_bytes!("../../assets/icons/error.png");
const ICON_PNG_SUCCESS: &[u8] = include_bytes!("../../assets/icons/success.png");
const ICON_PNG_WARN: &[u8] = include_bytes!("../../assets/icons/warn.png");
const ICON_PNG_INFO: &[u8] = include_bytes!("../../assets/icons/info.png");

static ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<isize>>>> = OnceLock::new();
static FOLDER_ICON: OnceLock<Option<isize>> = OnceLock::new();
static FILE_ICON: OnceLock<Option<isize>> = OnceLock::new();
static ERROR_ICON: OnceLock<Option<isize>> = OnceLock::new();
static SUCCESS_ICON: OnceLock<Option<isize>> = OnceLock::new();
static WARN_ICON: OnceLock<Option<isize>> = OnceLock::new();
static INFO_ICON: OnceLock<Option<isize>> = OnceLock::new();
static ICON_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn get_cache() -> &'static Mutex<HashMap<String, Option<isize>>> {
    ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 根据图标引用 ID 获取 HICON（支持内置预设图标、自定义缓存 ref_id 以及本地物理路径）
pub fn get_icon_by_ref(ref_id: &str) -> Option<HICON> {
    if ref_id.is_empty() {
        return None;
    }

    // 1. 检查内置预设图标 (零开销 / 极速直读)
    match ref_id.to_lowercase().as_str() {
        "folder" | "dir" | "directory" => return get_folder_icon(),
        "file" | "doc" | "document" => return get_file_icon(),
        "error" | "fail" | "failed" => return get_error_icon(),
        "success" | "ok" | "done" => return get_success_icon(),
        "warn" | "warning" | "alert" => return get_warn_icon(),
        "info" | "hint" | "tip" => return get_info_icon(),
        "calc" | "calculator" => return get_icon_for_path(r"C:\Windows\System32\calc.exe").or_else(get_file_icon),
        "terminal" | "cmd" | "console" => return get_icon_for_path(r"C:\Windows\System32\cmd.exe").or_else(get_file_icon),
        "settings" | "config" => return get_icon_for_path(r"C:\Windows\System32\control.exe").or_else(get_file_icon),
        "notepad" | "text" | "edit" => return get_icon_for_path(r"C:\Windows\System32\notepad.exe").or_else(get_file_icon),
        "shield" => unsafe { return LoadIconW(None, IDI_SHIELD).ok(); },
        _ => {}
    }

    // 2. 检查由插件动态调用 cache_icon 缓存的自定义图标引用
    let cache = get_cache();
    {
        let lock = cache.lock();
        if let Some(val) = lock.get(ref_id) {
            return val.map(|h| HICON(h as *mut _));
        }
    }

    // 3. 兜底尝试作为本地文件路径/UWP 路径直接抽取
    get_icon_for_path(ref_id)
}

/// 解析图标字节并写入内部图标缓存，返回其对应的 ref_id
pub fn cache_icon_from_bytes(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }

    let hicon_opt = unsafe {
        // 1. 如果为完整的 .ico 格式容器 (以 0x00, 0x00, 0x01, 0x00 开头)
        if bytes.len() >= 22 && bytes[0..4] == [0, 0, 1, 0] {
            let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
            let mut best_offset = 0;
            let mut best_size = 0;

            for i in 0..count {
                let entry_offset = 6 + i * 16;
                if entry_offset + 16 <= bytes.len() {
                    let w = bytes[entry_offset];
                    let bytes_in_res = u32::from_le_bytes([
                        bytes[entry_offset + 8],
                        bytes[entry_offset + 9],
                        bytes[entry_offset + 10],
                        bytes[entry_offset + 11],
                    ]) as usize;
                    let image_offset = u32::from_le_bytes([
                        bytes[entry_offset + 12],
                        bytes[entry_offset + 13],
                        bytes[entry_offset + 14],
                        bytes[entry_offset + 15],
                    ]) as usize;

                    if image_offset + bytes_in_res <= bytes.len() {
                        best_offset = image_offset;
                        best_size = bytes_in_res;
                        if w == 32 {
                            break;
                        }
                    }
                }
            }

            if best_size > 0 {
                let res_slice = &bytes[best_offset..best_offset + best_size];
                CreateIconFromResourceEx(res_slice, true, 0x00030000, 32, 32, LR_DEFAULTCOLOR).ok()
            } else {
                None
            }
        } else {
            // 2. 尝试作为原生图标资源数据 (如 PNG 数据块或 DIB) 直接解析
            CreateIconFromResourceEx(bytes, true, 0x00030000, 32, 32, LR_DEFAULTCOLOR).ok()
        }
    };

    if let Some(hicon) = hicon_opt {
        let id_num = ICON_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let ref_id = format!("icon_ref_{}", id_num);
        let cache = get_cache();
        let mut lock = cache.lock();
        lock.insert(ref_id.clone(), Some(hicon.0 as isize));
        Some(ref_id)
    } else {
        None
    }
}

/// 获取全局常驻的通用文件夹图标 (现代质感矢量黄金文件夹)
pub fn get_folder_icon() -> Option<HICON> {
    let icon_ptr = FOLDER_ICON.get_or_init(|| unsafe {
        CreateIconFromResourceEx(ICON_PNG_FOLDER, true, 0x00030000, 32, 32, LR_DEFAULTCOLOR)
            .ok()
            .map(|h| h.0 as isize)
    });
    icon_ptr.map(|h| HICON(h as *mut _))
}

/// 获取全局常驻的通用普通文件图标 (现代折角文档质感图标)
pub fn get_file_icon() -> Option<HICON> {
    let icon_ptr = FILE_ICON.get_or_init(|| unsafe {
        CreateIconFromResourceEx(ICON_PNG_FILE, true, 0x00030000, 32, 32, LR_DEFAULTCOLOR)
            .ok()
            .map(|h| h.0 as isize)
    });
    icon_ptr.map(|h| HICON(h as *mut _))
}

/// 获取内置错误警告红标图标 (32x32 红色圆形白色叉号)
pub fn get_error_icon() -> Option<HICON> {
    let icon_ptr = ERROR_ICON.get_or_init(|| unsafe {
        CreateIconFromResourceEx(ICON_PNG_ERROR, true, 0x00030000, 32, 32, LR_DEFAULTCOLOR)
            .ok()
            .map(|h| h.0 as isize)
    });
    icon_ptr.map(|h| HICON(h as *mut _))
}

/// 获取内置成功完成绿标图标 (32x32 翡翠绿色圆形白色对勾)
pub fn get_success_icon() -> Option<HICON> {
    let icon_ptr = SUCCESS_ICON.get_or_init(|| unsafe {
        CreateIconFromResourceEx(ICON_PNG_SUCCESS, true, 0x00030000, 32, 32, LR_DEFAULTCOLOR)
            .ok()
            .map(|h| h.0 as isize)
    });
    icon_ptr.map(|h| HICON(h as *mut _))
}

/// 获取内置警告提示黄标图标 (32x32 琥珀色感叹号)
pub fn get_warn_icon() -> Option<HICON> {
    let icon_ptr = WARN_ICON.get_or_init(|| unsafe {
        CreateIconFromResourceEx(ICON_PNG_WARN, true, 0x00030000, 32, 32, LR_DEFAULTCOLOR)
            .ok()
            .map(|h| h.0 as isize)
    });
    icon_ptr.map(|h| HICON(h as *mut _))
}

/// 获取内置信息蓝标图标 (32x32 蓝色信息标志)
pub fn get_info_icon() -> Option<HICON> {
    let icon_ptr = INFO_ICON.get_or_init(|| unsafe {
        CreateIconFromResourceEx(ICON_PNG_INFO, true, 0x00030000, 32, 32, LR_DEFAULTCOLOR)
            .ok()
            .map(|h| h.0 as isize)
    });
    icon_ptr.map(|h| HICON(h as *mut _))
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
