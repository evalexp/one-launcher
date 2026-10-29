use crate::config::ConfigManager;
use crate::hotkey::{register_global_hotkey, unregister_global_hotkey};
use crate::indexer::AppIndexManager;
use crate::plugin::PluginManager;
use crate::tray::{
    TrayIcon, ID_TRAY_EDIT_CONFIG, ID_TRAY_EXIT, ID_TRAY_RELOAD_CONFIG, WM_TRAYICON,
};
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
    KillTimer, LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassW, SetCaretPos, SetCursor,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowCaret, ShowWindow,
    TranslateMessage, DI_NORMAL, GWLP_USERDATA, HWND_TOPMOST, IDC_ARROW, MSG, SM_CXSCREEN,
    SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW,
    WA_INACTIVE, WM_ACTIVATE, WM_CHAR, WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY, WM_HOTKEY,
    WM_KEYDOWN, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_MOUSEACTIVATE, WM_PAINT, WM_RBUTTONUP,
    WM_SETCURSOR, WM_SETFOCUS, WM_TIMER, WM_USER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

const WINDOW_CLASS_NAME: &str = "OneLauncher_MainWindow_Class";
const INPUT_HEIGHT: i32 = 58;
const ITEM_HEIGHT: i32 = 56;
const FOOTER_HEIGHT: i32 = 28;
const TIMER_PLUGIN_DEBOUNCE: usize = 1001;
const WM_APP_PLUGIN_RESULTS: u32 = WM_USER + 101;

pub enum PluginWorkerTask {
    Query {
        request_id: u64,
        plugin_id: String,
        input: String,
    },
    Select {
        plugin_id: String,
        item_id: String,
        payload: String,
    },
    Reload {
        configs: std::collections::HashMap<String, crate::config::PluginConfig>,
    },
}

pub enum PluginWorkerResponse {
    Prefixes(Vec<(String, String, String)>),
    Results {
        request_id: u64,
        plugin_id: String,
        plugin_name: String,
        items: Vec<crate::plugin::model::PluginResultItem>,
    },
}

pub struct LauncherWindowState {
    hwnd: HWND,
    config_mgr: ConfigManager,
    app_indexer: AppIndexManager,
    plugin_prefixes: Vec<(String, String, String)>,
    tx_worker: crossbeam_channel::Sender<PluginWorkerTask>,
    rx_worker: crossbeam_channel::Receiver<PluginWorkerResponse>,
    tray: Option<TrayIcon>,
    query: String,
    results: Vec<SearchResultItem>,
    selected_index: usize,
    width: i32,
    caret_x: i32,
    request_id: u64,
    pending_plugin_id: String,
    pending_plugin_name: String,
    pending_query: String,
}

fn extract_plugin_prefixes(
    configs: &std::collections::HashMap<String, crate::config::PluginConfig>,
) -> Vec<(String, String, String)> {
    let mut list = Vec::new();
    for (id, cfg) in configs {
        if !cfg.enabled {
            continue;
        }

        let mut prefix = cfg.prefix.clone();
        let mut name = cfg.name.clone();

        // 若配置文件未显式指定 prefix 或 name，从插件 index.js 的 metadata 声明中解析
        if prefix.is_empty() || name.is_empty() {
            let dir = crate::plugin::resolve_plugin_dir(&cfg.get_path(id));
            let entry = dir.join(&cfg.entry);
            if let Ok(content) = std::fs::read_to_string(&entry) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if prefix.is_empty() {
                        if let Some(idx) = trimmed.find("prefix") {
                            let after = trimmed[idx + "prefix".len()..].trim_start();
                            if after.starts_with(':') {
                                let val = after[1..].trim().trim_matches(|c| c == '"' || c == '\'' || c == ',' || c == ' ');
                                if !val.is_empty() {
                                    prefix = val.to_string();
                                }
                            }
                        }
                    }
                    if name.is_empty() {
                        if let Some(idx) = trimmed.find("name") {
                            let after = trimmed[idx + "name".len()..].trim_start();
                            if after.starts_with(':') {
                                let val = after[1..].trim().trim_matches(|c| c == '"' || c == '\'' || c == ',' || c == ' ');
                                if !val.is_empty() {
                                    name = val.to_string();
                                }
                            }
                        }
                    }
                }
            }
        }

        if name.is_empty() {
            name = id.clone();
        }

        if !prefix.is_empty() {
            list.push((id.clone(), name, prefix));
        }
    }
    list.sort_by(|a, b| b.2.len().cmp(&a.2.len()));
    list
}

