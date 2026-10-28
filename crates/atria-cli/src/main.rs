//! Atria CLI — a Claude-Code-style terminal agent over `atria-core`.
//!
//! English UI. Same engine and same `~/.atria` data as the desktop app
//! (memory included).
//!
//! Usage:
//!   atria-cli                         interactive session (REPL)
//!   atria-cli "one-shot prompt"       single turn, then exit
//!   echo "text" | atria-cli           one-shot prompt from stdin
//!   atria-cli --init                  write a starter ~/.atria/cli.json
//!   atria-cli --config PATH           use an alternate config file
//!   atria-cli --model NAME            override the configured model
//!   atria-cli --provider NAME         openai|anthropic|deepseek|groq|openrouter|gemini|ollama|deepseekweb
//!   atria-cli --full                  Claude-Code style: terminal + computer + files in the current directory
//!   atria-cli --update                self-update from the latest GitHub release (SHA-256 verified)
//!   atria-cli --tools | --no-tools    force the tool loop on/off
//!   atria-cli --version               print version and build time
//!
//! The API key is read from the ATRIA_API_KEY environment variable only —
//! it is never written to disk. Inside the REPL, `/key <value>` can set it
//! for the current session only.

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct CliConfig {
    base_url: String,
    model: String,
    #[serde(default)]
    kind: String,
}

/* ---------------- argument parsing ---------------- */

struct Args {
    prompt: Option<String>,
    tools: Option<bool>,
    init: bool,
    config: PathBuf,
    model: Option<String>,
    provider: Option<String>,
    full: bool,
    update: bool,
    version: bool,
    help: bool,
}

fn parse_args(args: &[String]) -> Args {
    let mut out = Args {
        prompt: None,
        tools: None,
        init: false,
        config: default_cfg_path(),
        model: None,
        provider: None,
        full: false,
        update: false,
        version: false,
        help: false,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tools" => out.tools = Some(true),
            "--no-tools" => out.tools = Some(false),
            "--init" => out.init = true,
            "--help" | "-h" => out.help = true,
            "--version" | "-V" => out.version = true,
            "--update" => out.update = true,
            "--full" | "--dangerous" | "--everything" => out.full = true,
            "--provider" | "--prov" => {
                i += 1;
                if i < args.len() {
                    out.provider = Some(args[i].clone());
                }
            }
            "--config" => {
                i += 1;
                if i < args.len() {
                    out.config = PathBuf::from(args[i].as_str());
                }
            }
            "--model" => {
                i += 1;
                if i < args.len() {
                    out.model = Some(args[i].clone());
                }
            }
            other => match out.prompt.as_mut() {
                None => out.prompt = Some(other.to_string()),
                Some(p) => {
                    p.push(' ');
                    p.push_str(other);
                }
            },
        }
        i += 1;
    }
    out
}

/* ---------------- slash commands ---------------- */

#[derive(Debug, PartialEq, Eq)]
enum Cmd {
    Exit,
    Help,
    Clear,
    Model(Option<String>),
    Provider(Option<String>),
    Full(Option<String>),
    Key(Option<String>),
    Unknown(String),
    /// A plain user message (no leading slash).
    Plain,
}

fn parse_slash(line: &str) -> Cmd {
    let t = line.trim();
    if !t.starts_with('/') {
        return Cmd::Plain;
    }
    let mut parts = t[1..].splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").to_ascii_lowercase();
    let rest = parts.next().unwrap_or("").trim().to_string();
    let arg = if rest.is_empty() { None } else { Some(rest) };
    match name.as_str() {
        "exit" | "quit" | "q" => Cmd::Exit,
        "help" | "?" => Cmd::Help,
        "clear" | "reset" => Cmd::Clear,
        "model" => Cmd::Model(arg),
        "provider" | "prov" | "api" => Cmd::Provider(arg),
        "full" | "dangerous" => Cmd::Full(arg),
        "key" => Cmd::Key(arg),
        other => Cmd::Unknown(other.to_string()),
    }
}

fn home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Windows double-click users need the console to stay open while they read
/// the error — wait for a keypress before the process (and its window) dies.
fn pause_if_tty() {
    if std::io::stderr().is_terminal() || std::io::stdout().is_terminal() {
        eprint!("\nPress Enter to exit...");
        let _ = std::io::stderr().flush();
        let mut b = [0u8; 1];
        let _ = std::io::stdin().read(&mut b);
    }
}

/// Error exit that never flashes the window closed.
fn fail(code: i32) -> ! {
    pause_if_tty();
    std::process::exit(code);
}

/* ---------------- colors ---------------- */

static COLORS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn colors_on() -> bool {
    *COLORS.get().unwrap_or(&false)
}

fn colors_enabled_now() -> bool {
    std::env::var_os("NO_COLOR").is_none()
        && (std::io::stderr().is_terminal() || std::io::stdout().is_terminal())
}

