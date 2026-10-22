//! v0.18.0: local task scheduler.
//!
//! Jobs persist in `~/.atria/scheduled.json`. A 30-second tick on a
//! dedicated thread fires due jobs. Each firing appends a result line to
//! `~/.atria/scheduled-log.jsonl` and emits the Tauri event
//! `atria:schedule_result` (the frontend mirrors it into the
//! «⏰ زمان‌بندی‌ها» chat). Results written while the app was closed are
//! merged into that chat at boot, so nothing is ever silently lost.
//!
//! A job runs only if the user explicitly saved a scheduler connection
//! (`~/.atria/scheduler-conn.json`, written via the UI). Otherwise the
//! firing is logged as a visible "app was closed / no connection" entry.

use serde::{Deserialize, Serialize};
use tauri::Emitter;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub const SCHEDULED_FILE: &str = "scheduled.json";
pub const LOG_FILE: &str = "scheduled-log.jsonl";
pub const CONN_FILE: &str = "scheduler-conn.json";
const TICK_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    /// Human label shown in the UI and in the chat mirror.
    pub label: String,
    /// Next run, ISO-8601 UTC (e.g. "2026-10-22T14:30:00Z").
    pub next_run: String,
    /// Repeat interval in minutes; 0 = one-shot (deleted after firing).
    #[serde(default)]
    pub every_minutes: u64,
    /// The prompt sent to the model when the job fires.
    pub prompt: String,
    /// Frontend connection id (informational; execution uses the saved
    /// scheduler connection).
    #[serde(default)]
    pub conn_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleResult {
    pub job_id: String,
    pub label: String,
    /// RFC3339 UTC timestamp of the firing.
    pub ran_at: String,
    pub prompt: String,
    pub output: String,
    pub is_error: bool,
    /// True when the app was closed at fire time (boot-merge case).
    #[serde(default)]
    pub caught_up: bool,
}

/// The connection profile the scheduler may use to run jobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConn {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    /// "anthropic" | "openai"
    #[serde(default)]
    pub kind: String,
}

fn jobs_path(root: &Path) -> PathBuf {
    root.join(SCHEDULED_FILE)
}

fn log_path(root: &Path) -> PathBuf {
    root.join(LOG_FILE)
}

fn conn_path(root: &Path) -> PathBuf {
    root.join(CONN_FILE)
}

