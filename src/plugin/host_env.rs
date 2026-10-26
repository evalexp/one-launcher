use rquickjs::{Ctx, Function, Object};

/// 向 launcher 注册系统环境变量管理模块 (launcher.env)
pub fn register_env<'js>(ctx: &Ctx<'js>, launcher: &Object<'js>) -> Result<(), rquickjs::Error> {
    let raw_env_obj = Object::new(ctx.clone())?;

    // launcher.env.get(name: string) -> string | null
    let get_fn = Function::new(ctx.clone(), |name: String| {
        std::env::var(name).ok()
    })?;
    raw_env_obj.set("get", get_fn)?;

    // launcher.env.set(name: string, value: string) -> void
    let set_fn = Function::new(ctx.clone(), |name: String, value: String| {
        unsafe { std::env::set_var(name, value); }
        Ok::<_, rquickjs::Error>(())
    })?;
    raw_env_obj.set("set", set_fn)?;

    // launcher.env.has(name: string) -> boolean
    let has_fn = Function::new(ctx.clone(), |name: String| {
        std::env::var(name).is_ok()
    })?;
    raw_env_obj.set("has", has_fn)?;

    // launcher.env.remove(name: string) -> void
    let remove_fn = Function::new(ctx.clone(), |name: String| {
        unsafe { std::env::remove_var(name); }
        Ok::<_, rquickjs::Error>(())
    })?;
    raw_env_obj.set("remove", remove_fn)?;

    // launcher.env.all() -> Record<string, string>
    let all_fn = Function::new(ctx.clone(), |ctx: Ctx<'js>| {
        let all_obj = Object::new(ctx)?;
        for (k, v) in std::env::vars() {
            let _ = all_obj.set(k, v);
        }
        Ok::<_, rquickjs::Error>(all_obj)
    })?;
    raw_env_obj.set("all", all_fn)?;

    // 使用 Proxy 包装，使 launcher.env.FOO 与 launcher.env.get("FOO") 均可直接获取环境变量
    ctx.globals().set("__raw_launcher_env", raw_env_obj)?;
    let proxy_env: Object = ctx.eval(
        r#"
        new Proxy(__raw_launcher_env, {
            get(target, prop) {
                if (typeof prop === "string" && prop in target) {
                    return target[prop];
                }
                if (typeof prop === "string") {
                    return target.get(prop);
                }
                return undefined;
            },
            set(target, prop, value) {
                if (typeof prop === "string") {
                    target.set(prop, String(value));
                    return true;
                }
                return false;
            }
        })
        "#,
    )?;

    launcher.set("env", proxy_env)?;
    let _ = ctx.globals().remove("__raw_launcher_env");

    Ok(())
}
