use crate::plugin::host_fs::extract_bytes;
use rquickjs::function::Opt;
use rquickjs::{ArrayBuffer, Ctx, Function, Object, Value};
use std::collections::HashMap;
use std::ffi::c_void;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

/// RAII 守护句柄，在析构时安全关闭 WinHttp 句柄
struct AutoClose(*mut c_void);

impl Drop for AutoClose {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// 解析 URL 为 scheme, host, port, path_and_query
fn parse_url(url: &str) -> Result<(bool, String, u16, String), String> {
    let (is_https, rest) = if let Some(stripped) = url.strip_prefix("https://") {
        (true, stripped)
    } else if let Some(stripped) = url.strip_prefix("http://") {
        (false, stripped)
    } else {
        return Err(format!("不支持的 URL 协议头 (仅支持 http:// 或 https://): {}", url));
    };

    let (host_port, path) = match rest.find(['/', '?']) {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };

    let (host, port) = match host_port.find(':') {
        Some(idx) => {
            let h = &host_port[..idx];
            let p = host_port[idx + 1..]
                .parse::<u16>()
                .map_err(|_| format!("非法端口号: {}", &host_port[idx + 1..]))?;
            (h.to_string(), p)
        }
        None => (host_port.to_string(), if is_https { 443 } else { 80 }),
    };

    if host.is_empty() {
        return Err("URL 中缺少主机域名".to_string());
    }

    Ok((is_https, host, port, path.to_string()))
}

/// 发起原生 WinHttp 请求
fn perform_http_request(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    timeout_ms: u32,
) -> Result<(u32, String, HashMap<String, String>, Vec<u8>), String> {
    let (is_https, host, port, path) = parse_url(url)?;

    unsafe {
        let ua = HSTRING::from("OneLauncher/1.0");
        let session = WinHttpOpen(
            PCWSTR(ua.as_ptr()),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if session.is_null() {
            return Err("创建 WinHttp Session 失败".to_string());
        }
        let _session_guard = AutoClose(session);

        let host_w = HSTRING::from(host);
        let connect = WinHttpConnect(session, PCWSTR(host_w.as_ptr()), port, 0);
        if connect.is_null() {
            return Err("WinHttp 连接目标主机失败".to_string());
        }
        let _connect_guard = AutoClose(connect);

        let verb_w = HSTRING::from(method.to_uppercase());
        let path_w = HSTRING::from(path);
        let flags = if is_https {
            WINHTTP_FLAG_SECURE
        } else {
            WINHTTP_OPEN_REQUEST_FLAGS(0)
        };

        let request = WinHttpOpenRequest(
            connect,
            PCWSTR(verb_w.as_ptr()),
            PCWSTR(path_w.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        );
        if request.is_null() {
            return Err("创建 WinHttp Request 失败".to_string());
        }
        let _request_guard = AutoClose(request);

        let _ = WinHttpSetTimeouts(
            request,
            timeout_ms as i32,
            timeout_ms as i32,
            timeout_ms as i32,
            timeout_ms as i32,
        );

        let mut headers_string = String::new();
        for (k, v) in headers {
            headers_string.push_str(&format!("{}: {}\r\n", k, v));
        }
        let headers_w = HSTRING::from(headers_string);

        let headers_slice = if headers_w.is_empty() {
            None
        } else {
            Some(headers_w.as_wide())
        };

        let body_opt = if body.is_empty() {
            None
        } else {
            Some(body.as_ptr() as *const c_void)
        };

        if WinHttpSendRequest(
            request,
            headers_slice,
            body_opt,
            body.len() as u32,
            body.len() as u32,
            0,
        )
        .is_err()
        {
            return Err("WinHttp 发送网络请求失败".to_string());
        }

        if WinHttpReceiveResponse(request, std::ptr::null_mut()).is_err() {
            return Err("WinHttp 接收网络响应失败".to_string());
        }

        let mut status_code: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let _ = WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status_code as *mut _ as *mut c_void),
            &mut size,
            std::ptr::null_mut(),
        );

        let mut text_size: u32 = 0;
        let _ = WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_TEXT,
            PCWSTR::null(),
            None,
            &mut text_size,
            std::ptr::null_mut(),
        );
        let mut status_text = String::new();
        if text_size > 0 {
            let count = (text_size as usize / 2) + 1;
            let mut wide = vec![0u16; count];
            if WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_STATUS_TEXT,
                PCWSTR::null(),
                Some(wide.as_mut_ptr() as *mut c_void),
                &mut text_size,
                std::ptr::null_mut(),
            )
            .is_ok()
            {
                status_text = String::from_utf16_lossy(&wide)
                    .trim_end_matches('\0')
                    .to_string();
            }
        }

