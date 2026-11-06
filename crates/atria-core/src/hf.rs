//! Hugging Face Hub — model library backend.
//!
//! * **Search** public models through the public Hub API (`/api/models`).
//! * **File listing** for a model repo (`/api/models/{repo}/tree/main?recursive=true`),
//!   with real LFS sizes (the pointer-file `size` is replaced by `lfs.size`).
//! * **Resumable downloads** of model files into a user-selected folder:
//!   streamed through a `.part` file, HTTP `Range` resume, per-chunk progress,
//!   cooperative pause/cancel.
//! * **Persistent queue** (`HfQueue`) saved as JSON so the app can show
//!   in-flight/finished jobs after a restart.
//!
//! Safety: only `https://huggingface.co` is contacted; files only land under the
//! selected models root through sanitized relative paths (max depth 3, no
//! traversal, no reserved names).

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

pub const HF_API: &str = "https://huggingface.co/api";
pub const HF_WEB: &str = "https://huggingface.co";

const MAX_DEPTH: usize = 3;
const RETRIES: u32 = 3;
const BACKOFF_MS: u64 = 1200;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A model from the Hub search API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfModel {
    /// e.g. `stabilityai/stable-diffusion-xl-base-1.0`
    pub id: String,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub likes: u64,
    /// e.g. `image-generation`, `text-generation`
    #[serde(default)]
    pub pipeline_tag: Option<String>,
}

/// One downloadable file of a model repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfFile {
    pub path: String,
    /// real size (LFS-aware)
    pub size: u64,
    /// stored on Git LFS (true for model weights)
    #[serde(default)]
    pub lfs: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileState {
    Pending,
    Downloading,
    Paused,
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfFileJob {
    pub path: String,
    pub size: u64,
    /// bytes on disk (in the `.part` or final file)
    pub downloaded: u64,
    pub state: FileState,
    #[serde(default)]
    pub error: Option<String>,
}

/// One model job in the download queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfJob {
    pub id: String,
    /// `org/model` repo id
    pub repo: String,
    /// unix ms
    pub added_at: u64,
    /// absolute folder the files land in
    pub folder: PathBuf,
    pub files: Vec<HfFileJob>,
    #[serde(default)]
    pub error: Option<String>,
}

impl HfJob {
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    pub fn done_bytes(&self) -> u64 {
        self.files
            .iter()
            .map(|f| if f.state == FileState::Done { f.size } else { f.downloaded })
            .sum()
    }

    /// 0.0 .. 1.0 (1.0 when no bytes expected).
    pub fn progress(&self) -> f64 {
        let total = self.total_size();
        if total == 0 {
            return if self.files.iter().all(|f| f.state == FileState::Done) { 1.0 } else { 0.0 };
        }
        (self.done_bytes() as f64 / total as f64).min(1.0)
    }

    /// Aggregate state of the whole job.
    pub fn state(&self) -> FileState {
        if self.files.iter().any(|f| f.state == FileState::Done)
            && self.files.iter().all(|f| f.state == FileState::Done)
        {
            return FileState::Done;
        }
        if self.files.iter().any(|f| matches!(f.state, FileState::Error)) {
            return FileState::Error;
        }
        if self.files.iter().any(|f| f.state == FileState::Paused) {
            return FileState::Paused;
        }
        if self.files.iter().any(|f| matches!(f.state, FileState::Pending | FileState::Downloading))
        {
            return FileState::Pending;
        }
        FileState::Done
    }

    pub fn is_finished(&self) -> bool {
        self.state() == FileState::Done
    }
}

/// The persistent download queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfQueue {
    /// default folder for new jobs
    pub folder: PathBuf,
    #[serde(default)]
    pub jobs: Vec<HfJob>,
}

impl HfQueue {
    pub fn new(folder: PathBuf) -> Self {
        Self { folder, jobs: vec![] }
    }

    pub fn load(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create models folder: {e}"))?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, raw).map_err(|e| format!("cannot save models.json: {e}"))
    }

    pub fn job(&self, id: &str) -> Option<&HfJob> {
        self.jobs.iter().find(|j| j.id == id)
    }

