use tracing::{error, info};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
    DeleteObject, Ellipse, FillRect, Polygon, SelectObject,
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

impl TrayIcon {
    /// 创建并添加系统托盘图标（斜向 45 度低饱和质感小火箭）
    pub fn create(hwnd: HWND) -> Result<Self, String> {
        unsafe {
            // 优先使用动态生成的斜向 45 度专属小火箭图标
            let (hicon, is_custom) = match Self::create_rocket_icon() {
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
            let copy_len = tip_utf16.len().min(nid.szTip.len());
            nid.szTip[..copy_len].copy_from_slice(&tip_utf16[..copy_len]);

            let res = Shell_NotifyIconW(NIM_ADD, &nid);
            if !res.as_bool() {
                if is_custom {
                    let _ = DestroyIcon(hicon);
                }
                return Err("向系统通知区域注册托盘图标失败".to_string());
            }

            info!("系统托盘 45° 雅致小火箭图标已成功注册");
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

            // 必须先调用 SetForegroundWindow，否则点击菜单外部时菜单不会自动消失 (Win32 经典特性)
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

    /// 在内存中纯代码自绘 32x32 像素、斜向 45 度（头朝左上方 ↖）的低饱和质感小火箭
    unsafe fn create_rocket_icon() -> Result<HICON, String> {
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

            // 1. 绘制右下方推进火焰 (45° 尾喷：从机尾 21,21 向右下 28,28，低饱和暖黄与浅橙)
            let flame_brush = CreateSolidBrush(COLORREF(0x004BA2F5)); // 柔和橙 (RGB: 245, 162, 75)
            let old_brush = SelectObject(mem_dc, flame_brush);
            let flame_pts = [
                POINT { x: 19, y: 22 },
                POINT { x: 28, y: 28 }, // 火焰尖端朝向右下方
                POINT { x: 22, y: 19 },
            ];
            let _ = Polygon(mem_dc, &flame_pts);
            let _ = DeleteObject(SelectObject(mem_dc, old_brush));

            // 2. 绘制双侧机翼 (低饱和冷灰蓝 #64748B)
            let wing_brush = CreateSolidBrush(COLORREF(0x008B7464)); // (RGB: 100, 116, 139)
            let old_brush = SelectObject(mem_dc, wing_brush);
            // 左翼 (下翼)
            let wing_left = [
                POINT { x: 12, y: 18 },
                POINT { x: 8,  y: 25 },
                POINT { x: 17, y: 22 },
            ];
            let _ = Polygon(mem_dc, &wing_left);

            // 右翼 (上翼)
            let wing_right = [
                POINT { x: 18, y: 12 },
                POINT { x: 25, y: 8 },
                POINT { x: 22, y: 17 },
            ];
            let _ = Polygon(mem_dc, &wing_right);
            let _ = DeleteObject(SelectObject(mem_dc, old_brush));

            // 3. 绘制 45° 倾斜火箭主体 (柔和银白色 #E2E8F0，尖端朝左上 6,6)
            let body_brush = CreateSolidBrush(COLORREF(0x00F0E8E2)); // 雅致银白
            let old_brush = SelectObject(mem_dc, body_brush);
            let body_pts = [
                POINT { x: 6,  y: 6 },  // 火箭机头尖端 (左上方)
                POINT { x: 8,  y: 15 },
                POINT { x: 20, y: 22 }, // 机尾
                POINT { x: 22, y: 20 },
                POINT { x: 15, y: 8 },
            ];
            let _ = Polygon(mem_dc, &body_pts);
            let _ = DeleteObject(SelectObject(mem_dc, old_brush));

            // 4. 绘制机头部尖锥柔和点缀 (低饱和哑光红 #BE5046)
            let nose_brush = CreateSolidBrush(COLORREF(0x004650BE)); // (RGB: 190, 80, 70)
            let old_brush = SelectObject(mem_dc, nose_brush);
            let nose_pts = [
                POINT { x: 6,  y: 6 },
                POINT { x: 8,  y: 11 },
                POINT { x: 11, y: 8 },
            ];
            let _ = Polygon(mem_dc, &nose_pts);
            let _ = DeleteObject(SelectObject(mem_dc, old_brush));

            // 5. 绘制圆形舷窗 (微缩冷灰圆窗)
            let window_brush = CreateSolidBrush(COLORREF(0x00A09080)); // 柔和灰青
            let old_brush = SelectObject(mem_dc, window_brush);
            let _ = Ellipse(mem_dc, 12, 12, 16, 16);
            let _ = DeleteObject(SelectObject(mem_dc, old_brush));

            // 准备透明掩码
            let mask_bmp = CreateBitmap(size, size, 1, 1, None);

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

            Err("CreateIconIndirect 创建 45° 小火箭图标失败".to_string())
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
