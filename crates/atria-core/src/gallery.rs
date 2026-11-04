//! Persistent gallery of generated images: `{root}/images/{id}.png` +
//! `gallery.json` metadata (prompt, source, model, size, timestamp).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryEntry {
    /// `{tsms}-{rand4hex}`
    pub id: String,
    /// unix ms
    pub ts: u64,
    pub prompt: String,
    #[serde(default)]
    pub negative: String,
    pub source: String,
    pub model: String,
    pub width: u32,
    pub height: u32,
    /// mime type
    pub mime: String,
    /// original byte length
    pub bytes: u64,
    /// file name relative to the images dir
    pub file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct GalleryFile {
    #[serde(default)]
    entries: Vec<GalleryEntry>,
}

pub struct Gallery {
    dir: PathBuf,
    meta: GalleryFile,
}

impl Gallery {
    /// Open (or create) the gallery under `dir`.
    pub fn open(dir: &std::path::Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create gallery dir: {e}"))?;
        let meta_path = dir.join("gallery.json");
        let meta = if meta_path.exists() {
            let raw = std::fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            GalleryFile::default()
        };
        Ok(Self { dir: dir.to_path_buf(), meta })
    }

    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    fn save_meta(&self) -> Result<(), String> {
        let raw = serde_json::to_string_pretty(&self.meta).map_err(|e| e.to_string())?;
        std::fs::write(self.dir.join("gallery.json"), raw).map_err(|e| e.to_string())
    }

    /// Store a base64 image; returns the metadata entry.
    pub fn add(
        &mut self,
        b64: &str,
        mime: &str,
        entry: GalleryEntry,
    ) -> Result<GalleryEntry, String> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("invalid image data: {e}"))?;
        let ext = match mime {
            "image/jpeg" | "image/jpg" => "jpg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            "image/png" | _ => "png",
        };
        let mut entry = entry;
        let file = format!("{}.{}", entry.id, ext);
        let path = self.dir.join(&file);
        std::fs::write(&path, &bytes).map_err(|e| format!("cannot write image: {e}"))?;
        entry.file = file;
        entry.bytes = bytes.len() as u64;
        self.meta.entries.insert(0, entry.clone());
        // keep the metadata bounded
        if self.meta.entries.len() > 500 {
            self.meta.entries.truncate(500);
        }
        self.save_meta()?;
        Ok(entry)
    }

    /// Newest first.
    pub fn list(&self) -> Vec<GalleryEntry> {
        self.meta.entries.clone()
    }

    pub fn get_bytes(&self, id: &str) -> Result<Vec<u8>, String> {
        let entry = self
            .meta
            .entries
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| "image پیدا نشد".to_string())?;
        std::fs::read(self.dir.join(&entry.file)).map_err(|e| format!("cannot read image: {e}"))
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let pos = self
            .meta
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| "image پیدا نشد".to_string())?;
        let entry = self.meta.entries.remove(pos);
        let _ = std::fs::remove_file(self.dir.join(&entry.file));
        self.save_meta()
    }
}

/// Generate a gallery entry id: `{tsms}-{4hex}`.
pub fn new_id() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    // cheap entropy: process id xor time
    let rand = (ts ^ (std::process::id() as u64) << 13) % 0x10000;
    format!("{ts}-{rand:04x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_add_list_delete() {
        let dir = std::env::temp_dir().join(format!("atria-gallery-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = Gallery::open(&dir).unwrap();
        let png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let entry = GalleryEntry {
            id: new_id(),
            ts: 1,
            prompt: "test".into(),
            negative: String::new(),
            source: "hf".into(),
            model: "m".into(),
            width: 1,
            height: 1,
            mime: "image/png".into(),
            bytes: 0,
            file: String::new(),
        };
        let saved = g.add(png_b64, "image/png", entry).unwrap();
        assert!(dir.join(&saved.file).exists());
        assert_eq!(g.list().len(), 1);
        assert_eq!(g.list()[0].id, saved.id);
        let bytes = g.get_bytes(&saved.id).unwrap();
        assert!(bytes.len() > 10);
        g.delete(&saved.id).unwrap();
        assert!(g.list().is_empty());
        assert!(!dir.join(&saved.file).exists());
        assert!(g.delete(&saved.id).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gallery_persists_across_open() {
        let dir = std::env::temp_dir().join(format!("atria-gallery-p-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut g = Gallery::open(&dir).unwrap();
            let png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
            g.add(png_b64, "image/png", GalleryEntry {
                id: new_id(), ts: 1, prompt: "p".into(), negative: String::new(),
                source: "s".into(), model: "m".into(), width: 1, height: 1,
                mime: "image/png".into(), bytes: 0, file: String::new(),
            }).unwrap();
        }
        let g2 = Gallery::open(&dir).unwrap();
        assert_eq!(g2.list().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_are_unique() {
        let a = new_id();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = new_id();
        assert_ne!(a, b);
        assert!(a.len() >= 13);
    }
}
