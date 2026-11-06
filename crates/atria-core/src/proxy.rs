//! Optional HTTP proxy for outbound downloads (HF models + llama engine).
//!
//! Atria's HTTP clients connect **directly** by default. On networks where
//! huggingface.co / github.com are only reachable through a local proxy
//! (V2Ray, Clash, Clash Verge, etc. — very common in Iran), a direct
//! connect hangs for minutes and every "download" looks dead. The user can
//! set e.g. `http://127.0.0.1:7890` on the models page; all shared clients
//! then route through it. The value is process-global (`set`) and persisted
//! as `~/.atria/hf-proxy.json` (`save_file` / `load_file`) so it survives
//! restarts.

use std::sync::RwLock;

static PROXY: RwLock<Option<String>> = RwLock::new(None);

fn w() -> std::sync::RwLockWriteGuard<'static, Option<String>> {
    PROXY.write().unwrap_or_else(|p| p.into_inner())
}
fn r() -> std::sync::RwLockReadGuard<'static, Option<String>> {
    PROXY.read().unwrap_or_else(|p| p.into_inner())
}

/// Set (or clear, via empty/None) the process-wide proxy URL.
pub fn set(url: Option<String>) {
    let u = url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    *w() = u;
}

/// Currently configured proxy URL, if any.
pub fn current() -> Option<String> {
    r().clone()
}

/// Validate a proxy URL: reqwest supports `http(s)://` and `socks5://`.
pub fn validate(url: &str) -> Result<(), String> {
    let u = url.trim();
    if u.is_empty() {
        return Ok(());
    }
    if u.starts_with("http://") || u.starts_with("https://") || u.starts_with("socks5://") {
        Ok(())
    } else {
        Err("آدرس پروکسی باید با http:// یا socks5:// شروع شود (مثلاً http://127.0.0.1:7890)".into())
    }
}

/// Persist the proxy setting as JSON (`{"url": "..."}`).
pub fn save_file(path: &std::path::Path, url: &str) -> Result<(), String> {
    validate(url)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("cannot create folder: {e}"))?;
    }
    let v = serde_json::json!({ "url": url.trim() });
    std::fs::write(path, v.to_string()).map_err(|e| format!("cannot write proxy config: {e}"))
}

/// Load a proxy setting written by `save_file`. Invalid/missing → None.
pub fn load_file(path: &std::path::Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let u = v.get("url")?.as_str()?.trim().to_string();
    if u.is_empty() {
        return None;
    }
    validate(&u).ok()?;
    Some(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_validate_accepts_supported_schemes() {
        assert!(validate("http://127.0.0.1:7890").is_ok());
        assert!(validate("https://proxy.example:8080").is_ok());
        assert!(validate("socks5://127.0.0.1:1080").is_ok());
        assert!(validate("").is_ok());
    }

    #[test]
    fn proxy_validate_rejects_bad_schemes() {
        assert!(validate("ftp://x:1").is_err());
        assert!(validate("127.0.0.1:7890").is_err());
        assert!(validate("x").is_err());
        let e = validate("ftp://x:1").unwrap_err();
        assert!(e.contains("http://"), "error should hint the right format: {e}");
    }

    #[test]
    fn proxy_set_get_clear() {
        // NOTE: process-global — keep this test independent of order.
        set(Some("  http://127.0.0.1:7890  ".into()));
        assert_eq!(current().as_deref(), Some("http://127.0.0.1:7890"));
        set(Some("   ".into()));
        assert_eq!(current(), None);
        set(None);
        assert_eq!(current(), None);
    }

    #[test]
    fn proxy_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("atria-proxy-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("hf-proxy.json");
        save_file(&p, "http://127.0.0.1:7890").unwrap();
        assert_eq!(load_file(&p).as_deref(), Some("http://127.0.0.1:7890"));
        save_file(&p, "").unwrap();
        assert_eq!(load_file(&p), None);
        let bad = dir.join("bad.json");
        std::fs::write(&bad, "not-json").unwrap();
        assert_eq!(load_file(&bad), None);
        save_file(&dir.join("bad2.json"), "ftp://x").unwrap_err();
        std::fs::remove_dir_all(&dir).ok();
    }
}
