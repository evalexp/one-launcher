use crate::config::ConfigManager;
use crate::everything::EverythingClient;
use crate::hotkey::{register_global_hotkey, unregister_global_hotkey};
use crate::indexer::AppIndexManager;
use crate::tray::{
    TrayIcon, ID_TRAY_EDIT_CONFIG, ID_TRAY_EXIT, ID_TRAY_RELOAD_CONFIG, WM_TRAYICON,
};
use crate::ui::icon_extractor::get_icon_for_path;
use crate::ui::types::{SearchResultItem, SearchResultKind};
use tracing::{info, warn};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, GetTextExtentPoint32W, InvalidateRect,
    SelectObject, SetBkMode, SetTextColor, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE,
    DT_VCENTER, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::GlobalLock;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetActiveWindow, SetFocus, VIRTUAL_KEY, VK_A, VK_BACK, VK_CONTROL, VK_DELETE,
    VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateCaret, CreateWindowExW, DefWindowProcW, DestroyCaret, DestroyWindow, DispatchMessageW,
    DrawIconEx, GetClientRect, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, IsWindowVisible,
    LoadCursorW, PostQuitMessage, RegisterClassW, SetCaretPos, SetCursor, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowCaret, ShowWindow, TranslateMessage, DI_NORMAL,
    GWLP_USERDATA, HWND_TOPMOST, IDC_ARROW, MSG, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW, WA_INACTIVE, WM_ACTIVATE, WM_CHAR,
    WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_HOTKEY, WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN,
    WM_MOUSEACTIVATE, WM_PAINT, WM_RBUTTONUP, WM_SETCURSOR, WM_SETFOCUS, WNDCLASSW,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

const WINDOW_CLASS_NAME: &str = "OneLauncher_MainWindow_Class";
const INPUT_HEIGHT: i32 = 58;
const ITEM_HEIGHT: i32 = 56;
const FOOTER_HEIGHT: i32 = 28;

pub struct LauncherWindowState {
    hwnd: HWND,
    config_mgr: ConfigManager,
    app_indexer: AppIndexManager,
    everything_client: EverythingClient,
    tray: Option<TrayIcon>,
    query: String,
    results: Vec<SearchResultItem>,
    selected_index: usize,
    width: i32,
    caret_x: i32,
}

impl LauncherWindowState {
    pub fn new(
        hwnd: HWND,
        config_mgr: ConfigManager,
        app_indexer: AppIndexManager,
        everything_client: EverythingClient,
    ) -> Self {
        let cfg = config_mgr.get();
        let width = cfg.appearance.width.max(500) as i32;

        let tray = match TrayIcon::create(hwnd) {
            Ok(t) => Some(t),
            Err(e) => {
                warn!("未能初始化系统托盘图标: {}", e);
                None
            }
        };

        let mut state = Self {
            hwnd,
            config_mgr,
            app_indexer,
            everything_client,
            tray,
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            width,
            caret_x: 48,
        };
        state.update_search();
        state
    }

    /// 手动重新加载配置（由托盘菜单触发）
    pub fn reload_config(&mut self) {
        match self.config_mgr.reload() {
            Ok(new_cfg) => {
                if let Err(err) = register_global_hotkey(self.hwnd, &new_cfg.general.hotkey) {
                    warn!("重新注册热键提示: {}", err);
                }
                self.app_indexer.reload(&new_cfg.apps);
                self.width = new_cfg.appearance.width.max(500) as i32;
                self.update_search();
                self.update_window_size();
                unsafe {
                    let _ = InvalidateRect(self.hwnd, None, false);
                }
                info!("OneLauncher 配置与热键已成功重载生效！");
            }
            Err(err) => {
                warn!("重载配置失败: {}", err);
            }
        }
    }

    /// 执行搜索与数据聚合
    pub fn update_search(&mut self) {
        let cfg = self.config_mgr.get();
        let max_results = cfg.general.max_results;
        let q = self.query.trim().to_string();

        let mut items = Vec::new();

        let prefix = &cfg.everything.prefix;
        let is_everything_prefix = !prefix.is_empty() && q.starts_with(prefix);

        if is_everything_prefix && cfg.everything.enabled {
            let pure_query = q[prefix.len()..].trim();
            if !pure_query.is_empty() {
                match self.everything_client.search(pure_query, cfg.everything.max_results as usize) {
                    Ok(records) => {
                        for rec in records {
                            items.push(SearchResultItem::from_everything(rec));
                        }
                    }
                    Err(e) => {
                        debug_assert!(true, "Everything 检索出错: {}", e);
                    }
                }
            }
        } else {
            let matched_apps = self.app_indexer.search(&q, max_results);
            for app in matched_apps {
                items.push(SearchResultItem::from_app(app));
            }
        }

        self.results = items;
        if self.selected_index >= self.results.len() {
            self.selected_index = 0;
        }

        self.update_window_size();
    }

