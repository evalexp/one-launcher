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
