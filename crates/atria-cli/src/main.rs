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
//!   atria-cli --tools | --no-tools    force the tool loop on/off
//!
//! The API key is read from the ATRIA_API_KEY environment variable only —
//! it is never written to disk. Inside the REPL, `/key <value>` can set it
//! for the current session only.

use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
    help: bool,
}

fn parse_args(args: &[String]) -> Args {
    let mut out = Args {
        prompt: None,
        tools: None,
        init: false,
        config: default_cfg_path(),
        model: None,
        help: false,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tools" => out.tools = Some(true),
            "--no-tools" => out.tools = Some(false),
            "--init" => out.init = true,
            "--help" | "-h" => out.help = true,
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

const SYSTEM_PROMPT: &str = "You are Atria, a terminal agent embedded in the user's command line. \
Be concise and terminal-friendly: short answers, plain text over heavy markdown, fenced code \
blocks only when needed. Match the user's language. Use the built-in tools (web search, \
workspace files) when they help. Shell execution is available in the desktop app, not here.";

struct Session {
    base_url: String,
    model: String,
    kind: String,
    api_key: String,
    tools: bool,
}

impl Session {
    fn client(&self) -> atria_core::ClientConfig {
        let home = home();
        atria_core::ClientConfig {
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            max_tokens: 4096,
            temperature: 0.4,
            system: SYSTEM_PROMPT.to_string(),
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

/// Run one agent turn. Streams text to stdout, tool activity to stderr.
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
    let result = rt.block_on(async {
        let stop_flag = Arc::clone(&stop);
        let stop_task = tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            stop_flag.store(true, Ordering::SeqCst);
        });
        let mut mem = atria_core::MemoryStore::open(&mem_path);
        let res = atria_core::run_agent(&client, messages, &mut mem, Arc::clone(&stop), |ev| match ev {
            atria_core::AgentEvent::ToolStart { label, input, .. } => {
                eprint!(
                    "\r\x1b[2K  \u{2699} {} {}\n",
                    label,
                    truncate_chars(&first_line(&input.to_string()), 140)
                );
                let _ = std::io::stderr().flush();
            }
            atria_core::AgentEvent::ToolEnd { label, ok, output, .. } => {
                let mark = if ok { "\u{2713}" } else { "\u{2717}" };
                eprint!("\r\x1b[2K  {mark} {} {}\n", label, truncate_chars(&first_line(&output), 140));
                let _ = std::io::stderr().flush();
            }
            atria_core::AgentEvent::Stream(atria_core::StreamEvent::Text(delta)) => {
                print!("{delta}");
                let _ = std::io::stdout().flush();
            }
            _ => {}
        })
        .await;
        stop_task.abort();
        res
    });
    let cancelled = stop.load(Ordering::SeqCst);
    match result {
        Ok(out) => {
            println!();
            eprintln!(
                "  \u{b7} {} round{} \u{b7} {} in / {} out tokens{}\n",
                out.rounds,
                if out.rounds == 1 { "" } else { "s" },
                out.input_tokens,
                out.output_tokens,
                if cancelled { " (cancelled)" } else { "" }
            );
            if !cancelled {
                history.extend(out.new_messages);
            }
            Ok(cancelled)
        }
        Err(e) => Err(format!("{e}")),
    }
}

/* ---------------- output ---------------- */

fn print_banner(s: &Session, cfg_path: &Path) {
    let date = chrono::Local::now().format("%Y-%m-%d %H:%M");
    println!("Atria \u{2014} terminal agent  v{}", env!("CARGO_PKG_VERSION"));
    println!("model: {} @ {}", s.model, s.base_url);
    println!("config: {}", cfg_path.display());
    println!(
        "date: {}  \u{b7}  tools: {}",
        date,
        if s.tools { "on (web search + workspace files)" } else { "off" }
    );
    println!("type /help for commands \u{b7} /exit or Ctrl+D to quit");
    println!();
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
  atria-cli --tools | --no-tools   force the tool loop on/off

Session commands:
  /help                 this help
  /clear                clear the conversation (long-term memory is kept)
  /model [NAME]         show or change the model (session only)
  /key [VALUE]          show key status or set the key (session only)
  /exit                 quit (Ctrl+D also works)

Config (~/.atria/cli.json):
  {{ \"base_url\": \"...\", \"model\": \"...\", \"kind\": \"openai|anthropic\" }}

The API key comes from the ATRIA_API_KEY environment variable and is never
written to disk. File tools are scoped to ~/.atria/workspace; shell
execution belongs to the desktop app."
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

    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw);
    if args.help {
        print_help();
        return;
    }
    if args.init {
        write_starter(&args.config);
        return;
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

    let cfg = match load_config(&args.config) {
        Ok(c) => c,
        Err(e) => {
            if !args.config.exists() {
                // First run: create the starter config and keep going so the
                // window never flashes closed on a double-click.
                eprintln!("first run: creating a starter config at {}", args.config.display());
                write_starter(&args.config);
                match load_config(&args.config) {
                    Ok(c) => {
                        eprintln!("edit that file to set your provider/model, then start chatting.");
                        eprintln!();
                        c
                    }
                    Err(e2) => {
                        eprintln!("{e2}");
                        fail(2)
                    }
                }
            } else {
                eprintln!("{e}");
                eprintln!("run `atria-cli --init` to create a config file.");
                fail(2)
            }
        }
    };
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
            run_repl(cfg, &args.config, k, args.tools, args.model);
            return;
        }
        fail(2);
    }

    if interactive {
        run_repl(cfg, &args.config, api_key, args.tools, args.model);
        return;
    }

    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    let tools = args.tools.unwrap_or(false);
    let session = Session {
        base_url: cfg.base_url,
        model: args.model.clone().unwrap_or(cfg.model),
        kind: cfg.kind,
        api_key,
        tools,
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
            eprintln!("error: {e}");
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
) {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    let mut session = Session {
        base_url: cfg.base_url,
        model: model_flag.unwrap_or(cfg.model),
        kind: cfg.kind,
        api_key,
        tools: tools_flag.unwrap_or(true),
    };
    let mut history: Vec<atria_core::Message> = Vec::new();
    print_banner(&session, cfg_path);

    loop {
        eprint!("\u{276f} ");
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
            Cmd::Model(None) => eprintln!("model: {}", session.model),
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
                        eprintln!("error: {e}");
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
        v.iter().map(|x| x.to_string()).collect()
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
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate_chars("abcdef", 3), "abc…");
        assert_eq!(truncate_chars("ab", 5), "ab");
        assert_eq!(truncate_chars("سلام دنیا", 3), "سلا…");
        assert_eq!(truncate_chars("", 3), "");
    }

    #[test]
    fn first_line_takes_only_the_first() {
        assert_eq!(first_line("a\nb\nc"), "a");
        assert_eq!(first_line("only"), "only");
        assert_eq!(first_line(""), "");
    }
}
