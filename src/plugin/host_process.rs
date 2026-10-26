use rquickjs::function::Opt;
use rquickjs::{Array, Ctx, Function, Object, Value};
use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::Win32::Globalization::{MultiByteToWideChar, CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS};

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

/// 向 launcher 注册命令执行与外部进程管理能力
pub fn register_process<'js>(
    ctx: &Ctx<'js>,
    launcher: &Object<'js>,
    shell: &Object<'js>,
) -> Result<(), rquickjs::Error> {
    // launcher.shell.exec(command: string, options?: Object) -> { code, stdout, stderr, success }
    let exec_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>, command: String, options: Opt<Object<'js>>| {
            let mut opt_args: Option<Vec<String>> = None;
            let mut opt_cwd: Option<String> = None;
            let mut opt_env: Option<HashMap<String, String>> = None;
            let mut use_shell = true;

            if let Some(opts) = options.0 {
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

            let mut cmd = if use_shell {
                let mut c = Command::new("cmd.exe");
                c.arg("/C").arg(&command);
                c
            } else {
                let mut c = Command::new(&command);
                if let Some(args) = opt_args {
                    for arg in args {
                        c.arg(arg);
                    }
                }
                c
            };

            cmd.creation_flags(CREATE_NO_WINDOW);

            if let Some(cwd) = opt_cwd {
                cmd.current_dir(cwd);
            }
            if let Some(env) = opt_env {
                for (k, v) in env {
                    cmd.env(k, v);
                }
            }

            let output = cmd.output();
            let result_obj = Object::new(ctx.clone())?;

            match output {
                Ok(out) => {
                    let code = out.status.code().unwrap_or(-1);
                    result_obj.set("code", code)?;
                    result_obj.set("stdout", decode_output(&out.stdout))?;
                    result_obj.set("stderr", decode_output(&out.stderr))?;
                    result_obj.set("success", out.status.success())?;
                }
                Err(e) => {
                    result_obj.set("code", -1)?;
                    result_obj.set("stdout", "")?;
                    result_obj.set("stderr", e.to_string())?;
                    result_obj.set("success", false)?;
                }
            }

            Ok::<_, rquickjs::Error>(result_obj)
        },
    )?;

    shell.set("exec", exec_fn.clone())?;
    shell.set("run", exec_fn.clone())?;

    // launcher.shell.spawn(command: string, args?: string[]) -> boolean (脱机启动后台程序)
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
    shell.set("spawn", spawn_fn.clone())?;

    // 挂载 launcher.process 独立别名对象
    let process_obj = Object::new(ctx.clone())?;
    process_obj.set("exec", exec_fn)?;
    process_obj.set("spawn", spawn_fn)?;
    launcher.set("process", process_obj)?;

    Ok(())
}