fn paint(s: &str, code: &str, on: bool) -> String {
    if on {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn c(s: &str, code: &str) -> String {
    paint(s, code, colors_on())
}

#[cfg(windows)]
fn enable_windows_vt() {
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
        STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
    };
    unsafe {
        for handle in [GetStdHandle(STD_OUTPUT_HANDLE), GetStdHandle(STD_ERROR_HANDLE)] {
            if handle != 0 && handle != -1 {
                let mut mode = 0u32;
                if GetConsoleMode(handle, &mut mode) != 0 {
                    let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        }
    }
}

#[cfg(not(windows))]
fn enable_windows_vt() {}

/* ---------------- provider catalog ---------------- */

/// (friendly name, ApiKind string, default base_url, human note)
fn provider_catalog() -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        ("openai", "openai", "https://api.openai.com/v1",
         "OpenAI, or any OpenAI-compatible endpoint"),
        ("anthropic", "anthropic", "https://api.anthropic.com",
         "Claude — Anthropic Messages API"),
        ("deepseek", "openai", "https://api.deepseek.com/v1",
         "DeepSeek API (OpenAI-compatible)"),
        ("groq", "openai", "https://api.groq.com/openai/v1", "Groq"),
        ("openrouter", "openai", "https://openrouter.ai/api/v1",
         "OpenRouter — hundreds of models, one key"),
        ("gemini", "openai", "https://generativelanguage.googleapis.com/v1beta/openai",
         "Google Gemini (OpenAI-compat endpoint)"),
        ("ollama", "openai", "http://localhost:11434/v1",
         "Ollama — local models, no API key needed"),
        ("deepseekweb", "deepseekweb", "https://chat.deepseek.com",
         "DeepSeek web — key = signed-in userToken from the browser"),
    ]
}

fn provider_names() -> String {
    provider_catalog()
        .iter()
        .map(|(n, _, _, _)| *n)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Friendly name (or alias) -> (canonical name, kind, default base_url).
fn resolve_provider(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let n = name.trim().to_ascii_lowercase();
    if n.is_empty() {
        return None;
    }
    let aliases: [(&str, &str); 12] = [
        ("oai", "openai"), ("chatgpt", "openai"), ("openai-compat", "openai"),
        ("claude", "anthropic"), ("ds", "deepseek"), ("google", "gemini"),
        ("gem", "gemini"), ("open-router", "openrouter"), ("local", "ollama"),
        ("dsweb", "deepseekweb"), ("deepseek-web", "deepseekweb"),
        ("web", "deepseekweb"),
    ];
    let canon = aliases.iter().find(|(a, _)| *a == n).map(|(_, c)| *c).unwrap_or(&n);
    provider_catalog()
        .iter()
        .find(|(nm, _, _, _)| *nm == canon)
        .map(|(nm, kind, url, _)| (*nm, *kind, *url))
}

/* ---------------- config ---------------- */

fn default_cfg_path() -> PathBuf {
    home().join(".atria").join("cli.json")
}

fn load_config(path: &Path) -> Result<CliConfig, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("cannot read config {path:?}: {e}"))?;
    let cfg: CliConfig = serde_json::from_str(&raw).map_err(|e| format!("invalid config file: {e}"))?;
    if cfg.base_url.trim().is_empty() || cfg.model.trim().is_empty() {
        return Err("config needs non-empty \"base_url\" and \"model\"".into());
    }
    Ok(cfg)
}

fn write_starter(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let starter = serde_json::json!({
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-4o-mini",
        "kind": "openai"
    });
    if let Err(e) = std::fs::write(path, serde_json::to_string_pretty(&starter).unwrap()) {
        eprintln!("failed to write config file: {e}");
        std::process::exit(2);
    }
    eprintln!("config written: {}", path.display());
    eprintln!("edit it to point at your provider, then set ATRIA_API_KEY and run atria-cli");
}

/* ---------------- session ---------------- */

fn system_prompt(full: bool) -> String {
    let base = "You are Atria, a terminal agent embedded in the user's command line. \
Be concise and terminal-friendly: short answers, plain text over heavy markdown, \
fenced code blocks only when needed. Match the user's language.";
    if full {
        base.to_string()
            + " You have full Claude-Code-style powers in the current working directory: \
file tools (any path), terminal (run_command, run_code — PowerShell/Python/Node/sh) and \
computer tools (system_info, open_windows_settings). Use them proactively when asked; \
every action is logged for the user."
    } else {
        base.to_string()
            + " Use the built-in tools (web search, workspace files) when they help. \
Shell execution and computer control are available in the desktop app and in `atria-cli --full`, not here."
    }
}

struct Session {
    base_url: String,
    model: String,
    kind: String,
    api_key: String,
    tools: bool,
    full: bool,
}

impl Session {
    fn cwd_string() -> String {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    }

    fn client(&self) -> atria_core::ClientConfig {
        let home = home();
        let cwd = if self.full { Self::cwd_string() } else { String::new() };
        atria_core::ClientConfig {
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            max_tokens: 4096,
            temperature: 0.4,
            system: system_prompt(self.full),
            stream: true,
            tools: self.tools,
            tool_use_enabled: false,
            kind: atria_core::ApiKind::parse(&self.kind),
            file_tools: self.tools,
            workspace: home.join(".atria").join("workspace").to_string_lossy().to_string(),
            app_data: home.join(".atria").to_string_lossy().to_string(),
            web_tools: self.tools,
            github_tools: false,
            github_token: String::new(),
            autonomous_mode: false,
            full_access_mode: self.full,
            full_access_profile: self.full,
            full_access_system: self.full,
            computer_tools: self.full,
            terminal_tools: self.full,
            project_root: cwd,
            downloads_root: String::new(),
            web_thinking: false,
            web_search: false,
            web_session: String::new(),
            web_session_out: None,
            mcp_servers: Vec::new(),
        }
    }
}

fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let v: String = s.chars().take(n).collect();
    v + "…"
}

fn first_line(s: &str) -> String {
    s.lines().next().map(str::to_string).unwrap_or_default()
}

/// Claude-Code style detail for a tool call, keyed by the (English) tool name.
fn tool_detail(name: &str, input: &Value) -> String {
    match name {
        "run_command" => input
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string(),
        "run_code" => {
            let lang = input.get("lang").and_then(Value::as_str).unwrap_or("code");
            let code = input.get("code").and_then(Value::as_str).unwrap_or("");
            format!("{lang}: {}", first_line(code))
        }
        "web_search" | "search_web" => input
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "open_url" | "open_web_page" | "download_web_file" => input
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "list_files" | "read_file" | "write_file" => input
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "open_windows_settings" => input
            .get("area")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "system_info" => String::new(),
        "remember" | "recall" => input
            .get("text")
            .or_else(|| input.get("query"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => first_line(&input.to_string()),
    }
}

/// English one-line summary of a tool result (the core speaks Persian in its
/// wrapper lines; the CLI UI stays English).
fn tool_end_summary(name: &str, output: &str) -> String {
    if name == "run_command" || name == "run_code" {
        if let Some((_, after_code)) = output.split_once("کد خروجی: ") {
            let exit: String = after_code
                .chars()
                .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
                .collect();
            if let Some((_, out_part)) = after_code.split_once("خروجی:\n") {
                let fl = truncate_chars(&first_line(out_part).trim(), 120);
                if fl.is_empty() {
                    return format!("exit {exit}");
                }
                return format!("exit {exit} · {fl}");
            }
        }
    }
    first_line(output)
}

/// Run one agent turn. Streams text to stdout, tool activity to stderr, and
/// shows a "thinking" bar until the first output arrives.
/// On Ctrl+C during generation the turn is cancelled and the user message
/// stays in history (the model saw it); on error the caller drops it.
fn run_turn(
    rt: &tokio::runtime::Runtime,
    s: &Session,
    history: &mut Vec<atria_core::Message>,
) -> Result<bool, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let mem_path = home().join(".atria").join("memory.json");
    let client = s.client();
    let messages = history.clone();
    let started = std::time::Instant::now();
    let result = rt.block_on(async {
        let stop_flag = Arc::clone(&stop);
        let stop_task = tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            stop_flag.store(true, Ordering::SeqCst);
        });
        // Thinking bar: a magenta "✻ thinking… N.Ns" line that refreshes while
        // the model is silent and clears itself on the first real output.
        let first_output = Arc::new(AtomicBool::new(false));
        let spinner = {
            let first = Arc::clone(&first_output);
            let colors = colors_on();
            tokio::spawn(async move {
                let mut iv = tokio::time::interval(std::time::Duration::from_millis(250));
                loop {
                    iv.tick().await;
                    if first.load(Ordering::Relaxed) {
                        eprint!("\r\x1b[2K");
                        let _ = std::io::stderr().flush();
                        break;
                    }
                    let secs = started.elapsed().as_secs_f64();
                    eprint!(
                        "\r\x1b[2K  {} {} {} {}",
                        paint("✻", "35;1", colors),
                        paint("thinking", "35", colors),
                        paint("…", "35", colors),
                        paint(&format!("{secs:.1}s"), "90", colors)
                    );
                    let _ = std::io::stderr().flush();
                }
            })
        };
        let mut mem = atria_core::MemoryStore::open(&mem_path);
        // eol tracks whether the stream is currently at a line boundary so
        // tool lines never overwrite half-printed model text.
        let eol = Arc::new(AtomicBool::new(true));
        let res = atria_core::run_agent(&client, messages, &mut mem, Arc::clone(&stop), |ev| {
            // bookkeeping events don't touch the thinking-bar state
            if matches!(ev, atria_core::AgentEvent::Round(_) | atria_core::AgentEvent::Retry { .. }) {
                return;
            }
            let first = !first_output.swap(true, Ordering::SeqCst);
            if first {
                eprint!("\r\x1b[2K"); // clear the thinking bar line
            }
            match ev {
                atria_core::AgentEvent::ToolStart { name, input, .. } => {
                    if !eol.load(Ordering::Relaxed) {
                        eprint!("\n");
                    }
                    eol.store(true, Ordering::Relaxed);
                    let detail = truncate_chars(&tool_detail(&name, &input), 160);
                    if name == "run_command" {
                        eprintln!("  {} $ {detail}", c("⚙", "90"));
                    } else {
                        eprintln!("  {} {} {}", c("⚙", "90"), c(&name, "36"), detail);
                    }
                    let _ = std::io::stderr().flush();
                }
                atria_core::AgentEvent::ToolEnd { name, ok, output, .. } => {
                    if !eol.load(Ordering::Relaxed) {
                        eprint!("\n");
                    }
                    eol.store(true, Ordering::Relaxed);
                    let mark = if ok { c("✓", "32") } else { c("✗", "31") };
                    eprintln!("  {mark} {} {}", c(&name, "36"), truncate_chars(&tool_end_summary(&name, &output), 140));
                    let _ = std::io::stderr().flush();
                }
                atria_core::AgentEvent::Stream(atria_core::StreamEvent::Text(delta)) => {
                    eol.store(delta.ends_with('\n'), Ordering::Relaxed);
                    print!("{delta}");
                    let _ = std::io::stdout().flush();
                }
                _ => {}
            }
        })
        .await;
        stop_task.abort();
        spinner.abort();
        res
    });
    let elapsed = started.elapsed().as_secs_f64();
    let cancelled = stop.load(Ordering::SeqCst);
    match result {
        Ok(out) => {
            println!();
            eprintln!(
                "  {} {} {} {} {} {}{}{}",
                c("·", "90"),
                out.rounds,
                if out.rounds == 1 { c("round", "90") } else { c("rounds", "90") },
                c(&format!("{elapsed:.1}s"), "90"),
                c("·", "90"),
                c(
                    &format!("{} in / {} out tokens", out.input_tokens, out.output_tokens),
                    "90"
                ),
                if cancelled { c(" (cancelled)", "33") } else { String::new() },
                ""
            );
            if !cancelled {
                history.extend(out.new_messages);
            }
            Ok(cancelled)
        }
        Err(e) => Err(format!("{e}")),
    }
}

