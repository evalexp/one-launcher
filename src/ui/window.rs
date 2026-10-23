use crate::config::ConfigManager;
use crate::everything::EverythingClient;
use crate::hotkey::{register_global_hotkey, unregister_global_hotkey};
use crate::indexer::{AppIndexManager, AppItem};
use crate::tray::{
    TrayIcon, ID_TRAY_EDIT_CONFIG, ID_TRAY_EXIT, ID_TRAY_RELOAD_CONFIG, WM_TRAYICON,
};
use crate::ui::icon_extractor::get_icon_for_path;
use parking_lot::Mutex;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use slint::{ComponentHandle, SharedString, VecModel, Weak};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::OnceLock;
use tracing::{error, info, warn};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetActiveWindow, SetFocus};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetSystemMetrics, GetWindowLongPtrW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, TranslateMessage, GWL_EXSTYLE,
    HWND_MESSAGE, HWND_TOPMOST, MSG, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, WA_INACTIVE, WM_ACTIVATE, WM_COMMAND, WM_CONTEXTMENU, WM_DESTROY,
    WM_HOTKEY, WM_RBUTTONUP, WNDCLASSW, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

slint::include_modules!();

const SLINT_SUBCLASS_ID: usize = 0x4001;
static SLINT_WINDOW_WEAK: OnceLock<Mutex<Option<Weak<AppWindow>>>> = OnceLock::new();
static MSG_HWND_CONTAINER: OnceLock<Mutex<Option<isize>>> = OnceLock::new();

#[derive(Clone, Debug)]
enum LaunchTarget {
    App(AppItem),
    File(PathBuf),
}

