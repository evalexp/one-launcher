use rquickjs::function::Opt;
use rquickjs::{Array, ArrayBuffer, Ctx, Function, Object, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use windows::core::HSTRING;
use windows::Win32::Storage::FileSystem::{
    CopyFileW, DeleteFileW, MoveFileExW,
    MOVEFILE_COPY_ALLOWED, MOVEFILE_REPLACE_EXISTING,
};

/// 从 QuickJS Value 中智能提取字节数组（支持 String, ArrayBuffer, TypedArray 如 Uint8Array 等）
pub fn extract_bytes(val: &Value) -> Option<Vec<u8>> {
    if let Some(s) = val.as_string() {
        return s.to_string().ok().map(|s| s.into_bytes());
    }
    if let Some(ab) = ArrayBuffer::from_value(val.clone()) {
        return unsafe { ab.as_bytes() }.map(|s| s.to_vec());
    }
    if let Some(obj) = val.as_object() {
        if let Ok(ab) = obj.get::<_, ArrayBuffer>("buffer") {
            if let Some(slice) = unsafe { ab.as_bytes() } {
                let byte_offset: usize = obj.get("byteOffset").unwrap_or(0);
                let byte_len: usize = obj.get("byteLength").unwrap_or(slice.len().saturating_sub(byte_offset));
                let end = (byte_offset + byte_len).min(slice.len());
                return Some(slice[byte_offset..end].to_vec());
            }
        }
    }
    None
}

/// 向 launcher 对象注册 fs 模块
pub fn register_fs<'js>(ctx: &Ctx<'js>, launcher: &Object<'js>) -> Result<(), rquickjs::Error> {
    let fs_obj = Object::new(ctx.clone())?;

    // launcher.fs.read(path) -> ArrayBuffer | null
    let read_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>, path: String| -> Result<Option<ArrayBuffer<'js>>, rquickjs::Error> {
            match fs::read(&path) {
                Ok(bytes) => ArrayBuffer::new_copy(ctx, &bytes).map(Some),
                Err(_) => Ok(None),
            }
        },
    )?;
    fs_obj.set("read", read_fn)?;

    // launcher.fs.read_text(path) -> string | null
    let read_text_fn = Function::new(ctx.clone(), |path: String| {
        fs::read_to_string(&path).ok()
    })?;
    fs_obj.set("read_text", read_text_fn)?;

    // launcher.fs.write(path, data) -> boolean (支持 String, ArrayBuffer, Uint8Array)
    let write_fn = Function::new(ctx.clone(), |path: String, data: Value| {
        let bytes = match extract_bytes(&data) {
            Some(b) => b,
            None => return false,
        };
        let p = Path::new(&path);
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(p, bytes).is_ok()
    })?;
    fs_obj.set("write", write_fn.clone())?;
    fs_obj.set("write_file", write_fn.clone())?;
    fs_obj.set("write_text", write_fn)?;

    // launcher.fs.append(path, data) -> boolean
    let append_fn = Function::new(ctx.clone(), |path: String, data: Value| {
        let bytes = match extract_bytes(&data) {
            Some(b) => b,
            None => return false,
        };
        let p = Path::new(&path);
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .and_then(|mut f| f.write_all(&bytes))
            .is_ok()
    })?;
    fs_obj.set("append", append_fn.clone())?;
    fs_obj.set("append_text", append_fn)?;

    // launcher.fs.copy(src, dst, overwrite?: boolean) -> boolean (Windows CopyFileW)
    let copy_fn = Function::new(
        ctx.clone(),
        |src: String, dst: String, overwrite: Opt<bool>| {
            let p_dst = Path::new(&dst);
            if let Some(parent) = p_dst.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let src_w = HSTRING::from(src);
            let dst_w = HSTRING::from(dst);
            let fail_if_exists = !overwrite.0.unwrap_or(true);
            unsafe { CopyFileW(&src_w, &dst_w, fail_if_exists).is_ok() }
        },
    )?;
    fs_obj.set("copy", copy_fn)?;

    // launcher.fs.move(src, dst, overwrite?: boolean) -> boolean (Windows MoveFileExW)
    let move_fn = Function::new(
        ctx.clone(),
        |src: String, dst: String, overwrite: Opt<bool>| {
            let p_dst = Path::new(&dst);
            if let Some(parent) = p_dst.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let src_w = HSTRING::from(src);
            let dst_w = HSTRING::from(dst);
            let mut flags = MOVEFILE_COPY_ALLOWED;
            if overwrite.0.unwrap_or(true) {
                flags |= MOVEFILE_REPLACE_EXISTING;
            }
            unsafe { MoveFileExW(&src_w, &dst_w, flags).is_ok() }
        },
    )?;
    fs_obj.set("move", move_fn)?;

    // launcher.fs.remove(path) -> boolean
    let remove_fn = Function::new(ctx.clone(), |path: String| {
        let p = Path::new(&path);
        if !p.exists() {
            return false;
        }
        if p.is_dir() {
            fs::remove_dir_all(p).is_ok()
        } else {
            let path_w = HSTRING::from(path);
            unsafe { DeleteFileW(&path_w).is_ok() }
        }
    })?;
    fs_obj.set("remove", remove_fn.clone())?;
    fs_obj.set("delete", remove_fn)?;

    // launcher.fs.mkdir(path) -> boolean
    let mkdir_fn = Function::new(ctx.clone(), |path: String| {
        fs::create_dir_all(&path).is_ok()
    })?;
    fs_obj.set("mkdir", mkdir_fn)?;

    // launcher.fs.exists(path) -> boolean
    let exists_fn = Function::new(ctx.clone(), |path: String| {
        Path::new(&path).exists()
    })?;
    fs_obj.set("exists", exists_fn)?;

    // launcher.fs.list(path) -> Array<{ name, path, is_dir, is_file, size }> | null
    let list_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>, path: String| -> Result<Option<Array<'js>>, rquickjs::Error> {
            let entries = match fs::read_dir(&path) {
                Ok(e) => e,
                Err(_) => return Ok(None),
            };
            let arr = Array::new(ctx.clone())?;
            let mut idx = 0;
            for entry in entries.flatten() {
                let p = entry.path();
                let meta = entry.metadata().ok();
                let item = Object::new(ctx.clone())?;
                item.set("name", entry.file_name().to_string_lossy().to_string())?;
                item.set("path", p.to_string_lossy().to_string())?;
                item.set("is_dir", meta.as_ref().map(|m| m.is_dir()).unwrap_or(false))?;
                item.set("is_file", meta.as_ref().map(|m| m.is_file()).unwrap_or(false))?;
                item.set("size", meta.as_ref().map(|m| m.len()).unwrap_or(0))?;
                arr.set(idx, item)?;
                idx += 1;
            }
            Ok(Some(arr))
        },
    )?;
    fs_obj.set("list", list_fn.clone())?;
    fs_obj.set("read_dir", list_fn)?;

    // launcher.fs.stat(path) -> { is_dir, is_file, size, modified_ms } | null
    let stat_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'js>, path: String| -> Result<Option<Object<'js>>, rquickjs::Error> {
            let meta = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => return Ok(None),
            };
            let item = Object::new(ctx)?;
            item.set("is_dir", meta.is_dir())?;
            item.set("is_file", meta.is_file())?;
            item.set("size", meta.len())?;
            if let Ok(modified) = meta.modified() {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    item.set("modified_ms", duration.as_millis() as f64)?;
                }
            }
            Ok(Some(item))
        },
    )?;
    fs_obj.set("stat", stat_fn)?;

    launcher.set("fs", fs_obj)?;
    Ok(())
}