/* ---------------- self update ---------------- */

const REPO: &str = "Manixpcshot/atria-ai";

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

fn sums_hash_of(data: &str, file: &str) -> Option<String> {
    data.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        let (hash, name) = (parts.next()?, parts.next()?);
        if name == file {
            Some(hash.to_ascii_lowercase())
        } else {
            None
        }
    })
}

/// Fetch the release's SHA256SUMS.txt. Returns (release tag, sums text).
fn fetch_latest_sums(rt: &tokio::runtime::Runtime) -> Result<(String, String), String> {
    let rel_bytes = rt
        .block_on(atria_core::http_get_bytes(
            &format!("https://api.github.com/repos/{REPO}/releases/latest"),
            10,
        ))
        .map_err(|e| e.to_string())?;
    let rel: Value =
        serde_json::from_slice(&rel_bytes).map_err(|e| format!("bad release json: {e}"))?;
    let tag = rel
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or("release has no tag")?
        .to_string();
    let sums_url = rel
        .pointer("/assets")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .find(|a| a.get("name").and_then(Value::as_str) == Some("SHA256SUMS.txt"))
                .and_then(|a| a.get("browser_download_url").and_then(Value::as_str))
        })
        .ok_or("release has no SHA256SUMS.txt")?
        .to_string();
    let sums_bytes = rt
        .block_on(atria_core::http_get_bytes(&sums_url, 30))
        .map_err(|e| e.to_string())?;
    let sums = String::from_utf8_lossy(&sums_bytes).to_string();
    Ok((tag, sums))
}

/// Some((release tag, published hash)) when the published exe differs from
/// ours; Ok(None) when we are up to date.
fn update_available(rt: &tokio::runtime::Runtime) -> Result<Option<(String, String)>, String> {
    let sums_path = home().join(".atria").join(".cli_update_check");
    let fresh = std::fs::read_to_string(&sums_path)
        .ok()
        .filter(|_| {
            std::fs::metadata(&sums_path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs() < 6 * 3600)
                .unwrap_or(false)
        });
    let (tag, sums) = match fresh {
        Some(text) => ("latest".to_string(), text),
        None => {
            let (tag, sums) = fetch_latest_sums(rt)?;
            let _ = std::fs::create_dir_all(sums_path.parent().unwrap_or(Path::new(".")));
            let _ = std::fs::write(&sums_path, &sums);
            (tag, sums)
        }
    };
    let want = sums_hash_of(&sums, "atria-cli.exe")
        .ok_or("no atria-cli.exe line in SHA256SUMS.txt")?;
    let cur_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let cur = std::fs::read(&cur_path).map_err(|e| format!("cannot read current exe: {e}"))?;
    if sha256_hex(&cur) == want {
        Ok(None)
    } else {
        Ok(Some((tag, want)))
    }
}

