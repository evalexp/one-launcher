use parking_lot::{Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(windows)]
use windows::core::{HSTRING, PCWSTR};
#[cfg(windows)]
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
#[cfg(windows)]
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
#[cfg(windows)]
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, FrameRect, InvalidateRect, SelectObject,
    SetBkMode, SetTextColor, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
#[cfg(windows)]
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetMessageW, GetSystemMetrics, GetWindowLongPtrW, LoadCursorW, PostMessageW,
    PostQuitMessage, RegisterClassW, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, ShowWindow, TranslateMessage, IDC_ARROW, MSG, SM_CXSCREEN, SM_CYSCREEN,
    SW_SHOW, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_NCLBUTTONDOWN, WM_PAINT,
    WM_USER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const PROGRESS_WINDOW_CLASS: &str = "OneLauncher_ProgressBar_Window_Class";
const WM_PROGRESS_UPDATE: u32 = WM_USER + 201;

static CLASS_REGISTERED: std::sync::Once = std::sync::Once::new();
static ACTIVE_PROGRESS: Mutex<Option<Arc<ProgressBar>>> = Mutex::new(None);

/// 进度条内部共享状态
#[derive(Debug, Clone)]
pub struct ProgressState {
    pub title: String,
    pub message: String,
    pub percent: u32,
    pub closed: bool,
}

/// 进度条句柄控制器
pub struct ProgressBar {
    state: Arc<RwLock<ProgressState>>,
    hwnd: Arc<Mutex<Option<isize>>>,
    closed: Arc<AtomicBool>,
}

impl ProgressBar {
    /// 创建并展示一个原生进度条窗口
    pub fn new(title: &str, message: &str, percent: u32) -> Self {
        let percent = percent.min(100);
        let state = Arc::new(RwLock::new(ProgressState {
            title: title.to_string(),
            message: message.to_string(),
            percent,
            closed: false,
        }));
        let hwnd_holder = Arc::new(Mutex::new(None));
        let closed = Arc::new(AtomicBool::new(false));

        tracing::info!("[ProgressBar] 创建进度条: [{}] {} ({}%)", title, message, percent);

        #[cfg(windows)]
        {
            let state_clone = state.clone();
            let hwnd_holder_clone = hwnd_holder.clone();
            let closed_clone = closed.clone();

            std::thread::Builder::new()
                .name("OneLauncher-ProgressBar".to_string())
                .spawn(move || {
                    run_progress_window(state_clone, hwnd_holder_clone, closed_clone);
                })
                .ok();
        }

        Self {
            state,
            hwnd: hwnd_holder,
            closed,
        }
    }

    /// 更新进度百分比 (0-100) 与消息文本
    pub fn update(&self, percent: u32, message: Option<&str>) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let percent = percent.min(100);
        {
            let mut lock = self.state.write();
            lock.percent = percent;
            if let Some(msg) = message {
                lock.message = msg.to_string();
            }
        }

        tracing::debug!("[ProgressBar] 更新进度: {}% - {:?}", percent, message);

