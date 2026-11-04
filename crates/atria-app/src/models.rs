//! Model library IPC: Hugging Face search, job queue, downloads with
//! live progress events, folder/token settings.
//!
//! Engine model: a global `Mutex<ModelsState>` holds the persistent queue;
//! at most one job downloads at a time (a spawned tokio task walks its
//! files sequentially). Pause/stop is cooperative via `hf::Ctrl`.

use atria_core::hf::{self, Ctrl, FileState, HfFileJob, HfJob, HfQueue};
use crate::commands::AppState;
use crate::secrets;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU8;
use std::sync::{Arc, LazyLock, Mutex};
use tauri::{AppHandle, Emitter, State};

static ENGINE: LazyLock<Mutex<ModelsState>> = LazyLock::new(|| Mutex::new(ModelsState {
    queue: None,
    queue_path: None,
    active: None,
    cancel: None,
}));

struct ModelsState {
    queue: Option<HfQueue>,
    queue_path: Option<PathBuf>,
    /// job id currently downloading
    active: Option<String>,
    /// cooperative control for the active job
    cancel: Option<Arc<AtomicU8>>,
}

fn root() -> PathBuf {
    crate::commands::atria_root()
}

fn queue_path() -> PathBuf {
    root().join("models.json")
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Load (or create) the queue, run `f`, then persist.
fn with_queue<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(&mut HfQueue, &PathBuf) -> Result<T, String>,
{
    let mut guard = ENGINE.lock().unwrap_or_else(|p| p.into_inner());
    let path = queue_path();
    if guard.queue.is_none() {
        let default_folder = root().join("models");
        let q = HfQueue::load(&path).unwrap_or_else(|| HfQueue::new(default_folder));
        guard.queue_path = Some(path.clone());
        guard.queue = Some(q);
    }
    let p = guard.queue_path.clone().unwrap_or_else(queue_path);
    let q = guard.queue.as_mut().expect("queue init above");
    let out = f(q, &p)?;
    persist(&mut guard)?;
    Ok(out)
}

fn persist(guard: &mut ModelsState) -> Result<(), String> {
    if guard.queue.is_none() {
        return Ok(());
    }
    let p = guard.queue_path.clone().unwrap_or_else(queue_path);
    guard.queue.as_mut().unwrap().save(&p)
}

/// Free space (bytes) of the drive holding `path`.
fn free_space(path: &Path) -> u64 {
    disk_space::DiskSpace::new(path).map(|d| d.available).unwrap_or(0)
}

#[tauri::command]
pub async fn models_state(st: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let _ = st;
    with_queue(|q, _p| {
        let folder = q.folder.clone();
        let active = ENGINE
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .active
            .clone();
        let token = secrets::get(&root(), "hf-token").ok().flatten().is_some();
        Ok(serde_json::json!({
            "folder": folder.to_string_lossy(),
            "free_space": free_space(&folder),
            "token_configured": token,
            "active": active,
            "jobs": q.jobs,
        }))
    })
}

#[tauri::command]
pub async fn models_set_folder(folder: PathBuf) -> Result<serde_json::Value, String> {
    let folder = folder
        .canonicalize()
        .map_err(|_| "پوشه پیدا نشد — دوباره انتخاب کنید".to_string())?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("cannot create folder: {e}"))?;
    let out = with_queue(|q, _p| {
        q.folder = folder.clone();
        Ok(folder)
    })?;
    Ok(serde_json::json!({ "folder": out.to_string_lossy(), "free_space": free_space(&out) }))
}

