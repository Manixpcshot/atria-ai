//! Image studio IPC: generation through the six providers, gallery list/
//! delete/bytes. Source configs (with keys) arrive per call — nothing is
//! persisted by this module (the gallery stores results only).

use crate::commands::AppState;
use atria_core::gallery::{Gallery, GalleryEntry, new_id};
use atria_core::imggen::{self, ImageRequest, ImageSource};
use std::path::PathBuf;
use std::sync::LazyLock;
use tauri::State;

fn root() -> PathBuf {
    crate::commands::atria_root()
}

/// v0.25.0: configurable image output folder. Default is `~/.atria/images`;
/// a user-picked override is stored in `~/.atria/image-out.json`. The gallery
/// is self-contained (files + gallery.json), so switching folders migrates
/// the existing images with it.
fn images_dir_cfg_path() -> PathBuf {
    root().join("image-out.json")
}
fn images_dir() -> PathBuf {
    let raw = std::fs::read_to_string(images_dir_cfg_path()).ok();
    if let Some(dir) = raw
        .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
        .and_then(|v| v.get("dir").and_then(|d| d.as_str()).map(String::from))
    {
        let p = PathBuf::from(dir);
        if p.is_absolute() {
            let _ = std::fs::create_dir_all(&p);
            return p;
        }
    }
    root().join("images")
}

fn gallery() -> Result<Gallery, String> {
    Gallery::open(&images_dir())
}

/// Generation can take minutes (local SDXL); serialize to avoid piling up
/// parallel renders that would exhaust RAM.
static GEN_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

#[tauri::command]
pub async fn img_generate(
    source: ImageSource,
    req: ImageRequest,
    st: State<'_, AppState>,
) -> Result<Vec<GalleryEntry>, String> {
    let _ = st;
    let mut req = req;
    req.validate()?;
    let _guard = GEN_LOCK.lock().await;
    let outs = imggen::generate(&source, &req).await?;
    let mut g = gallery()?;
    let mut entries = Vec::new();
    for out in outs {
        let entry = GalleryEntry {
            id: new_id(),
            ts: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            prompt: req.prompt.trim().to_string(),
            negative: req.negative.trim().to_string(),
            source: out.source.clone(),
            model: out.model.clone(),
            width: req.width,
            height: req.height,
            mime: out.mime.clone(),
            bytes: out.bytes,
            file: String::new(),
        };
        let saved = g.add(&out.b64, &out.mime, entry)?;
        entries.push(saved);
    }
    Ok(entries)
}

#[tauri::command]
pub async fn img_gallery_list(st: State<'_, AppState>) -> Result<Vec<GalleryEntry>, String> {
    let _ = st;
    Ok(gallery()?.list())
}

#[tauri::command]
pub async fn img_delete(id: String, st: State<'_, AppState>) -> Result<(), String> {
    let _ = st;
    let mut g = gallery()?;
    g.delete(&id)
}

#[tauri::command]
pub async fn img_bytes(id: String, st: State<'_, AppState>) -> Result<String, String> {
    let _ = st;
    use base64::Engine;
    let bytes = gallery()?.get_bytes(&id)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[tauri::command]
pub async fn img_folder(st: State<'_, AppState>) -> Result<String, String> {
    let _ = st;
    gallery().map(|g| g.dir().to_string_lossy().into_owned())
}

/// v0.25.0: point the image output folder at a user-selected directory.
/// Existing images are migrated into the new folder so nothing is lost.
#[tauri::command]
pub async fn img_set_folder(
    folder: PathBuf,
    st: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let _ = st;
    let folder = folder
        .canonicalize()
        .map_err(|_| "پوشه پیدا نشد — دوباره انتخاب کنید".to_string())?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("cannot create folder: {e}"))?;
    let current = images_dir();
    if current.canonicalize().ok() != Some(folder.clone()) {
        for entry in std::fs::read_dir(&current).map_err(|e| format!("cannot read current folder: {e}"))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let to = folder.join(entry.file_name());
            let _ = std::fs::rename(entry.path(), &to);
        }
    }
    std::fs::write(
        images_dir_cfg_path(),
        serde_json::json!({ "dir": folder.to_string_lossy() }).to_string(),
    )
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "folder": folder.to_string_lossy() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_dir_is_under_atria_root() {
        // v0.25.0: the output folder is configurable — test the DEFAULT by
        // temporarily hiding any user override.
        let cfg_path = images_dir_cfg_path();
        let had_cfg = std::fs::read(&cfg_path).ok();
        let _ = std::fs::remove_file(&cfg_path);
        let g = gallery().unwrap();
        let path = g.dir();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert_eq!(name, "images");
        assert!(path.parent().map(|p| p == root()).unwrap_or(false));
        if let Some(raw) = had_cfg {
            let _ = std::fs::write(&cfg_path, raw);
        }
    }
}
