use rquickjs::{Ctx, Function, Object};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

pub struct LoadedDynamicLibraries {
    // 保持已加载 DLL 的引用计数，确保插件在整个生命周期内不被卸载
    pub _libs: HashMap<String, Arc<libloading::Library>>,
}

/// 解析 JS metadata 中的 dynamic_library 声明，并在插件加载阶段完成 DLL 载入与符号校验绑定
pub fn bind_dynamic_libraries<'js>(
    ctx: &Ctx<'js>,
    meta_obj: &Object<'js>,
    plugin_dir: &Path,
) -> Result<LoadedDynamicLibraries, String> {
    let mut loaded_libs = HashMap::new();

    // 检查是否存在 dynamic_library 属性
    let dyn_libs_val = match meta_obj.get::<_, rquickjs::Value>("dynamic_library") {
        Ok(v) => v,
        Err(_) => return Ok(LoadedDynamicLibraries { _libs: loaded_libs }),
    };

    if !dyn_libs_val.is_object() {
        return Ok(LoadedDynamicLibraries { _libs: loaded_libs });
    }

    let dyn_libs = dyn_libs_val.into_object().unwrap();
    let native_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;

    for key_res in dyn_libs.keys::<String>() {
        let lib_name = key_res.map_err(|e| format!("解析动态库名称键失败: {}", e))?;
        let lib_spec: Object = dyn_libs
            .get(&lib_name)
            .map_err(|e| format!("读取动态库 {} 配置失败: {}", lib_name, e))?;

        let path_str: String = lib_spec
            .get("path")
            .map_err(|e| format!("动态库 {} 缺少 path 路径声明: {}", lib_name, e))?;

        // 优先相对于插件目录解析路径
        let dll_path = if Path::new(&path_str).is_absolute() {
            std::path::PathBuf::from(&path_str)
        } else {
            plugin_dir.join(&path_str)
        };

        if !dll_path.exists() {
            let err_msg = format!(
                "动态链接库不存在: {} (插件: {})",
                dll_path.display(),
                plugin_dir.display()
            );
            warn!("{}", err_msg);
            return Err(err_msg);
        }

        // 加载 DLL
        let lib = unsafe {
            libloading::Library::new(&dll_path).map_err(|e| {
                let err_msg = format!("加载动态链接库 {} 失败: {}", dll_path.display(), e);
                warn!("{}", err_msg);
                err_msg
            })?
        };
        let lib = Arc::new(lib);
        loaded_libs.insert(lib_name.clone(), lib.clone());

        let lib_obj = Object::new(ctx.clone()).map_err(|e| e.to_string())?;
        let symbols: rquickjs::Array = lib_spec
            .get("symbols")
            .map_err(|e| format!("动态库 {} 缺少 symbols 列表声明: {}", lib_name, e))?;

        for i in 0..symbols.len() {
            let sym_obj: Object = symbols
                .get(i)
                .map_err(|e| format!("读取符号索引 {} 失败: {}", i, e))?;

            let sym_name: String = sym_obj
                .get("name")
                .map_err(|e| format!("符号定义缺少 name: {}", e))?;

            let params: rquickjs::Array = sym_obj
                .get("params")
                .map_err(|e| format!("符号 {} 缺少 params 列表: {}", sym_name, e))?;

            let returns: String = sym_obj
                .get("returns")
                .unwrap_or_else(|_| "void".to_string());

            let mut param_types = Vec::new();
            for j in 0..params.len() {
                let p: String = params
                    .get(j)
                    .map_err(|e| format!("解析符号参数类型失败: {}", e))?;
                param_types.push(p);
            }

            // 严格检验符号是否存在（加载期 Fail-Fast）
            let sym_bytes = format!("{}\0", sym_name);
            unsafe {
                if let Err(e) = lib.get::<*const ()>(sym_bytes.as_bytes()) {
                    let err_msg = format!(
                        "动态库 {} 中未找到导出符号 '{}': {}",
                        dll_path.display(),
                        sym_name,
                        e
                    );
                    warn!("{}", err_msg);
                    return Err(err_msg);
                }
            }

            // 绑定动态 JS 函数
            let lib_c = lib.clone();
            let s_name = sym_name.clone();

            if param_types.iter().any(|p| p == "buffer") {
                // 带输出缓冲区的 UTF-16 文本读取函数
                let func = Function::new(ctx.clone(), move |index: u32| {
                    unsafe {
                        let sym_bytes = format!("{}\0", s_name);
                        let fn_ptr: libloading::Symbol<
                            unsafe extern "system" fn(u32, *mut u16, u32) -> u32,
                        > = lib_c
                            .get(sym_bytes.as_bytes())
                            .map_err(|_| rquickjs::Error::Unknown)?;
                        let mut buf = vec![0u16; 512];
                        let len = fn_ptr(index, buf.as_mut_ptr(), buf.len() as u32);
                        let decoded = String::from_utf16_lossy(&buf[..len as usize]);
                        Ok::<_, rquickjs::Error>(decoded)
                    }
                })
                .map_err(|e| e.to_string())?;

                lib_obj
                    .set(&sym_name, func)
                    .map_err(|e| format!("注入符号 {} 失败: {}", sym_name, e))?;
            } else if param_types == vec!["string"] && returns == "void" {
                let func = Function::new(ctx.clone(), move |s: String| {
                    unsafe {
                        let sym_bytes = format!("{}\0", s_name);
                        let fn_ptr: libloading::Symbol<unsafe extern "system" fn(*const u16)> =
                            lib_c
                                .get(sym_bytes.as_bytes())
                                .map_err(|_| rquickjs::Error::Unknown)?;
                        let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
                        fn_ptr(wide.as_ptr());
                    }
                    Ok::<_, rquickjs::Error>(())
                })
                .map_err(|e| e.to_string())?;

                lib_obj
                    .set(&sym_name, func)
                    .map_err(|e| format!("注入符号 {} 失败: {}", sym_name, e))?;
            } else if param_types == vec!["bool"] && returns == "bool" {
                let func = Function::new(ctx.clone(), move |b: bool| {
                    unsafe {
                        let sym_bytes = format!("{}\0", s_name);
                        let fn_ptr: libloading::Symbol<unsafe extern "system" fn(i32) -> i32> =
                            lib_c
                                .get(sym_bytes.as_bytes())
                                .map_err(|_| rquickjs::Error::Unknown)?;
                        let ret = fn_ptr(if b { 1 } else { 0 });
                        Ok::<_, rquickjs::Error>(ret != 0)
                    }
                })
                .map_err(|e| e.to_string())?;

                lib_obj
                    .set(&sym_name, func)
                    .map_err(|e| format!("注入符号 {} 失败: {}", sym_name, e))?;
            } else if param_types.is_empty() && (returns == "u32" || returns == "i32") {
                let func = Function::new(ctx.clone(), move || {
                    unsafe {
                        let sym_bytes = format!("{}\0", s_name);
                        let fn_ptr: libloading::Symbol<unsafe extern "system" fn() -> u32> = lib_c
                            .get(sym_bytes.as_bytes())
                            .map_err(|_| rquickjs::Error::Unknown)?;
                        let ret = fn_ptr();
                        Ok::<_, rquickjs::Error>(ret)
                    }
                })
                .map_err(|e| e.to_string())?;

                lib_obj
                    .set(&sym_name, func)
                    .map_err(|e| format!("注入符号 {} 失败: {}", sym_name, e))?;
            } else if param_types == vec!["u32"] && returns == "bool" {
                let func = Function::new(ctx.clone(), move |val: u32| {
                    unsafe {
                        let sym_bytes = format!("{}\0", s_name);
                        let fn_ptr: libloading::Symbol<unsafe extern "system" fn(u32) -> i32> =
                            lib_c
                                .get(sym_bytes.as_bytes())
                                .map_err(|_| rquickjs::Error::Unknown)?;
                        let ret = fn_ptr(val);
                        Ok::<_, rquickjs::Error>(ret != 0)
                    }
                })
                .map_err(|e| e.to_string())?;

                lib_obj
                    .set(&sym_name, func)
                    .map_err(|e| format!("注入符号 {} 失败: {}", sym_name, e))?;
            }
        }

        native_obj
            .set(lib_name, lib_obj)
            .map_err(|e| format!("挂载 native 动态库对象失败: {}", e))?;
    }

    ctx.globals()
        .set("native", native_obj)
        .map_err(|e| format!("设置全局 native 对象失败: {}", e))?;

    info!("动态库加载成功");
    Ok(LoadedDynamicLibraries { _libs: loaded_libs })
}