        #[cfg(windows)]
        {
            if let Some(hwnd_val) = *self.hwnd.lock() {
                unsafe {
                    let target = HWND(hwnd_val as *mut _);
                    let _ = PostMessageW(target, WM_PROGRESS_UPDATE, WPARAM(percent as usize), LPARAM(0));
                }
            }
        }
    }

    /// 关闭并销毁进度条窗口
    pub fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        {
            let mut lock = self.state.write();
            lock.closed = true;
        }

        tracing::info!("[ProgressBar] 关闭进度条");

        #[cfg(windows)]
        {
            if let Some(hwnd_val) = *self.hwnd.lock() {
                unsafe {
                    let target = HWND(hwnd_val as *mut _);
                    let _ = PostMessageW(target, WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        self.close();
    }
}

/// 显示或替换全局单例进度条
pub fn show_global_progress(title: &str, message: &str, percent: u32) -> Arc<ProgressBar> {
    let mut lock = ACTIVE_PROGRESS.lock();
    if let Some(existing) = lock.take() {
        existing.close();
    }
    let pb = Arc::new(ProgressBar::new(title, message, percent));
    *lock = Some(pb.clone());
    pb
}

/// 更新全局单例进度条
pub fn update_global_progress(percent: u32, message: Option<&str>) {
    let lock = ACTIVE_PROGRESS.lock();
    if let Some(pb) = &*lock {
        pb.update(percent, message);
    }
}

/// 关闭全局单例进度条
pub fn close_global_progress() {
    let mut lock = ACTIVE_PROGRESS.lock();
    if let Some(pb) = lock.take() {
        pb.close();
    }
}

#[cfg(windows)]
fn run_progress_window(
    state: Arc<RwLock<ProgressState>>,
    hwnd_holder: Arc<Mutex<Option<isize>>>,
    closed: Arc<AtomicBool>,
) {
    CLASS_REGISTERED.call_once(|| {
        let class_name_w = HSTRING::from(PROGRESS_WINDOW_CLASS);
        let hinstance = unsafe { GetModuleHandleW(PCWSTR::null()).unwrap_or_default() };
        let wc = WNDCLASSW {
            style: windows::Win32::UI::WindowsAndMessaging::CS_HREDRAW
                | windows::Win32::UI::WindowsAndMessaging::CS_VREDRAW,
            lpfnWndProc: Some(progress_wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name_w.as_ptr()),
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() },
            ..Default::default()
        };
        unsafe {
            let _ = RegisterClassW(&wc);
        }
    });

    let width = 420;
    let height = 110;
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let x = (screen_w - width) / 2;
    let y = (screen_h - height) / 2;

    let class_name_w = HSTRING::from(PROGRESS_WINDOW_CLASS);
    let title_w = HSTRING::from("OneLauncher Progress");
    let hinstance = unsafe { GetModuleHandleW(PCWSTR::null()).unwrap_or_default() };

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            PCWSTR(class_name_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            WS_POPUP,
            x,
            y,
            width,
            height,
            HWND::default(),
            None,
            hinstance,
            None,
        )
    };

    let hwnd = match hwnd {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("创建原生进度条窗口失败: {}", e);
            closed.store(true, Ordering::SeqCst);
            return;
        }
    };

    // 启用窗口圆角
    let corner_preference = DWMWCP_ROUND;
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_preference as *const _ as _,
            std::mem::size_of_val(&corner_preference) as u32,
        )
    };

    *hwnd_holder.lock() = Some(hwnd.0 as isize);
    let state_ptr = Box::into_raw(Box::new(state.clone()));

    unsafe {
        SetWindowLongPtrW(hwnd, windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA, state_ptr as isize);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        let _ = InvalidateRect(hwnd, None, false);
    }

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let ptr = SetWindowLongPtrW(hwnd, windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA, 0)
            as *mut Arc<RwLock<ProgressState>>;
        if !ptr.is_null() {
            let _ = Box::from_raw(ptr);
        }
    }

    *hwnd_holder.lock() = None;
    closed.store(true, Ordering::SeqCst);
}

