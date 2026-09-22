//! Persistent note storage for the `remember` / `recall` agent tools.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single saved note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub title: String,
    pub content: String,
    pub ts: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Store {
    notes: Vec<Note>,
}

/// JSON-file-backed note store.
#[derive(Debug)]
pub struct MemoryStore {
    path: Option<PathBuf>,
    notes: Vec<Note>,
}

impl MemoryStore {
    /// Open (or create) a store at `path`.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let notes = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Store>(&s).ok())
            .map(|st| st.notes)
            .unwrap_or_default();
        Self { path: Some(path), notes }
    }

    /// Store that lives only in RAM (used in tests).
    pub fn in_memory() -> Self {
        Self { path: None, notes: Vec::new() }
    }

    pub fn add(&mut self, title: String, content: String) {
        let ts = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
        self.notes.push(Note { title, content, ts });
        self.save();
    }

    pub fn clear(&mut self) {
        self.notes.clear();
        self.save();
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// Case-insensitive search in title + content. `None` lists everything.
    pub fn search(&self, q: Option<&str>) -> Vec<&Note> {
        match q {
            None | Some("") => self.notes.iter().collect(),
            Some(q) => {
                let ql = q.to_lowercase();
                self.notes
                    .iter()
                    .filter(|n| {
                        n.title.to_lowercase().contains(&ql) || n.content.to_lowercase().contains(&ql)
                    })
                    .collect()
            }
        }
    }

    fn save(&self) {
        if let Some(p) = &self.path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let st = Store { notes: self.notes.clone() };
            if let Ok(s) = serde_json::to_string_pretty(&st) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_search_roundtrip() {
        let dir = std::env::temp_dir().join(format!("atria-mem-test-{}", std::process::id()));
        let path = dir.join("memory.json");
        let _ = std::fs::remove_dir_all(&dir);

        let mut mem = MemoryStore::open(&path);
        mem.add("قهوه".into(), "قهوه با شیر بدون شکر".into());
        mem.add("project".into(), "Atria uses Rust".into());

        let mut mem2 = MemoryStore::open(&path);
        assert_eq!(mem2.notes().len(), 2, "store must persist to disk");
        assert_eq!(mem2.search(Some("rust")).len(), 1);
        assert_eq!(mem2.search(Some("قهوه")).len(), 1);
        assert_eq!(mem2.search(None).len(), 2);

        mem2.clear();
        assert_eq!(MemoryStore::open(&path).notes().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
