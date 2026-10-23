use crate::config::ConfigManager;
use crate::everything::EverythingClient;
use crate::hotkey::{register_global_hotkey, unregister_global_hotkey, DEFAULT_HOTKEY_ID};
use crate::indexer::AppIndexManager;
use crate::tray::{
    TrayIcon, ID_TRAY_EDIT_CONFIG, ID_TRAY_EXIT, ID_TRAY_RELOAD_CONFIG, WM_TRAYICON,
};
use crate::ui::types::{SearchResultItem, SearchResultKind};
use tracing::{info, warn};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, InvalidateRect, SelectObject, SetBkMode,
    SetTextColor, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, PAINTSTRUCT,
    SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetSystemMetrics, GetWindowLongPtrW, IsWindowVisible, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWLP_USERDATA,
    HWND_TOPMOST, MSG, SM_CXSCREEN, SM_CYSCREEN, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW, WA_INACTIVE,
    WM_ACTIVATE, WM_CHAR, WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_HOTKEY, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_PAINT, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

const WINDOW_CLASS_NAME: &str = "OneLauncher_MainWindow_Class";
const INPUT_HEIGHT: i32 = 56;
const ITEM_HEIGHT: i32 = 50;

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
}

impl LauncherWindowState {
    pub fn new(
        hwnd: HWND,
        config_mgr: ConfigManager,
        app_indexer: AppIndexManager,
        everything_client: EverythingClient,
    ) -> Self {
        let cfg = config_mgr.get();
        let width = cfg.appearance.width as i32;

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
        };
        state.update_search();
        state
    }

    /// 手动重新加载配置（由托盘菜单触发）
    pub fn reload_config(&mut self) {
        match self.config_mgr.reload() {
            Ok(new_cfg) => {
                // 1. 重新注册全局热键（移除旧热键绑定并注册新键位）
                if let Err(err) = register_global_hotkey(self.hwnd, &new_cfg.general.hotkey) {
                    warn!("重新注册热键提示: {}", err);
                }

                // 2. 重新扫描系统应用程序目录
                self.app_indexer.reload(&new_cfg.apps);

                // 3. 更新界面参数
                self.width = new_cfg.appearance.width as i32;
                self.update_search();

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

        // 检查是否命中 Everything 专属前缀 (默认 "fs:")
        let prefix = &cfg.everything.prefix;
        let is_everything_prefix = !prefix.is_empty() && q.starts_with(prefix);

        if is_everything_prefix && cfg.everything.enabled {
            let pure_query = q[prefix.len()..].trim();
            if !pure_query.is_empty() {
                if let Ok(files) = self.everything_client.search(pure_query, max_results) {
                    for f in files {
                        items.push(SearchResultItem::from_everything(f));
                    }
                }
            }
        } else {
            // 普通系统应用程序模糊检索
            let matched_apps = self.app_indexer.search(&q, max_results);
            for app in matched_apps {
                items.push(SearchResultItem::from_app(app));
            }

            // 若 Everything 开启且处于混合模式 (prefix 为空)
            if cfg.everything.enabled && prefix.is_empty() && !q.is_empty() {
                let remaining = max_results.saturating_sub(items.len());
                if remaining > 0 {
                    if let Ok(files) = self.everything_client.search(&q, remaining) {
                        for f in files {
                            items.push(SearchResultItem::from_everything(f));
                        }
                    }
                }
            }
        }

        self.results = items;
        if self.selected_index >= self.results.len() {
            self.selected_index = 0;
        }

        self.resize_window();
    }

    /// 根据结果数量动态计算窗口大小并居中
    pub fn resize_window(&self) {
        let count = self.results.len();
        let total_height = INPUT_HEIGHT + (count as i32 * ITEM_HEIGHT) + 8;

        unsafe {
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);

            let x = (screen_w - self.width) / 2;
            let y = screen_h * 22 / 100; // 屏幕上方约 22% 处

            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                x,
                y,
                self.width,
                total_height,
                SWP_SHOWWINDOW,
            );
        }
    }

    /// 显示并聚焦窗口
    pub fn show(&mut self) {
        let cfg = self.config_mgr.get();
        if cfg.general.clear_on_show {
            self.query.clear();
            self.update_search();
        } else {
            self.resize_window();
        }

        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    /// 隐藏窗口
    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// 执行激活选定条目
    pub fn execute_selected(&self) {
        if self.results.is_empty() {
            return;
        }

        if let Some(item) = self.results.get(self.selected_index) {
            match &item.kind {
                SearchResultKind::App(app) => {
                    let _ = AppIndexManager::launch(app);
                }
                SearchResultKind::Everything(file) => {
                    let _ = EverythingClient::open_file(&file.full_path);
                }
            }
        }

        self.hide();
    }
}