impl LauncherWindowState {
    pub fn new(
        hwnd: HWND,
        config_mgr: ConfigManager,
        app_indexer: AppIndexManager,
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

        let (tx_worker, rx_task) = crossbeam_channel::unbounded::<PluginWorkerTask>();
        let (tx_res, rx_worker) = crossbeam_channel::unbounded::<PluginWorkerResponse>();

        let plugin_prefixes = extract_plugin_prefixes(&cfg.plugins);
        let worker_hwnd_val = hwnd.0 as isize;
        let initial_configs = cfg.plugins.clone();
        let config_path = config_mgr.path().to_path_buf();

        // 启动专属的后台插件 Worker 线程 (彻底解放 Win32 UI 线程，QuickJS 与 FFI 全程在后台运行)
        std::thread::Builder::new()
            .name("OneLauncher-PluginWorker".to_string())
            .spawn(move || {
                let mut p_mgr = PluginManager::init_with_host(&initial_configs, worker_hwnd_val, Some(config_path.clone()));

                // 将 Worker 实际加载解析后的精准 (id, name, prefix) 派发给 UI 线程
                let prefixes: Vec<(String, String, String)> = p_mgr
                    .plugins
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone(), p.prefix.clone()))
                    .collect();
                let _ = tx_res.send(PluginWorkerResponse::Prefixes(prefixes));
                unsafe {
                    let target_hwnd = HWND(worker_hwnd_val as *mut _);
                    let _ = PostMessageW(
                        target_hwnd,
                        WM_APP_PLUGIN_RESULTS,
                        windows::Win32::Foundation::WPARAM(0),
                        windows::Win32::Foundation::LPARAM(0),
                    );
                }

                while let Ok(task) = rx_task.recv() {
                    match task {
                        PluginWorkerTask::Query { request_id, plugin_id, input } => {
                            let plugin_name = p_mgr.get_plugin_name(&plugin_id);
                            let items = p_mgr.query(&plugin_id, &input);
                            let _ = tx_res.send(PluginWorkerResponse::Results {
                                request_id,
                                plugin_id,
                                plugin_name,
                                items,
                            });
                            unsafe {
                                let target_hwnd = HWND(worker_hwnd_val as *mut _);
                                let _ = PostMessageW(
                                    target_hwnd,
                                    WM_APP_PLUGIN_RESULTS,
                                    windows::Win32::Foundation::WPARAM(request_id as usize),
                                    windows::Win32::Foundation::LPARAM(0),
                                );
                            }
                        }
                        PluginWorkerTask::Select { plugin_id, item_id, payload } => {
                            p_mgr.select(&plugin_id, &item_id, &payload);
                        }
                        PluginWorkerTask::Reload { configs } => {
                            p_mgr = PluginManager::init_with_host(&configs, worker_hwnd_val, Some(config_path.clone()));
                            let prefixes: Vec<(String, String, String)> = p_mgr
                                .plugins
                                .iter()
                                .map(|p| (p.id.clone(), p.name.clone(), p.prefix.clone()))
                                .collect();
                            let _ = tx_res.send(PluginWorkerResponse::Prefixes(prefixes));
                            unsafe {
                                let target_hwnd = HWND(worker_hwnd_val as *mut _);
                                let _ = PostMessageW(
                                    target_hwnd,
                                    WM_APP_PLUGIN_RESULTS,
                                    windows::Win32::Foundation::WPARAM(0),
                                    windows::Win32::Foundation::LPARAM(0),
                                );
                            }
                        }
                    }
                }
            })
            .expect("创建插件后台 Worker 线程失败");

        let mut state = Self {
            hwnd,
            config_mgr,
            app_indexer,
            plugin_prefixes,
            tx_worker,
            rx_worker,
            tray,
            query: String::new(),
            results: Vec::new(),
            selected_index: 0,
            width,
            caret_x: 48,
            request_id: 0,
            pending_plugin_id: String::new(),
            pending_plugin_name: String::new(),
            pending_query: String::new(),
        };
        state.update_search();
        state
    }

    /// 手动重新加载配置（由托盘菜单触发）
    pub fn reload_config(&mut self) {
        match self.config_mgr.reload() {
            Ok(new_cfg) => {
                crate::plugin::host_http::set_global_proxy(
                    new_cfg.network.get_effective_proxy().map(str::to_string),
                );
                if let Err(err) = register_global_hotkey(self.hwnd, &new_cfg.general.hotkey) {
                    warn!("重新注册热键提示: {}", err);
                }
                self.app_indexer.reload(&new_cfg.apps);
                self.plugin_prefixes = extract_plugin_prefixes(&new_cfg.plugins);
                let _ = self.tx_worker.send(PluginWorkerTask::Reload {
                    configs: new_cfg.plugins.clone(),
                });
                self.width = new_cfg.appearance.width.max(500) as i32;
                self.update_search();
                self.update_window_size();
                unsafe {
                    let _ = InvalidateRect(self.hwnd, None, false);
                }
                info!("配置与插件重载完成");
            }
            Err(err) => {
                warn!("重载配置失败: {}", err);
            }
        }
    }

    /// 执行搜索与数据路由分发
    pub fn update_search(&mut self) {
        let cfg = self.config_mgr.get();
        let max_results = cfg.general.max_results;
        let query_trimmed_start = self.query.trim_start();

        let mut items = Vec::new();

        // 默认不显示任何结果，只有在输入后才搜索显示结果
        if query_trimmed_start.is_empty() {
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_PLUGIN_DEBOUNCE);
            }
            self.pending_plugin_id.clear();
            self.pending_query.clear();
            self.results = items;
            self.selected_index = 0;
            self.update_window_size();
            return;
        }

        // 检查是否命中已注册的插件前缀
        let mut matched_plugin = None;
        for (p_id, p_name, p_prefix) in &self.plugin_prefixes {
            if query_trimmed_start.starts_with(p_prefix) {
                let pure_query = query_trimmed_start[p_prefix.len()..].trim_start().to_string();
                matched_plugin = Some((p_id.clone(), p_name.clone(), pure_query));
                break;
            }
        }

        if let Some((plugin_id, plugin_name, pure_query)) = matched_plugin {
            self.pending_plugin_id = plugin_id;
            self.pending_plugin_name = plugin_name;
            self.pending_query = pure_query;
            self.request_id = self.request_id.wrapping_add(1);

            let debounce_ms = cfg.general.plugin_debounce_ms;
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_PLUGIN_DEBOUNCE);
                if debounce_ms > 0 {
                    let _ = SetTimer(self.hwnd, TIMER_PLUGIN_DEBOUNCE, debounce_ms, None);
                } else {
                    let _ = self.tx_worker.send(PluginWorkerTask::Query {
                        request_id: self.request_id,
                        plugin_id: self.pending_plugin_id.clone(),
                        input: self.pending_query.clone(),
                    });
                }
            }
        } else {
            // 未匹配前缀，执行系统应用搜索
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_PLUGIN_DEBOUNCE);
            }
            self.pending_plugin_id.clear();
            self.pending_query.clear();
            let q_trimmed = query_trimmed_start.trim();
            let matched_apps = self.app_indexer.search(q_trimmed, max_results);
            for app in matched_apps {
                items.push(SearchResultItem::from_app(app));
            }
            self.results = items;
            if self.selected_index >= self.results.len() {
                self.selected_index = 0;
            }
            self.update_window_size();
        }
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
            SearchResultKind::Plugin { plugin_id, item_id, payload } => {
                let _ = self.tx_worker.send(PluginWorkerTask::Select {
                    plugin_id: plugin_id.clone(),
                    item_id: item_id.clone(),
                    payload: payload.clone(),
                });
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
            WM_TIMER if wparam.0 == TIMER_PLUGIN_DEBOUNCE => {
                let _ = KillTimer(hwnd, TIMER_PLUGIN_DEBOUNCE);
                if !state.pending_plugin_id.is_empty() {
                    let _ = state.tx_worker.send(PluginWorkerTask::Query {
                        request_id: state.request_id,
                        plugin_id: state.pending_plugin_id.clone(),
                        input: state.pending_query.clone(),
                    });
                }
                LRESULT(0)
            }
            WM_APP_PLUGIN_RESULTS => {
                let mut updated = false;
                while let Ok(msg) = state.rx_worker.try_recv() {
                    match msg {
                        PluginWorkerResponse::Prefixes(mut prefixes) => {
                            prefixes.sort_by(|a, b| b.2.len().cmp(&a.2.len()));
                            state.plugin_prefixes = prefixes;
                            if !state.query.trim_start().is_empty() {
                                state.update_search();
                                updated = true;
                            }
                        }
                        PluginWorkerResponse::Results {
                            request_id,
                            plugin_id,
                            plugin_name,
                            items,
                        } => {
                            if request_id == state.request_id {
                                let mut res_items = Vec::new();
                                for p_item in items {
                                    res_items.push(SearchResultItem::from_plugin(&plugin_name, &plugin_id, p_item));
                                }
                                state.results = res_items;
                                if state.selected_index >= state.results.len() {
                                    state.selected_index = 0;
                                }
                                state.update_window_size();
                                updated = true;
                            }
                        }
                    }
                }
                if updated {
                    let _ = InvalidateRect(hwnd, None, false);
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

/// 绘制启动器界面
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

        // 7. 绘制列表条目
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

            // 1. 左侧绘制 32x32 图标 (对文件夹使用预缓存文件夹图标，对普通文件使用预缓存文件图标，对应用使用原生抽取图标)
            let icon_x = 16;
            let icon_y = y_offset + 12;

            let icon_opt = if !item.icon_path.is_empty() {
                crate::ui::icon_extractor::get_icon_by_ref(&item.icon_path)
            } else {
                crate::ui::icon_extractor::get_file_icon()
            };

            if let Some(hicon) = icon_opt {
                let _ = DrawIconEx(mem_dc, icon_x, icon_y, hicon, 32, 32, 0, None, DI_NORMAL);
            } else {
                // 回退轻量图标占位符
                SelectObject(mem_dc, font_search);
                SetTextColor(mem_dc, COLORREF(0x00F8BD38));
                let mut fallback_rect = RECT { left: icon_x, top: icon_y, right: icon_x + 32, bottom: icon_y + 32 };
                let sym = match &item.kind {
                    SearchResultKind::App(_) => "⚡",
                    _ if item.icon_path == "folder" || item.icon_path == "dir" => "📁",
                    _ => "📄",
                };
                let sym_h = HSTRING::from(sym);
                let mut sym_w = sym_h.as_wide().to_vec();
                let _ = DrawTextW(mem_dc, &mut sym_w, &mut fallback_rect, DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
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
                SearchResultKind::Plugin { .. } if item.badge == "文件夹" || item.icon_path == "folder" => (COLORREF(0x00112233), COLORREF(0x0024BFFB)), // 琥珀暖黄 #FBBF24
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

        // 启用窗口圆角
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
        ));

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

        info!("引擎已就绪");

        // Win32 消息循环
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