    /// 根据当前检索项数量动态调整窗口几何高度与屏幕居中
    pub fn update_window_size(&self) {
        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };

        let cfg = self.config_mgr.get();
        let max_results = cfg.general.max_results;
        let display_count = self.results.len().min(max_results);

        let total_height = if display_count > 0 {
            INPUT_HEIGHT + 1 + (display_count as i32 * ITEM_HEIGHT) + FOOTER_HEIGHT
        } else {
            INPUT_HEIGHT + FOOTER_HEIGHT
        };

        let x = (screen_w - self.width) / 2;
        let y = (screen_h as f32 * 0.22) as i32;

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                x,
                y,
                self.width,
                total_height,
                SWP_NOACTIVATE,
            );
        }
    }

    /// 显示并激活启动器窗口
    pub fn show(&mut self) {
        let cfg = self.config_mgr.get();
        if cfg.general.clear_on_show {
            self.query.clear();
            self.selected_index = 0;
            self.update_search();
        }

        self.update_window_size();
        self.update_caret();

        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetActiveWindow(self.hwnd);
            let _ = SetFocus(self.hwnd);
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    /// 隐藏启动器窗口
    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// 启动当前选中的条目
    pub fn execute_selected(&mut self) {
        if self.results.is_empty() || self.selected_index >= self.results.len() {
            return;
        }

        let item = &self.results[self.selected_index];
        match &item.kind {
            SearchResultKind::App(app) => {
                if let Err(e) = AppIndexManager::launch(app) {
                    warn!("启动应用失败: {}", e);
                }
            }
            SearchResultKind::Everything(file) => {
                if let Err(e) = EverythingClient::open_file(&file.full_path) {
                    warn!("打开文件失败: {}", e);
                }
            }
        }

        self.hide();
        self.query.clear();
        self.selected_index = 0;
        self.update_search();
    }

    /// 更新文本光标 (Caret) 坐标
    pub fn update_caret(&mut self) {
        unsafe {
            let hdc = windows::Win32::Graphics::Gdi::GetDC(self.hwnd);
            let font = CreateFontW(
                17, 0, 0, 0, 400, false.into(), false.into(), false.into(), 1, 0, 0, 0, 0,
                PCWSTR(HSTRING::from("Segoe UI, Microsoft YaHei").as_ptr()),
            );
            let old_font = SelectObject(hdc, font);

            let mut size = SIZE::default();
            let query_h = HSTRING::from(self.query.as_str());
            let query_w = query_h.as_wide();
            let _ = GetTextExtentPoint32W(hdc, query_w, &mut size);

            SelectObject(hdc, old_font);
            let _ = DeleteObject(font);
            let _ = windows::Win32::Graphics::Gdi::ReleaseDC(self.hwnd, hdc);

            self.caret_x = 48 + size.cx;
            let _ = SetCaretPos(self.caret_x, 19);
        }
    }
}