    pub fn job_mut(&mut self, id: &str) -> Option<&mut HfJob> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    pub fn remove(&mut self, id: &str) -> Option<HfJob> {
        let pos = self.jobs.iter().position(|j| j.id == id)?;
        Some(self.jobs.remove(pos))
    }
}

// ---------------------------------------------------------------------------
// Hub API
// ---------------------------------------------------------------------------

/// Search public models. `pipeline` filters by task tag (e.g. `image-generation`).
pub async fn search_models(
    query: &str,
    pipeline: Option<&str>,
    limit: usize,
) -> Result<Vec<HfModel>, String> {
    let limit = limit.clamp(1, 40);
    let mut url = format!(
        "{HF_API}/models?limit={limit}&sort=downloads&direction=-1"
    );
    let q = query.trim();
    if !q.is_empty() {
        url.push_str(&format!("&search={}", urlenc(q)));
    }
    if let Some(tag) = pipeline.map(str::trim).filter(|t| !t.is_empty()) {
        url.push_str(&format!("&filter={}", urlenc(tag)));
    }
    let v: Vec<serde_json::Value> = http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("اتصال به Hugging Face ممکن نشد: {e}"))?
        .error_for_status()
        .map_err(|e| hf_http_err(e, None))?
        .json()
        .await
        .map_err(|e| format!("پاسخ Hugging Face معتبر نبود: {e}"))?;
    Ok(v
        .into_iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?.to_string();
            Some(HfModel {
                id,
                downloads: m.get("downloads").and_then(|d| d.as_u64()).unwrap_or(0),
                likes: m.get("likes").and_then(|d| d.as_u64()).unwrap_or(0),
                pipeline_tag: m.get("pipeline_tag").and_then(|d| d.as_str()).map(String::from),
            })
        })
        .collect())
}

/// List downloadable files of a model repo (recursive, main branch).
pub async fn repo_files(repo: &str) -> Result<Vec<HfFile>, String> {
    let repo = repo.trim();
    if !is_repo_id(repo) {
        return Err("repo id نامعتبر است — شکل: `org/model-name`".into());
    }
    let url = tree_url(repo);
    let v: Vec<serde_json::Value> = http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("اتصال به Hugging Face ممکن نشد: {e}"))?
        .error_for_status()
        .map_err(|e| hf_http_err(e, Some(repo)))?
        .json()
        .await
        .map_err(|e| format!("Hugging Face response is not JSON: {e}"))?;
    let mut out = Vec::new();
    for item in v {
        if item.get("type").and_then(|t| t.as_str()) != Some("file") {
            continue;
        }
        let path = match item.get("path").and_then(|p| p.as_str()) {
            Some(p) => p.to_string(),
            None => continue,
        };
        let lfs = item.get("lfs");
        let size = lfs
            .and_then(|l| l.get("size").and_then(|s| s.as_u64()))
            .or_else(|| item.get("size").and_then(|s| s.as_u64()))
            .unwrap_or(0);
        out.push(HfFile {
            path,
            size,
            lfs: lfs.is_some(),
        });
    }
    if out.is_empty() {
        return Err("فایلی برای دانلود در این repo پیدا نشد".into());
    }
    Ok(out)
}

/// `true` for `org/name` (single slash, sane segments).
pub fn is_repo_id(repo: &str) -> bool {
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() != 2 || parts.iter().any(|p| p.trim().is_empty()) {
        return false;
    }
    parts
        .iter()
        .all(|p| p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)))
}

/// Tree-listing API URL for a repo. The repo id is validated by
/// `is_repo_id` (ASCII `[A-Za-z0-9-_.]` segments, single slash) so it is
/// embedded verbatim — percent-encoding the slash (`%2F`) makes the HF API
/// return 400 Bad Request.
pub fn tree_url(repo: &str) -> String {
    format!("{HF_API}/models/{}/tree/main?recursive=true", repo.trim())
}