fn run_update(rt: &tokio::runtime::Runtime) -> ! {
    eprintln!("checking for updates...");
    let want = match update_available(rt) {
        Ok(Some((_tag, w))) => w,
        Ok(None) => {
            eprintln!("{} atria-cli is up to date.", c("✓", "32"));
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("{} {e}", c("✗", "31"));
            fail(1);
        }
    };
    let exe_url = format!(
        "https://github.com/{REPO}/releases/latest/download/atria-cli.exe"
    );
    eprintln!("downloading {exe_url}");
    let bytes = rt
        .block_on(atria_core::http_get_bytes(&exe_url, 120))
        .map_err(|e| e.to_string())
        .unwrap_or_else(|e| {
            eprintln!("{} {e}", c("✗", "31"));
            fail(1);
        });
    if sha256_hex(&bytes) != want {
        eprintln!("{} SHA-256 mismatch — refusing to replace the binary.", c("✗", "31"));
        fail(1);
    }
    let cur = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{} cannot locate current exe: {e}", c("✗", "31"));
            fail(1);
        }
    };
    let tmp = cur.with_extension("exe.new");
    if let Err(e) = std::fs::write(&tmp, &bytes) {
        eprintln!("{} cannot write {}: {e}", c("✗", "31"), tmp.display());
        fail(1);
    }
    #[cfg(windows)]
    {
        // The running exe cannot be replaced in place: a small detached .bat
        // waits a moment, then deletes and moves.
        let bat = std::env::temp_dir().join("atria-cli-update.bat");
        let body = format!(
            "@echo off\r\nping 127.0.0.1 -n 3 >nul\r\ndel /f /q \"{}\"\r\nmove /y \"{}\" \"{}\"\r\n",
            cur.display(),
            tmp.display(),
            cur.display()
        );
        if let Err(e) = std::fs::write(&bat, body) {
            eprintln!("{} cannot write updater script: {e}", c("✗", "31"));
            fail(1);
        }
        use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
        if std::process::Command::new("cmd")
            .args(["/C", bat.to_str().unwrap_or("")] )
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .is_err()
        {
            eprintln!("{} could not start the replacement step — move {} over {} manually.", c("✗", "31"), tmp.display(), cur.display());
            fail(1);
        }
        eprintln!("{} update verified and downloaded — atria-cli.exe is being replaced (a few seconds). Run atria-cli again.", c("✓", "32"));
    }
    #[cfg(not(windows))]
    {
        if let Err(e) = std::fs::rename(&tmp, &cur) {
            eprintln!("{} cannot replace the binary: {e}", c("✗", "31"));
            fail(1);
        }
        eprintln!("{} updated.", c("✓", "32"));
    }
    std::process::exit(0);
}


/* ---------------- output ---------------- */

fn print_banner(s: &Session, cfg_path: &Path) {
    let date = chrono::Local::now().format("%Y-%m-%d %H:%M");
    println!("{}  v{}", c("Atria — terminal agent", "1"), env!("CARGO_PKG_VERSION"));
    println!("{} {}  ({})", c("provider:", "90"), s.kind, c(&s.base_url, "90"));
    println!("{} {}", c("model:", "90"), s.model);
    if s.full {
        println!(
            "{} {} — terminal, computer control, files in {}",
            c("mode:", "90"),
            c("FULL", "33;1"),
            c(&Session::cwd_string(), "33")
        );
    } else {
        println!("{} {} (web search + workspace files)", c("mode:", "90"), c("standard", "90"));
    }
    println!("{} {}", c("config:", "90"), cfg_path.display());
    println!(
        "{} {}  {}  tools: {}",
        c("date:", "90"),
        date,
        c("·", "90"),
        if s.tools { "on" } else { "off" }
    );
    println!("type /help for commands {} /exit or Ctrl+D to quit", c("·", "90"));
    println!();
    if s.full {
        eprintln!(
            "{} FULL mode: commands run immediately and are logged in this chat. No admin escalation. /full off to back out.",
            c("⚠", "33;1")
        );
    }
}