/// Win32 启动器主窗口事件处理函数
unsafe extern "system" fn launcher_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut LauncherWindowState;

        if state_ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }

        let state = &mut *state_ptr;

        match msg {
            WM_HOTKEY => {
                if IsWindowVisible(hwnd).as_bool() {
                    state.hide();
                } else {
                    state.show();
                }
                LRESULT(0)
            }
            WM_ACTIVATE => {
                let active_state = (wparam.0 & 0xFFFF) as u32;
                if active_state == WA_INACTIVE {
                    let cfg = state.config_mgr.get();
                    if cfg.general.hide_on_blur {
                        state.hide();
                    }
                }
                LRESULT(0)
            }
            WM_SETCURSOR => {
                // 确保鼠标指到窗口内时显示清晰明了的标准箭头光标，杜绝沙漏或等待光标
                let _ = SetCursor(LoadCursorW(None, IDC_ARROW).unwrap());
                LRESULT(1)
            }
            WM_CHAR => {
                let ch = char::from_u32(wparam.0 as u32);
                if let Some(c) = ch {
                    if c == '\x08' {
                        // Backspace
                        state.query.pop();
                        state.update_search();
                        state.update_caret();
                        let _ = InvalidateRect(hwnd, None, false);
                    } else if c >= ' ' {
                        // 普通可打印字符与中文输入法文本
                        state.query.push(c);
                        state.update_search();
                        state.update_caret();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                let vk = VIRTUAL_KEY(wparam.0 as u16);
                let ctrl_pressed = (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0;

                match vk {
                    VK_ESCAPE => {
                        state.hide();
                    }
                    VK_RETURN => {
                        state.execute_selected();
                    }
                    VK_UP => {
                        if !state.results.is_empty() {
                            if state.selected_index == 0 {
                                state.selected_index = state.results.len() - 1;
                            } else {
                                state.selected_index -= 1;
                            }
                            let _ = InvalidateRect(hwnd, None, false);
                        }
                    }
                    VK_DOWN => {
                        if !state.results.is_empty() {
                            state.selected_index = (state.selected_index + 1) % state.results.len();
                            let _ = InvalidateRect(hwnd, None, false);
                        }
                    }
                    VK_BACK => {
                        if ctrl_pressed {
                            // Ctrl + Backspace: 快速删除整个词
                            if let Some(pos) = state.query.rfind(|c: char| c.is_whitespace() || c == '/' || c == '\\') {
                                state.query.truncate(pos);
                            } else {
                                state.query.clear();
                            }
                            state.update_search();
                            state.update_caret();
                            let _ = InvalidateRect(hwnd, None, false);
                        }
                    }
                    VK_DELETE => {
                        state.query.clear();
                        state.update_search();
                        state.update_caret();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                    VK_A if ctrl_pressed => {
                        // Ctrl + A: 快捷清空输入框
                        state.query.clear();
                        state.update_search();
                        state.update_caret();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                    VK_V if ctrl_pressed => {
                        // Ctrl + V: 粘贴剪贴板文本
                        if IsClipboardFormatAvailable(13).is_ok() && OpenClipboard(hwnd).is_ok() {
                            let handle = GetClipboardData(13);
                            if let Ok(h) = handle {
                                let ptr = GlobalLock(windows::Win32::Foundation::HGLOBAL(h.0));
                                if !ptr.is_null() {
                                    let slice = std::slice::from_raw_parts(ptr as *const u16, usize::MAX / 2);
                                    let len = slice.iter().position(|&x| x == 0).unwrap_or(0);
                                    if let Ok(s) = String::from_utf16(&slice[..len]) {
                                        state.query.push_str(&s);
                                        state.update_search();
                                        state.update_caret();
                                        let _ = InvalidateRect(hwnd, None, false);
                                    }
                                    let _ = windows::Win32::System::Memory::GlobalUnlock(windows::Win32::Foundation::HGLOBAL(h.0));
                                }
                            }
                            let _ = CloseClipboard();
                        }
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let y = (lparam.0 >> 16) as i16 as i32;
                let cfg = state.config_mgr.get();
                let max_results = cfg.general.max_results;
                let display_count = state.results.len().min(max_results);

                if y > INPUT_HEIGHT && y < INPUT_HEIGHT + 1 + (display_count as i32 * ITEM_HEIGHT) {
                    let clicked_idx = ((y - INPUT_HEIGHT - 1) / ITEM_HEIGHT) as usize;
                    if clicked_idx < state.results.len() {
                        state.selected_index = clicked_idx;
                        state.execute_selected();
                    }
                }
                LRESULT(0)
            }
            WM_SETFOCUS => {
                let _ = CreateCaret(hwnd, None, 2, 22);
                let _ = SetCaretPos(state.caret_x, 19);
                let _ = ShowCaret(hwnd);
                LRESULT(0)
            }
            WM_KILLFOCUS => {
                let _ = DestroyCaret();
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => {
                // 点击窗口时激活并处理点击事件
                LRESULT(1) // MA_ACTIVATE
            }
            WM_TRAYICON => {
                let event = (lparam.0 & 0xFFFF) as u32;
                if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                    if let Some(tray) = &state.tray {
                        tray.show_context_menu();
                    }
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let cmd_id = (wparam.0 & 0xFFFF) as usize;
                match cmd_id {
                    ID_TRAY_EDIT_CONFIG => {
                        if let Err(e) = state.config_mgr.open_in_editor() {
                            warn!("打开配置文件失败: {}", e);
                        }
                    }
                    ID_TRAY_RELOAD_CONFIG => {
                        state.reload_config();
                    }
                    ID_TRAY_EXIT => {
                        info!("收到托盘退出指令，正在关闭 OneLauncher...");
                        PostQuitMessage(0);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_PAINT => {
                paint_launcher(hwnd, state);
                LRESULT(0)
            }
            WM_DESTROY => {
                unregister_global_hotkey(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// 绘制极速轻量原生启动器界面 (GDI 双缓冲无闪烁绘制，uTools 经典高审美排版)
unsafe fn paint_launcher(hwnd: HWND, state: &LauncherWindowState) {
    unsafe {
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

        // 2. 纯黑/深灰暗黑基底色 (#18181B)
        let bg_color = COLORREF(0x001B1818);
        let bg_brush = CreateSolidBrush(bg_color);
        FillRect(mem_dc, &rect, bg_brush);
        let _ = DeleteObject(bg_brush);

        // 3. 配置字体族 (微软雅黑 / Segoe UI)
        let font_search = CreateFontW(
            17, 0, 0, 0, 400, false.into(), false.into(), false.into(), 1, 0, 0, 0, 0,
            PCWSTR(HSTRING::from("Segoe UI, Microsoft YaHei").as_ptr()),
        );
        let font_title = CreateFontW(
            14, 0, 0, 0, 600, false.into(), false.into(), false.into(), 1, 0, 0, 0, 0,
            PCWSTR(HSTRING::from("Segoe UI, Microsoft YaHei").as_ptr()),
        );
        let font_sub = CreateFontW(
            11, 0, 0, 0, 400, false.into(), false.into(), false.into(), 1, 0, 0, 0, 0,
            PCWSTR(HSTRING::from("Segoe UI, Microsoft YaHei").as_ptr()),
        );
        let font_badge = CreateFontW(
            10, 0, 0, 0, 600, false.into(), false.into(), false.into(), 1, 0, 0, 0, 0,
            PCWSTR(HSTRING::from("Segoe UI, Microsoft YaHei").as_ptr()),
        );

        let old_font = SelectObject(mem_dc, font_search);
        SetBkMode(mem_dc, TRANSPARENT);

        // 4. 绘制搜索栏放大镜图标 (x=18, y=19)
        SetTextColor(mem_dc, COLORREF(0x007A7171));
        let mut icon_rect = RECT { left: 18, top: 18, right: 42, bottom: 42 };
        let icon_h = HSTRING::from("🔍");
        let mut icon_w = icon_h.as_wide().to_vec();
        DrawTextW(mem_dc, &mut icon_w, &mut icon_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

        // 5. 绘制搜索文本或占位符
        if state.query.is_empty() {
            SetTextColor(mem_dc, COLORREF(0x005B5252)); // 暗灰占位色 #52525B
            let mut placeholder_rect = RECT { left: 48, top: 0, right: width - 20, bottom: INPUT_HEIGHT };
            let ph_h = HSTRING::from("输入应用名称、简拼(如 wx) 或输入 fs: 搜索全盘文件...");
            let mut ph_w = ph_h.as_wide().to_vec();
            DrawTextW(mem_dc, &mut ph_w, &mut placeholder_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        } else {
            SetTextColor(mem_dc, COLORREF(0x00F5F4F4)); // 冰白高亮文字色 #F4F4F5
            let mut text_rect = RECT { left: 48, top: 0, right: width - 20, bottom: INPUT_HEIGHT };
            let q_h = HSTRING::from(state.query.as_str());
            let mut q_w = q_h.as_wide().to_vec();
            DrawTextW(mem_dc, &mut q_w, &mut text_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
        }

        // 6. 绘制分割线 (#27272A)
        let divider_rect = RECT { left: 0, top: INPUT_HEIGHT, right: width, bottom: INPUT_HEIGHT + 1 };
        let div_brush = CreateSolidBrush(COLORREF(0x002A2727));
        FillRect(mem_dc, &divider_rect, div_brush);
        let _ = DeleteObject(div_brush);

        // 7. 绘制 uTools 经典列表条目 (左侧程序图标，右侧程序名 + 低色调路径)
        let mut y_offset = INPUT_HEIGHT + 1;
        let cfg = state.config_mgr.get();
        let max_results = cfg.general.max_results;
        let display_count = state.results.len().min(max_results);

        for idx in 0..display_count {
            let item = &state.results[idx];
            let is_selected = idx == state.selected_index;

            let item_rect = RECT {
                left: 6,
                top: y_offset,
                right: width - 6,
                bottom: y_offset + ITEM_HEIGHT,
            };

            // 选中高亮底色 (#27272A)
            if is_selected {
                let hl_brush = CreateSolidBrush(COLORREF(0x002A2727));
                FillRect(mem_dc, &item_rect, hl_brush);
                let _ = DeleteObject(hl_brush);
            }

            // 1. 左侧绘制 32x32 程序图标 (类似 uTools 呈现)
            let icon_x = 16;
            let icon_y = y_offset + 12;

            if let Some(hicon) = get_icon_for_path(&item.icon_path) {
                let _ = DrawIconEx(mem_dc, icon_x, icon_y, hicon, 32, 32, 0, None, DI_NORMAL);
            } else {
                // 回退轻量图标占位符
                SelectObject(mem_dc, font_search);
                SetTextColor(mem_dc, COLORREF(0x00F8BD38));
                let mut fallback_rect = RECT { left: icon_x, top: icon_y, right: icon_x + 32, bottom: icon_y + 32 };
                let sym = match &item.kind {
                    SearchResultKind::App(_) => "⚡",
                    SearchResultKind::Everything(e) if e.is_folder => "📁",
                    _ => "📄",
                };
                let sym_h = HSTRING::from(sym);
                let mut sym_w = sym_h.as_wide().to_vec();
                DrawTextW(mem_dc, &mut sym_w, &mut fallback_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
            }

            // 2. 中间绘制应用程序名称 (上面大字，选中纯白 #FFFFFF，默认柔白 #E4E4E7)
            SelectObject(mem_dc, font_title);
            SetTextColor(mem_dc, if is_selected { COLORREF(0x00FFFFFF) } else { COLORREF(0x00E7E4E4) });
            let mut title_rect = RECT {
                left: 60,
                top: y_offset + 8,
                right: width - 110,
                bottom: y_offset + 28,
            };
            let title_h = HSTRING::from(item.title.as_str());
            let mut title_w = title_h.as_wide().to_vec();
            DrawTextW(mem_dc, &mut title_w, &mut title_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS);

            // 3. 中间下方绘制具体文件路径 (下面小字低色调，选中灰白 #A1A1AA，默认低饱和灰 #71717A)
            SelectObject(mem_dc, font_sub);
            SetTextColor(mem_dc, if is_selected { COLORREF(0x00AAA1A1) } else { COLORREF(0x007A7171) });
            let mut sub_rect = RECT {
                left: 60,
                top: y_offset + 30,
                right: width - 110,
                bottom: y_offset + ITEM_HEIGHT - 6,
            };
            let sub_h = HSTRING::from(item.subtitle.as_str());
            let mut sub_w = sub_h.as_wide().to_vec();
            DrawTextW(mem_dc, &mut sub_w, &mut sub_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS);

            // 4. 右侧绘制回车启动提示 (选中状态可见)
            if is_selected {
                SetTextColor(mem_dc, COLORREF(0x00AAA1A1));
                let mut enter_rect = RECT {
                    left: width - 120,
                    top: y_offset,
                    right: width - 68,
                    bottom: y_offset + ITEM_HEIGHT,
                };
                let enter_h = HSTRING::from("↵ 打开");
                let mut enter_w = enter_h.as_wide().to_vec();
                DrawTextW(mem_dc, &mut enter_w, &mut enter_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
            }

            // 5. 右侧绘制类型标识徽章
            SelectObject(mem_dc, font_badge);
            let (badge_bg, badge_fg) = match &item.kind {
                SearchResultKind::App(_) => (COLORREF(0x00332211), COLORREF(0x00F8BD38)), // 蔚蓝天青 #38BDF8
                SearchResultKind::Everything(e) if e.is_folder => (COLORREF(0x00112233), COLORREF(0x0024BFFB)), // 琥珀暖黄 #FBBF24
                _ => (COLORREF(0x00113322), COLORREF(0x0099D334)), // 翡翠翠绿 #34D399
            };

            let badge_rect = RECT {
                left: width - 64,
                top: y_offset + 17,
                right: width - 16,
                bottom: y_offset + 39,
            };
            let badge_brush = CreateSolidBrush(badge_bg);
            FillRect(mem_dc, &badge_rect, badge_brush);
            let _ = DeleteObject(badge_brush);

            SetTextColor(mem_dc, badge_fg);
            let mut badge_txt_rect = badge_rect;
            let badge_h = HSTRING::from(item.badge.as_str());
            let mut badge_w = badge_h.as_wide().to_vec();
            DrawTextW(mem_dc, &mut badge_w, &mut badge_txt_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

            y_offset += ITEM_HEIGHT;
        }

        // 8. 绘制底部操作状态条 (Footer)
        let footer_rect = RECT { left: 0, top: y_offset, right: width, bottom: y_offset + FOOTER_HEIGHT };
        let footer_brush = CreateSolidBrush(COLORREF(0x00141212)); // #121214
        FillRect(mem_dc, &footer_rect, footer_brush);
        let _ = DeleteObject(footer_brush);

        SelectObject(mem_dc, font_sub);
        SetTextColor(mem_dc, COLORREF(0x005B5252));

        // 左侧统计文字
        let status_str = if state.results.is_empty() {
            "就绪".to_string()
        } else {
            format!("{} 个结果", state.results.len())
        };
        let mut left_stat_rect = RECT { left: 16, top: y_offset, right: 200, bottom: y_offset + FOOTER_HEIGHT };
        let stat_h = HSTRING::from(status_str.as_str());
        let mut stat_w = stat_h.as_wide().to_vec();
        DrawTextW(mem_dc, &mut stat_w, &mut left_stat_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

        // 右侧按键指引
        let mut right_hint_rect = RECT { left: width - 240, top: y_offset, right: width - 16, bottom: y_offset + FOOTER_HEIGHT };
        let hint_h = HSTRING::from("↑↓ 选择   ↵ 启动   Esc 隐藏");
        let mut hint_w = hint_h.as_wide().to_vec();
        DrawTextW(mem_dc, &mut hint_w, &mut right_hint_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);

        // 9. 将内存双缓冲 DC 内容极速映射至屏幕 (零闪烁)
        let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

        // 10. 释放清理 GDI 对象
        SelectObject(mem_dc, old_font);
        SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(font_search);
        let _ = DeleteObject(font_title);
        let _ = DeleteObject(font_sub);
        let _ = DeleteObject(font_badge);
        let _ = DeleteObject(mem_bmp);
        let _ = DeleteDC(mem_dc);
        let _ = EndPaint(hwnd, &ps);
    }
}

/// 构建并运行启动器主窗口与 Win32 消息循环
pub fn run_launcher(
    config_mgr: ConfigManager,
    app_indexer: AppIndexManager,
    everything_client: EverythingClient,
) -> Result<(), String> {
    unsafe {
        let class_name_w = HSTRING::from(WINDOW_CLASS_NAME);
        let hinstance = GetModuleHandleW(PCWSTR::null())
            .map_err(|e| format!("获取模块句柄失败: {}", e))?;

        let wc = WNDCLASSW {
            lpfnWndProc: Some(launcher_wndproc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name_w.as_ptr()),
            ..Default::default()
        };

        RegisterClassW(&wc);

        let cfg = config_mgr.get();
        let width = cfg.appearance.width.max(500) as i32;

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class_name_w.as_ptr()),
            PCWSTR(HSTRING::from("OneLauncher").as_ptr()),
            WS_POPUP,
            0,
            0,
            width,
            INPUT_HEIGHT + FOOTER_HEIGHT,
            HWND::default(),
            None,
            hinstance,
            None,
        ).map_err(|e| format!("创建启动器窗口失败: {}", e))?;

        // 启用 Windows 11 DWM 原生优雅圆角
        let corner_preference = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_preference as *const _ as _,
            std::mem::size_of_val(&corner_preference) as u32,
        );

        // 注册全局热键 (默认 Alt+Space)
        if let Err(err) = register_global_hotkey(hwnd, &cfg.general.hotkey) {
            warn!("初始热键注册提示: {}", err);
        }

        // 初始化窗口状态（包含系统托盘图标）
        let state = Box::new(LauncherWindowState::new(
            hwnd,
            config_mgr,
            app_indexer,
            everything_client,
        ));

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

        info!("OneLauncher 极简原生引擎已成功就绪，内存占用超低，按 Alt+Space 即可唤出！");

        // 核心 Win32 消息循环 (必须调用 TranslateMessage 以正确支持字符与 IME 输入！)
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        unregister_global_hotkey(hwnd);
        let _ = DestroyWindow(hwnd);
        Ok(())
    }
}
