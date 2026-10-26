use crate::config::AppsConfig;
use crate::indexer::model::AppItem;
use crate::indexer::pinyin_helper::generate_pinyin;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;
use windows::core::HSTRING;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, IEnumShellItems, IShellItem, SHCreateItemFromParsingName,
    SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY,
};

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
            Self::scan_uwp_apps(config, &mut apps, &mut seen_names);
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

    /// 通过 Windows Shell AppsFolder 动态枚举已安装的系统与 UWP/MSIX 应用（如 Windows 终端、应用商店应用等）
    fn scan_uwp_apps(config: &AppsConfig, apps: &mut Vec<AppItem>, seen: &mut HashSet<String>) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let apps_folder_name = HSTRING::from("shell:AppsFolder");
            if let Ok(folder_item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&apps_folder_name, None) {
                if let Ok(enum_items) = folder_item.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) {
                    let mut item_arr: [Option<IShellItem>; 32] = Default::default();
                    loop {
                        let mut fetched = 0;
                        let res = enum_items.Next(&mut item_arr, Some(&mut fetched));
                        if res.is_err() || fetched == 0 {
                            break;
                        }

                        for j in 0..fetched as usize {
                            if let Some(ref item) = item_arr[j] {
                                let name = match item.GetDisplayName(SIGDN_NORMALDISPLAY) {
                                    Ok(pw) => {
                                        let s = pw.to_string().unwrap_or_default();
                                        windows::Win32::System::Com::CoTaskMemFree(Some(pw.0 as *const _));
                                        s
                                    }
                                    Err(_) => continue,
                                };

                                let aumid = match item.GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING) {
                                    Ok(pw) => {
                                        let s = pw.to_string().unwrap_or_default();
                                        windows::Win32::System::Com::CoTaskMemFree(Some(pw.0 as *const _));
                                        s
                                    }
                                    Err(_) => continue,
                                };

                                if name.is_empty() || aumid.is_empty() {
                                    continue;
                                }

                                // 排除黑名单检查
                                let is_excluded = config.exclude_patterns.iter().any(|pattern| {
                                    name.contains(pattern) || aumid.contains(pattern)
                                });
                                if is_excluded {
                                    continue;
                                }

                                let key = name.to_lowercase();
                                if seen.contains(&key) {
                                    continue;
                                }
                                seen.insert(key);

                                let (pinyin_full, pinyin_abbr) = generate_pinyin(&name);
                                apps.push(AppItem::new_uwp(
                                    name,
                                    aumid,
                                    "应用商店/系统应用".to_string(),
                                    pinyin_full,
                                    pinyin_abbr,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}
