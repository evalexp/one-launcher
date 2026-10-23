use crate::config::AppsConfig;
use crate::indexer::model::AppItem;
use crate::indexer::pinyin_helper::generate_pinyin;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

pub struct AppScanner;

impl AppScanner {
    /// 根据配置扫描所有系统的已安装应用程序
    pub fn scan_all(config: &AppsConfig) -> Vec<AppItem> {
        let mut apps = Vec::new();
        let mut seen_names = HashSet::new();

        // 1. 扫描系统与用户开始菜单目录
        let mut scan_dirs = Self::get_standard_start_menu_dirs();
        // 2. 添加用户自定义目录
        for extra in &config.extra_directories {
            if extra.exists() && extra.is_dir() {
                scan_dirs.push(extra.clone());
            }
        }

        for dir in scan_dirs {
            Self::scan_directory(&dir, config, &mut apps, &mut seen_names);
        }

        // 3. 扫描 Windows UWP 应用
        if config.scan_uwp {
            Self::scan_common_uwp_apps(&mut apps, &mut seen_names);
        }

        info!("系统应用程序扫描完毕，共发现有效启动项: {} 个", apps.len());
        apps
    }

    /// 获取 Windows 标准开始菜单目录列表
    pub fn get_standard_start_menu_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // 机器级开始菜单 (C:\ProgramData\Microsoft\Windows\Start Menu\Programs)
        if let Ok(program_data) = std::env::var("ProgramData") {
            let p = PathBuf::from(program_data).join(r"Microsoft\Windows\Start Menu\Programs");
            if p.exists() {
                dirs.push(p);
            }
        }

        // 用户级开始菜单 (%APPDATA%\Microsoft\Windows\Start Menu\Programs)
        if let Some(app_data) = dirs::data_dir() {
            let p = app_data.join(r"Microsoft\Windows\Start Menu\Programs");
            if p.exists() {
                dirs.push(p);
            }
        }

        dirs
    }

    /// 递归扫描一个文件夹并收录符合扩展名和黑名单过滤规则的文件
    fn scan_directory(
        dir: &Path,
        config: &AppsConfig,
        apps: &mut Vec<AppItem>,
        seen: &mut HashSet<String>,
    ) {
        let entries = match fs::read_dir(dir) {
            Ok(iter) => iter,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                Self::scan_directory(&path, config, apps, seen);
                continue;
            }

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if !config.extensions.iter().any(|e| e.to_lowercase() == ext) {
                continue;
            }

            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .trim();

            if file_stem.is_empty() {
                continue;
            }

            // 检查排除黑名单
            let path_str = path.to_string_lossy();
            let is_excluded = config.exclude_patterns.iter().any(|pattern| {
                file_stem.contains(pattern) || path_str.contains(pattern)
            });

            if is_excluded {
                continue;
            }

            let key = file_stem.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);

            let (pinyin_full, pinyin_abbr) = generate_pinyin(file_stem);
            let description = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            apps.push(AppItem::new(
                file_stem.to_string(),
                path,
                description,
                pinyin_full,
                pinyin_abbr,
            ));
        }
    }

    /// 预置并扫描常见的 Windows 内置核心应用 (计算器、记事本、系统设置、任务管理器等)
    fn scan_common_uwp_apps(apps: &mut Vec<AppItem>, seen: &mut HashSet<String>) {
        let uwp_presets = [
            ("计算器", "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"),
            ("Windows 设置", "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel"),
            ("任务管理器", "Microsoft.Windows.TaskTaskManager_8wekyb3d8bbwe!App"),
            ("截图工具", "Microsoft.ScreenSketch_8wekyb3d8bbwe!App"),
            ("时钟", "Microsoft.WindowsAlarms_8wekyb3d8bbwe!App"),
            ("画图", "Microsoft.Paint_8wekyb3d8bbwe!App"),
            ("相机", "Microsoft.WindowsCamera_8wekyb3d8bbwe!App"),
            ("Microsoft Store", "Microsoft.WindowsStore_8wekyb3d8bbwe!App"),
        ];

        for (name, aumid) in uwp_presets {
            let key = name.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);

            let (pinyin_full, pinyin_abbr) = generate_pinyin(name);
            apps.push(AppItem::new_uwp(
                name.to_string(),
                aumid.to_string(),
                "系统内置应用".to_string(),
                pinyin_full,
                pinyin_abbr,
            ));
        }
    }
}
