use crate::ui::progress::{
    close_global_progress, show_global_progress, update_global_progress, ProgressBar,
};
use rquickjs::function::Opt;
use rquickjs::{Ctx, Function, Object};
use std::sync::Arc;

/// 向 launcher 与 app 挂载 progress 模块与进度展示能力
pub fn register_progress<'js>(
    ctx: &Ctx<'js>,
    launcher: &Object<'js>,
    app_obj: &Object<'js>,
) -> Result<(), rquickjs::Error> {
    let progress_obj = Object::new(ctx.clone())?;

    // launcher.progress.create(title?, message?, percent?) -> ProgressHandle
    let create_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>,
         title: Opt<String>,
         message: Opt<String>,
         percent: Opt<u32>|
         -> Result<Object<'js>, rquickjs::Error> {
            let t = title.0.unwrap_or_else(|| "OneLauncher".to_string());
            let m = message.0.unwrap_or_default();
            let p = percent.0.unwrap_or(0);

            let pb = Arc::new(ProgressBar::new(&t, &m, p));
            let handle_obj = Object::new(ctx.clone())?;

            let pb_up = pb.clone();
            let update_fn = Function::new(
                ctx.clone(),
                move |pct: u32, msg: Opt<String>| {
                    pb_up.update(pct, msg.0.as_deref());
                    Ok::<_, rquickjs::Error>(())
                },
            )?;
            handle_obj.set("update", update_fn.clone())?;
            handle_obj.set("set", update_fn)?;

            let pb_close = pb.clone();
            let close_fn = Function::new(
                ctx.clone(),
                move || {
                    pb_close.close();
                    Ok::<_, rquickjs::Error>(())
                },
            )?;
            handle_obj.set("close", close_fn)?;

            Ok(handle_obj)
        },
    )?;
    progress_obj.set("create", create_fn)?;

    // launcher.progress.show(title?, message?, percent?)
    let show_fn = Function::new(
        ctx.clone(),
        |title: Opt<String>, message: Opt<String>, percent: Opt<u32>| {
            let t = title.0.unwrap_or_else(|| "OneLauncher".to_string());
            let m = message.0.unwrap_or_default();
            let p = percent.0.unwrap_or(0);
            show_global_progress(&t, &m, p);
            Ok::<_, rquickjs::Error>(())
        },
    )?;
    progress_obj.set("show", show_fn.clone())?;

    // launcher.progress.update(percent, message?)
    let update_fn = Function::new(
        ctx.clone(),
        |pct: u32, msg: Opt<String>| {
            update_global_progress(pct, msg.0.as_deref());
            Ok::<_, rquickjs::Error>(())
        },
    )?;
    progress_obj.set("update", update_fn.clone())?;
    progress_obj.set("set", update_fn.clone())?;

    // launcher.progress.close()
    let close_fn = Function::new(
        ctx.clone(),
        || {
            close_global_progress();
            Ok::<_, rquickjs::Error>(())
        },
    )?;
    progress_obj.set("close", close_fn.clone())?;

    // 挂载至 launcher.progress 与 app.progress
    launcher.set("progress", progress_obj.clone())?;
    app_obj.set("progress", progress_obj)?;

    // 便捷函数: launcher.show_progress / update_progress / close_progress
    launcher.set("show_progress", show_fn.clone())?;
    launcher.set("update_progress", update_fn.clone())?;
    launcher.set("close_progress", close_fn.clone())?;

    app_obj.set("show_progress", show_fn)?;
    app_obj.set("update_progress", update_fn)?;
    app_obj.set("close_progress", close_fn)?;

    Ok(())
}