fn print_help() {
    println!(
        "atria-cli — Atria terminal agent

Usage:
  atria-cli                        start an interactive session
  atria-cli \"prompt\"               single turn, then exit
  echo \"text\" | atria-cli          single turn from stdin
  atria-cli --init                 write a starter ~/.atria/cli.json
  atria-cli --config PATH          alternate config file
  atria-cli --model NAME           override the configured model
  atria-cli --provider NAME        openai|anthropic|deepseek|groq|openrouter|gemini|ollama|deepseekweb
  atria-cli --full                 Claude-Code style: terminal + computer + files in the current dir
  atria-cli --update               self-update from the latest release (SHA-256 verified)
  atria-cli --tools | --no-tools   force the tool loop on/off
  atria-cli --version              print version and build time

Session commands:
  /help                 this help
  /clear                clear the conversation (long-term memory is kept)
  /model [NAME]         show or change the model (no arg = list models from the provider)
  /provider [NAME] [URL]  show or switch provider (session only); an optional
                          custom base_url can follow the name
  /full [on|off]        FULL mode: terminal + computer control + files in the
                        current directory (same Atria Full Access rules:
                        every command logged, no admin escalation)
  /key [VALUE]          show key status or set the key (session only)
  /exit                 quit (Ctrl+D also works)

Config (~/.atria/cli.json):
  {{ \"base_url\": \"...\", \"model\": \"...\", \"kind\": \"openai|anthropic|deepseekweb\" }}

The API key comes from the ATRIA_API_KEY environment variable and is never
written to disk. In standard mode, file tools are scoped to
~/.atria/workspace; with --full they work in the current directory, like
Claude Code. Self-update replaces only atria-cli.exe after a SHA-256 check."
    );
}

/* ---------------- main ---------------- */

fn main() {
    // A crash must never flash the console window closed either.
    std::panic::set_hook(Box::new(|info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        eprintln!("\natria-cli hit an unexpected error: {msg}");
        eprintln!("Re-run with RUST_BACKTRACE=1 for details, and report this to Atria.");
        pause_if_tty();
    }));

    COLORS.set(colors_enabled_now()).ok();
    if colors_on() {
        enable_windows_vt();
    }

    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw);
    if args.version {
        println!("atria-cli v{} (built {})", env!("CARGO_PKG_VERSION"), env!("BUILD_TIME"));
        return;
    }
    if args.help {
        print_help();
        return;
    }
    if args.init {
        write_starter(&args.config);
        return;
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");

    if args.update {
        run_update(&rt);
    }

    // one-shot prompt: from args, or (when piped) from stdin
    let mut prompt = args.prompt.unwrap_or_default();
    let piped = !std::io::stdin().is_terminal();
    if prompt.trim().is_empty() && piped {
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_err() {
            prompt = String::new();
        } else {
            prompt = buf;
        }
    }
    let interactive = prompt.trim().is_empty();

    let mut cfg = match load_config(&args.config) {
        Ok(c) => c,
        Err(e) => {
            if !args.config.exists() {
                // First run: create the starter config and keep going so the
                // window never flashes closed on a double-click.
                eprintln!(
                    "first run: creating a starter config at {}",
                    args.config.display()
                );
                write_starter(&args.config);
                match load_config(&args.config) {
                    Ok(c) => {
                        eprintln!("edit that file to set your provider/model, then start chatting.");
                        eprintln!();
                        c
                    }
                    Err(e2) => {
                        eprintln!("{e2}");
                        fail(2);
                    }
                }
            } else {
                eprintln!("{e}");
                eprintln!("run `atria-cli --init` to create a config file.");
                fail(2);
            }
        }
    };
    if let Some(p) = &args.provider {
        match resolve_provider(p) {
            Some((nm, kind, url)) => {
                cfg.kind = kind.to_string();
                cfg.base_url = url.to_string();
                eprintln!("provider: {nm}  ({url})");
            }
            None => {
                eprintln!("unknown provider: {p} — try: {}", provider_names());
                fail(2);
            }
        }
    }
    let api_key = std::env::var("ATRIA_API_KEY").unwrap_or_default();
    if api_key.trim().is_empty() {
        eprintln!("ATRIA_API_KEY is not set. Export it — the key is never written to disk.");
        if interactive {
            eprintln!("(you can also type /key <value> inside the session)");
            eprint!("API key: ");
            let _ = std::io::stdout().flush();
            let mut k = String::new();
            let _ = std::io::stdin().read_line(&mut k);
            let k = k.trim().to_string();
            if k.is_empty() {
                eprintln!("no key given — exiting. Set ATRIA_API_KEY or paste one when asked.");
                fail(2);
            }
            run_repl(cfg, &args.config, k, args.tools, args.model, args.full);
            return;
        }
        fail(2);
    }

    if interactive {
        run_repl(cfg, &args.config, api_key, args.tools, args.model, args.full);
        return;
    }

    let tools = args.tools.unwrap_or(false);
    let session = Session {
        base_url: cfg.base_url,
        model: args.model.clone().unwrap_or(cfg.model),
        kind: cfg.kind,
        api_key,
        tools,
        full: args.full,
    };
    let mut history = vec![atria_core::Message {
        role: atria_core::Role::User,
        content: vec![atria_core::Block::Text {
            text: prompt.trim().to_string(),
        }],
    }];
    match run_turn(&rt, &session, &mut history) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{} {e}", c("error:", "31;1"));
            fail(1);
        }
    }
}

