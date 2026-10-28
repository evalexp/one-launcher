use rquickjs::{Context, Runtime, Object, ArrayBuffer};
use std::fs;

#[test]
fn test_plugin_host_fs_and_process() {
    let rt = Runtime::new().unwrap();
    let ctx = Context::full(&rt).unwrap();

    ctx.with(|ctx| {
        let launcher = Object::new(ctx.clone()).unwrap();
        let shell = Object::new(ctx.clone()).unwrap();

        one_launcher::plugin::host_process::register_process(&ctx, &launcher, &shell, 0).unwrap();
        launcher.set("shell", shell).unwrap();
        one_launcher::plugin::host_fs::register_fs(&ctx, &launcher).unwrap();
        one_launcher::plugin::host_http::register_http(&ctx, &launcher).unwrap();

        ctx.globals().set("launcher", launcher).unwrap();

        // 1. 测试命令执行: launcher.shell.exec
        {
            let cmd_res: Object = ctx.eval(r#"
                launcher.shell.exec("cmd.exe", { args: ["/C", "echo hello_from_exec"] })
            "#).unwrap();
            let code: i32 = cmd_res.get("code").unwrap();
            let stdout: String = cmd_res.get("stdout").unwrap();
            let success: bool = cmd_res.get("success").unwrap();
            assert_eq!(code, 0);
            assert!(success);
            assert!(stdout.contains("hello_from_exec"));
        }

        // 2. 测试 launcher.process 别名执行
        {
            let proc_res: Object = ctx.eval(r#"
                launcher.process.exec("echo hello_process")
            "#).unwrap();
            let proc_stdout: String = proc_res.get("stdout").unwrap();
            assert!(proc_stdout.contains("hello_process"));
        }

        // 3. 测试 batch_exec 多命令批处理与 auto_sudo 机制
        {
            let batch_res: Object = ctx.eval(r#"
                launcher.shell.batch_exec([
                    "echo batch_step_1",
                    "echo batch_step_2"
                ], { auto_sudo: true })
            "#).unwrap();
            let code: i32 = batch_res.get("code").unwrap();
            let stdout: String = batch_res.get("stdout").unwrap();
            let success: bool = batch_res.get("success").unwrap();
            assert_eq!(code, 0);
            assert!(success);
            assert!(stdout.contains("batch_step_1"));
            assert!(stdout.contains("batch_step_2"));
        }

        // 4. 测试 top-level launcher.batch_exec 以及 options.sudo = true (在无窗口测试环境下安全执行)
        {
            let sudo_res: Object = ctx.eval(r#"
                launcher.batch_exec([
                    "echo elevated_batch_step"
                ], { sudo: true })
            "#).unwrap();
            let success: bool = sudo_res.get("success").unwrap();
            assert!(success);
        }

        // 5. 测试 exec 携带 auto_sudo: true
        {
            let auto_res: Object = ctx.eval(r#"
                launcher.shell.exec("echo hello_auto_sudo", { auto_sudo: true })
            "#).unwrap();
            let auto_code: i32 = auto_res.get("code").unwrap();
            let auto_out: String = auto_res.get("stdout").unwrap();
            assert_eq!(auto_code, 0);
            assert!(auto_out.contains("hello_auto_sudo"));
        }

        // 3. 测试文件操作: 写入与读取原始 buffer
        let temp_dir = std::env::temp_dir().join("one_launcher_test_fs");
        let test_file = temp_dir.join("test_file.bin");
        let test_file_str = test_file.to_string_lossy().replace('\\', "/");

        let write_res: bool = ctx.eval(format!(r#"
            const bytes = new Uint8Array([72, 101, 108, 108, 111]); // "Hello"
            launcher.fs.write("{}", bytes);
        "#, test_file_str)).unwrap();
        assert!(write_res);
        assert!(test_file.exists());

        // 读取 buffer
        {
            let read_buf: ArrayBuffer = ctx.eval(format!(r#"
                launcher.fs.read("{}")
            "#, test_file_str)).unwrap();
            let raw_bytes = unsafe { read_buf.as_bytes().unwrap() };
            assert_eq!(raw_bytes, b"Hello");
        }

        // 读取文本
        {
            let read_txt: String = ctx.eval(format!(r#"
                launcher.fs.read_text("{}")
            "#, test_file_str)).unwrap();
            assert_eq!(read_txt, "Hello");
        }

        // 追加内容
        let append_res: bool = ctx.eval(format!(r#"
            launcher.fs.append("{}", " World");
        "#, test_file_str)).unwrap();
        assert!(append_res);

        {
            let read_txt2: String = ctx.eval(format!(r#"
                launcher.fs.read_text("{}")
            "#, test_file_str)).unwrap();
            assert_eq!(read_txt2, "Hello World");
        }

        // 复制文件
        let copy_file = temp_dir.join("test_copy.txt");
        let copy_file_str = copy_file.to_string_lossy().replace('\\', "/");
        let copy_res: bool = ctx.eval(format!(r#"
            launcher.fs.copy("{}", "{}");
        "#, test_file_str, copy_file_str)).unwrap();
        assert!(copy_res);
        assert!(copy_file.exists());

        // 移动文件
        let moved_file = temp_dir.join("test_moved.txt");
        let moved_file_str = moved_file.to_string_lossy().replace('\\', "/");
        let move_res: bool = ctx.eval(format!(r#"
            launcher.fs.move("{}", "{}");
        "#, copy_file_str, moved_file_str)).unwrap();
        assert!(move_res);
        assert!(!copy_file.exists());
        assert!(moved_file.exists());

        // 查看 stat
        {
            let stat_obj: Object = ctx.eval(format!(r#"
                launcher.fs.stat("{}")
            "#, moved_file_str)).unwrap();
            let is_file: bool = stat_obj.get("is_file").unwrap();
            let size: u64 = stat_obj.get("size").unwrap();
            assert!(is_file);
            assert_eq!(size, 11);
        }

        // 删除测试文件与目录
        let remove_res: bool = ctx.eval(format!(r#"
            launcher.fs.remove("{}") && launcher.fs.remove("{}")
        "#, test_file_str, moved_file_str)).unwrap();
        assert!(remove_res);
        assert!(!test_file.exists());
        assert!(!moved_file.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    });
}

#[test]
fn test_plugin_host_http() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    // 启动极简单次 HTTP 测试服务
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    thread::spawn(move || {
        for _ in 0..2 {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"msg\":\"hello_http\",\"value\":42}";
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        }
    });

    let rt = Runtime::new().unwrap();
    let ctx = Context::full(&rt).unwrap();

    ctx.with(|ctx| {
        let launcher = Object::new(ctx.clone()).unwrap();
        one_launcher::plugin::host_http::register_http(&ctx, &launcher).unwrap();
        ctx.globals().set("launcher", launcher).unwrap();

        // 1. 测试 launcher.http.get
        {
            let res: Object = ctx.eval(format!(r#"
                launcher.http.get("http://127.0.0.1:{}")
            "#, port)).unwrap();
            let status: u32 = res.get("status").unwrap();
            let ok: bool = res.get("ok").unwrap();
            let body: String = res.get("body").unwrap();
            assert_eq!(status, 200);
            assert!(ok);
            assert!(body.contains("hello_http"));

            // 测试 res.json()
            let data: Object = ctx.eval(format!(r#"
                const res = launcher.http.get("http://127.0.0.1:{}");
                res.json()
            "#, port)).unwrap();
            let msg: String = data.get("msg").unwrap();
            let val: i32 = data.get("value").unwrap();
            assert_eq!(msg, "hello_http");
            assert_eq!(val, 42);
        }
    });
}

#[test]
fn test_launcher_vm_and_isolated_context() {
    let rt = Runtime::new().unwrap();
    let ctx = Context::full(&rt).unwrap();

    ctx.with(|ctx| {
        let launcher = Object::new(ctx.clone()).unwrap();
        one_launcher::plugin::host_vm::register_vm(&ctx, &launcher).unwrap();
        ctx.globals().set("launcher", launcher).unwrap();

        // 1. 测试 launcher.create_context 与 isolated eval
        let eval_res: i32 = ctx
            .eval(
                r#"
            {
                const vm1 = launcher.create_context();
                vm1.eval("21 * 2")
            }
        "#,
            )
            .unwrap();
        assert_eq!(eval_res, 42);

        // 2. 测试 isolated context 独立作用域不污染当前全局
        ctx.eval::<(), _>(
            r#"
            {
                const vm2 = launcher.create_context();
                vm2.eval("globalThis.secret = 'isolated_val';");
            }
        "#,
        )
        .unwrap();
        let parent_secret: rquickjs::Value = ctx.eval("globalThis.secret").unwrap();
        assert!(parent_secret.is_undefined());
        let parent_secret: rquickjs::Value = ctx.eval("globalThis.secret").unwrap();
        assert!(parent_secret.is_undefined());

        // 3. 测试 launcher.eval_module 静态方法解析 ES 模块与 metadata
        let mod_res: Object = ctx
            .eval(
                r#"
            launcher.eval_module(`
                export const metadata = {
                    name: "测试模块",
                    prefix: "tm:",
                    dynamic_library: {
                        MyLib: { path: "./mylib.dll" }
                    }
                };
                export function on_setup() {}
                export function on_query(input) { return []; }
                export function on_select(item) {}
            `)
        "#,
            )
            .unwrap();
        let meta: Object = mod_res.get("metadata").unwrap();
        let name: String = meta.get("name").unwrap();
        let prefix: String = meta.get("prefix").unwrap();
        assert_eq!(name, "测试模块");
        assert_eq!(prefix, "tm:");

        let exports: Vec<String> = mod_res.get("exports").unwrap();
        assert!(exports.contains(&"metadata".to_string()));
        assert!(exports.contains(&"on_setup".to_string()));
        assert!(exports.contains(&"on_query".to_string()));
        assert!(exports.contains(&"on_select".to_string()));

        // 4. 测试 vm.eval_module 与 setup 中的 mock 钩子捕获
        let intercepted: Vec<String> = ctx
            .eval(
                r#"
            const vm2 = launcher.create_context();
            vm2.eval(`
                globalThis.__captured_icons = [];
                globalThis.launcher = {
                    cache_icon: (p) => {
                        globalThis.__captured_icons.push(p);
                        return p;
                    }
                };
            `);
            vm2.eval_module(`
                export const metadata = { name: "IconTest", prefix: "it:" };
                export function on_setup() {
                    launcher.cache_icon("./custom_icon.png");
                }
            `);
            vm2.call("on_setup");
            vm2.eval("globalThis.__captured_icons")
        "#,
            )
            .unwrap();
        assert_eq!(intercepted, vec!["./custom_icon.png"]);
    });
}
