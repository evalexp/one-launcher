use rquickjs::function::Opt;
use rquickjs::{Array, Ctx, Function, Object, Value};
use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::Globalization::{MultiByteToWideChar, CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS};
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW, SEE_MASK_NOCLOSEPROCESS};
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 智能解码外部命令的输出字节流（优先尝试 UTF-8，失败则回退至系统 ANSI 代码页，如中文 GBK/CP936）
pub fn decode_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    unsafe {
        let len = MultiByteToWideChar(CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
        if len > 0 {
            let mut wide = vec![0u16; len as usize];
            let res = MultiByteToWideChar(
                CP_ACP,
                MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0),
                bytes,
                Some(&mut wide),
            );
            if res > 0 {
                return String::from_utf16_lossy(&wide);
            }
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}

#[derive(Debug, Clone)]
pub struct BatchRunResult {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub cancelled: bool,
}

/// 执行命令列表（支持普通模式或单次 UAC 提权执行整个批处理脚本）
pub fn run_batch(
    commands: &[String],
    sudo: bool,
    cwd: Option<&str>,
    env: Option<&HashMap<String, String>>,
    stop_on_error: bool,
    timeout_ms: Option<u32>,
    show_window: bool,
    hwnd: isize,
) -> BatchRunResult {
    if commands.is_empty() {
        return BatchRunResult {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
            success: true,
            cancelled: false,
        };
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let temp_dir = std::env::temp_dir();
    let bat_path = temp_dir.join(format!("one_launcher_batch_{}_{}.cmd", std::process::id(), now));
    let out_path = temp_dir.join(format!("one_launcher_batch_{}_{}.out", std::process::id(), now));

    let mut script = String::from("@echo off\r\nchcp 65001 >nul\r\n");
    if let Some(dir) = cwd {
        script.push_str(&format!("cd /d \"{}\"\r\n", dir));
    }
    for cmd in commands {
        script.push_str(cmd);
        script.push_str("\r\n");
        if stop_on_error {
            script.push_str("if %errorlevel% neq 0 exit /b %errorlevel%\r\n");
        }
    }

    if let Err(e) = std::fs::write(&bat_path, script.as_bytes()) {
        return BatchRunResult {
            code: -1,
            stdout: String::new(),
            stderr: format!("写入临时批处理文件失败: {}", e),
            success: false,
            cancelled: false,
        };
    }

    // 判断是否在测试模式下自动免弹窗模拟（避免 CI / headless 测试挂起）
    let test_mock = std::env::var("ONE_LAUNCHER_TEST_NO_UAC").is_ok()
        || (hwnd == 0
            && std::env::var("CARGO_MANIFEST_DIR").is_ok()
            && std::env::var("ONE_LAUNCHER_ALLOW_UAC").is_err());

    if sudo && !test_mock {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let to_wide = |s: &str| -> Vec<u16> {
            OsStr::new(s)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        };

        let verb_w = to_wide("runas");
        let file_w = to_wide("cmd.exe");
        let params_str = format!(
            "/c \"\"{}\" > \"{}\" 2>&1\"",
            bat_path.display(),
            out_path.display()
        );
        let params_w = to_wide(&params_str);
        let dir_w = cwd.map(&to_wide);

        let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOCLOSEPROCESS;
        sei.hwnd = HWND(hwnd as *mut _);
        sei.lpVerb = windows::core::PCWSTR(verb_w.as_ptr());
        sei.lpFile = windows::core::PCWSTR(file_w.as_ptr());
        sei.lpParameters = windows::core::PCWSTR(params_w.as_ptr());
        if let Some(ref d) = dir_w {
            sei.lpDirectory = windows::core::PCWSTR(d.as_ptr());
        }
        sei.nShow = if show_window {
            SW_SHOWNORMAL.0 as i32
        } else {
            SW_HIDE.0 as i32
        };

        let res = unsafe { ShellExecuteExW(&mut sei) };
        let (code, cancelled, err_msg) = match res {
            Ok(()) => {
                let mut exit_code: u32 = 0;
                if !sei.hProcess.is_invalid() && !sei.hProcess.0.is_null() {
                    unsafe {
                        let wait_ms = timeout_ms.unwrap_or(INFINITE);
                        WaitForSingleObject(sei.hProcess, wait_ms);
                        let _ = GetExitCodeProcess(sei.hProcess, &mut exit_code);
                        let _ = CloseHandle(sei.hProcess);
                    }
                }
                (exit_code as i32, false, None)
            }
            Err(e) => {
                let is_cancelled = (e.code().0 as u32 & 0xFFFF) == 1223;
                if is_cancelled {
                    (1223, true, Some("UAC 提权已被用户取消".to_string()))
                } else {
                    (-1, false, Some(e.to_string()))
                }
            }
        };

        let _ = std::fs::remove_file(&bat_path);

        let mut output_str = String::new();
        if out_path.exists() {
            if let Ok(bytes) = std::fs::read(&out_path) {
                output_str = decode_output(&bytes);
            }
            let _ = std::fs::remove_file(&out_path);
        }

        if cancelled {
            return BatchRunResult {
                code: 1223,
                stdout: String::new(),
                stderr: err_msg.unwrap_or_else(|| "UAC 提权已被用户取消".to_string()),
                success: false,
                cancelled: true,
            };
        }

        if let Some(err) = err_msg {
            return BatchRunResult {
                code,
                stdout: String::new(),
                stderr: err,
                success: false,
                cancelled: false,
            };
        }

        let success = code == 0;
        let (stdout, stderr) = if success {
            (output_str, String::new())
        } else {
            (String::new(), output_str)
        };

        BatchRunResult {
            code,
            stdout,
            stderr,
            success,
            cancelled: false,
        }
    } else {
        // 普通批处理执行（或免提权测试模式）
        let mut cmd = Command::new("cmd.exe");
        cmd.arg("/C").arg(&bat_path);
        cmd.creation_flags(CREATE_NO_WINDOW);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        if let Some(env_map) = env {
            for (k, v) in env_map {
                cmd.env(k, v);
            }
        }
        let output = cmd.output();
        let _ = std::fs::remove_file(&bat_path);
        let _ = std::fs::remove_file(&out_path);

        match output {
            Ok(out) => {
                let code = out.status.code().unwrap_or(-1);
                let success = out.status.success();
                let stdout = decode_output(&out.stdout);
                let stderr = decode_output(&out.stderr);
                BatchRunResult {
                    code,
                    stdout,
                    stderr,
                    success,
                    cancelled: false,
                }
            }
            Err(e) => BatchRunResult {
                code: -1,
                stdout: String::new(),
                stderr: e.to_string(),
                success: false,
                cancelled: false,
            },
        }
    }
}

/// 向 launcher 注册命令执行与外部进程管理能力（支持 sudo 提权与 batch_exec 批处理）
pub fn register_process<'js>(
    ctx: &Ctx<'js>,
    launcher: &Object<'js>,
    shell: &Object<'js>,
    hwnd: isize,
) -> Result<(), rquickjs::Error> {
    // 1. launcher.shell.exec(command: string, options?: Object) -> { code, stdout, stderr, success, cancelled, retried_with_sudo }
    let exec_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, command: String, options: Opt<Object<'js>>| {
            let mut opt_args: Option<Vec<String>> = None;
            let mut opt_cwd: Option<String> = None;
            let mut opt_env: Option<HashMap<String, String>> = None;
            let mut use_shell = true;
            let mut sudo = false;
            let mut auto_sudo = false;
            let mut timeout_ms: Option<u32> = None;
            let mut show_window = false;

            if let Some(opts) = options.0 {
                if let Ok(s) = opts.get::<_, bool>("sudo") {
                    sudo = s;
                }
                if let Ok(as_val) = opts.get::<_, bool>("auto_sudo") {
                    auto_sudo = as_val;
                }
                if let Ok(tm) = opts.get::<_, u32>("timeout_ms") {
                    timeout_ms = Some(tm);
                }
                if let Ok(sw) = opts.get::<_, bool>("show_window") {
                    show_window = sw;
                }
                if let Ok(args_arr) = opts.get::<_, Array>("args") {
                    let mut list = Vec::new();
                    for item in args_arr.into_iter() {
                        if let Ok(v) = item {
                            if let Some(s) = v.as_string() {
                                if let Ok(str_val) = s.to_string() {
                                    list.push(str_val);
                                }
                            }
                        }
                    }
                    opt_args = Some(list);
                    use_shell = false;
                }
                if let Ok(cwd_str) = opts.get::<_, String>("cwd") {
                    opt_cwd = Some(cwd_str);
                }
                if let Ok(sh) = opts.get::<_, bool>("shell") {
                    use_shell = sh;
                }
                if let Ok(env_obj) = opts.get::<_, Object>("env") {
                    let mut env_map = HashMap::new();
                    for key_res in env_obj.keys::<String>() {
                        if let Ok(k) = key_res {
                            if let Ok(v) = env_obj.get::<_, String>(&k) {
                                env_map.insert(k, v);
                            }
                        }
                    }
                    opt_env = Some(env_map);
                }
            }

            let result_obj = Object::new(ctx.clone())?;
            let full_cmd = if let Some(ref args) = opt_args {
                format!("\"{}\" {}", command, args.join(" "))
            } else {
                command.clone()
            };

            if sudo {
                // 直接使用 UAC 提权执行
                let res = run_batch(
                    &[full_cmd],
                    true,
                    opt_cwd.as_deref(),
                    opt_env.as_ref(),
                    true,
                    timeout_ms,
                    show_window,
                    hwnd,
                );
                result_obj.set("code", res.code)?;
                result_obj.set("stdout", res.stdout)?;
                result_obj.set("stderr", res.stderr)?;
                result_obj.set("success", res.success)?;
                result_obj.set("cancelled", res.cancelled)?;
                result_obj.set("retried_with_sudo", false)?;
            } else {
                // 普通非提权尝试
                let mut cmd = if use_shell {
                    let mut c = Command::new("cmd.exe");
                    c.arg("/C").arg(&command);
                    c
                } else {
                    let mut c = Command::new(&command);
                    if let Some(ref args) = opt_args {
                        for arg in args {
                            c.arg(arg);
                        }
                    }
                    c
                };

                cmd.creation_flags(CREATE_NO_WINDOW);

                if let Some(ref cwd) = opt_cwd {
                    cmd.current_dir(cwd);
                }
                if let Some(ref env) = opt_env {
                    for (k, v) in env {
                        cmd.env(k, v);
                    }
                }

                let output = cmd.output();
                let (mut code, mut stdout, mut stderr, mut success) = match output {
                    Ok(out) => {
                        let c = out.status.code().unwrap_or(-1);
                        (c, decode_output(&out.stdout), decode_output(&out.stderr), out.status.success())
                    }
                    Err(e) => (-1, String::new(), e.to_string(), false),
                };

                let mut retried = false;
                let mut cancelled = false;

                // 若执行失败且开启了 auto_sudo，则自动提权重试
                if !success && auto_sudo {
                    tracing::info!("命令执行失败，auto_sudo 触发提权重试: {}", full_cmd);
                    let retry_res = run_batch(
                        &[full_cmd],
                        true,
                        opt_cwd.as_deref(),
                        opt_env.as_ref(),
                        true,
                        timeout_ms,
                        show_window,
                        hwnd,
                    );
                    code = retry_res.code;
                    stdout = retry_res.stdout;
                    stderr = retry_res.stderr;
                    success = retry_res.success;
                    cancelled = retry_res.cancelled;
                    retried = true;
                }

                result_obj.set("code", code)?;
                result_obj.set("stdout", stdout)?;
                result_obj.set("stderr", stderr)?;
                result_obj.set("success", success)?;
                result_obj.set("cancelled", cancelled)?;
                result_obj.set("retried_with_sudo", retried)?;
            }

            Ok::<_, rquickjs::Error>(result_obj)
        },
    )?;

    // 2. launcher.shell.batch_exec(commands: string[] | string | Object[], options?: Object) -> { code, stdout, stderr, success, cancelled, retried_with_sudo }
    let batch_exec_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, commands_val: Value<'js>, options: Opt<Object<'js>>| {
            let mut cmd_list = Vec::new();
            if let Some(arr) = commands_val.as_array() {
                for item_res in arr.iter::<Value<'js>>() {
                    if let Ok(item) = item_res {
                        if let Some(s) = item.as_string() {
                            if let Ok(str_val) = s.to_string() {
                                cmd_list.push(str_val);
                            }
                        } else if let Some(obj) = item.as_object() {
                            if let Ok(c) = obj.get::<_, String>("command") {
                                let mut line = c;
                                if let Ok(args_arr) = obj.get::<_, Array>("args") {
                                    for arg in args_arr.iter::<String>().flatten() {
                                        line.push(' ');
                                        line.push_str(&arg);
                                    }
                                }
                                cmd_list.push(line);
                            }
                        }
                    }
                }
            } else if let Some(s) = commands_val.as_string() {
                if let Ok(str_val) = s.to_string() {
                    cmd_list.push(str_val);
                }
            }

            let mut sudo = false;
            let mut auto_sudo = false;
            let mut opt_cwd: Option<String> = None;
            let mut opt_env: Option<HashMap<String, String>> = None;
            let mut stop_on_error = true;
            let mut timeout_ms: Option<u32> = None;
            let mut show_window = false;

            if let Some(opts) = options.0 {
                if let Ok(s) = opts.get::<_, bool>("sudo") {
                    sudo = s;
                }
                if let Ok(as_val) = opts.get::<_, bool>("auto_sudo") {
                    auto_sudo = as_val;
                }
                if let Ok(cwd_str) = opts.get::<_, String>("cwd") {
                    opt_cwd = Some(cwd_str);
                }
                if let Ok(soe) = opts.get::<_, bool>("stop_on_error") {
                    stop_on_error = soe;
                }
                if let Ok(tm) = opts.get::<_, u32>("timeout_ms") {
                    timeout_ms = Some(tm);
                }
                if let Ok(sw) = opts.get::<_, bool>("show_window") {
                    show_window = sw;
                }
                if let Ok(env_obj) = opts.get::<_, Object>("env") {
                    let mut env_map = HashMap::new();
                    for key_res in env_obj.keys::<String>() {
                        if let Ok(k) = key_res {
                            if let Ok(v) = env_obj.get::<_, String>(&k) {
                                env_map.insert(k, v);
                            }
                        }
                    }
                    opt_env = Some(env_map);
                }
            }

            let mut res = run_batch(
                &cmd_list,
                sudo,
                opt_cwd.as_deref(),
                opt_env.as_ref(),
                stop_on_error,
                timeout_ms,
                show_window,
                hwnd,
            );

            let mut retried = false;
            if !sudo && !res.success && auto_sudo {
                tracing::info!("批处理执行失败，auto_sudo 触发提权重试");
                res = run_batch(
                    &cmd_list,
                    true,
                    opt_cwd.as_deref(),
                    opt_env.as_ref(),
                    stop_on_error,
                    timeout_ms,
                    show_window,
                    hwnd,
                );
                retried = true;
            }

            let result_obj = Object::new(ctx.clone())?;
            result_obj.set("code", res.code)?;
            result_obj.set("stdout", res.stdout)?;
            result_obj.set("stderr", res.stderr)?;
            result_obj.set("success", res.success)?;
            result_obj.set("cancelled", res.cancelled)?;
            result_obj.set("retried_with_sudo", retried)?;

            Ok::<_, rquickjs::Error>(result_obj)
        },
    )?;

    // 3. launcher.shell.spawn(command: string, args?: string[]) -> boolean (脱机启动后台程序)
    let spawn_fn = Function::new(
        ctx.clone(),
        |command: String, args: Opt<Value<'js>>| {
            let mut cmd = Command::new(&command);
            cmd.creation_flags(CREATE_NO_WINDOW);

            if let Some(val) = args.0 {
                if let Some(arr) = val.as_array() {
                    for item in arr.iter::<String>().flatten() {
                        cmd.arg(item);
                    }
                }
            }

            cmd.spawn().is_ok()
        },
    )?;

    // 绑定到 shell
    shell.set("exec", exec_fn.clone())?;
    shell.set("run", exec_fn.clone())?;
    shell.set("batch_exec", batch_exec_fn.clone())?;
    shell.set("spawn", spawn_fn.clone())?;

    // 绑定到 launcher.process
    let process_obj = Object::new(ctx.clone())?;
    process_obj.set("exec", exec_fn.clone())?;
    process_obj.set("run", exec_fn)?;
    process_obj.set("batch_exec", batch_exec_fn.clone())?;
    process_obj.set("spawn", spawn_fn)?;
    launcher.set("process", process_obj)?;

    // 绑定便捷顶层接口到 launcher
    launcher.set("batch_exec", batch_exec_fn)?;

    Ok(())
}