/// Friendly, actionable error text for HF API failures.
fn hf_err_text(status: Option<reqwest::StatusCode>, generic: &str, repo: Option<&str>) -> String {
    match status {
        Some(reqwest::StatusCode::NOT_FOUND) => match repo {
            Some(r) => format!("مدل `{r}` پیدا نشد (repo وجود ندارد یا خصوصی است)"),
            None => "چیزی پیدا نشد (404)".into(),
        },
        Some(reqwest::StatusCode::UNAUTHORIZED) => {
            "لوازم ورود: توکن Hugging Face را در بخش «مدل‌ها» ذخیره کنید (مدل gated/خصوصی است)".into()
        }
        Some(reqwest::StatusCode::FORBIDDEN) => match repo {
            Some(r) => format!("دسترسی به `{r}` رد شد — احتمالاً خصوصی است و توکن با دسترسی لازم ندارد"),
            None => "دسترسی رد شد (403)".into(),
        },
        Some(reqwest::StatusCode::TOO_MANY_REQUESTS) => {
            "Hugging Face اجازهٔ بیشتری در این لحظه نمی‌دهد — چند لحظه صبر کنید و دوباره امتحان کنید".into()
        }
        Some(code) => format!("Hugging Face error: HTTP {code}"),
        None => generic.to_string(),
    }
}

fn hf_http_err(e: reqwest::Error, repo: Option<&str>) -> String {
    let status = e.status();
    let generic = format!("Hugging Face error: {e}");
    hf_err_text(status, &generic, repo)
}

/// Direct download URL for a file in a repo (main branch).
pub fn file_url(repo: &str, path: &str) -> String {
    let mut out = format!("{HF_WEB}/{}/resolve/main/", repo.trim());
    for (i, seg) in path.trim_start_matches('/').split('/').enumerate() {
        if i > 0 {
            out.push('/');
        }
        out.push_str(&urlenc(seg));
    }
    out
}

fn urlenc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------------

/// Cooperative control for a running download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ctrl {
    Run,
    Pause,
    Stop,
}

/// Terminal outcome of a `download_file` call.
///
/// **Regression (v0.25.0):** pause used to surface as `Ok(())`, which the
/// caller treated as "file complete" — a paused job was marked *done* with
/// the full size and its resume skipped the partially-downloaded file,
/// leaving a broken model while the UI showed success. Pause is now an
/// explicit, distinguishable outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DlOutcome {
    /// File fully present at `dest`.
    Done,
    /// User paused; the `.part` file is kept for a later resume.
    Paused,
}

pub fn ctrl_cell() -> AtomicU8 {
    AtomicU8::new(0)
}
pub fn ctrl_get(c: &AtomicU8) -> Ctrl {
    match c.load(Ordering::Relaxed) {
        1 => Ctrl::Pause,
        2 => Ctrl::Stop,
        _ => Ctrl::Run,
    }
}
pub fn ctrl_set(c: &AtomicU8, v: Ctrl) {
    c.store(match v {
        Ctrl::Run => 0,
        Ctrl::Pause => 1,
        Ctrl::Stop => 2,
    }, Ordering::Relaxed);
}

/// Sanitize a repo-relative file path against the destination root.
/// Returns the destination path or an error.
pub fn safe_dest(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim_matches('/');
    if rel.is_empty() {
        return Err("empty file path".into());
    }
    let mut dest = root.to_path_buf();
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            return Err(format!("مسیر `{rel}` معتبر نیست (traversal)"));
        }
        if dest.components().count() > root.components().count() + MAX_DEPTH {
            return Err(format!("مسیر `{rel}` بیش از حد عمیق است"));
        }
        dest = dest.join(seg);
    }
    Ok(dest)
}

fn part_path(dest: &Path) -> PathBuf {
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = dest.parent().unwrap_or(Path::new("."));
    parent.join(format!("{name}.atria-part"))
}