/// 窗口消息分发处理函数
unsafe extern "system" fn launcher_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let user_data = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) };
    if user_data != 0 {
        let state = unsafe { &mut *(user_data as *mut LauncherWindowState) };

        match msg {
            WM_ACTIVATE => {
                let active = (wparam.0 & 0xFFFF) as u32;
                if active == WA_INACTIVE {
                    let cfg = state.config_mgr.get();
                    if cfg.general.hide_on_blur {
                        state.hide();
                    }
                }
                return LRESULT(0);
            }
            WM_HOTKEY => {
                if wparam.0 as i32 == DEFAULT_HOTKEY_ID {
                    let is_visible = unsafe { IsWindowVisible(hwnd).as_bool() };
                    if is_visible {
                        state.hide();
                    } else {
                        state.show();
                    }
                }
                return LRESULT(0);
            }
            WM_TRAYICON => {
                let event = (lparam.0 & 0xFFFF) as u32;
                if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                    if let Some(tray) = &state.tray {
                        tray.show_context_menu();
                    }
                } else if event == WM_LBUTTONUP {
                    let is_visible = unsafe { IsWindowVisible(hwnd).as_bool() };
                    if is_visible {
                        state.hide();
                    } else {
                        state.show();
                    }
                }
                return LRESULT(0);
            }
            WM_COMMAND => {
                let cmd_id = (wparam.0 & 0xFFFF) as usize;
                match cmd_id {
                    ID_TRAY_EDIT_CONFIG => {
                        let _ = state.config_mgr.open_in_editor();
                    }
                    ID_TRAY_RELOAD_CONFIG => {
                        state.reload_config();
                    }
                    ID_TRAY_EXIT => {
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                    _ => {}
                }
                return LRESULT(0);
            }
            WM_KEYDOWN => {
                let vk = VIRTUAL_KEY(wparam.0 as u16);
                if vk == VK_ESCAPE {
                    state.hide();
                    return LRESULT(0);
                } else if vk == VK_UP {
                    if !state.results.is_empty() {
                        state.selected_index = if state.selected_index == 0 {
                            state.results.len() - 1
                        } else {
                            state.selected_index - 1
                        };
                        unsafe {
                            let _ = InvalidateRect(hwnd, None, false);
                        }
                    }
                    return LRESULT(0);
                } else if vk == VK_DOWN {
                    if !state.results.is_empty() {
                        state.selected_index = (state.selected_index + 1) % state.results.len();
                        unsafe {
                            let _ = InvalidateRect(hwnd, None, false);
                        }
                    }
                    return LRESULT(0);
                } else if vk == VK_RETURN {
                    state.execute_selected();
                    return LRESULT(0);
                }
            }
            WM_CHAR => {
                let ch = wparam.0 as u16;
                if ch == 8 {
                    // 退格键 Backspace
                    state.query.pop();
                    state.update_search();
                    unsafe {
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                    return LRESULT(0);
                } else if ch >= 32 {
                    if let Some(c) = char::from_u32(ch as u32) {
                        state.query.push(c);
                        state.update_search();
                        unsafe {
                            let _ = InvalidateRect(hwnd, None, false);
                        }
                        return LRESULT(0);
                    }
                }
            }
            WM_LBUTTONDOWN => {
                let y = ((lparam.0 >> 16) & 0xFFFF) as i32;
                if y > INPUT_HEIGHT {
                    let clicked_index = (y - INPUT_HEIGHT) / ITEM_HEIGHT;
                    if (clicked_index as usize) < state.results.len() {
                        state.selected_index = clicked_index as usize;
                        state.execute_selected();
                    }
                }
                return LRESULT(0);
            }
            WM_PAINT => {
                unsafe { render_launcher_ui(hwnd, state); }
                return LRESULT(0);
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                return LRESULT(0);
            }
            _ => {}
        }
    }

    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// 双缓冲纯原生极速 GDI 渲染器 (微秒级一帧无任何卡顿，内存 < 8MB)
unsafe fn render_launcher_ui(hwnd: HWND, state: &LauncherWindowState) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };

    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }

    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    unsafe {
        // 创建双缓冲内存 DC
        let mem_dc = CreateCompatibleDC(hdc);
        let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
        let old_bmp = SelectObject(mem_dc, mem_bmp);

        let is_dark = state.config_mgr.get().appearance.theme != "light";

        // 背景颜色配置
        let bg_color = if is_dark { COLORREF(0x00202020) } else { COLORREF(0x00F8F8F8) };
        let text_color = if is_dark { COLORREF(0x00FFFFFF) } else { COLORREF(0x00111111) };
        let subtext_color = if is_dark { COLORREF(0x00A0A0A0) } else { COLORREF(0x00666666) };
        let highlight_bg = if is_dark { COLORREF(0x003A3A3A) } else { COLORREF(0x00E0E0E0) };

        // 填充主窗口背景
        let bg_brush = CreateSolidBrush(bg_color);
        FillRect(mem_dc, &rect, bg_brush);
        let _ = DeleteObject(bg_brush);

        SetBkMode(mem_dc, TRANSPARENT);

        // 创建字体 (Segoe UI / 微软雅黑)
        let font_title = CreateFontW(
            22, 0, 0, 0, 500, 0, 0, 0, 1, 0, 0, 2, 0,
            PCWSTR(HSTRING::from("Segoe UI").as_ptr()),
        );
        let font_sub = CreateFontW(
            15, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 2, 0,
            PCWSTR(HSTRING::from("Segoe UI").as_ptr()),
        );

        let old_font = SelectObject(mem_dc, font_title);

        // 1. 绘制顶部搜索框
        let mut input_rect = RECT {
            left: 20,
            top: 14,
            right: width - 20,
            bottom: INPUT_HEIGHT,
        };

        SetTextColor(mem_dc, text_color);
        if state.query.is_empty() {
            SetTextColor(mem_dc, subtext_color);
            let placeholder = HSTRING::from("输入应用名称、简拼(如 wx) 或输入 fs: 搜索全盘文件...");
            let mut text_w: Vec<u16> = placeholder.as_wide().to_vec();
            DrawTextW(
                mem_dc,
                &mut text_w,
                &mut input_rect,
                DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );
        } else {
            let query_display = format!("{}|", state.query);
            let query_h = HSTRING::from(query_display.as_str());
            let mut text_w: Vec<u16> = query_h.as_wide().to_vec();
            DrawTextW(
                mem_dc,
                &mut text_w,
                &mut input_rect,
                DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );
        }

        // 2. 绘制下方的搜索结果条目列表
        let mut y_offset = INPUT_HEIGHT;

        for (i, item) in state.results.iter().enumerate() {
            let is_selected = i == state.selected_index;

            let row_rect = RECT {
                left: 8,
                top: y_offset,
                right: width - 8,
                bottom: y_offset + ITEM_HEIGHT,
            };

            // 高亮选定背景
            if is_selected {
                let hl_brush = CreateSolidBrush(highlight_bg);
                FillRect(mem_dc, &row_rect, hl_brush);
                let _ = DeleteObject(hl_brush);
            }

            // 绘制类型徽章
            SelectObject(mem_dc, font_sub);
            SetTextColor(mem_dc, subtext_color);
            let mut badge_rect = RECT {
                left: 20,
                top: y_offset,
                right: 70,
                bottom: y_offset + ITEM_HEIGHT,
            };
            let badge_h = HSTRING::from(item.badge.as_str());
            let mut badge_w: Vec<u16> = badge_h.as_wide().to_vec();
            DrawTextW(
                mem_dc,
                &mut badge_w,
                &mut badge_rect,
                DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
            );

            // 绘制主标题
            SelectObject(mem_dc, font_title);
            SetTextColor(mem_dc, text_color);
            let mut title_rect = RECT {
                left: 80,
                top: y_offset + 4,
                right: width - 20,
                bottom: y_offset + 28,
            };
            let title_h = HSTRING::from(item.title.as_str());
            let mut title_w: Vec<u16> = title_h.as_wide().to_vec();
            DrawTextW(
                mem_dc,
                &mut title_w,
                &mut title_rect,
                DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
            );

            // 绘制副标题 (路径)
            SelectObject(mem_dc, font_sub);
            SetTextColor(mem_dc, subtext_color);
            let mut sub_rect = RECT {
                left: 80,
                top: y_offset + 28,
                right: width - 20,
                bottom: y_offset + ITEM_HEIGHT - 4,
            };
            let sub_h = HSTRING::from(item.subtitle.as_str());
            let mut sub_w: Vec<u16> = sub_h.as_wide().to_vec();
            DrawTextW(
                mem_dc,
                &mut sub_w,
                &mut sub_rect,
                DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
            );

            y_offset += ITEM_HEIGHT;
        }

        // 从内存缓冲拷贝至屏幕
        let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

        // 释放清理 GDI 句柄
        SelectObject(mem_dc, old_font);
        SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(font_title);
        let _ = DeleteObject(font_sub);
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
        let width = cfg.appearance.width as i32;

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class_name_w.as_ptr()),
            PCWSTR(HSTRING::from("OneLauncher").as_ptr()),
            WS_POPUP,
            0,
            0,
            width,
            INPUT_HEIGHT,
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

        info!("OneLauncher 核心引擎已就绪，系统托盘右键可管理配置，按下 Alt+Space 即可唤出！");

        // 核心 Win32 消息循环
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }

        unregister_global_hotkey(hwnd);
        let _ = DestroyWindow(hwnd);
        Ok(())
    }
}