fn run_repl(
    cfg: CliConfig,
    cfg_path: &Path,
    api_key: String,
    tools_flag: Option<bool>,
    model_flag: Option<String>,
    full: bool,
) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let mut session = Session {
        base_url: cfg.base_url,
        model: model_flag.unwrap_or(cfg.model),
        kind: cfg.kind,
        api_key,
        tools: tools_flag.unwrap_or(true),
        full,
    };
    let mut history: Vec<atria_core::Message> = Vec::new();
    print_banner(&session, cfg_path);

    // One-line update notice (best-effort, never blocks startup meaningfully).
    let check = std::thread::scope(|s| {
        s.spawn(|| -> Result<Option<(String, String)>, String> {
            update_available(&rt)
        })
        .join()
    })
    .ok();
    if let Some(Ok(Some(_))) = check {
        eprintln!(
            "{} a newer atria-cli is available — run `atria-cli --update`",
            c("↑", "35;1")
        );
    }

    loop {
        eprint!("{} ", c("❯", "35;1"));
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) => {
                println!();
                break; // Ctrl+D / EOF
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("input error: {e}");
                break;
            }
        }
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        match parse_slash(&line) {
            Cmd::Exit => break,
            Cmd::Help => print_help(),
            Cmd::Clear => {
                history.clear();
                eprintln!("conversation cleared (long-term memory is kept)");
            }
            Cmd::Model(Some(name)) => {
                session.model = name;
                eprintln!("model set to: {}", session.model);
            }
            Cmd::Model(None) => {
                eprintln!("model: {}", session.model);
                let models = rt.block_on(atria_core::list_models(
                    &session.base_url,
                    &session.api_key,
                ));
                match models {
                    Ok(ids) => {
                        let ids: Vec<String> = ids
                            .into_iter()
                            .filter(|m| !m.contains("embedding"))
                            .collect();
                        eprintln!("{} models from the provider:", ids.len());
                        for m in ids.iter().take(100) {
                            eprintln!("  {m}");
                        }
                        if ids.len() > 100 {
                            eprintln!("  … {} more", ids.len() - 100);
                        }
                        eprintln!("set one with: /model <name>");
                    }
                    Err(e) => eprintln!("could not list models: {e}\nset one manually with: /model <name>"),
                }
            }
            Cmd::Provider(Some(spec)) => {
                let mut parts = spec.splitn(2, char::is_whitespace);
                let name = parts.next().unwrap_or("");
                let custom_url = parts.next().map(str::trim).filter(|u| !u.is_empty());
                match resolve_provider(name) {
                    Some((nm, kind, url)) => {
                        session.kind = kind.to_string();
                        let target = custom_url.unwrap_or(url);
                        session.base_url = target.to_string();
                        eprintln!(
                            "provider set to: {nm}  (base_url: {})  — /model to pick a model",
                            session.base_url
                        );
                    }
                    None => eprintln!(
                        "unknown provider: {name} — try: {}",
                        provider_names()
                    ),
                }
            }
            Cmd::Provider(None) => {
                eprintln!("provider: {}  (base_url: {})", session.kind, session.base_url);
                eprintln!("known providers: {}", provider_names());
            }
            Cmd::Full(Some(v)) => match v.to_ascii_lowercase().as_str() {
                "on" | "true" | "1" | "yes" | "full" => {
                    session.full = true;
                    eprintln!(
                        "FULL mode ON — terminal, computer control, files in {}; every command is logged. No admin escalation.",
                        Session::cwd_string()
                    );
                }
                "off" | "false" | "0" | "no" | "standard" => {
                    session.full = false;
                    eprintln!("FULL mode OFF — back to standard (web + workspace files).");
                }
                other => eprintln!("usage: /full on | /full off (got: {other})"),
            },
            Cmd::Full(None) => {
                let state = if session.full { c("ON", "33;1") } else { c("off", "90") };
                eprintln!("FULL mode: {state}");
            }
            Cmd::Key(Some(value)) => {
                session.api_key = value;
                eprintln!("api key updated for this session (not saved to disk)");
            }
            Cmd::Key(None) => {
                if session.api_key.is_empty() {
                    eprintln!("no api key set");
                } else {
                    let head: String = session.api_key.chars().take(4).collect();
                    eprintln!("api key: {head}… (session only)");
                }
            }
            Cmd::Unknown(name) => eprintln!("unknown command: /{name} — try /help"),
            Cmd::Plain => {
                let turn_line = line.clone();
                history.push(atria_core::Message {
                    role: atria_core::Role::User,
                    content: vec![atria_core::Block::Text { text: turn_line }],
                });
                match run_turn(&rt, &session, &mut history) {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("{} {e}", c("error:", "31;1"));
                        history.pop(); // drop the failed user turn so it can be retried
                    }
                }
            }
        }
    }
    println!("bye");
}