/// 拦截 Slint 主窗口的 Win32 消息，监听失去焦点 (WM_ACTIVATE -> WA_INACTIVE) 并自动隐藏
unsafe extern "system" fn slint_wnd_subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _uid: usize,
    _data: usize,
) -> LRESULT {
    if msg == WM_ACTIVATE {
        let state = (wparam.0 & 0xFFFF) as u32;
        if state == WA_INACTIVE {
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(lock) = SLINT_WINDOW_WEAK.get() {
                    if let Some(weak) = lock.lock().as_ref() {
                        if let Some(win) = weak.upgrade() {
                            if win.window().is_visible() {
                                let _ = win.hide();
                            }
                        }
                    }
                }
            });
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

/// 后台消息专用窗口的 WndProc（管理系统托盘与全局热键）
unsafe extern "system" fn msg_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_HOTKEY => {
            unsafe {
                let _ = AllowSetForegroundWindow(std::process::id());
            }
            let _ = slint::invoke_from_event_loop(move || {
                toggle_launcher_window();
            });
            LRESULT(0)
        }
        WM_TRAYICON => {
            let event = (lparam.0 & 0xFFFF) as u32;
            if event == WM_RBUTTONUP || event == WM_CONTEXTMENU {
                // 托盘右键菜单
                unsafe {
                    let state_ptr = GetWindowLongPtrW(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                    ) as *mut TrayContext;
                    if !state_ptr.is_null() {
                        let ctx = &*state_ptr;
                        if let Some(tray) = &ctx.tray {
                            tray.show_context_menu();
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let cmd_id = (wparam.0 & 0xFFFF) as usize;
            unsafe {
                let state_ptr = GetWindowLongPtrW(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
                ) as *mut TrayContext;

                if !state_ptr.is_null() {
                    let ctx = &mut *state_ptr;
                    match cmd_id {
                        ID_TRAY_EDIT_CONFIG => {
                            if let Err(e) = ctx.config_mgr.open_in_editor() {
                                warn!("打开配置文件失败: {}", e);
                            }
                        }
                        ID_TRAY_RELOAD_CONFIG => {
                            match ctx.config_mgr.reload() {
                                Ok(new_cfg) => {
                                    if let Err(e) = register_global_hotkey(hwnd, &new_cfg.general.hotkey) {
                                        warn!("重载热键提示: {}", e);
                                    }
                                    ctx.app_indexer.reload(&new_cfg.apps);
                                    info!("OneLauncher 配置与热键已成功重载生效！");
                                    // 刷新 UI 中的结果
                                    let app_indexer = ctx.app_indexer.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        refresh_initial_apps(&app_indexer);
                                    });
                                }
                                Err(e) => warn!("重载配置失败: {}", e),
                            }
                        }
                        ID_TRAY_EXIT => {
                            info!("正在退出 OneLauncher...");
                            let _ = slint::quit_event_loop();
                            PostQuitMessage(0);
                        }
                        _ => {}
                    }
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unregister_global_hotkey(hwnd);
            unsafe {
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

struct TrayContext {
    config_mgr: ConfigManager,
    app_indexer: AppIndexManager,
    tray: Option<TrayIcon>,
}

/// 切换启动器主窗口显示状态
fn toggle_launcher_window() {
    if let Some(lock) = SLINT_WINDOW_WEAK.get() {
        if let Some(weak) = lock.lock().as_ref() {
            if let Some(win) = weak.upgrade() {
                if win.window().is_visible() {
                    let _ = win.hide();
                } else {
                    let _ = win.show();
                    // 居中并置顶
                    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
                    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
                    let win_w = 720;
                    let x = (screen_w - win_w) / 2;
                    let y = (screen_h as f32 * 0.22) as i32;
                    win.window().set_position(slint::PhysicalPosition::new(x, y));

                    if let Ok(handle) = win.window().window_handle().window_handle() {
                        if let RawWindowHandle::Win32(wh) = handle.as_raw() {
                            let slint_hwnd = HWND(wh.hwnd.get() as _);
                            unsafe {
                                let _ = SetWindowPos(
                                    slint_hwnd,
                                    HWND_TOPMOST,
                                    0,
                                    0,
                                    0,
                                    0,
                                    SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                                );
                                let _ = SetForegroundWindow(slint_hwnd);
                                let _ = SetActiveWindow(slint_hwnd);
                                let _ = SetFocus(slint_hwnd);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 刷新默认初始应用列表
fn refresh_initial_apps(app_indexer: &AppIndexManager) {
    if let Some(lock) = SLINT_WINDOW_WEAK.get() {
        if let Some(weak) = lock.lock().as_ref() {
            if let Some(win) = weak.upgrade() {
                let initial_apps = app_indexer.search("", 20);
                let mut slint_items = Vec::with_capacity(initial_apps.len());
                for app in initial_apps {
                    let path_str = app.path.to_string_lossy().to_string();
                    let icon = get_icon_for_path(&path_str);
                    slint_items.push(SearchItemData {
                        title: SharedString::from(&app.name),
                        path: SharedString::from(&path_str),
                        badge: SharedString::from("应用"),
                        is_app: true,
                        is_folder: false,
                        has_icon: icon.is_some(),
                        icon: icon.unwrap_or_default(),
                    });
                }
                win.set_results(Rc::new(VecModel::from(slint_items)).into());
                win.set_selected_index(0);
            }
        }
    }
}

/// 主启动入口
pub fn run_launcher(
    config_mgr: ConfigManager,
    app_indexer: AppIndexManager,
    everything_client: EverythingClient,
) -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config_mgr.get();
    let window = AppWindow::new()?;
    let window_weak = window.as_weak();

    // 存储全局弱引用，供 Win32 回调安全调度
    SLINT_WINDOW_WEAK
        .get_or_init(|| Mutex::new(None))
        .lock()
        .replace(window_weak.clone());

    // 获取 Slint 原生 Win32 HWND 句柄
    let slint_handle = window.window().window_handle();
    let raw_handle = slint_handle.window_handle()?.as_raw();
    let slint_hwnd = match raw_handle {
        RawWindowHandle::Win32(handle) => HWND(handle.hwnd.get() as _),
        _ => return Err("未能获取 Win32 窗口句柄".into()),
    };

    // 设置初始几何属性与屏幕垂直 22% 黄金分割居中
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let win_w = cfg.appearance.width.max(500) as i32;
    let x = (screen_w - win_w) / 2;
    let y = (screen_h as f32 * 0.22) as i32;
    window.set_window_width(win_w);
    window.window().set_position(slint::PhysicalPosition::new(x, y));

    // 配置无边框圆角、置顶并移除任务栏图标 (WS_EX_TOOLWINDOW)
    unsafe {
        let ex_style = GetWindowLongPtrW(slint_hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(
            slint_hwnd,
            GWL_EXSTYLE,
            (ex_style | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize),
        );
        let _ = SetWindowPos(
            slint_hwnd,
            HWND_TOPMOST,
            x,
            y,
            win_w,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE,
        );
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            slint_hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const _,
            std::mem::size_of_val(&corner) as u32,
        );

        // 安装窗口子类过程，捕获 WA_INACTIVE 失去焦点时自动隐藏
        let _ = SetWindowSubclass(
            slint_hwnd,
            Some(slint_wnd_subclass_proc),
            SLINT_SUBCLASS_ID,
            0,
        );
    }

    // 后台独立线程创建用于托盘和全局热键的无干扰消息窗口
    let config_mgr_clone = config_mgr.clone();
    let app_indexer_clone = app_indexer.clone();
    let hotkey_str = cfg.general.hotkey.clone();

    std::thread::spawn(move || unsafe {
        let class_name = w!("OneLauncher_Tray_Msg_Class");
        let hinstance = GetModuleHandleW(None).unwrap_or_default();

        let wc = WNDCLASSW {
            lpfnWndProc: Some(msg_wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);

        let msg_hwnd_res = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name,
            w!("OneLauncher_Tray_Msg_Window"),
            windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            hinstance,
            None,
        );

        let msg_hwnd = match msg_hwnd_res {
            Ok(h) => h,
            Err(e) => {
                error!("创建后台托盘消息窗口失败: {}", e);
                return;
            }
        };

        MSG_HWND_CONTAINER
            .get_or_init(|| Mutex::new(None))
            .lock()
            .replace(msg_hwnd.0 as isize);

        // 初始化系统托盘图标
        let tray = match TrayIcon::create(msg_hwnd) {
            Ok(t) => Some(t),
            Err(e) => {
                warn!("未能创建托盘图标: {}", e);
                None
            }
        };

        // 注册全局热键
        if let Err(e) = register_global_hotkey(msg_hwnd, &hotkey_str) {
            warn!("注册全局呼出热键 '{}' 提示: {}", hotkey_str, e);
        }

        let ctx = Box::new(TrayContext {
            config_mgr: config_mgr_clone,
            app_indexer: app_indexer_clone,
            tray,
        });

        SetWindowLongPtrW(
            msg_hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            Box::into_raw(ctx) as isize,
        );

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });

    // 当前检索条目的启动目标映射
    let current_targets: Rc<RefCell<Vec<LaunchTarget>>> = Rc::new(RefCell::new(Vec::new()));

    // 初始化搜索结果：默认呈现常用系统应用
    {
        let initial_apps = app_indexer.search("", cfg.general.max_results);
        let mut slint_items = Vec::with_capacity(initial_apps.len());
        let mut targets = Vec::with_capacity(initial_apps.len());

        for app in initial_apps {
            let path_str = app.path.to_string_lossy().to_string();
            let icon = get_icon_for_path(&path_str);
            slint_items.push(SearchItemData {
                title: SharedString::from(&app.name),
                path: SharedString::from(&path_str),
                badge: SharedString::from("应用"),
                is_app: true,
                is_folder: false,
                has_icon: icon.is_some(),
                icon: icon.unwrap_or_default(),
            });
            targets.push(LaunchTarget::App(app));
        }

        *current_targets.borrow_mut() = targets;
        window.set_results(Rc::new(VecModel::from(slint_items)).into());
        window.set_selected_index(0);
    }

    // 1. 响应用户输入变化
    {
        let window_weak = window.as_weak();
        let app_indexer = app_indexer.clone();
        let everything_client = everything_client.clone();
        let config_mgr = config_mgr.clone();
        let current_targets = current_targets.clone();

        window.on_text_changed(move |text| {
            let cfg = config_mgr.get();
            let query = text.trim();
            let mut targets = Vec::new();
            let mut slint_items = Vec::new();

            if query.starts_with(&cfg.everything.prefix) {
                // Everything 文件秒级搜索
                let file_query = query[cfg.everything.prefix.len()..].trim();
                if !file_query.is_empty() {
                    match everything_client.search(file_query, cfg.everything.max_results as usize) {
                        Ok(items) => {
                            for item in items {
                                let icon = get_icon_for_path(&item.path);
                                let is_folder = item.is_folder;
                                slint_items.push(SearchItemData {
                                    title: SharedString::from(&item.name),
                                    path: SharedString::from(&item.path),
                                    badge: SharedString::from(if is_folder { "文件夹" } else { "Everything" }),
                                    is_app: false,
                                    is_folder,
                                    has_icon: icon.is_some(),
                                    icon: icon.unwrap_or_default(),
                                });
                                targets.push(LaunchTarget::File(item.full_path));
                            }
                        }
                        Err(e) => {
                            debug_assert!(true, "Everything 搜索异常: {}", e);
                        }
                    }
                }
            } else {
                // 应用程序搜索（拼音首字母/英文缩写/全名模糊匹配）
                let matched = app_indexer.search(query, cfg.general.max_results);
                for app in matched {
                    let path_str = app.path.to_string_lossy().to_string();
                    let icon = get_icon_for_path(&path_str);
                    slint_items.push(SearchItemData {
                        title: SharedString::from(&app.name),
                        path: SharedString::from(&path_str),
                        badge: SharedString::from("应用"),
                        is_app: true,
                        is_folder: false,
                        has_icon: icon.is_some(),
                        icon: icon.unwrap_or_default(),
                    });
                    targets.push(LaunchTarget::App(app));
                }
            }

            *current_targets.borrow_mut() = targets;
            if let Some(win) = window_weak.upgrade() {
                win.set_results(Rc::new(VecModel::from(slint_items)).into());
                win.set_selected_index(0);
            }
        });
    }

    // 2. 响应回车启动或点击激活
    {
        let window_weak = window.as_weak();
        let current_targets = current_targets.clone();

        window.on_item_activated(move |idx| {
            let target_opt = {
                let targets = current_targets.borrow();
                if idx >= 0 && (idx as usize) < targets.len() {
                    Some(targets[idx as usize].clone())
                } else {
                    None
                }
            };

            if let Some(target) = target_opt {
                match target {
                    LaunchTarget::App(app) => {
                        if let Err(e) = AppIndexManager::launch(&app) {
                            warn!("启动应用程序失败: {}", e);
                        }
                    }
                    LaunchTarget::File(path) => {
                        if let Err(e) = EverythingClient::open_file(&path) {
                            warn!("打开目标文件失败: {}", e);
                        }
                    }
                }

                if let Some(win) = window_weak.upgrade() {
                    let _ = win.hide();
                    win.set_search_text(SharedString::new());
                }
            }
        });
    }

    // 3. 响应 Esc 键关闭窗口
    {
        let window_weak = window.as_weak();
        window.on_escape_pressed(move || {
            if let Some(win) = window_weak.upgrade() {
                let _ = win.hide();
            }
        });
    }

    info!("OneLauncher 现代化界面已成功就绪，等待热键唤出。");

    // 初始显示一次并自动隐藏（让 Slint / Winit 完成 Win32 消息循环挂载）
    let _ = window.show();
    let _ = window.hide();

    // 运行 Slint 事件循环直到退出
    slint::run_event_loop()?;

    // 退出时销毁后台托盘窗口
    if let Some(lock) = MSG_HWND_CONTAINER.get() {
        if let Some(msg_hwnd_val) = *lock.lock() {
            unsafe {
                let _ = DestroyWindow(HWND(msg_hwnd_val as *mut _));
            }
        }
    }

    info!("OneLauncher 进程已安全优雅退出。");
    Ok(())
}
