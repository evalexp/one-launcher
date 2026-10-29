use crate::plugin::host_fs::extract_bytes;
use parking_lot::RwLock;
use rquickjs::function::Opt;
use rquickjs::{ArrayBuffer, Ctx, Function, Object, Value};
use std::collections::HashMap;
use std::time::Duration;

static GLOBAL_PROXY: RwLock<Option<String>> = RwLock::new(None);

/// 设置全局 HTTP 代理地址
pub fn set_global_proxy(proxy: Option<String>) {
    let mut lock = GLOBAL_PROXY.write();
    *lock = proxy.and_then(|p| {
        let p_trim = p.trim().to_string();
        if p_trim.is_empty() {
            None
        } else {
            Some(p_trim)
        }
    });
}

/// 获取当前生效的全局 HTTP 代理地址
pub fn get_global_proxy() -> Option<String> {
    GLOBAL_PROXY.read().clone()
}

/// 发起 HTTP 同步请求（基于第三方 reqwest 客户端库）
pub fn perform_http_request(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    timeout_ms: u32,
) -> Result<(u32, String, HashMap<String, String>, Vec<u8>), String> {
    let mut client_builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(timeout_ms as u64));

    if let Some(proxy_url) = get_global_proxy() {
        let proxy = reqwest::Proxy::all(&proxy_url)
            .map_err(|e| format!("配置系统代理失败 ({}): {}", proxy_url, e))?;
        client_builder = client_builder.proxy(proxy);
    } else {
        // 未配置代理时强制直连，不受外部不可控环境变量干扰
        client_builder = client_builder.no_proxy();
    }

    let client = client_builder
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let http_method = match method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        "OPTIONS" => reqwest::Method::OPTIONS,
        other => reqwest::Method::from_bytes(other.as_bytes())
            .map_err(|e| format!("不支持的 HTTP 方法 {}: {}", other, e))?,
    };

    let mut builder = client.request(http_method, url);

    for (k, v) in headers {
        if let (Ok(name), Ok(val)) = (
            reqwest::header::HeaderName::from_bytes(k.as_bytes()),
            reqwest::header::HeaderValue::from_str(v),
        ) {
            builder = builder.header(name, val);
        }
    }

    if !body.is_empty() {
        builder = builder.body(body.to_vec());
    }

    let resp = builder.send().map_err(|e| format!("HTTP 请求失败: {}", e))?;

    let status = resp.status().as_u16() as u32;
    let status_text = resp
        .status()
        .canonical_reason()
        .unwrap_or("OK")
        .to_string();

    let mut resp_headers = HashMap::new();
    for (name, val) in resp.headers() {
        if let Ok(v_str) = val.to_str() {
            resp_headers.insert(name.as_str().to_lowercase(), v_str.to_string());
        }
    }

    let resp_bytes = resp
        .bytes()
        .map_err(|e| format!("读取 HTTP 响应数据失败: {}", e))?
        .to_vec();

    Ok((status, status_text, resp_headers, resp_bytes))
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
    res_obj.set("body", body_str)?;

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

/// 向 launcher 注册 HTTP 请求模块与全局 fetch
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
    http_obj.set("request", request_fn)?;

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