/* ---------------- tests ---------------- */

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| to_string(x)).collect()
    }

    fn to_string(x: &str) -> String {
        x.to_string()
    }

    #[test]
    fn slash_commands_parse() {
        assert_eq!(parse_slash("/exit"), Cmd::Exit);
        assert_eq!(parse_slash("/quit"), Cmd::Exit);
        assert_eq!(parse_slash("/q"), Cmd::Exit);
        assert_eq!(parse_slash("/HELP"), Cmd::Help);
        assert_eq!(parse_slash("/?"), Cmd::Help);
        assert_eq!(parse_slash("/clear"), Cmd::Clear);
        assert_eq!(parse_slash("/reset"), Cmd::Clear);
        assert_eq!(parse_slash("/model gpt-4o"), Cmd::Model(Some("gpt-4o".into())));
        assert_eq!(parse_slash("/model"), Cmd::Model(None));
        assert_eq!(parse_slash("/key sk-123"), Cmd::Key(Some("sk-123".into())));
        assert_eq!(parse_slash("/bogus"), Cmd::Unknown("bogus".into()));
    }

    #[test]
    fn plain_text_is_not_a_command() {
        assert_eq!(parse_slash("write a haiku"), Cmd::Plain);
        assert_eq!(parse_slash("  /help  "), Cmd::Help);
    }

    #[test]
    fn args_parse_one_shot_and_flags() {
        let a = parse_args(&s(&["--tools", "hello", "world"]));
        assert_eq!(a.prompt.as_deref(), Some("hello world"));
        assert_eq!(a.tools, Some(true));

        let a = parse_args(&s(&["--no-tools", "--model", "m1", "hi"]));
        assert_eq!(a.tools, Some(false));
        assert_eq!(a.model.as_deref(), Some("m1"));

        let a = parse_args(&s(&["--init"]));
        assert!(a.init);
        assert_eq!(a.prompt, None);
    }

    #[test]
    fn provider_catalog_resolves_names_and_aliases() {
        assert_eq!(resolve_provider("openai"), Some(("openai", "openai", "https://api.openai.com/v1")));
        assert_eq!(resolve_provider("anthropic"), Some(("anthropic", "anthropic", "https://api.anthropic.com")));
        assert_eq!(resolve_provider("claude").unwrap().1, "anthropic");
        assert_eq!(resolve_provider("DS").unwrap().1, "openai");
        assert_eq!(resolve_provider("deepseek").unwrap().2, "https://api.deepseek.com/v1");
        assert_eq!(resolve_provider("ollama").unwrap().2, "http://localhost:11434/v1");
        assert_eq!(resolve_provider("deepseekweb").unwrap().1, "deepseekweb");
        assert_eq!(resolve_provider("nope"), None);
        assert_eq!(resolve_provider(""), None);
    }

    #[test]
    fn provider_slash_and_flag_parse() {
        assert_eq!(parse_slash("/provider anthropic"), Cmd::Provider(Some("anthropic".into())));
        assert_eq!(parse_slash("/prov"), Cmd::Provider(None));
        assert_eq!(
            parse_slash("/provider openai http://myproxy:8080/v1"),
            Cmd::Provider(Some("openai http://myproxy:8080/v1".into()))
        );
        let a = parse_args(&s(&["--provider", "groq", "hi"]));
        assert_eq!(a.provider.as_deref(), Some("groq"));
        assert_eq!(a.prompt.as_deref(), Some("hi"));
    }

    #[test]
    fn full_mode_and_update_flags_parse() {
        let a = parse_args(&s(&["--full", "hi"]));
        assert!(a.full);
        assert_eq!(a.prompt.as_deref(), Some("hi"));
        let a = parse_args(&s(&["--dangerous"]));
        assert!(a.full);
        let a = parse_args(&s(&["--update"]));
        assert!(a.update);
        let a = parse_args(&s(&["--version"]));
        assert!(a.version);
        assert_eq!(parse_slash("/full on"), Cmd::Full(Some("on".into())));
        assert_eq!(parse_slash("/full off"), Cmd::Full(Some("off".into())));
        assert_eq!(parse_slash("/full"), Cmd::Full(None));
        assert_eq!(parse_slash("/dangerous on"), Cmd::Full(Some("on".into())));
    }

    #[test]
    fn tool_detail_formats_common_tools() {
        let v = serde_json::json!({"command": "cargo build --release"});
        assert_eq!(tool_detail("run_command", &v), "cargo build --release");

        let v = serde_json::json!({"code": "Get-Process | Measure\nWrite-Output hi", "lang": "powershell"});
        assert!(tool_detail("run_code", &v).starts_with("powershell: Get-Process"));

        let v = serde_json::json!({"query": "rust async"});
        assert_eq!(tool_detail("web_search", &v), "rust async");

        let v = serde_json::json!({"url": "https://example.com/a.pdf"});
        assert_eq!(tool_detail("download_web_file", &v), "https://example.com/a.pdf");

        let v = serde_json::json!({"path": "C:\\proj\\notes.md"});
        assert_eq!(tool_detail("write_file", &v), "C:\\proj\\notes.md");

        let v = serde_json::json!({"area": "power"});
        assert_eq!(tool_detail("open_windows_settings", &v), "power");

        // unknown tool falls back to a truncated json view — must not panic
        let v = serde_json::json!({"x": [1, 2, 3]});
        assert!(!tool_detail("mystery_tool", &v).is_empty());
    }

    #[test]
    fn paint_is_noop_when_disabled() {
        assert_eq!(paint("hi", "31", false), "hi");
        assert_eq!(paint("hi", "31", true), "\x1b[31mhi\x1b[0m");
    }

    #[test]
    fn sums_hash_of_finds_line() {
        let sums = "aa11  atria.exe\nbb22  Atria-Dawn-win64.zip\ncc33  atria-cli.exe\n";
        assert_eq!(sums_hash_of(sums, "atria-cli.exe"), Some("cc33".into()));
        assert_eq!(sums_hash_of(sums, "missing.exe"), None);
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn tool_end_summary_translates_terminal_wrapper() {
        let out = "دستور اجرا شد. کد خروجی: 0\nپوشهٔ اجرا: /x\nخروجی:\nhello-from-mock\nsecond line";
        assert_eq!(tool_end_summary("run_command", out), "exit 0 · hello-from-mock");
        let out2 = "دستور اجرا شد. کد خروجی: 1\nپوشهٔ اجرا: /x\nخروجی:\n";
        assert_eq!(tool_end_summary("run_command", out2), "exit 1");
        // non-terminal or unparseable output falls back to the raw first line
        assert_eq!(tool_end_summary("web_search", "6 results"), "6 results");
        assert_eq!(tool_end_summary("run_command", "boom"), "boom");
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate_chars("abcdef", 3), "abc…");
        assert_eq!(truncate_chars("ab", 5), "ab");
        assert_eq!(truncate_chars("سلام دنیا", 3), "سلا…");
        assert_eq!(truncate_chars("", 5), "");
    }

    #[test]
    fn first_line_takes_only_the_first() {
        assert_eq!(first_line("a\nb\nc"), "a");
        assert_eq!(first_line("only"), "only");
        assert_eq!(first_line(""), "");
    }
}
