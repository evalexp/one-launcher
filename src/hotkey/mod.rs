use tracing::info;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};

pub const DEFAULT_HOTKEY_ID: i32 = 0x1001;

/// 解析快捷键字符串 (例如 "Alt+Space", "Ctrl+Shift+Space")
pub fn parse_hotkey(hotkey_str: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let mut modifiers = MOD_NOREPEAT.0;
    let mut vk_code = 0u32;

    for part in hotkey_str.split('+') {
        let trimmed = part.trim();
        match trimmed.to_lowercase().as_str() {
            "alt" => modifiers |= MOD_ALT.0,
            "ctrl" | "control" => modifiers |= MOD_CONTROL.0,
            "shift" => modifiers |= MOD_SHIFT.0,
            "win" | "windows" => modifiers |= MOD_WIN.0,
            "space" => vk_code = 0x20, // VK_SPACE
            "tab" => vk_code = 0x09,
            "enter" | "return" => vk_code = 0x0D,
            "esc" | "escape" => vk_code = 0x1B,
            s if s.starts_with('f') && s.len() > 1 => {
                if let Ok(num) = s[1..].parse::<u32>() {
                    if (1..=24).contains(&num) {
                        vk_code = 0x70 + (num - 1); // VK_F1..VK_F24
                    }
                }
            }
            s if s.len() == 1 => {
                let ch = s.chars().next()?.to_ascii_uppercase();
                if ch.is_ascii_alphanumeric() {
                    vk_code = ch as u32;
                }
            }
            _ => return None,
        }
    }

    if vk_code == 0 {
        return None;
    }

    Some((HOT_KEY_MODIFIERS(modifiers), vk_code))
}

/// 注册全局唤出热键
pub fn register_global_hotkey(hwnd: HWND, hotkey_str: &str) -> Result<(), String> {
    let (modifiers, vk) = parse_hotkey(hotkey_str)
        .ok_or_else(|| format!("无效的热键格式: '{}' (支持如 'Alt+Space')", hotkey_str))?;

    unsafe {
        // 先注销原有热键
        let _ = UnregisterHotKey(hwnd, DEFAULT_HOTKEY_ID);

        RegisterHotKey(hwnd, DEFAULT_HOTKEY_ID, modifiers, vk)
            .map_err(|e| format!("注册全局热键 '{}' 失败 (可能被其他程序占用): {}", hotkey_str, e))?;

        info!("成功注册全局呼出热键: {}", hotkey_str);
        Ok(())
    }
}

/// 注销全局热键
pub fn unregister_global_hotkey(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(hwnd, DEFAULT_HOTKEY_ID);
    }
}