pub fn load_jobs(root: &Path) -> Vec<ScheduledJob> {
    let raw = match std::fs::read_to_string(jobs_path(root)) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

pub fn save_jobs(root: &Path, jobs: &[ScheduledJob]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(jobs)
        .map_err(|e| format!("serialize jobs failed: {e}"))?;
    std::fs::write(jobs_path(root), json).map_err(|e| format!("cannot save jobs: {e}"))
}

pub fn load_conn(root: &Path) -> Option<SchedulerConn> {
    std::fs::read_to_string(conn_path(root))
        .ok()
        .and_then(|r| serde_json::from_str(&r).ok())
}

pub fn save_conn(root: &Path, conn: &SchedulerConn) -> Result<(), String> {
    let json = serde_json::to_string_pretty(conn).map_err(|e| e.to_string())?;
    std::fs::write(conn_path(root), json).map_err(|e| format!("cannot save scheduler connection: {e}"))
}

pub fn clear_conn(root: &Path) {
    let _ = std::fs::remove_file(conn_path(root));
}

pub fn read_log(root: &Path) -> Vec<ScheduleResult> {
    let raw = match std::fs::read_to_string(log_path(root)) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    raw.lines()
        .filter_map(|l| serde_json::from_str::<ScheduleResult>(l).ok())
        .collect()
}

/// Truncate the log up to (and including) the newest processed `ran_at`.
/// Returns how many lines remain unprocessed.
pub fn ack_log(root: &Path, up_to_ran_at: &str) -> usize {
    let entries = read_log(root);
    let idx = entries
        .iter()
        .rposition(|e| e.ran_at.as_str() <= up_to_ran_at)
        .map(|i| i + 1)
        .unwrap_or(0);
    let rest: Vec<String> = entries[idx..]
        .iter()
        .filter_map(|e| serde_json::to_string(e).ok())
        .collect();
    let joined = rest.join("\n");
    let out = if joined.is_empty() { String::new() } else { joined + "\n" };
    if std::fs::write(log_path(root), out).is_ok() {
        rest.len()
    } else {
        entries.len() - idx
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Parse the job's ISO-8601 next_run as a unix timestamp (seconds).
pub fn next_run_epoch(job: &ScheduledJob) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(&job.next_run)
        .ok()
        .map(|d| d.timestamp())
        .or_else(|| {
            // Allow naive local-ish "YYYY-MM-DDTHH:MM:SS" as UTC.
            chrono::NaiveDateTime::parse_from_str(&job.next_run, "%Y-%m-%dT%H:%M:%S")
                .ok()
                .map(|d| d.and_utc().timestamp())
        })
}

/// Advance `every_minutes` minutes from `from_epoch`.
pub fn advance_epoch(from_epoch: i64, every_minutes: u64) -> i64 {
    from_epoch + (every_minutes as i64) * 60
}

fn epoch_to_iso(epoch: i64) -> String {
    chrono::DateTime::from_timestamp(epoch, 0)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| now_rfc3339())
}

/// Run one due job: with a saved connection the prompt goes through the
/// normal (tool-less) agent turn; without one it is logged as a visible
/// miss so the boot-merge shows it.
fn fire_job(root: &Path, job: &ScheduledJob) -> ScheduleResult {
    let ran_at = now_rfc3339();
    let Some(conn) = load_conn(root) else {
        return ScheduleResult {
            job_id: job.id.clone(),
            label: job.label.clone(),
            ran_at,
            prompt: job.prompt.clone(),
            output: "زمان اجرای وظیفه فرا رسید ولی برنامه بسته بود یا ارتباط زمان‌بندی ذخیره نشده است؛ این ثبت در اولین بازگشایی آتریا به چت «⏰ زمان‌بندی‌ها» منتقل می‌شود.".to_string(),
            is_error: false,
            caught_up: true,
        };
    };
    if conn.api_key.trim().is_empty() || conn.base_url.trim().is_empty() {
        return ScheduleResult {
            job_id: job.id.clone(),
            label: job.label.clone(),
            ran_at,
            prompt: job.prompt.clone(),
            output: "ارتباط زمان‌بندی نامعتبر است (پایگاه یا کلید خالی)؛ وظیفه اجرا نشد.".to_string(),
            is_error: true,
            caught_up: false,
        };
    }
    // Block the scheduler thread for the turn — it is a dedicated thread,
    // and the bounded agent turn keeps this predictable.
    let cfg = atria_core::ClientConfig {
        api_key: conn.api_key,
        base_url: conn.base_url,
        model: conn.model,
        max_tokens: 2048,
        temperature: 0.4,
        system: "تو دستیار آتریای داری یک وظیفهٔ زمان‌بندی‌شدهٔ کوتاه را اجرا می‌کنی. پاسخ خلاصه و کاربردی بده.".to_string(),
        stream: false,
        tools: false,
        tool_use_enabled: false,
        kind: atria_core::ApiKind::parse(&conn.kind),
        file_tools: false,
        workspace: root.join("workspace").to_string_lossy().to_string(),
        app_data: root.to_string_lossy().to_string(),
        web_tools: false,
        github_tools: false,
        github_token: String::new(),
        autonomous_mode: false,
        full_access_mode: false,
        full_access_profile: false,
        full_access_system: false,
        computer_tools: false,
        terminal_tools: false,
        project_root: String::new(),
        downloads_root: String::new(),
        web_thinking: false,
        web_search: false,
        web_session: String::new(),
        web_session_out: None,
        mcp_servers: Vec::new(),
    };
    let prompt = format!(
        "[وظیفهٔ زمان‌بندی‌شده: {}] {}\n\nخروجی کوتاه بده.",
        job.label, job.prompt
    );
    let messages = vec![atria_core::Message {
        role: atria_core::Role::User,
        content: vec![atria_core::Block::Text { text: prompt }],
    }];
    let stop = Arc::new(AtomicBool::new(false));
    let rt = tokio::runtime::Runtime::new().unwrap_or_else(|_| {
        // Fallback: a current-thread runtime still satisfies the agent.
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("rt")
    });
    let mem_path = root.join("memory.json");
    let block = || {
        let mut mem = atria_core::MemoryStore::open(&mem_path);
        rt.block_on(atria_core::run_agent(&cfg, messages.clone(), &mut mem, stop.clone(), |ev| {
            let _ = ev; // scheduler runs are silent in the UI until the event fires
        }))
    };
    match block() {
        Ok(out) => ScheduleResult {
            job_id: job.id.clone(),
            label: job.label.clone(),
            ran_at,
            prompt: job.prompt.clone(),
            output: if out.final_text.trim().is_empty() {
                "(بدون خروجی)".to_string()
            } else {
                out.final_text.chars().take(8_000).collect()
            },
            is_error: false,
            caught_up: false,
        },
        Err(e) => ScheduleResult {
            job_id: job.id.clone(),
            label: job.label.clone(),
            ran_at,
            prompt: job.prompt.clone(),
            output: format!("اجرای وظیفه ناکام بود: {e}"),
            is_error: true,
            caught_up: false,
        },
    }
}

/// One scheduler tick: fire due jobs, persist results, emit the Tauri event.
/// `emit` is called for each fired result (the frontend mirrors it to chat).
pub fn tick(root: &Path, emit: &dyn Fn(ScheduleResult)) {
    let now = chrono::Utc::now().timestamp();
    let mut jobs = load_jobs(root);
    let mut fired: Vec<ScheduleResult> = Vec::new();
    let mut remaining: Vec<ScheduledJob> = Vec::new();
    for mut job in jobs.into_iter() {
        let due = matches!(next_run_epoch(&job), Some(t) if t <= now);
        if !due {
            remaining.push(job);
            continue;
        }
        let result = fire_job(root, &job);
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path(root))
            .and_then(|mut f| {
                use std::io::Write;
                f.write_all((serde_json::to_string(&result).unwrap_or_default() + "\n").as_bytes())
            });
        emit(result.clone());
        fired.push(result);
        if job.every_minutes > 0 {
            let from = next_run_epoch(&job).unwrap_or(now);
            job.next_run = epoch_to_iso(advance_epoch(from, job.every_minutes));
            remaining.push(job);
        }
        // one-shot jobs are dropped after firing
    }
    let _ = save_jobs(root, &remaining);
    let _ = fired;
}

/// Spawn the scheduler thread. Returns nothing; the loop is daemon-like.
pub fn spawn(app: tauri::AppHandle) {
    let root = crate::commands::atria_root();
    let _ = std::fs::create_dir_all(&root);
    std::thread::Builder::new()
        .name("atria-scheduler".into())
        .spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(TICK_SECS));
            tick(
                &root,
                &|r| {
                    let _ = app.emit("atria:schedule_result", &r);
                },
            );
        })
        .expect("scheduler thread");
}
