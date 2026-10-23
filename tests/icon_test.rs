use one_launcher::ui::icon_extractor::get_icon_for_path;

#[test]
fn test_icon_extraction_for_system_app() {
    let explorer_path = r"C:\Windows\explorer.exe";
    let icon_opt = get_icon_for_path(explorer_path);
    assert!(
        icon_opt.is_some(),
        "应能成功提取 Windows 资源管理器 (explorer.exe) 的 HICON 图标"
    );

    // 测试空路径返回 None
    let empty_icon = get_icon_for_path("");
    assert!(empty_icon.is_none());

    // 再次读取（命中缓存）
    let cached_icon = get_icon_for_path(explorer_path);
    assert!(cached_icon.is_some());
}

#[test]
fn test_uwp_icon_extraction() {
    // Windows Calculator UWP
    let calc_path = r"shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App";
    let icon_opt = get_icon_for_path(calc_path);
    assert!(
        icon_opt.is_some(),
        "应能成功提取 UWP 应用 (计算器) 的 32x32 原生 HICON 图标"
    );

    // 再次读取（验证缓存命中）
    let cached = get_icon_for_path(calc_path);
    assert!(cached.is_some());
}