#[tauri::command]
pub async fn models_hf_token(value: String) -> Result<(), String> {
    let v = value.trim().to_string();
    if v.is_empty() {
        secrets::delete(&root(), "hf-token").map_err(|e| e.to_string())
    } else {
        secrets::set(&root(), "hf-token", &v).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn models_hf_token_get() -> Result<Option<String>, String> {
    Ok(secrets::get(&root(), "hf-token").map_err(|e| e.to_string())?)
}

#[tauri::command]
pub async fn models_search(
    query: String,
    pipeline: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<hf::HfModel>, String> {
    let limit = limit.unwrap_or(20) as usize;
    let q = query.trim().to_string();
    let pipe = pipeline.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
    hf::search_models(&q, pipe.as_deref(), limit).await
}

#[tauri::command]
pub async fn models_add(repo: String) -> Result<serde_json::Value, String> {
    let repo = repo.trim().to_string();
    if !hf::is_repo_id(&repo) {
        return Err("repo id نامعتبر است — شکل: `org/model-name`".into());
    }
    let files = hf::repo_files(&repo).await?;

    let job = with_queue(|q, _p| {
        if q.jobs.iter().any(|j| j.repo == repo) {
            return Err("این مدل قبلاً به صف اضافه شده است".into());
        }
        let folder = q.folder.join(&repo.replace('/', "-"));
        std::fs::create_dir_all(&folder).map_err(|e| format!("cannot create folder: {e}"))?;
        let ts = now_ms();
        let job = HfJob {
            id: format!("{ts}"),
            repo: repo.clone(),
            added_at: ts,
            folder,
            files: files
                .into_iter()
                .map(|f| HfFileJob {
                    path: f.path,
                    size: f.size,
                    downloaded: 0,
                    state: FileState::Pending,
                    error: None,
                })
                .collect(),
            error: None,
        };
        q.jobs.push(job.clone());
        Ok(job)
    })?;
    Ok(serde_json::json!({ "job": job }))
}

/// Start (or resume) downloading a job. One job at a time.
#[tauri::command]
pub async fn models_start(app: AppHandle, job_id: String) -> Result<serde_json::Value, String> {
    let snapshot = {
        let mut guard = ENGINE.lock().unwrap_or_else(|p| p.into_inner());
        if guard.queue.is_none() {
            // force load
            let path = queue_path();
            let q = HfQueue::load(&path)
                .unwrap_or_else(|| HfQueue::new(root().join("models")));
            guard.queue_path = Some(path);
            guard.queue = Some(q);
        }
        if let Some(active) = &guard.active {
            return Err(format!("اول دانلود `{active}` را تمام یا معلق کنید"));
        }
        let snapshot = guard
            .queue
            .as_ref()
            .and_then(|q| q.job(&job_id))
            .cloned()
            .ok_or_else(|| "job پیدا نشد".to_string())?;
        let ctrl = Arc::new(AtomicU8::new(0));
        guard.active = Some(job_id.clone());
        guard.cancel = Some(ctrl.clone());
        (snapshot, ctrl)
    };
    let token = secrets::get(&root(), "hf-token").ok().flatten();
    let (job, ctrl) = snapshot;
    let job_id2 = job_id.clone();

    tokio::spawn(async move {
        download_job(&app, &job_id2, &job, &ctrl, token.as_deref()).await;
    });
    Ok(serde_json::json!({ "started": job_id }))
}

async fn download_job(
    app: &AppHandle,
    job_id: &str,
    job: &HfJob,
    ctrl: &Arc<AtomicU8>,
    token: Option<&str>,
) {
    // throttle progress events to ~4/sec
    let last_emit = Mutex::new(std::time::Instant::now() - std::time::Duration::from_millis(1000));

    'files: for (idx, f) in job.files.iter().enumerate() {
        if f.state == FileState::Done {
            continue;
        }
        set_file_state(job_id, idx, FileState::Downloading, f.downloaded, None);
        let url = hf::file_url(&job.repo, &f.path);
        let dest = match hf::safe_dest(&job.folder, &f.path) {
            Ok(d) => d,
            Err(e) => {
                set_file_state(job_id, idx, FileState::Error, 0, Some(e));
                emit_now(app, job_id, &f.path, 0, f.size, "error");
                break 'files;
            }
        };
        let part = {
            let name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            dest.parent().unwrap_or(std::path::Path::new(".")).join(format!("{name}.atria-part"))
        };
        let total = f.size;
        let li = idx;

        // I/O-bound: await directly on the app runtime (no 'static needed).
        let result: Result<(), String> = hf::download_file(
            &url,
            token,
            &dest,
            &**ctrl,
            |done, tot| {
                let mut last = last_emit.lock().unwrap_or_else(|p| p.into_inner());
                if last.elapsed() >= std::time::Duration::from_millis(250) {
                    *last = std::time::Instant::now();
                    app.emit(
                        "atria-models-progress",
                        serde_json::json!({
                            "job_id": job_id,
                            "file": f.path,
                            "downloaded": done,
                            "total": tot,
                            "state": "downloading",
                            "file_count": job.files.len(),
                            "file_index": li + 1,
                        }),
                    )
                    .ok();
                }
            },
        )
        .await;

        match result {
            Ok(()) => {
                let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(total);
                set_file_state(job_id, li, FileState::Done, size, None);
                emit_now(app, job_id, &f.path, size, size, "done");
            }
            Err(e) => {
                let cancelled = e == "cancelled";
                let paused = !cancelled && hf::ctrl_get(&**ctrl) == Ctrl::Pause;
                let kept = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
                if cancelled {
                    // reset to pending; partial was removed by download_file
                    set_file_state(job_id, li, FileState::Pending, 0, None);
                    emit_now(app, job_id, &f.path, 0, total, "stopped");
                } else if paused {
                    set_file_state(job_id, li, FileState::Paused, kept, None);
                    emit_now(app, job_id, &f.path, kept, total, "paused");
                } else {
                    set_file_state(job_id, li, FileState::Error, kept, Some(e));
                    emit_now(app, job_id, &f.path, kept, total, "error");
                }
                break 'files;
            }
        }
    }

    {
        let mut guard = ENGINE.lock().unwrap_or_else(|p| p.into_inner());
        guard.active = None;
        guard.cancel = None;
        // final aggregate state from the queue
        let state = guard
            .queue
            .as_ref()
            .and_then(|q| q.job(job_id))
            .map(|j| match j.state() {
                FileState::Done => "done",
                FileState::Paused => "paused",
                FileState::Error => "error",
                _ => "pending",
            })
            .unwrap_or("done");
        if let Err(e) = persist(&mut guard) {
            eprintln!("models: persist failed: {e}");
        }
        let _ = app.emit(
            "atria-models-status",
            serde_json::json!({ "job_id": job_id, "state": state }),
        );
    }
}

fn set_file_state(job_id: &str, idx: usize, state: FileState, downloaded: u64, error: Option<String>) {
    let mut guard = ENGINE.lock().unwrap_or_else(|p| p.into_inner());
    if guard.queue.is_none() {
        return;
    }
    let p = guard.queue_path.clone().unwrap_or_else(queue_path);
    if let Some(j) = guard.queue.as_mut().unwrap().job_mut(job_id) {
        if idx < j.files.len() {
            j.files[idx].state = state;
            j.files[idx].downloaded = downloaded;
            j.files[idx].error = error;
        }
    }
    let _ = guard.queue.as_ref().unwrap().save(&p);
}

fn emit_now(app: &AppHandle, job_id: &str, file: &str, downloaded: u64, total: u64, state: &str) {
    app.emit(
        "atria-models-progress",
        serde_json::json!({
            "job_id": job_id,
            "file": file,
            "downloaded": downloaded,
            "total": total,
            "state": state,
        }),
    )
    .ok();
}

#[tauri::command]
pub async fn models_pause(job_id: String) -> Result<serde_json::Value, String> {
    let guard = ENGINE.lock().unwrap_or_else(|p| p.into_inner());
    if guard.active.as_deref() != Some(job_id.as_str()) {
        return Err("این job در حال دانلود نیست".into());
    }
    if let Some(c) = &guard.cancel {
        hf::ctrl_set(c, Ctrl::Pause);
    }
    Ok(serde_json::json!({ "paused": job_id }))
}

#[tauri::command]
pub async fn models_stop(job_id: String) -> Result<serde_json::Value, String> {
    let guard = ENGINE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(c) = &guard.cancel {
        hf::ctrl_set(c, Ctrl::Stop);
    }
    Ok(serde_json::json!({ "stopped": job_id }))
}

/// Remove a job from the queue (downloaded files stay on disk).
#[tauri::command]
pub async fn models_remove(job_id: String) -> Result<serde_json::Value, String> {
    {
        let guard = ENGINE.lock().unwrap_or_else(|p| p.into_inner());
        if guard.active.as_deref() == Some(job_id.as_str()) {
            return Err("اول دانلود را معلق یا متوقف کنید".into());
        }
    }
    with_queue(|q, _p| {
        q.remove(&job_id);
        Ok(())
    })?;
    Ok(serde_json::json!({ "removed": job_id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_space_nonzero_on_existing_dir() {
        let d = std::env::temp_dir();
        assert!(free_space(&d) > 0);
    }

    #[test]
    fn ids_are_timestamp_based() {
        let a = now_ms();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = now_ms();
        assert!(b >= a);
        assert!(a > 1_700_000_000_000);
    }
}
