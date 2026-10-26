use rquickjs::function::Opt;
use rquickjs::{Ctx, Function, Object};
use std::path::PathBuf;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, PostMessageW, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK,
    MB_SETFOREGROUND, MB_TOPMOST, WM_COMMAND,
};

/// 向 QuickJS 全局 launcher 对象挂载 app 模块 (launcher.app) 及系统级弹窗能力
pub fn register_app<'js>(
    ctx: &Ctx<'js>,
    launcher: &Object<'js>,
    hwnd: isize,
    config_path: PathBuf,
) -> Result<(), rquickjs::Error> {
    let app_obj = Object::new(ctx.clone())?;

    let cfg_str = config_path.to_string_lossy().to_string();
    app_obj.set("config_path", cfg_str.clone())?;

    // launcher.app.quit() / launcher.app.exit()
    let quit_fn = Function::new(ctx.clone(), move || {
        if hwnd != 0 {
            unsafe {
                let target = HWND(hwnd as *mut _);
                let _ = PostMessageW(
                    target,
                    WM_COMMAND,
                    WPARAM(crate::tray::ID_TRAY_EXIT),
                    LPARAM(0),
                );
            }
        } else {
            std::process::exit(0);
        }
        Ok::<_, rquickjs::Error>(())
    })?;
    app_obj.set("quit", quit_fn.clone())?;
    app_obj.set("exit", quit_fn.clone())?;
    launcher.set("exit", quit_fn)?;

    // launcher.app.reload()
    let reload_fn = Function::new(ctx.clone(), move || {
        if hwnd != 0 {
            unsafe {
                let target = HWND(hwnd as *mut _);
                let _ = PostMessageW(
                    target,
                    WM_COMMAND,
                    WPARAM(crate::tray::ID_TRAY_RELOAD_CONFIG),
                    LPARAM(0),
                );
            }
        }
        Ok::<_, rquickjs::Error>(())
    })?;
    app_obj.set("reload", reload_fn)?;

    // launcher.app.edit_config()
    let cfg_str_clone = cfg_str.clone();
    let edit_fn = Function::new(ctx.clone(), move |ctx: Ctx<'js>| {
        if hwnd != 0 {
            unsafe {
                let target = HWND(hwnd as *mut _);
                let _ = PostMessageW(
                    target,
                    WM_COMMAND,
                    WPARAM(crate::tray::ID_TRAY_EDIT_CONFIG),
                    LPARAM(0),
                );
            }
        } else {
            let launcher_obj: Object = ctx.globals().get("launcher")?;
            let shell_obj: Object = launcher_obj.get("shell")?;
            let open_fn: Function = shell_obj.get("open")?;
            let _ = open_fn.call::<_, ()>((cfg_str_clone.clone(),));
        }
        Ok::<_, rquickjs::Error>(())
    })?;
    app_obj.set("edit_config", edit_fn)?;

    // launcher.message_box(text, title?, type?) / launcher.alert
    let msgbox_fn = Function::new(
        ctx.clone(),
        move |text: String, title: Opt<String>, msg_type: Opt<String>| {
            let title_str = title.0.unwrap_or_else(|| "OneLauncher".to_string());
            let text_w = HSTRING::from(text.clone());
            let title_w = HSTRING::from(title_str.clone());

            let icon_flag = match msg_type.0.as_deref() {
                Some("error") => MB_ICONERROR,
                Some("warn") | Some("warning") => MB_ICONWARNING,
                _ => MB_ICONINFORMATION,
            };

            let style = MB_OK | icon_flag | MB_TOPMOST | MB_SETFOREGROUND;

            if hwnd != 0 {
                unsafe {
                    let _ = MessageBoxW(
                        HWND::default(),
                        PCWSTR(text_w.as_ptr()),
                        PCWSTR(title_w.as_ptr()),
                        style,
                    );
                }
            } else {
                tracing::info!("[MessageBox] {}: {}", title_str, text);
            }
            Ok::<_, rquickjs::Error>(())
        },
    )?;
    app_obj.set("message_box", msgbox_fn.clone())?;
    launcher.set("message_box", msgbox_fn.clone())?;
    launcher.set("alert", msgbox_fn)?;

    launcher.set("app", app_obj)?;
    Ok(())
}