        let mut headers_size: u32 = 0;
        let _ = WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_RAW_HEADERS_CRLF,
            PCWSTR::null(),
            None,
            &mut headers_size,
            std::ptr::null_mut(),
        );
        let mut headers_map = HashMap::new();
        if headers_size > 0 {
            let count = (headers_size as usize / 2) + 1;
            let mut wide = vec![0u16; count];
            if WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_RAW_HEADERS_CRLF,
                PCWSTR::null(),
                Some(wide.as_mut_ptr() as *mut c_void),
                &mut headers_size,
                std::ptr::null_mut(),
            )
            .is_ok()
            {
                let raw = String::from_utf16_lossy(&wide);
                for line in raw.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        headers_map.insert(k.trim().to_lowercase(), v.trim().to_string());
                    }
                }
            }
        }

        let mut response_body = Vec::new();
        loop {
            let mut available: u32 = 0;
            if WinHttpQueryDataAvailable(request, &mut available).is_err() || available == 0 {
                break;
            }
            let mut chunk = vec![0u8; available as usize];
            let mut read_bytes: u32 = 0;
            if WinHttpReadData(
                request,
                chunk.as_mut_ptr() as *mut c_void,
                available,
                &mut read_bytes,
            )
            .is_ok()
                && read_bytes > 0
            {
                response_body.extend_from_slice(&chunk[..read_bytes as usize]);
            } else {
                break;
            }
        }

        Ok((status_code, status_text, headers_map, response_body))
    }
}

/// 将 HTTP 结果打包为标准 JavaScript Response 对象
fn build_js_response<'js>(
    ctx: &Ctx<'js>,
    status: u32,
    status_text: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
) -> Result<Object<'js>, rquickjs::Error> {
    let res_obj = Object::new(ctx.clone())?;
    res_obj.set("status", status)?;
    res_obj.set("status_text", status_text)?;
    res_obj.set("ok", (200..300).contains(&status))?;

    let headers_obj = Object::new(ctx.clone())?;
    for (k, v) in headers {
        headers_obj.set(k, v)?;
    }
    res_obj.set("headers", headers_obj)?;

    let body_str = String::from_utf8_lossy(&body).into_owned();
    res_obj.set("body", body_str.clone())?;

    let buffer = ArrayBuffer::new_copy(ctx.clone(), &body)?;
    res_obj.set("buffer", buffer)?;

    // 注入 json() 解析方法: 读取 this.body 并调用 JSON.parse
    let json_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>, this: rquickjs::function::This<Object<'js>>| -> Result<Value<'js>, rquickjs::Error> {
            let body_str: String = this.0.get("body")?;
            let json_obj: Object = ctx.globals().get("JSON")?;
            let parse_fn: Function = json_obj.get("parse")?;
            parse_fn.call::<_, Value>((body_str,))
        },
    )?;
    res_obj.set("json", json_fn)?;

    Ok(res_obj)
}

