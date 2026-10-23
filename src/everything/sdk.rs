use std::ffi::OsString;
use std::fs;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::Mutex;
use tracing::info;
use windows::core::{HSTRING, PCSTR, PCWSTR};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

// 内嵌官方 64 位 Everything SDK DLL (91KB)
const EVERYTHING64_DLL_BYTES: &[u8] = include_bytes!("../../vendor/everything-sdk/dll/Everything64.dll");

pub const EVERYTHING_OK: u32 = 0;
pub const EVERYTHING_ERROR_IPC: u32 = 2;

type FnSetSearchW = unsafe extern "system" fn(*const u16);
type FnSetMax = unsafe extern "system" fn(u32);
type FnQueryW = unsafe extern "system" fn(i32) -> i32;
type FnGetNumResults = unsafe extern "system" fn() -> u32;
type FnGetResultFileNameW = unsafe extern "system" fn(u32) -> *const u16;
type FnGetResultPathW = unsafe extern "system" fn(u32) -> *const u16;
type FnIsFolderResult = unsafe extern "system" fn(u32) -> i32;
type FnGetLastError = unsafe extern "system" fn() -> u32;
type FnCleanUp = unsafe extern "system" fn();
type FnIsDbLoaded = unsafe extern "system" fn() -> i32;

/// Everything 搜索结果条目
#[derive(Debug, Clone)]
pub struct EverythingItem {
    pub name: String,
    pub path: String,
    pub full_path: PathBuf,
    pub is_folder: bool,
}

/// 官方 Everything SDK 绑定
pub struct EverythingSdk {
    _hmodule: HMODULE,
    set_search_w: FnSetSearchW,
    set_max: FnSetMax,
    query_w: FnQueryW,
    get_num_results: FnGetNumResults,
    get_result_file_name_w: FnGetResultFileNameW,
    get_result_path_w: FnGetResultPathW,
    is_folder_result: FnIsFolderResult,
    get_last_error: FnGetLastError,
    clean_up: FnCleanUp,
    is_db_loaded: Option<FnIsDbLoaded>,
    query_lock: Mutex<()>,
}

// SDK 函数指针具备线程安全性（配合互斥锁执行串行查询）
unsafe impl Send for EverythingSdk {}
unsafe impl Sync for EverythingSdk {}

impl EverythingSdk {
    /// 加载并初始化 Everything SDK
    pub fn load() -> Result<Self, String> {
        let dll_path = Self::ensure_dll_available()?;
        let path_h = HSTRING::from(dll_path.to_str().ok_or("无效的 DLL 路径编码")?);

        unsafe {
            let hmodule = LoadLibraryW(PCWSTR(path_h.as_ptr()))
                .map_err(|e| format!("加载 Everything64.dll 失败 ({:?}): {}", dll_path, e))?;

            macro_rules! get_proc {
                ($name:expr, $type:ty) => {{
                    let c_name = concat!($name, "\0");
                    let proc = GetProcAddress(hmodule, PCSTR(c_name.as_ptr()))
                        .ok_or_else(|| format!("在 Everything64.dll 中未找到导出符号 {}", $name))?;
                    std::mem::transmute::<_, $type>(proc)
                }};
            }

            macro_rules! get_opt_proc {
                ($name:expr, $type:ty) => {{
                    let c_name = concat!($name, "\0");
                    if let Some(proc) = GetProcAddress(hmodule, PCSTR(c_name.as_ptr())) {
                        Some(std::mem::transmute::<_, $type>(proc))
                    } else {
                        None
                    }
                }};
            }

            let set_search_w: FnSetSearchW = get_proc!("Everything_SetSearchW", FnSetSearchW);
            let set_max: FnSetMax = get_proc!("Everything_SetMax", FnSetMax);
            let query_w: FnQueryW = get_proc!("Everything_QueryW", FnQueryW);
            let get_num_results: FnGetNumResults = get_proc!("Everything_GetNumResults", FnGetNumResults);
            let get_result_file_name_w: FnGetResultFileNameW = get_proc!("Everything_GetResultFileNameW", FnGetResultFileNameW);
            let get_result_path_w: FnGetResultPathW = get_proc!("Everything_GetResultPathW", FnGetResultPathW);
            let is_folder_result: FnIsFolderResult = get_proc!("Everything_IsFolderResult", FnIsFolderResult);
            let get_last_error: FnGetLastError = get_proc!("Everything_GetLastError", FnGetLastError);
            let clean_up: FnCleanUp = get_proc!("Everything_CleanUp", FnCleanUp);
            let is_db_loaded: Option<FnIsDbLoaded> = get_opt_proc!("Everything_IsDBLoaded", FnIsDbLoaded);

            info!("成功挂载官方 Everything64 SDK (来源: {:?})", dll_path);

            Ok(Self {
                _hmodule: hmodule,
                set_search_w,
                set_max,
                query_w,
                get_num_results,
                get_result_file_name_w,
                get_result_path_w,
                is_folder_result,
                get_last_error,
                clean_up,
                is_db_loaded,
                query_lock: Mutex::new(()),
            })
        }
    }