#[cfg(windows)]
unsafe extern "system" fn progress_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_PROGRESS_UPDATE => {
                let _ = InvalidateRect(hwnd, None, false);
                LRESULT(0)
            }
            WM_PAINT => {
                paint_progress(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                // 支持鼠标按住窗口任意位置自由拖拽
                let _ = ReleaseCapture();
                let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(2), LPARAM(0));
                LRESULT(0)
            }
            WM_KEYDOWN if wparam.0 == 0x1B => {
                // ESC 键快速关闭进度条
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

#[cfg(windows)]
unsafe fn paint_progress(hwnd: HWND) {
    unsafe {
        let state_ptr = GetWindowLongPtrW(hwnd, windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA)
            as *mut Arc<RwLock<ProgressState>>;
        if state_ptr.is_null() {
            return;
        }
        let state_arc = &*state_ptr;
        let (title, message, percent) = {
            let lock = state_arc.read();
            (lock.title.clone(), lock.message.clone(), lock.percent)
        };

        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);

        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        if width <= 0 || height <= 0 {
            let _ = EndPaint(hwnd, &ps);
            return;
        }

        // 1. 创建内存双缓冲 DC
        let mem_dc = CreateCompatibleDC(hdc);
        let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
        let old_bmp = SelectObject(mem_dc, mem_bmp);

        // 2. 绘制深灰背景 (#1E1E1E)
        let bg_color = COLORREF(0x001E1E1E);
        let bg_brush = CreateSolidBrush(bg_color);
        FillRect(mem_dc, &rect, bg_brush);
        let _ = DeleteObject(bg_brush);

        // 3. 绘制 1px 细边框 (#3E3E42)
        let border_brush = CreateSolidBrush(COLORREF(0x00423E3E));
        FrameRect(mem_dc, &rect, border_brush);
        let _ = DeleteObject(border_brush);

        // 4. 创建字体族
        let font_title = CreateFontW(
            15, 0, 0, 0, 600, false.into(), false.into(), false.into(), 1, 0, 0, 0, 0,
            PCWSTR(HSTRING::from("Segoe UI, Microsoft YaHei").as_ptr()),
        );
        let font_msg = CreateFontW(
            12, 0, 0, 0, 400, false.into(), false.into(), false.into(), 1, 0, 0, 0, 0,
            PCWSTR(HSTRING::from("Segoe UI, Microsoft YaHei").as_ptr()),
        );
        let font_pct = CreateFontW(
            13, 0, 0, 0, 600, false.into(), false.into(), false.into(), 1, 0, 0, 0, 0,
            PCWSTR(HSTRING::from("Segoe UI, Microsoft YaHei").as_ptr()),
        );

        SetBkMode(mem_dc, TRANSPARENT);

        // 5. 绘制标题 (左侧)
        let old_font = SelectObject(mem_dc, font_title);
        SetTextColor(mem_dc, COLORREF(0x00FFFFFF));
        let mut title_rect = RECT { left: 20, top: 16, right: 330, bottom: 38 };
        let mut title_w = HSTRING::from(title).as_wide().to_vec();
        DrawTextW(mem_dc, &mut title_w, &mut title_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);

        // 6. 绘制百分比文字 (右侧高亮)
        SelectObject(mem_dc, font_pct);
        SetTextColor(mem_dc, COLORREF(0x00F8BD38)); // Sky Blue (#38BDF8)
        let pct_str = format!("{}%", percent);
        let mut pct_rect = RECT { left: 330, top: 16, right: 400, bottom: 38 };
        let mut pct_w = HSTRING::from(pct_str).as_wide().to_vec();
        DrawTextW(mem_dc, &mut pct_w, &mut pct_rect, DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

        // 7. 绘制状态描述信息
        SelectObject(mem_dc, font_msg);
        SetTextColor(mem_dc, COLORREF(0x00AAAAAA));
        let mut msg_rect = RECT { left: 20, top: 44, right: 400, bottom: 64 };
        let mut msg_w = HSTRING::from(message).as_wide().to_vec();
        DrawTextW(mem_dc, &mut msg_w, &mut msg_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);

        // 8. 绘制进度槽背景 (#2D2D30)
        let track_rect = RECT { left: 20, top: 74, right: 400, bottom: 84 };
        let track_brush = CreateSolidBrush(COLORREF(0x00302D2D));
        FillRect(mem_dc, &track_rect, track_brush);
        let _ = DeleteObject(track_brush);

        // 9. 绘制进度条高亮填充 (#0078D4)
        if percent > 0 {
            let bar_width = ((400 - 20) * percent) / 100;
            let fill_rect = RECT { left: 20, top: 74, right: 20 + bar_width as i32, bottom: 84 };
            let fill_brush = CreateSolidBrush(COLORREF(0x00D47800)); // Accent Blue
            FillRect(mem_dc, &fill_rect, fill_brush);
            let _ = DeleteObject(fill_brush);
        }

        // 清理字体
        SelectObject(mem_dc, old_font);
        let _ = DeleteObject(font_title);
        let _ = DeleteObject(font_msg);
        let _ = DeleteObject(font_pct);

        // 双缓冲贴图
        let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

        SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(mem_bmp);
        let _ = DeleteDC(mem_dc);
        let _ = EndPaint(hwnd, &ps);
    }
}