/// Download `url` to `dest` with resume + progress + cooperative control.
///
/// `progress` receives `(downloaded_bytes, total_bytes)` where total may be 0
/// when the server sends no Content-Length.
///
/// Returns [`DlOutcome::Done`] when the file is complete, [`DlOutcome::Paused`]
/// when the user paused (`.part` kept), or `Err("cancelled")` on stop.
pub async fn download_file(
    url: &str,
    token: Option<&str>,
    dest: &Path,
    ctrl: &AtomicU8,
    mut progress: impl FnMut(u64, u64),
) -> Result<DlOutcome, String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create folder: {e}"))?;
    }
    if dest.exists() {
        // already complete (e.g. resumed job)
        let size = dest.metadata().map(|m| m.len()).unwrap_or(0);
        progress(size, size);
        return Ok(DlOutcome::Done);
    }
    let part = part_path(dest);
    let mut existing = part.metadata().map(|m| m.len()).unwrap_or(0);
    let mut last_err = String::new();

    for attempt in 0..RETRIES {
        if ctrl_get(ctrl) == Ctrl::Stop {
            return Err("cancelled".into());
        }
        let req = {
            let mut req = http_client().get(url);
            if existing > 0 {
                req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
            }
            if let Some(t) = token.map(str::trim).filter(|t| !t.is_empty()) {
                req = req.header(reqwest::header::AUTHORIZATION, format!("Bearer {t}"));
            }
            req.timeout(Duration::from_secs(3600))
        };
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("connection failed: {e}");
                tokio::time::sleep(Duration::from_millis(BACKOFF_MS * (attempt as u64 + 1))).await;
                continue;
            }
        };

        // 200 = server ignored Range → restart from zero.
        if resp.status() == reqwest::StatusCode::OK && existing > 0 {
            existing = 0;
            let _ = std::fs::File::create(&part);
        }
        if !resp.status().is_success()
            && resp.status() != reqwest::StatusCode::RANGE_NOT_SATISFIABLE
        {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let hint = if body.len() < 300 {
                body
            } else {
                String::from(&body[..300])
            };
            let msg = match status {
                reqwest::StatusCode::UNAUTHORIZED => {
                    "این مدل gated/خصوصی است — برای دانلود فایل‌ها باید توکن Hugging Face (با پذیرش لایسنس مدل) را در بخش «مدل‌ها» ذخیره کنید".to_string()
                }
                reqwest::StatusCode::FORBIDDEN => {
                    "دسترسی به فایل رد شد (403) — مدل خصوصی است یا توکن با دسترسی لازم ندارد".to_string()
                }
                reqwest::StatusCode::NOT_FOUND => "فایل در repo پیدا نشد (404)".to_string(),
                _ => format!("Hugging Face error: HTTP {status} — {hint}"),
            };
            return Err(msg);
        }
        if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            // part file already complete → finalize
            let _ = std::fs::rename(&part, dest);
            let size = dest.metadata().map(|m| m.len()).unwrap_or(0);
            progress(size, size);
            return Ok(DlOutcome::Done);
        }

        let total = match resp.content_length() {
            Some(t) => existing + t,
            None => 0,
        };
        progress(existing, total);

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        // append(true) in both cases: new file (empty) or resume at end of `.part`
        let mut file = match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&part)
        {
            Ok(f) => f,
            Err(e) => return Err(format!("cannot open temp file: {e}")),
        };
        let mut loop_err = None;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if let Err(e) = file.write_all(&bytes) {
                        loop_err = Some(format!("write failed: {e}"));
                        break;
                    }
                    existing += bytes.len() as u64;
                    progress(existing, total);
                }
                Err(e) => {
                    loop_err = Some(format!("stream error: {e}"));
                    break;
                }
            }
            match ctrl_get(ctrl) {
                Ctrl::Run => {}
                Ctrl::Pause => {
                    return Ok(DlOutcome::Paused); // keep `.part` for resume
                }
                Ctrl::Stop => {
                    let _ = std::fs::remove_file(&part);
                    return Err("cancelled".into());
                }
            }
        }
        if let Some(e) = loop_err {
            last_err = e;
            tokio::time::sleep(Duration::from_millis(BACKOFF_MS * (attempt as u64 + 1))).await;
            continue;
        }
        // stream finished
        let _ = std::fs::rename(&part, dest);
        let size = dest.metadata().map(|m| m.len()).unwrap_or(0);
        progress(size, total.max(size));
        return Ok(DlOutcome::Done);
    }
    Err(format!("download failed after {RETRIES} attempts: {last_err}"))
}