    /// 检查 Everything 是否可用
    pub fn is_available(&self) -> bool {
        let _guard = self.query_lock.lock().unwrap();
        unsafe {
            // 空查询快速探测
            (self.set_search_w)([0u16].as_ptr());
            (self.set_max)(1);
            let ok = (self.query_w)(1);
            if ok != 0 {
                (self.clean_up)();
                return true;
            }
            let err = (self.get_last_error)();
            (self.clean_up)();
            err != EVERYTHING_ERROR_IPC
        }
    }

    /// 执行搜索并返回结果列表
    pub fn search(&self, query: &str, max_results: usize) -> Result<Vec<EverythingItem>, String> {
        let _guard = self.query_lock.lock().unwrap();
        let query_utf16: Vec<u16> = query.encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            (self.set_search_w)(query_utf16.as_ptr());
            (self.set_max)(max_results as u32);

            let success = (self.query_w)(1);
            if success == 0 {
                let err = (self.get_last_error)();
                (self.clean_up)();
                if err == EVERYTHING_ERROR_IPC {
                    return Err("Everything 服务未运行或尚未启动".to_string());
                } else {
                    return Err(format!("Everything 查询失败，错误码: {}", err));
                }
            }

            let count = (self.get_num_results)() as usize;
            let mut items = Vec::with_capacity(count);

            for i in 0..(count as u32) {
                let file_ptr = (self.get_result_file_name_w)(i);
                let path_ptr = (self.get_result_path_w)(i);
                let is_folder = (self.is_folder_result)(i) != 0;

                let name = lpcwstr_to_string(file_ptr);
                let path = lpcwstr_to_string(path_ptr);

                if !name.is_empty() {
                    let mut full_path = PathBuf::from(&path);
                    full_path.push(&name);

                    items.push(EverythingItem {
                        name,
                        path,
                        full_path,
                        is_folder,
                    });
                }
            }

            (self.clean_up)();
            Ok(items)
        }
    }

    /// 确保本地存在 Everything64.dll，如不存在则将内嵌资源写入临时目录
    fn ensure_dll_available() -> Result<PathBuf, String> {
        // 1. 检查当前可执行文件同级目录
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let local_dll = parent.join("Everything64.dll");
                if local_dll.exists() {
                    return Ok(local_dll);
                }
            }
        }

        // 2. 检查 vendor 目录
        let vendor_dll = PathBuf::from("vendor/everything-sdk/dll/Everything64.dll");
        if vendor_dll.exists() {
            return Ok(vendor_dll);
        }

        // 3. 释放到系统临时目录 (%TEMP%/one_launcher_everything64.dll)
        let temp_dll = std::env::temp_dir().join("one_launcher_everything64.dll");
        if !temp_dll.exists() || temp_dll.metadata().map(|m| m.len() as usize).unwrap_or(0) != EVERYTHING64_DLL_BYTES.len() {
            fs::write(&temp_dll, EVERYTHING64_DLL_BYTES)
                .map_err(|e| format!("释放 Everything64.dll 到临时目录失败: {}", e))?;
        }

        Ok(temp_dll)
    }
}

/// 将以 0 结尾的宽字符指针转为 String
unsafe fn lpcwstr_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    unsafe {
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        OsString::from_wide(slice).to_string_lossy().to_string()
    }
}
