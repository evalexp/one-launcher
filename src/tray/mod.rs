use tracing::{error, info};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, CreateSolidBrush,
    DeleteDC, DeleteObject, Ellipse, FillRect, Polygon, PS_SOLID, SelectObject,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos,
    LoadIconW, SetForegroundWindow, TrackPopupMenu, HICON, ICONINFO, IDI_APPLICATION,
    MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_RIGHTBUTTON, WM_USER,
};

pub const WM_TRAYICON: u32 = WM_USER + 100;

pub const ID_TRAY_EDIT_CONFIG: usize = 2001;
pub const ID_TRAY_RELOAD_CONFIG: usize = 2002;
pub const ID_TRAY_EXIT: usize = 2003;

/// Windows 系统托盘图标管理器
pub struct TrayIcon {
    nid: NOTIFYICONDATAW,
    custom_icon: Option<HICON>,
}

unsafe impl Send for TrayIcon {}
unsafe impl Sync for TrayIcon {}

impl TrayIcon {
    /// 创建并添加系统托盘图标 (基于 OneLauncher "Unified Lens & One" 极简现代高审美设计)
    pub fn create(hwnd: HWND) -> Result<Self, String> {
        unsafe {
            let (hicon, is_custom) = match Self::create_unified_one_icon() {
                Ok(icon) => (icon, true),
                Err(e) => {
                    info!("回退使用系统默认图标: {}", e);
                    let sys_icon = LoadIconW(None, IDI_APPLICATION)
                        .map_err(|err| format!("加载默认应用图标失败: {}", err))?;
                    (sys_icon, false)
                }
            };

            let mut nid = NOTIFYICONDATAW::default();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            nid.uCallbackMessage = WM_TRAYICON;
            nid.hIcon = hicon;

            // 悬停提示文字 "OneLauncher (Alt+Space 唤出)"
            let tip = "OneLauncher (Alt+Space 唤出)";
            let tip_utf16: Vec<u16> = tip.encode_utf16().chain(std::iter::once(0)).collect();
            let copy_len = tip_utf16.len().min(nid.szTip.len() - 1);
            nid.szTip[..copy_len].copy_from_slice(&tip_utf16[..copy_len]);

            // 系统托盘注册重试机制 (应对 Explorer 准备阶段或高 DPI 初始化抖动)
            let mut added = false;
            for _ in 0..5 {
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
                if Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                    added = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            if !added {
                if is_custom {
                    let _ = DestroyIcon(hicon);
                }
                return Err("向系统通知区域注册托盘图标失败 (Shell_NotifyIconW NIM_ADD 拒绝)".to_string());
            }

            info!("系统托盘专属 OneLauncher 极简透镜图标已成功注册");
            Ok(Self {
                nid,
                custom_icon: if is_custom { Some(hicon) } else { None },
            })
        }
    }

    /// 弹出托盘上下文右键菜单
    pub fn show_context_menu(&self) {
        unsafe {
            let hmenu = match CreatePopupMenu() {
                Ok(m) => m,
                Err(e) => {
                    error!("创建托盘弹出菜单失败: {}", e);
                    return;
                }
            };

            let edit_text = HSTRING::from("编辑配置 (&E)");
            let reload_text = HSTRING::from("重载配置 (&R)");
            let exit_text = HSTRING::from("退出 OneLauncher (&X)");

            let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_EDIT_CONFIG, PCWSTR(edit_text.as_ptr()));
            let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_RELOAD_CONFIG, PCWSTR(reload_text.as_ptr()));
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_EXIT, PCWSTR(exit_text.as_ptr()));

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);

            let _ = SetForegroundWindow(self.nid.hWnd);

            let _ = TrackPopupMenu(
                hmenu,
                TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                0,
                self.nid.hWnd,
                None,
            );

            let _ = DestroyMenu(hmenu);
        }
    }

    /// 在内存中纯代码自绘 32x32 专属 "One" 聚光透镜几何图标
    /// 设计内涵: 外部圆环象征聚光搜索透镜 (Aperture Lens)，中心核心立体数字 "1" 象征统一一触即达 (The Unified One)
    unsafe fn create_unified_one_icon() -> Result<HICON, String> {
        let size = 32i32;

        unsafe {
            let screen_dc = windows::Win32::Graphics::Gdi::GetDC(HWND::default());
            let mem_dc = CreateCompatibleDC(screen_dc);
            let color_bmp = CreateCompatibleBitmap(screen_dc, size, size);
            let old_bmp = SelectObject(mem_dc, color_bmp);

            // 背景填黑作为透明基底
            let bg_brush = CreateSolidBrush(COLORREF(0x00000000));
            let full_rect = RECT { left: 0, top: 0, right: size, bottom: size };
            FillRect(mem_dc, &full_rect, bg_brush);
            let _ = DeleteObject(bg_brush);

            // 1. 绘制外部聚焦透镜圆环 (低饱和深空灰白微光 #94A3B8, 2px 宽)
            let ring_pen = CreatePen(PS_SOLID, 2, COLORREF(0x00B8A394));
            let old_pen = SelectObject(mem_dc, ring_pen);
            let null_brush = windows::Win32::Graphics::Gdi::GetStockObject(windows::Win32::Graphics::Gdi::NULL_BRUSH);
            let old_brush = SelectObject(mem_dc, null_brush);

            let _ = Ellipse(mem_dc, 3, 3, 29, 29);

            SelectObject(mem_dc, old_brush);
            let _ = DeleteObject(SelectObject(mem_dc, old_pen));

            // 2. 绘制核心 "1" (The One: 纯净冰魄银白 #F8FAFC)
            let one_brush = CreateSolidBrush(COLORREF(0x00FCFAF8));
            let old_brush = SelectObject(mem_dc, one_brush);
            let null_pen = windows::Win32::Graphics::Gdi::GetStockObject(windows::Win32::Graphics::Gdi::NULL_PEN);
            let old_pen = SelectObject(mem_dc, null_pen);

            // 几何 "1" 的利落切角与立柱点阵
            let one_pts = [
                POINT { x: 13, y: 13 }, // 45° 切角起始
                POINT { x: 17, y: 9 },
                POINT { x: 19, y: 9 },
                POINT { x: 19, y: 23 }, // 垂直立柱右缘
                POINT { x: 16, y: 23 },
                POINT { x: 16, y: 13 },
                POINT { x: 13, y: 15 },
            ];
            let _ = Polygon(mem_dc, &one_pts);

            // 3. 右上角极速点芒 (Launch Spark: 冰晶天青色 #38BDF8)
            let spark_brush = CreateSolidBrush(COLORREF(0x00F8BD38));
            SelectObject(mem_dc, spark_brush);
            let _ = Ellipse(mem_dc, 22, 6, 26, 10);
            let _ = DeleteObject(spark_brush);

            SelectObject(mem_dc, old_pen);
            let _ = DeleteObject(SelectObject(mem_dc, old_brush));

            // 准备透明掩码 (全 0 初始化，表示透明度由颜色缓冲主导)
            let mask_bits = vec![0u8; ((size * size) / 8) as usize];
            let mask_bmp = CreateBitmap(
                size,
                size,
                1,
                1,
                Some(mask_bits.as_ptr() as *const _),
            );

            SelectObject(mem_dc, old_bmp);
            let _ = DeleteDC(mem_dc);
            let _ = windows::Win32::Graphics::Gdi::ReleaseDC(HWND::default(), screen_dc);

            let icon_info = ICONINFO {
                fIcon: true.into(),
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask_bmp,
                hbmColor: color_bmp,
            };

            let hicon = CreateIconIndirect(&icon_info);
            let _ = DeleteObject(color_bmp);
            let _ = DeleteObject(mask_bmp);

            if let Ok(icon) = hicon {
                if !icon.0.is_null() {
                    return Ok(icon);
                }
            }

            Err("CreateIconIndirect 创建 OneLauncher 专属图标失败".to_string())
        }
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.nid);
            if let Some(hicon) = self.custom_icon {
                let _ = DestroyIcon(hicon);
            }
            info!("系统托盘图标已安全移除");
        }
    }
}