/// Shared HTTP client for all HF traffic (search, file lists, downloads).
///
/// v0.26.2: routes through the user-configured proxy when one is set (see
/// `crate::proxy`), and fails the CONNECT step fast (15s) so a blocked /
/// filtered network surfaces an error within seconds instead of hanging the
/// download button for minutes.
fn http_client() -> reqwest::Client {
    let mut b = reqwest::Client::builder()
        .user_agent("atria-desktop/0.26 (model library)")
        .timeout(Duration::from_secs(90))
        .connect_timeout(Duration::from_secs(15));
    if let Some(p) = crate::proxy::current() {
        match reqwest::Proxy::all(&p) {
            Ok(pr) => b = b.proxy(pr),
            Err(_) => eprintln!("hf: ignoring invalid proxy url: {p}"),
        }
    }
    b.build()
        .expect("reqwest client")
}

/// Quick reachability probe behind the «آزمون اتصال» button.
/// Returns elapsed milliseconds on success, a Persian error otherwise.
pub async fn ping_hf() -> Result<u128, String> {
    let started = std::time::Instant::now();
    let r = http_client()
        .get(format!("{HF_API}/models?limit=1"))
        .send()
        .await
        .map_err(|e| hf_http_err(e, None))?;
    if !r.status().is_success() {
        return Err(hf_err_text(Some(r.status()), "هوشینگ‌فیس پاسخ درست نداد", None));
    }
    Ok(started.elapsed().as_millis())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn repo_id_validation() {
        assert!(is_repo_id("org/model"));
        assert!(is_repo_id("stabilityai/stable-diffusion-xl-base-1.0"));
        assert!(is_repo_id("org/model_2-x"));
        assert!(!is_repo_id("noslash"));
        assert!(!is_repo_id("a/b/c"));
        assert!(!is_repo_id("/model"));
        assert!(!is_repo_id("org/"));
        assert!(!is_repo_id("org/bad name"));
    }

    #[test]
    fn url_encoding() {
        assert_eq!(urlenc("a/b c~1"), "a%2Fb%20c~1");
        assert_eq!(file_url("org/m", "weights/model.safetensors"),
            "https://huggingface.co/org/m/resolve/main/weights/model.safetensors");
    }

    #[test]
    fn tree_url_keeps_repo_slash_verbatim() {
        // Regression: percent-encoding the repo slash (%2F) made the HF API
        // answer 400 Bad Request.
        assert_eq!(
            tree_url("black-forest-labs/Flux-3-image-klein-comfy"),
            "https://huggingface.co/api/models/black-forest-labs/Flux-3-image-klein-comfy/tree/main?recursive=true"
        );
    }

    #[test]
    fn hf_error_text_is_actionable() {
        use reqwest::StatusCode;
        assert!(hf_err_text(Some(StatusCode::NOT_FOUND), "g", Some("o/m")).contains("پیدا نشد"));
        assert!(hf_err_text(Some(StatusCode::UNAUTHORIZED), "g", None).contains("توکن"));
        assert!(hf_err_text(Some(StatusCode::FORBIDDEN), "g", Some("o/m")).contains("دسترسی"));
        assert!(hf_err_text(Some(StatusCode::TOO_MANY_REQUESTS), "g", None).contains("صبر کنید"));
        assert_eq!(hf_err_text(Some(StatusCode::BAD_REQUEST), "g", None), "Hugging Face error: HTTP 400 Bad Request");
        assert_eq!(hf_err_text(None, "connection dropped", None), "connection dropped");
    }

    #[test]
    fn safe_dest_blocks_traversal_and_depth() {
        let root = PathBuf::from("/home/u/.atria/models");
        assert_eq!(safe_dest(&root, "org/model.safetensors").unwrap(),
            root.join("org/model.safetensors"));
        assert!(safe_dest(&root, "../etc/passwd").is_err());
        assert!(safe_dest(&root, "a/b/c/d/e/f.bin").is_err());
        assert!(safe_dest(&root, "").is_err());
        assert!(safe_dest(&root, "/").is_err());
    }

    #[test]
    fn job_aggregate_states() {
        let mk = |files: Vec<FileState>| HfJob {
            id: "j".into(),
            repo: "o/m".into(),
            added_at: 0,
            folder: PathBuf::from("/x"),
            files: files
                .into_iter()
                .enumerate()
                .map(|(i, s)| HfFileJob {
                    path: format!("f{i}"),
                    size: 100,
                    downloaded: if s == FileState::Done { 100 } else { 0 },
                    state: s,
                    error: None,
                })
                .collect(),
            error: None,
        };
        assert_eq!(mk(vec![FileState::Done, FileState::Done]).state(), FileState::Done);
        assert_eq!(mk(vec![FileState::Done, FileState::Pending]).state(), FileState::Pending);
        assert_eq!(mk(vec![FileState::Error, FileState::Pending]).state(), FileState::Error);
        assert_eq!(mk(vec![FileState::Paused, FileState::Pending]).state(), FileState::Paused);
        assert!(mk(vec![FileState::Done]).is_finished());
        assert!(!mk(vec![FileState::Pending]).is_finished());
    }

    #[test]
    fn job_progress_math() {
        let j = HfJob {
            id: "j".into(),
            repo: "o/m".into(),
            added_at: 0,
            folder: PathBuf::from("/x"),
            files: vec![
                HfFileJob { path: "a".into(), size: 100, downloaded: 50, state: FileState::Downloading, error: None },
                HfFileJob { path: "b".into(), size: 300, downloaded: 300, state: FileState::Done, error: None },
            ],
            error: None,
        };
        // (50 partial + 300 done) / 400 = 0.875
        assert!((j.progress() - 0.875).abs() < 1e-9);
        assert_eq!(j.total_size(), 400);
    }

    #[test]
    fn queue_roundtrip() {
        let dir = std::env::temp_dir().join(format!("hf-queue-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let q = HfQueue {
            folder: dir.clone(),
            jobs: vec![HfJob {
                id: "j1".into(),
                repo: "o/m".into(),
                added_at: 123,
                folder: dir.join("o"),
                files: vec![HfFileJob {
                    path: "w.bin".into(),
                    size: 10,
                    downloaded: 4,
                    state: FileState::Paused,
                    error: None,
                }],
                error: None,
            }],
        };
        let p = dir.join("models.json");
        q.save(&p).unwrap();
        let loaded = HfQueue::load(&p).unwrap();
        assert_eq!(loaded.jobs[0].id, "j1");
        assert_eq!(loaded.jobs[0].files[0].state, FileState::Paused);
        let mut l2 = loaded.clone();
        assert!(l2.remove("j1").is_some());
        assert!(l2.jobs.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ctrl_cell_transitions() {
        let c = ctrl_cell();
        assert_eq!(ctrl_get(&c), Ctrl::Run);
        ctrl_set(&c, Ctrl::Pause);
        assert_eq!(ctrl_get(&c), Ctrl::Pause);
        ctrl_set(&c, Ctrl::Stop);
        assert_eq!(ctrl_get(&c), Ctrl::Stop);
        ctrl_set(&c, Ctrl::Run);
        assert_eq!(ctrl_get(&c), Ctrl::Run);
    }

    #[test]
    fn parse_tree_payload_prefers_lfs_size() {
        // Mirrors repo_files() parsing against a captured HF payload shape.
        let v: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
              {"type":"directory","path":"weights"},
              {"type":"file","path":"README.md","size":1234},
              {"type":"file","path":"weights/model.safetensors","size":10,"lfs":{"size":2700000000,"sha256":"abc"}},
              {"type":"file","path":"model_index.json","size":512}
            ]"#,
        )
        .unwrap();
        let mut out = Vec::new();
        for item in v {
            if item.get("type").and_then(|t| t.as_str()) != Some("file") {
                continue;
            }
            let path = match item.get("path").and_then(|p| p.as_str()) {
                Some(p) => p.to_string(),
                None => continue,
            };
            let lfs = item.get("lfs");
            let size = lfs
                .and_then(|l| l.get("size").and_then(|s| s.as_u64()))
                .or_else(|| item.get("size").and_then(|s| s.as_u64()))
                .unwrap_or(0);
            out.push(HfFile { path, size, lfs: lfs.is_some() });
        }
        assert_eq!(out.len(), 3);
        assert_eq!(out[1].size, 2_700_000_000);
        assert!(out[1].lfs);
        assert_eq!(out[0].size, 1234);
        assert!(!out[0].lfs);
    }

    // ------------------------------------------------------------------
    // In-test HTTP server + download regression tests (v0.25.0)
    // ------------------------------------------------------------------

    /// Tiny single-file HTTP server: honors `Range` (206/416) or always
    /// answers 200 (when `honors_range` is false). Serves in `chunk`-sized
    /// pieces with a small pause between them so clients can observe
    /// partial progress. Records each request's Range header.
    async fn start_test_server(
        body: Vec<u8>,
        chunk: usize,
        honors_range: bool,
    ) -> (u16, Arc<tokio::sync::Mutex<Vec<String>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let reqs = requests.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(_) => break,
                };
                let body = body.clone();
                let reqs = reqs.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 8192];
                    let mut got = 0usize;
                    while !buf[..got].windows(4).any(|w| w == b"\r\n\r\n") {
                        match sock.read(&mut buf[got..]).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => got += n,
                        }
                    }
                    let head = String::from_utf8_lossy(&buf[..got]).to_string();
                    let mut range: Option<String> = None;
                    for l in head.lines() {
                        let low = l.to_lowercase();
                        if let Some(v) = low.strip_prefix("range:") {
                            range = Some(v.trim().to_string());
                            break;
                        }
                    }
                    reqs.lock().await.push(range.clone().unwrap_or_else(|| "none".into()));
                    let (status, start) = match range.as_deref() {
                        Some(r) if honors_range => {
                            let s = r.strip_prefix("bytes=").unwrap_or(r).split('-').next().and_then(|x| x.parse::<usize>().ok()).unwrap_or(0);
                            if s >= body.len() {
                                let resp = format!(
                                    "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                                    body.len()
                                );
                                let _ = sock.write_all(resp.as_bytes()).await;
                                return;
                            }
                            ("HTTP/1.1 206 Partial Content\r\n", s)
                        }
                        _ => ("HTTP/1.1 200 OK\r\n", 0),
                    };
                    let slice = &body[start..];
                    let mut resp = String::from(status);
                    if range.is_some() {
                        resp.push_str(&format!("Content-Range: bytes {start}-{}\r\n", body.len() - 1));
                    }
                    resp.push_str(&format!("Content-Length: {}\r\nConnection: close\r\n\r\n", slice.len()));
                    if sock.write_all(resp.as_bytes()).await.is_err() {
                        return;
                    }
                    for piece in slice.chunks(chunk) {
                        if sock.write_all(piece).await.is_err() || sock.flush().await.is_err() {
                            return;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                });
            }
        });
        (port, requests)
    }

    /// v0.25.0 regression: pause used to be reported as `Ok(())` (done) and the
    /// job marked complete with an incomplete file. Now: pause → Paused, the
    /// `.part` keeps the bytes, resume → Range → exact full content.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn download_pause_then_resume_is_complete() {
        let body: Vec<u8> = (0..64 * 1024).map(|i| (i % 251) as u8).collect();
        let (port, requests) = start_test_server(body.clone(), 8 * 1024, true).await;
        let dir = std::env::temp_dir().join(format!("hf-dl-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("sub/model.bin");
        let part = dir.join("sub/model.bin.atria-part");
        let url = format!("http://127.0.0.1:{port}/model.bin");

        // 1) start, pause after the first progress tick
        let ctrl = ctrl_cell();
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = fired.clone();
        let res = download_file(&url, None, &dest, &ctrl, |_d, _t| {
            if !f2.swap(true, Ordering::SeqCst) {
                ctrl_set(&ctrl, Ctrl::Pause);
            }
        })
        .await
        .unwrap();
        assert_eq!(res, DlOutcome::Paused, "pause must be an explicit outcome");
        assert!(!dest.exists(), "paused file must NOT be treated as complete");
        let kept = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        assert!(kept > 0 && kept < body.len() as u64, "part kept with partial bytes");

        // 2) resume → completes with byte-exact content via HTTP Range
        let ctrl2 = ctrl_cell();
        let res = download_file(&url, None, &dest, &ctrl2, |_d, _t| {}).await.unwrap();
        assert_eq!(res, DlOutcome::Done);
        assert_eq!(std::fs::read(&dest).unwrap(), body, "resumed bytes must match");
        assert!(!part.exists(), "part renamed to final file");

        // 3) the second request used a Range header equal to the kept bytes
        let reqs = requests.lock().await;
        assert_eq!(reqs[0], "none");
        assert_eq!(reqs[1], format!("bytes={kept}-"), "resume must request from the kept offset");
        drop(reqs);

        // 4) re-running on the finished file is an instant Done (no re-download)
        let ctrl3 = ctrl_cell();
        assert_eq!(
            download_file(&url, None, &dest, &ctrl3, |_d, _t| {}).await.unwrap(),
            DlOutcome::Done
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn download_stop_removes_part_and_dest_stays_absent() {
        let body: Vec<u8> = vec![7u8; 32 * 1024];
        let (port, _requests) = start_test_server(body, 4 * 1024, true).await;
        let dir = std::env::temp_dir().join(format!("hf-dl-stop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("m.bin");
        let part = dir.join("m.bin.atria-part");
        let url = format!("http://127.0.0.1:{port}/m.bin");
        let ctrl = ctrl_cell();
        let fired = Arc::new(AtomicBool::new(false));
        let f2 = fired.clone();
        let res = download_file(&url, None, &dest, &ctrl, |_d, _t| {
            if !f2.swap(true, Ordering::SeqCst) {
                ctrl_set(&ctrl, Ctrl::Stop);
            }
        })
        .await;
        assert_eq!(res.unwrap_err(), "cancelled");
        assert!(!part.exists(), "stop must remove the .part");
        assert!(!dest.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A server that ignores Range (always 200) must make the client restart
    /// from zero instead of corrupting the file.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn download_server_ignoring_range_restarts_clean() {
        let body: Vec<u8> = (0..16 * 1024).map(|i| (i % 199) as u8).collect();
        let (port, _requests) = start_test_server(body.clone(), 4 * 1024, false).await;
        let dir = std::env::temp_dir().join(format!("hf-dl-norange-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("m.bin");
        let part = dir.join("m.bin.atria-part");
        let url = format!("http://127.0.0.1:{port}/m.bin");
        // seed a partial part (pretend a previous run was interrupted)
        std::fs::write(&part, &body[..4096]).unwrap();
        let ctrl = ctrl_cell();
        let res = download_file(&url, None, &dest, &ctrl, |_d, _t| {}).await.unwrap();
        assert_eq!(res, DlOutcome::Done);
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live check: download a small real file from the Hub through the full
    /// production path (tree listing → resolve URL → streaming + verify size).
    /// Ignored by default; run with `cargo test -- --ignored --include-ignored`.
    #[tokio::test]
    #[ignore = "hits the live Hugging Face API and downloads a real file"]
    async fn live_small_file_download_end_to_end() {
        // karpathy/tinyllamas has small plain files; pick the smallest file
        // of a known public repo instead of a weight (fast, stable).
        let files = repo_files("karpathy/tinyllamas").await.unwrap();
        let f = files.iter().min_by_key(|x| x.size).unwrap();
        assert!(f.size > 0 && f.size < 5_000_000, "expected a small file, got {} bytes", f.size);
        let dir = std::env::temp_dir().join(format!("hf-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dest = dir.join(&f.path);
        let ctrl = ctrl_cell();
        let mut max_prog = 0u64;
        let res = download_file(&file_url("karpathy/tinyllamas", &f.path), None, &dest, &ctrl, |d, _t| {
            max_prog = max_prog.max(d);
        })
        .await
        .unwrap();
        assert_eq!(res, DlOutcome::Done);
        let on_disk = std::fs::metadata(&dest).unwrap().len();
        assert_eq!(on_disk, f.size, "downloaded bytes must equal the hub-reported size");
        assert_eq!(max_prog, f.size);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live check against the real Hub (network). Ignored by default.
    #[tokio::test]
    #[ignore = "hits the live Hugging Face API"]
    async fn live_search_and_tree() {
        let models = search_models("stable diffusion", Some("image-generation"), 5).await.unwrap();
        assert!(!models.is_empty());
        let files = repo_files(&models[0].id).await.unwrap();
        assert!(!files.is_empty());
        assert!(files.iter().all(|f| !f.path.contains("..")));
    }
}
