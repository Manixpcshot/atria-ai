//! Local LLM runtime (v0.25.0).
//!
//! Downloads the prebuilt **llama.cpp** `llama-server` binary for the current
//! OS from the official GitHub releases, then runs a downloaded GGUF model as
//! an OpenAI-compatible local API (`http://127.0.0.1:<port>/v1`) that Atria's
//! existing "Chat Completions" connections can talk to — so a model you
//! downloaded in the model library is actually *usable* for chat.
//!
//! Safety surface:
//! * Only `https://api.github.com/repos/ggml-org/llama.cpp` and the
//!   `browser_download_url` from its latest-release payload are contacted.
//! * The binary lands in `<root>/runtime/llama/llama-server` and is spawned
//!   bound to `127.0.0.1` only (no LAN exposure), with a fixed small argument
//!   set and its output logged to `llama.log`.
//! * The server is a child process; `stop()` kills it.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use crate::hf;

pub const RUNTIME_SUBDIR: &str = "runtime";
pub const LLAMA_DIR: &str = "llama";
/// llama.cpp publishes CPU prebuilts per build (tags `bXXXX`). The "latest
/// release" pointer can be an empty meta-release, so we scan recent releases.
pub const LLAMA_RELEASES_API: &str =
    "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=15";

// ---------------------------------------------------------------------------
// Release resolution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseInfo {
    #[serde(default)]
    pub tag_name: String,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    #[serde(default)]
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

/// Pick the CPU prebuilt for the current OS/arch from a release's assets.
///
/// llama.cpp renamed assets over time — both schemes are understood:
/// * old:  `llama-bXXXX-bin-ubuntu-x64.zip`, `...-macos-arm64.zip`, `...-win-x64.zip`
/// * new:  `llama-bXXXX-bin-ubuntu-x64.tar.gz`, `...-win-cpu-x64.zip`
/// The trailing dot keeps CUDA/OpenVINO/SYCL builds out of the match.
pub fn pick_asset(assets: &[ReleaseAsset]) -> Option<&ReleaseAsset> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let pats: &[&str] = match (os, arch) {
        ("linux", "x86_64") => &["bin-ubuntu-x64.", "bin-linux-x64."],
        ("linux", "aarch64") => &["bin-ubuntu-arm64.", "bin-linux-arm64."],
        ("macos", "aarch64") => &["bin-macos-arm64.", "bin-macos-universal."],
        ("macos", "x86_64") => &["bin-macos-x64.", "bin-macos-universal."],
        ("windows", "x86_64") => &["bin-win-cpu-x64.", "bin-win-x64."],
        ("windows", "aarch64") => &["bin-win-cpu-arm64."],
        _ => &[],
    };
    for p in pats {
        if let Some(a) = assets.iter().find(|a| a.name.contains(p)) {
            return Some(a);
        }
    }
    None
}

/// Scan recent releases (newest first) for the newest one shipping a CPU
/// prebuilt for this platform. Returns (release tag, asset).
pub fn resolve_runtime(releases: &[ReleaseInfo]) -> Option<(String, &ReleaseAsset)> {
    for r in releases {
        if let Some(a) = pick_asset(&r.assets) {
            return Some((r.tag_name.clone(), a));
        }
    }
    None
}

/// Find the `llama-server` entry inside an archive (the path may be nested).
pub fn find_server_entry(names: &[String]) -> Option<&String> {
    let base = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    names.iter().find(|n| n.rsplit('/').next().unwrap_or(n.as_str()) == base)
}

// ---------------------------------------------------------------------------
// Runtime install
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub tag: String,
    #[serde(default)]
    pub asset: String,
    #[serde(default)]
    pub binary_size: u64,
    #[serde(default)]
    pub installed_at_ms: u64,
}

pub fn runtime_dir(root: &Path) -> PathBuf {
    root.join(RUNTIME_SUBDIR).join(LLAMA_DIR)
}

pub fn binary_path(root: &Path) -> PathBuf {
    let name = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    runtime_dir(root).join(name)
}

fn version_path(root: &Path) -> PathBuf {
    runtime_dir(root).join("version.json")
}

pub fn installed_info(root: &Path) -> Option<RuntimeInfo> {
    let raw = std::fs::read_to_string(version_path(root)).ok()?;
    let info: RuntimeInfo = serde_json::from_str(&raw).ok()?;
    if binary_path(root).exists() {
        Some(info)
    } else {
        None
    }
}

