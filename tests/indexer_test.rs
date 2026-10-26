use one_launcher::config::AppsConfig;
use one_launcher::indexer::matcher::AppMatcher;
use one_launcher::indexer::model::AppItem;
use one_launcher::indexer::pinyin_helper::generate_pinyin;
use one_launcher::indexer::scanner::AppScanner;
use std::path::PathBuf;

#[test]
fn test_pinyin_generation() {
    let (full1, abbr1) = generate_pinyin("微信");
    assert_eq!(abbr1, "wx");
    assert!(full1.contains("weixin"));

    let (full2, abbr2) = generate_pinyin("网易云音乐");
    assert_eq!(abbr2, "wyyyl");
    assert!(full2.contains("wangyiyunyin"));

    let (full3, abbr3) = generate_pinyin("Visual Studio Code");
    assert_eq!(full3, "visual studio code");
    assert_eq!(abbr3, "vsc");
}

#[test]
fn test_app_fuzzy_and_pinyin_matching() {
    let matcher = AppMatcher::new();
    let apps = vec![
        AppItem::new(
            "微信".to_string(),
            PathBuf::from("C:\\WeChat.exe"),
            "".to_string(),
            "weixin".to_string(),
            "wx".to_string(),
        ),
        AppItem::new(
            "企业微信".to_string(),
            PathBuf::from("C:\\WXWork.exe"),
            "".to_string(),
            "qiyeweixin".to_string(),
            "qywx".to_string(),
        ),
        AppItem::new(
            "网易云音乐".to_string(),
            PathBuf::from("C:\\CloudMusic.exe"),
            "".to_string(),
            "wangyiyunyinyue".to_string(),
            "wyyyy".to_string(),
        ),
        AppItem::new(
            "Google Chrome".to_string(),
            PathBuf::from("C:\\chrome.exe"),
            "".to_string(),
            "google chrome".to_string(),
            "google chrome".to_string(),
        ),
    ];

    // 输入 "wx" 应该优先匹配 "微信"
    let results_wx = matcher.match_apps(&apps, "wx", 5);
    assert!(!results_wx.is_empty());
    assert_eq!(results_wx[0].name, "微信");

    // 输入 "wyy" 应该匹配 "网易云音乐"
    let results_wyy = matcher.match_apps(&apps, "wyy", 5);
    assert!(!results_wyy.is_empty());
    assert_eq!(results_wyy[0].name, "网易云音乐");

    // 输入 "chrome" 应该匹配 "Google Chrome"
    let results_chrome = matcher.match_apps(&apps, "chrome", 5);
    assert!(!results_chrome.is_empty());
    assert_eq!(results_chrome[0].name, "Google Chrome");
}

#[test]
fn test_real_system_apps_scanning() {
    let config = AppsConfig::default();
    let apps = AppScanner::scan_all(&config);
    println!("本机已安装应用扫描结果条数: {}", apps.len());
    assert!(!apps.is_empty(), "标准 Windows 系统中应能扫描到应用快捷方式或 UWP 应用");

    for app in apps.iter().take(5) {
        println!("- 扫描到应用: {} ({:?}) [简拼: {}]", app.name, app.path, app.pinyin_abbr);
    }

    // 验证包含终端 (Windows Terminal / 终端)
    let found_terminal = apps.iter().find(|a| a.name == "终端" || a.name == "Windows Terminal");
    assert!(found_terminal.is_some(), "动态枚举中必须能扫描到终端应用");
    println!("成功检索到终端应用: {:?}", found_terminal.unwrap().name);
}
