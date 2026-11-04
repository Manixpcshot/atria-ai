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

fn gallery() -> Result<Gallery, String> {
    Gallery::open(&root().join("images"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_dir_is_under_atria_root() {
        let g = gallery().unwrap();
        let path = g.dir();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert_eq!(name, "images");
        assert!(path.parent().map(|p| p == root()).unwrap_or(false));
    }
}