/// 向 launcher 注册原生 HTTP 请求模块与全局 fetch
pub fn register_http<'js>(ctx: &Ctx<'js>, launcher: &Object<'js>) -> Result<(), rquickjs::Error> {
    let http_obj = Object::new(ctx.clone())?;

    // launcher.http.request(options: { url, method, headers, body, timeout_ms }) -> Response
    let request_fn = Function::new(ctx.clone(), |ctx: Ctx<'js>, options: Object<'js>| {
        let url: String = options.get("url")?;
        let method: String = options.get("method").unwrap_or_else(|_| "GET".to_string());
        let timeout_ms: u32 = options.get("timeout_ms").unwrap_or(10_000);

        let mut headers_map = HashMap::new();
        if let Ok(h_obj) = options.get::<_, Object>("headers") {
            for key_res in h_obj.keys::<String>() {
                if let Ok(k) = key_res {
                    if let Ok(v) = h_obj.get::<_, String>(&k) {
                        headers_map.insert(k, v);
                    }
                }
            }
        }

        let body_bytes = if let Ok(body_val) = options.get::<_, Value>("body") {
            extract_bytes(&body_val).unwrap_or_default()
        } else {
            Vec::new()
        };

        match perform_http_request(&url, &method, &headers_map, &body_bytes, timeout_ms) {
            Ok((status, status_text, headers, body)) => {
                build_js_response(&ctx, status, status_text, headers, body)
            }
            Err(err) => {
                let mut err_headers = HashMap::new();
                err_headers.insert("error".to_string(), err.clone());
                build_js_response(&ctx, 0, err, err_headers, Vec::new())
            }
        }
    })?;
    http_obj.set("request", request_fn.clone())?;

    // launcher.http.get(url, options?) -> Response
    let get_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>, url: String, options: Opt<Object<'js>>| {
            let mut headers_map = HashMap::new();
            let mut timeout_ms = 10_000;

            if let Some(opts) = options.0 {
                if let Ok(t) = opts.get::<_, u32>("timeout_ms") {
                    timeout_ms = t;
                }
                if let Ok(h_obj) = opts.get::<_, Object>("headers") {
                    for key_res in h_obj.keys::<String>() {
                        if let Ok(k) = key_res {
                            if let Ok(v) = h_obj.get::<_, String>(&k) {
                                headers_map.insert(k, v);
                            }
                        }
                    }
                }
            }

            match perform_http_request(&url, "GET", &headers_map, &[], timeout_ms) {
                Ok((status, status_text, headers, body)) => {
                    build_js_response(&ctx, status, status_text, headers, body)
                }
                Err(err) => {
                    let mut err_headers = HashMap::new();
                    err_headers.insert("error".to_string(), err.clone());
                    build_js_response(&ctx, 0, err, err_headers, Vec::new())
                }
            }
        },
    )?;
    http_obj.set("get", get_fn)?;

    // launcher.http.post(url, body, options?) -> Response
    let post_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>, url: String, body: Opt<Value<'js>>, options: Opt<Object<'js>>| {
            let mut headers_map = HashMap::new();
            let mut timeout_ms = 10_000;

            if let Some(opts) = options.0 {
                if let Ok(t) = opts.get::<_, u32>("timeout_ms") {
                    timeout_ms = t;
                }
                if let Ok(h_obj) = opts.get::<_, Object>("headers") {
                    for key_res in h_obj.keys::<String>() {
                        if let Ok(k) = key_res {
                            if let Ok(v) = h_obj.get::<_, String>(&k) {
                                headers_map.insert(k, v);
                            }
                        }
                    }
                }
            }

            let body_bytes = if let Some(b) = body.0 {
                extract_bytes(&b).unwrap_or_default()
            } else {
                Vec::new()
            };

            match perform_http_request(&url, "POST", &headers_map, &body_bytes, timeout_ms) {
                Ok((status, status_text, headers, body)) => {
                    build_js_response(&ctx, status, status_text, headers, body)
                }
                Err(err) => {
                    let mut err_headers = HashMap::new();
                    err_headers.insert("error".to_string(), err.clone());
                    build_js_response(&ctx, 0, err, err_headers, Vec::new())
                }
            }
        },
    )?;
    http_obj.set("post", post_fn)?;

    launcher.set("http", http_obj)?;

    // launcher.fetch(url, options?) - 开发者标准别名
    let fetch_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>, url_or_opts: Value<'js>, options: Opt<Object<'js>>| {
            if let Some(url_str) = url_or_opts.as_string() {
                let url = url_str.to_string().unwrap_or_default();
                let method = options
                    .0
                    .as_ref()
                    .and_then(|o| o.get::<_, String>("method").ok())
                    .unwrap_or_else(|| "GET".to_string());
                let timeout_ms = options
                    .0
                    .as_ref()
                    .and_then(|o| o.get::<_, u32>("timeout_ms").ok())
                    .unwrap_or(10_000);

                let mut headers_map = HashMap::new();
                if let Some(opts) = &options.0 {
                    if let Ok(h_obj) = opts.get::<_, Object>("headers") {
                        for key_res in h_obj.keys::<String>() {
                            if let Ok(k) = key_res {
                                if let Ok(v) = h_obj.get::<_, String>(&k) {
                                    headers_map.insert(k, v);
                                }
                            }
                        }
                    }
                }

                let body_bytes = if let Some(opts) = &options.0 {
                    if let Ok(b_val) = opts.get::<_, Value>("body") {
                        extract_bytes(&b_val).unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                match perform_http_request(&url, &method, &headers_map, &body_bytes, timeout_ms) {
                    Ok((status, status_text, headers, body)) => {
                        build_js_response(&ctx, status, status_text, headers, body)
                    }
                    Err(err) => {
                        let mut err_headers = HashMap::new();
                        err_headers.insert("error".to_string(), err.clone());
                        build_js_response(&ctx, 0, err, err_headers, Vec::new())
                    }
                }
            } else if let Some(opts) = url_or_opts.as_object() {
                let url: String = opts.get("url")?;
                let method: String = opts.get("method").unwrap_or_else(|_| "GET".to_string());
                let timeout_ms: u32 = opts.get("timeout_ms").unwrap_or(10_000);

                let mut headers_map = HashMap::new();
                if let Ok(h_obj) = opts.get::<_, Object>("headers") {
                    for key_res in h_obj.keys::<String>() {
                        if let Ok(k) = key_res {
                            if let Ok(v) = h_obj.get::<_, String>(&k) {
                                headers_map.insert(k, v);
                            }
                        }
                    }
                }

                let body_bytes = if let Ok(body_val) = opts.get::<_, Value>("body") {
                    extract_bytes(&body_val).unwrap_or_default()
                } else {
                    Vec::new()
                };

                match perform_http_request(&url, &method, &headers_map, &body_bytes, timeout_ms) {
                    Ok((status, status_text, headers, body)) => {
                        build_js_response(&ctx, status, status_text, headers, body)
                    }
                    Err(err) => {
                        let mut err_headers = HashMap::new();
                        err_headers.insert("error".to_string(), err.clone());
                        build_js_response(&ctx, 0, err, err_headers, Vec::new())
                    }
                }
            } else {
                build_js_response(
                    &ctx,
                    0,
                    "Invalid arguments to fetch".to_string(),
                    HashMap::new(),
                    Vec::new(),
                )
            }
        },
    )?;

    launcher.set("fetch", fetch_fn.clone())?;
    ctx.globals().set("fetch", fetch_fn)?;

    Ok(())
}
