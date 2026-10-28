use rquickjs::function::Opt;
use rquickjs::{Context, Ctx, Function, Module, Object, Runtime, Value};
use std::cell::RefCell;
use std::rc::Rc;

struct VmHolder {
    _rt: Runtime,
    ctx: Context,
}

/// 向 QuickJS 全局 launcher 挂载隔离虚拟机 / 上下文管理能力 (launcher.create_context, launcher.eval_module)
pub fn register_vm<'js>(
    ctx: &Ctx<'js>,
    launcher: &Object<'js>,
) -> Result<(), rquickjs::Error> {
    // 1. launcher.create_context(): 创建独立完全隔离的 QuickJS 运行时与上下文
    let create_ctx_fn = Function::new(ctx.clone(), |host_ctx: Ctx<'js>| -> Result<Object<'js>, rquickjs::Error> {
        let rt = Runtime::new().map_err(|e| {
            rquickjs::Error::new_from_js_message("runtime", "isolated_runtime", format!("创建 QuickJS Runtime 失败: {}", e))
        })?;
        let sub_ctx = Context::full(&rt).map_err(|e| {
            rquickjs::Error::new_from_js_message("context", "isolated_context", format!("创建 QuickJS Context 失败: {}", e))
        })?;

        let holder = Rc::new(RefCell::new(VmHolder {
            _rt: rt,
            ctx: sub_ctx,
        }));

        let vm_obj = Object::new(host_ctx.clone())?;

        // vm.eval(code: string) -> any
        let h_eval = holder.clone();
        let eval_fn = Function::new(
            host_ctx.clone(),
            move |call_ctx: Ctx<'js>, code: String| -> Result<Value<'js>, rquickjs::Error> {
                let borrow = h_eval.borrow();
                let res_json = borrow.ctx.with(|sub_c| -> Result<String, rquickjs::Error> {
                    let val: Value = sub_c.eval(code)?;
                    if val.is_undefined() {
                        return Ok("__UNDEFINED__".to_string());
                    }
                    if val.is_null() {
                        return Ok("null".to_string());
                    }
                    let json_opt = sub_c.json_stringify(val)?;
                    let json_str = match json_opt {
                        Some(s) => s.to_string()?,
                        None => "null".to_string(),
                    };
                    Ok(json_str)
                })?;

                if res_json == "__UNDEFINED__" {
                    Ok(Value::new_undefined(call_ctx))
                } else {
                    let parsed = call_ctx.json_parse(res_json)?;
                    Ok(parsed)
                }
            },
        )?;
        vm_obj.set("eval", eval_fn)?;

        // vm.eval_module(code: string) -> { metadata: any, exports: string[] }
        let h_mod = holder.clone();
        let eval_mod_fn = Function::new(
            host_ctx.clone(),
            move |call_ctx: Ctx<'js>, code: String| -> Result<Object<'js>, rquickjs::Error> {
                let borrow = h_mod.borrow();
                let (meta_json, export_names) = borrow.ctx.with(
                    |sub_c| -> Result<(Option<String>, Vec<String>), rquickjs::Error> {
                        let module = match Module::declare(sub_c.clone(), "isolated_module", code) {
                            Ok(m) => match m.eval() {
                                Ok((mod_eval, _)) => mod_eval,
                                Err(e) => {
                                    return Err(rquickjs::Error::new_from_js_message(
                                        "module",
                                        "eval",
                                        format!("ES 模块 eval 失败: {}", e),
                                    ))
                                }
                            },
                            Err(e) => {
                                return Err(rquickjs::Error::new_from_js_message(
                                    "module",
                                    "declare",
                                    format!("ES 模块 declare 失败: {}", e),
                                ))
                            }
                        };

                        let mut exports = Vec::new();
                        for name in &["metadata", "on_setup", "on_query", "on_select", "default"] {
                            if let Ok(val) = module.get::<_, Value>(*name) {
                                exports.push(name.to_string());
                                // 将导出函数/对象同步至隔离上下文的全局作用域，便于直接调用
                                let _ = sub_c.globals().set(*name, val);
                            }
                        }

                        let meta_json = if let Ok(meta_obj) = module.get::<_, Object>("metadata") {
                            let json_opt = sub_c.json_stringify(meta_obj.into_value())?;
                            json_opt.and_then(|s| s.to_string().ok())
                        } else {
                            None
                        };

                        Ok((meta_json, exports))
                    },
                )?;

                let res_obj = Object::new(call_ctx.clone())?;
                if let Some(json_str) = meta_json {
                    let parsed = call_ctx.json_parse(json_str)?;
                    res_obj.set("metadata", parsed)?;
                } else {
                    res_obj.set("metadata", Value::new_null(call_ctx.clone()))?;
                }

                let exports_arr = rquickjs::Array::new(call_ctx.clone())?;
                for (idx, name) in export_names.into_iter().enumerate() {
                    exports_arr.set(idx, name)?;
                }
                res_obj.set("exports", exports_arr)?;

                Ok(res_obj)
            },
        )?;
        vm_obj.set("eval_module", eval_mod_fn)?;

        // vm.call(fn_name: string, args?: any) -> any
        let h_call = holder.clone();
        let call_fn = Function::new(
            host_ctx.clone(),
            move |call_ctx: Ctx<'js>, fn_name: String, args_opt: Opt<Value<'js>>| -> Result<Value<'js>, rquickjs::Error> {
                let borrow = h_call.borrow();
                let args_json = if let Some(val) = args_opt.0 {
                    if val.is_undefined() {
                        "undefined".to_string()
                    } else {
                        call_ctx
                            .json_stringify(val)?
                            .and_then(|s| s.to_string().ok())
                            .unwrap_or_else(|| "null".to_string())
                    }
                } else {
                    "undefined".to_string()
                };

                let res_json = borrow.ctx.with(|sub_c| -> Result<String, rquickjs::Error> {
                    let code = if args_json == "undefined" {
                        format!("{}()", fn_name)
                    } else {
                        format!("{}({})", fn_name, args_json)
                    };
                    let val: Value = sub_c.eval(code)?;
                    if val.is_undefined() {
                        return Ok("__UNDEFINED__".to_string());
                    }
                    if val.is_null() {
                        return Ok("null".to_string());
                    }
                    let json_opt = sub_c.json_stringify(val)?;
                    let json_str = match json_opt {
                        Some(s) => s.to_string()?,
                        None => "null".to_string(),
                    };
                    Ok(json_str)
                })?;

                if res_json == "__UNDEFINED__" {
                    Ok(Value::new_undefined(call_ctx))
                } else {
                    let parsed = call_ctx.json_parse(res_json)?;
                    Ok(parsed)
                }
            },
        )?;
        vm_obj.set("call", call_fn)?;

        Ok(vm_obj)
    })?;

    launcher.set("create_context", create_ctx_fn)?;

    // 2. launcher.eval_module(code): 便捷静态模块评估方法
    let eval_module_static = Function::new(
        ctx.clone(),
        |host_ctx: Ctx<'js>, code: String| -> Result<Object<'js>, rquickjs::Error> {
            let rt = Runtime::new().map_err(|e| {
                rquickjs::Error::new_from_js_message("runtime", "temp_runtime", format!("创建 QuickJS Runtime 失败: {}", e))
            })?;
            let sub_ctx = Context::full(&rt).map_err(|e| {
                rquickjs::Error::new_from_js_message("context", "temp_context", format!("创建 QuickJS Context 失败: {}", e))
            })?;

            let (meta_json, export_names) = sub_ctx.with(
                |sub_c| -> Result<(Option<String>, Vec<String>), rquickjs::Error> {
                    let module = match Module::declare(sub_c.clone(), "temp_module", code) {
                        Ok(m) => match m.eval() {
                            Ok((mod_eval, _)) => mod_eval,
                            Err(e) => {
                                return Err(rquickjs::Error::new_from_js_message(
                                    "module",
                                    "eval",
                                    format!("ES 模块 eval 失败: {}", e),
                                ))
                            }
                        },
                        Err(e) => {
                            return Err(rquickjs::Error::new_from_js_message(
                                "module",
                                "declare",
                                format!("ES 模块 declare 失败: {}", e),
                            ))
                        }
                    };

                    let mut exports = Vec::new();
                    for name in &["metadata", "on_setup", "on_query", "on_select", "default"] {
                        if let Ok(val) = module.get::<_, Value>(*name) {
                            exports.push(name.to_string());
                            let _ = sub_c.globals().set(*name, val);
                        }
                    }

                    let meta_json = if let Ok(meta_obj) = module.get::<_, Object>("metadata") {
                        let json_opt = sub_c.json_stringify(meta_obj.into_value())?;
                        json_opt.and_then(|s| s.to_string().ok())
                    } else {
                        None
                    };

                    Ok((meta_json, exports))
                },
            )?;

            let res_obj = Object::new(host_ctx.clone())?;
            if let Some(json_str) = meta_json {
                let parsed = host_ctx.json_parse(json_str)?;
                res_obj.set("metadata", parsed)?;
            } else {
                res_obj.set("metadata", Value::new_null(host_ctx.clone()))?;
            }

            let exports_arr = rquickjs::Array::new(host_ctx.clone())?;
            for (idx, name) in export_names.into_iter().enumerate() {
                exports_arr.set(idx, name)?;
            }
            res_obj.set("exports", exports_arr)?;

            Ok(res_obj)
        },
    )?;

    launcher.set("eval_module", eval_module_static)?;

    Ok(())
}
