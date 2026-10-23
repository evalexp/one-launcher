use one_launcher::hotkey::parse_hotkey;
use windows::Win32::UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN};

#[test]
fn test_parse_hotkey() {
    // 默认 Alt+Space
    let (mod1, vk1) = parse_hotkey("Alt+Space").unwrap();
    assert_eq!(mod1.0, MOD_ALT.0 | MOD_NOREPEAT.0);
    assert_eq!(vk1, 0x20); // VK_SPACE

    // Ctrl+Shift+P
    let (mod2, vk2) = parse_hotkey("Ctrl+Shift+P").unwrap();
    assert_eq!(mod2.0, MOD_CONTROL.0 | MOD_SHIFT.0 | MOD_NOREPEAT.0);
    assert_eq!(vk2, b'P' as u32);

    // Win+F12
    let (mod3, vk3) = parse_hotkey("Win+F12").unwrap();
    assert_eq!(mod3.0, MOD_WIN.0 | MOD_NOREPEAT.0);
    assert_eq!(vk3, 0x70 + 11); // VK_F12

    // 无效格式
    assert!(parse_hotkey("InvalidKeyCombination").is_none());
}