/// Download + extract the llama.cpp runtime. Idempotent: a previously
/// installed runtime (binary + version.json) is kept unless `force`.
///
/// `progress` receives `(downloaded_bytes, total_bytes)`.
pub async fn install_runtime(
    root: &Path,
    force: bool,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let bin = binary_path(root);
    if !force && installed_info(root).is_some() && bin.exists() {
        return Ok(bin);
    }

    // 1) resolve the newest release that ships a CPU prebuilt for this platform
    let releases: Vec<ReleaseInfo> = hf_client()
        .get(LLAMA_RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("اتصال به GitHub ممکن نشد: {e}"))?
        .error_for_status()
        .map_err(|e| format!("GitHub error: HTTP {:?}", e.status()))?
        .json()
        .await
        .map_err(|e| format!("پاسخ GitHub معتبر نبود: {e}"))?;
    let (tag, asset) = resolve_runtime(&releases)
        .ok_or_else(|| "موتور سازگاری با این سیستم پیدا نشد (فقط x64/ARM64 پشتیبانی می‌شود)".to_string())?;

    // 2) download the archive (reuses the battle-tested hf downloader:
    //    retries, Range resume, .part files)
    std::fs::create_dir_all(runtime_dir(root))
        .map_err(|e| format!("cannot create runtime folder: {e}"))?;
    let is_zip = asset.name.ends_with(".zip");
    let arc_path = runtime_dir(root).join(if is_zip { "llama.zip" } else { "llama.tar.gz" });
    let _ = std::fs::remove_file(&arc_path);
    let ctrl = hf::ctrl_cell();
    // single source (GitHub release) — the multi-source engine runs it as-is
    let srcs = vec![(asset.browser_download_url.clone(), "github.com".to_string())];
    hf::download_file(&srcs, None, &arc_path, &ctrl, |d, t, _s| progress(d, t))
        .await
        .map_err(|e| format!("دانلود موتور محلی ناموفق بود: {e}"))?;

    // 3) extract the full runtime (zip or tar.gz). Newer builds ship
    //    `llama-server` as a launcher that dlopen's sibling .so files, so the
    //    WHOLE distribution must be unpacked, not just the server binary.
    extract_runtime(&arc_path, &runtime_dir(root))?;
    let _ = std::fs::remove_file(&arc_path);

    // 4) record what we installed
    let size = std::fs::metadata(&bin).map(|m| m.len()).unwrap_or(0);
    let info = RuntimeInfo {
        tag: tag.clone(),
        asset: asset.name.clone(),
        binary_size: size,
        installed_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    };
    std::fs::write(
        version_path(root),
        serde_json::to_string_pretty(&info).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(bin)
}

/// Unpack the whole runtime archive (zip or tar.gz) into `dest_dir`,
/// stripping the single top-level `llama-bXXXX/` folder. The newer builds
/// need every file (launcher + shared libraries) sitting side by side, and
/// the zip variant must get executable bits back (zip may drop them on
/// Windows-built archives).
fn extract_runtime(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let raw = std::fs::read(archive_path).map_err(|e| format!("cannot read archive: {e}"))?;
    let want = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    let is_zip = archive_path.extension().and_then(|e| e.to_str()) == Some("zip");
    if is_zip {
        let mut zip =
            zip::ZipArchive::new(std::io::Cursor::new(raw)).map_err(|e| format!("zip نامعتبر: {e}"))?;
        let mut names: Vec<String> = Vec::new();
        for i in 0..zip.len() {
            if let Ok(e) = zip.by_index(i) {
                names.push(
                    e.enclosed_name()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
            }
        }
        let top = top_prefix(&names);
        for name in &names {
            if name.ends_with('/') {
                continue;
            }
            let rel = strip_prefix(name, &top);
            let out = dest_dir.join(&rel);
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut entry = zip.by_name(name).map_err(|e| format!("خواندن آرشیو: {e}"))?;
            if entry.is_symlink() {
                // zip symlinks carry the target path as their "content"
                let mut buf = Vec::new();
                std::io::copy(&mut entry, &mut buf).map_err(|e| e.to_string())?;
                let target = String::from_utf8_lossy(&buf).into_owned();
                #[cfg(unix)]
                {
                    let _ = std::fs::remove_file(&out);
                    std::os::unix::fs::symlink(&target, &out).map_err(|e| e.to_string())?;
                }
                continue;
            }
            let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut f).map_err(|e| format!("نوشتن فایل: {e}"))?;
        }
    } else {
        // pass 1: safe file names → common top folder
        let mut gz = flate2::read::GzDecoder::new(std::io::Cursor::new(raw.as_slice()));
        let mut tar = tar::Archive::new(&mut gz);
        let mut names: Vec<String> = Vec::new();
        for e in tar.entries().map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            if e.header().entry_type().is_dir() {
                continue;
            }
            let name = e
                .path()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            // path safety: only relative names, never `..`
            if name.starts_with('/') || name.split('/').any(|c| c == "..") {
                continue;
            }
            names.push(name);
        }
        let top = top_prefix(&names);
        // pass 2: extract
        let mut gz = flate2::read::GzDecoder::new(std::io::Cursor::new(raw.as_slice()));
        let mut tar = tar::Archive::new(&mut gz);
        for e in tar.entries().map_err(|e| e.to_string())? {
            let mut e = e.map_err(|e| e.to_string())?;
            if e.header().entry_type().is_dir() {
                continue;
            }
            let name = e
                .path()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name.starts_with('/') || name.split('/').any(|c| c == "..") {
                continue;
            }
            let out = dest_dir.join(strip_prefix(&name, &top));
            if let Some(target) = e.link_name().map_err(|er| er.to_string())?.and_then(|t| Some(t.to_string_lossy().into_owned())) {
                // symlink entry (e.g. libllama-common.so.0 → libllama-common.so.0.9.0);
                // the target is a plain sibling name inside the archive
                if let Some(parent) = out.parent() {
                    std::fs::create_dir_all(parent).map_err(|er| er.to_string())?;
                }
                #[cfg(unix)]
                {
                    let _ = std::fs::remove_file(&out);
                    std::os::unix::fs::symlink(&target, &out).map_err(|er| er.to_string())?;
                }
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|er| er.to_string())?;
            }
            let mut f = std::fs::File::create(&out).map_err(|er| er.to_string())?;
            std::io::copy(&mut e, &mut f).map_err(|er| format!("نوشتن فایل: {er}"))?;
        }
    }
    let server = dest_dir.join(want);
    if !server.exists() || server.metadata().map(|m| m.len() == 0).unwrap_or(true) {
        return Err("llama-server داخل آرشیو پیدا نشد".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&server, std::fs::Permissions::from_mode(0o755));
        // zip archives built on Windows carry no exec bits; tar preserves
        // them, so only re-mark everything when the source was a zip.
        if is_zip {
            let mut stack = vec![dest_dir.to_path_buf()];
            while let Some(d) = stack.pop() {
                if let Ok(rd) = std::fs::read_dir(&d) {
                    for de in rd.flatten() {
                        let p = de.path();
                        if p.is_dir() {
                            stack.push(p);
                        } else {
                            let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// The common single top-level folder of an archive listing (e.g.
/// `llama-b12144/`). Empty string when entries are already flat.
fn top_prefix(names: &[String]) -> String {
    let first = names.iter().find(|n| !n.is_empty() && !n.ends_with('/'));
    match first {
        Some(n) => n
            .split_once('/')
            .map(|(h, _)| {
                // shared only when EVERY non-empty entry starts with it
                let h = format!("{h}/");
                if names.iter().all(|m| m.is_empty() || m.ends_with('/') || m.starts_with(&h)) {
                    h
                } else {
                    String::new()
                }
            })
            .unwrap_or_default(),
        None => String::new(),
    }
}

fn strip_prefix(name: &str, prefix: &str) -> String {
    if !prefix.is_empty() && name.starts_with(prefix) {
        name[prefix.len()..].to_string()
    } else {
        name.to_string()
    }
}

fn hf_client() -> reqwest::Client {
    let mut b = reqwest::Client::builder()
        .user_agent("atria-desktop (local-llm runtime)")
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(15));
    if let Some(p) = crate::proxy::current() {
        if let Ok(pr) = reqwest::Proxy::all(&p) {
            b = b.proxy(pr);
        }
    }
    b.build()
        .expect("reqwest client")
}

// ---------------------------------------------------------------------------
// Server process management
// ---------------------------------------------------------------------------

/// Local server settings, persisted at `<root>/local-llm.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_ctx")]
    pub ctx_size: u32,
}
fn default_port() -> u16 { 8790 }
fn default_ctx() -> u32 { 4096 }
impl Default for LlmConfig {
    fn default() -> Self {
        Self { port: default_port(), ctx_size: default_ctx() }
    }
}
impl LlmConfig {
    pub fn path(root: &Path) -> PathBuf {
        root.join("local-llm.json")
    }
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(Self::path(root))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, root: &Path) -> Result<(), String> {
        std::fs::write(Self::path(root), serde_json::to_string_pretty(self).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmStatus {
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    pub running: bool,
    pub ready: bool,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctx_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// model id as reported by /v1/models (what to put in a connection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

struct LlmState {
    child: Option<Child>,
    model: Option<PathBuf>,
    port: Option<u16>,
}

static LLM: LazyLock<Mutex<LlmState>> =
    LazyLock::new(|| Mutex::new(LlmState { child: None, model: None, port: None }));

/// Start (or restart) the local llama-server for `model`. Returns the port.
pub fn spawn(root: &Path, model: &Path, cfg: &LlmConfig) -> Result<u16, String> {
    let bin = binary_path(root);
    if !bin.exists() {
        return Err("موتور محلی نصب نیست — اول «نصب موتور» را بزنید".to_string());
    }
    if !model.exists() {
        return Err(format!("فایل مدل پیدا نشد: {}", model.display()));
    }
    let mut st = LLM.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(mut c) = st.child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    let log_path = runtime_dir(root).join("llama.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("cannot open llama.log: {e}"))?;
    let mut cmd = Command::new(&bin);
    cmd.arg("--model")
        .arg(model)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(cfg.port.to_string())
        .arg("--ctx-size")
        .arg(cfg.ctx_size.to_string())
        .arg("--parallel")
        .arg("1")
        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log.try_clone().map_err(|e| e.to_string())?));
    let child = cmd
        .spawn()
        .map_err(|e| format!("راه‌اندازی llama-server ناموفق بود: {e} (آخرین خطاها: llama.log)"))?;
    st.child = Some(child);
    st.model = Some(model.to_path_buf());
    st.port = Some(cfg.port);
    Ok(cfg.port)
}

/// Stop the local server (no-op if not running).
pub fn stop() {
    let mut st = LLM.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(mut c) = st.child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    st.model = None;
    st.port = None;
}

/// Kill any server we started (called on app exit).
pub fn shutdown() {
    stop();
}

/// One quick probe of `/v1/models`. `Some(model_id)` when the server is up
/// **and** the model has finished loading (llama.cpp keeps answering 503
/// while loading, then lists the model afterwards).
///
/// Uses reqwest's **blocking** client on purpose: this runs from sync AND
/// async Tauri commands (and the CLI) — a tokio `block_on` here would panic
/// when called from inside an async runtime.
/// One quick probe, done on a dedicated std thread: reqwest's blocking
/// client owns an internal runtime, and creating/dropping it on a tokio
/// worker thread (sync Tauri command, CLI, tests) can panic. A plain thread
/// has no runtime, so the blocking client is always safe there.
pub fn probe_ready(port: u16) -> Option<String> {
    std::thread::scope(|s| {
        let h = s.spawn(|| {
            let client = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(2))
                .build()
                .ok()?;
            let resp = client
                .get(format!("http://127.0.0.1:{port}/v1/models"))
                .send()
                .ok()?;
            if !resp.status().is_success() {
                return None;
            }
            let v: serde_json::Value = resp.json().ok()?;
            v.get("data")
                .and_then(|d| d.get(0))
                .and_then(|m| m.get("id"))
                .and_then(|i| i.as_str())
                .map(String::from)
        });
        h.join().ok().flatten()
    })
}

/// Poll `probe_ready` until ready or `timeout` elapses. Used by callers that
/// want to wait (UI shows progress while this runs in a spawned task).
pub async fn wait_ready(port: u16, timeout: Duration) -> Option<String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Some(id) = probe_ready(port) {
            return Some(id);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}

/// Full status snapshot. `ready` is a single quick probe (never hangs).
pub fn status(root: &Path) -> LlmStatus {
    let cfg = LlmConfig::load(root);
    let mut st = LLM.lock().unwrap_or_else(|p| p.into_inner());
    let running = st
        .child
        .as_mut()
        .map(|c| c.try_wait().ok().flatten().is_none())
        .unwrap_or(false);
    let model = st.model.clone();
    let port = st.port.unwrap_or(cfg.port);
    drop(st);
    let model_id = if running { probe_ready(port) } else { None };
    let info = installed_info(root);
    LlmStatus {
        installed: info.is_some(),
        tag: info.map(|i| i.tag),
        running,
        ready: model_id.is_some(),
        port,
        ctx_size: Some(cfg.ctx_size),
        model: model.map(|m| m.to_string_lossy().into_owned()),
        model_id,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_asset_matches_platform_conventions() {
        // New (b12144+) naming: tar.gz for linux/mac, bin-win-cpu-x64.zip.
        let new = vec![
            ReleaseAsset { name: "llama-b12144-bin-ubuntu-x64.tar.gz".into(), browser_download_url: "u1".into(), size: 1 },
            ReleaseAsset { name: "llama-b12144-bin-ubuntu-cuda-13.4-x64.tar.gz".into(), browser_download_url: "u2".into(), size: 2 },
            ReleaseAsset { name: "llama-b12144-bin-ubuntu-vulkan-x64.tar.gz".into(), browser_download_url: "u3".into(), size: 3 },
            ReleaseAsset { name: "llama-b12144-bin-macos-arm64.tar.gz".into(), browser_download_url: "u4".into(), size: 4 },
            ReleaseAsset { name: "llama-b12144-bin-win-cpu-x64.zip".into(), browser_download_url: "u5".into(), size: 5 },
            ReleaseAsset { name: "llama-b12144-xcframework.zip".into(), browser_download_url: "u6".into(), size: 6 },
        ];
        let a = pick_asset(&new).expect("current platform has an asset in this set");
        // Must pick the CPU build — never CUDA/Vulkan/OpenVINO variants.
        assert!(
            !a.name.contains("cuda") && !a.name.contains("vulkan") && !a.name.contains("rocm")
                && !a.name.contains("sycl") && !a.name.contains("openvino"),
            "must pick the CPU build, got {}",
            a.name
        );
        // Old naming scheme must keep working.
        let old = vec![
            ReleaseAsset { name: "llama-b6122-bin-ubuntu-x64.zip".into(), browser_download_url: "o1".into(), size: 1 },
            ReleaseAsset { name: "llama-b6122-bin-macos-arm64.zip".into(), browser_download_url: "o2".into(), size: 2 },
            ReleaseAsset { name: "llama-b6122-bin-win-x64.zip".into(), browser_download_url: "o3".into(), size: 3 },
        ];
        assert!(pick_asset(&old).is_some(), "old naming must still resolve");
        assert!(pick_asset(&[ReleaseAsset { name: "nightly-tag.txt".into(), browser_download_url: "n".into(), size: 0 }]).is_none());
    }

    #[test]
    fn resolve_runtime_skips_meta_releases_without_binaries() {
        // "latest" can be an empty meta-release (v0.9.0 ships no binaries);
        // resolution must fall through to the newest bXXXX build.
        let releases = vec![
            ReleaseInfo { tag_name: "v0.9.0".into(), assets: vec![ReleaseAsset { name: "nightly-tag.txt".into(), browser_download_url: "a".into(), size: 1 }] },
            ReleaseInfo { tag_name: "b12144".into(), assets: vec![
                ReleaseAsset { name: "llama-b12144-bin-ubuntu-x64.tar.gz".into(), browser_download_url: "b".into(), size: 2 },
                ReleaseAsset { name: "llama-b12144-bin-macos-arm64.tar.gz".into(), browser_download_url: "c".into(), size: 3 },
                ReleaseAsset { name: "llama-b12144-bin-win-cpu-x64.zip".into(), browser_download_url: "d".into(), size: 4 },
            ] },
        ];
        let (tag, asset) = resolve_runtime(&releases).expect("b12144 must resolve");
        assert_eq!(tag, "b12144");
        assert!(asset.name.contains("llama-b12144-bin-"));
        assert!(resolve_runtime(&[releases[0].clone()]).is_none());
    }

    #[test]
    fn find_server_entry_handles_nested_paths() {
        let base = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
        let nested = format!("build/bin/{base}");
        let names = vec![
            nested.clone(),
            "build/bin/llama-cli".to_string(),
            "README.md".to_string(),
        ];
        assert_eq!(find_server_entry(&names).map(|s| s.as_str()), Some(nested.as_str()));
        let flat = vec![base.to_string()];
        assert_eq!(find_server_entry(&flat).map(|s| s.as_str()), Some(base));
        assert!(find_server_entry(&["a/llama-cli".into()]).is_none());
    }

    #[test]
    fn llm_config_roundtrip_and_defaults() {
        let dir = std::env::temp_dir().join(format!("llm-cfg-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let c = LlmConfig::load(&dir);
        assert_eq!(c.port, 8790);
        assert_eq!(c.ctx_size, 4096);
        let c2 = LlmConfig { port: 9999, ctx_size: 8192 };
        c2.save(&dir).unwrap();
        let c3 = LlmConfig::load(&dir);
        assert_eq!(c3.port, 9999);
        assert_eq!(c3.ctx_size, 8192);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spawn_without_runtime_fails_cleanly() {
        let dir = std::env::temp_dir().join(format!("llm-spawn-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let cfg = LlmConfig::default();
        let err = spawn(&dir, &dir.join("model.gguf"), &cfg).unwrap_err();
        assert!(err.contains("نصب"), "error should say the runtime is missing: {err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Deletes its dir on drop so a mid-test panic never fills the disk.
    struct DirGuard(PathBuf);
    impl DirGuard {
        fn new(p: PathBuf) -> Self {
            let _ = std::fs::create_dir_all(&p);
            DirGuard(p)
        }
    }
    impl Drop for DirGuard {
        fn drop(&mut self) {
            stop(); // never leak the server child across panics
            if std::env::var("ATRIA_KEEP_E2E").is_ok() {
                eprintln!("ATRIA_KEEP_E2E set — dir kept at {}", self.0.display());
                return;
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Live: actually download the runtime + a small GGUF and run a chat
    /// completion end to end. Heavy — ignored by default.
    #[tokio::test]
    #[ignore = "downloads the runtime (~30MB) + a ~415MB GGUF and runs local inference"]
    async fn live_local_llm_end_to_end() {
        // /tmp is a ~1GB tmpfs in this sandbox — the ~415MB model would not
        // fit. Work on the real home filesystem instead.
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
        let root = std::path::Path::new(&home)
            .join(".atria-e2e")
            .join(format!("{}", std::process::id()));
        let _guard = DirGuard::new(root.clone());

        // 1) runtime
        let bin = install_runtime(&root, false, |d, t| eprintln!("runtime {d}/{t}"))
            .await
            .expect("runtime install");
        assert!(bin.exists());

        // 2) a small public GGUF through the real hf::download_file path
        //    (Qwen 0.5B q2_k ≈ 415 MB — small enough for a sandbox CPU)
        let repo = "Qwen/Qwen2.5-0.5B-Instruct-GGUF";
        let files = hf::repo_files(repo).await.expect("repo files");
        let gguf = files
            .iter()
            .filter(|f| f.path.to_lowercase().ends_with(".gguf"))
            .min_by_key(|f| f.size)
            .expect("a gguf file");
        let model = root.join(gguf.path.replace('/', "-"));
        if !model.exists() {
            let ctrl = hf::ctrl_cell();
            let u = hf::file_url(repo, &gguf.path);
            hf::download_file(&[(u, "huggingface.co".to_string())], None, &model, &ctrl, |d, t, _s| eprintln!("model {d}/{t}"))
                .await
                .expect("model download");
        }

        // 3) run the server
        let cfg = LlmConfig { port: 8899, ctx_size: 512 };
        spawn(&root, &model, &cfg).expect("spawn");
        let mut ready = false;
        for _ in 0..120 {
            let s = status(&root);
            if s.ready { ready = true; break; }
            if !s.running { break; }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
        assert!(ready, "llama-server should become ready (check llama.log)");

        // 4) chat through the OpenAI-compatible API
        let client = reqwest::Client::new();
        let resp = client
            .post("http://127.0.0.1:8899/v1/chat/completions")
            .json(&serde_json::json!({
                "model": "tiny",
                "messages": [{ "role": "user", "content": "Say hello in one word." }],
                "max_tokens": 12
            }))
            .send()
            .await
            .expect("chat request");
        let v: serde_json::Value = resp.json().await.expect("chat json");
        let text = v["choices"][0]["message"]["content"].as_str().unwrap_or("");
        assert!(!text.trim().is_empty(), "expected a non-empty reply, got: {text}");

        stop();
        let _ = std::fs::remove_dir_all(&root);
    }
}
