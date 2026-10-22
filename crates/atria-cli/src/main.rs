//! Atria CLI — thin terminal front-end over `atria-core`.
//!
//! Config: `~/.atria/cli.json`
//!   { "base_url": "https://api.openai.com/v1", "model": "gpt-4o", "kind": "openai" }
//! The API key is read from the ATRIA_API_KEY environment variable only —
//! it is never written to disk by the CLI.
//!
//! Usage:
//!   atria "prompt text"                 single turn, tools off
//!   atria --tools "prompt"              with the built-in tool loop
//!   echo "text" | atria                 reads the prompt from stdin
//!   atria --config /path/cli.json       alternate config file
//!   atria --init                        create a starter config file

use std::io::{Read, Write};
use std::path::PathBuf;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct CliConfig {
    base_url: String,
    model: String,
    #[serde(default)]
    kind: String,
}

fn home() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn default_cfg_path() -> PathBuf {
    home().join(".atria").join("cli.json")
}

fn load_config(path: &PathBuf) -> Result<CliConfig, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("خواندن تنظیمات {path:?} نشد: {e}"))?;
    let cfg: CliConfig = serde_json::from_str(&raw).map_err(|e| format!("تنظیمات نامعتبر است: {e}"))?;
    if cfg.base_url.trim().is_empty() || cfg.model.trim().is_empty() {
        return Err("base_url و model در تنظیمات لازم است".into());
    }
    Ok(cfg)
}

fn write_starter(path: &PathBuf) {
    let _ = std::fs::create_dir_all(path.parent().unwrap_or_else(|| std::path::Path::new(".")));
    let starter = serde_json::json!({
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-4o-mini",
        "kind": "openai"
    });
    if let Err(e) = std::fs::write(path, serde_json::to_string_pretty(&starter).unwrap()) {
        eprintln!("نوشتن تنظیمات آغازین نشد: {e}");
        std::process::exit(2);
    }
    eprintln!("فایل تنظیمات ساخته شد: {}", path.display());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut tools = false;
    let mut cfg_path = default_cfg_path();
    let mut prompt: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--tools" => tools = true,
            "--init" => {
                write_starter(&cfg_path);
                return;
            }
            "--config" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("--config یک مسیر می‌خواهد");
                    std::process::exit(2);
                }
                cfg_path = PathBuf::from(&args[i]);
            }
            "--help" | "-h" => {
                print_help();
                return;
            }
            other => {
                if prompt.is_none() {
                    prompt = Some(other.to_string());
                } else {
                    let p = prompt.get_or_insert(String::new());
                    p.push(' ');
                    p.push_str(other);
                }
            }
        }
        i += 1;
    }
    let mut prompt = prompt.unwrap_or_default();
    if prompt.trim().is_empty() {
        // stdin
        let mut buf = String::new();
        if std::io::stdin().read_to_string(&mut buf).is_err() {
            prompt = String::new();
        } else {
            prompt = buf;
        }
    }
    if prompt.trim().is_empty() {
        print_help();
        std::process::exit(2);
    }
    let api_key = std::env::var("ATRIA_API_KEY").unwrap_or_default();
    if api_key.trim().is_empty() {
        eprintln!("متغیر محیطی ATRIA_API_KEY را تنظیم کن (کلید هرگز روی دیسک ذخیره نمی‌شود).");
        std::process::exit(2);
    }
    let cfg = match load_config(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("با `atria --init` یک فایل تنظیمات بساز.");
            std::process::exit(2);
        }
    };
    let client = atria_core::ClientConfig {
        api_key,
        base_url: cfg.base_url,
        model: cfg.model,
        max_tokens: 4096,
        temperature: 0.4,
        system: "تو دستیار خط فرمان آتریای هستی؛ پاسخ‌ها فشرده و قابل‌کپی باشند.".to_string(),
        stream: false,
        tools,
        tool_use_enabled: false,
        kind: atria_core::ApiKind::parse(&cfg.kind),
        file_tools: tools,
        workspace: home().join(".atria").join("workspace").to_string_lossy().to_string(),
        app_data: home().join(".atria").to_string_lossy().to_string(),
        web_tools: tools,
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
    let messages = vec![atria_core::Message {
        role: atria_core::Role::User,
        content: vec![atria_core::Block::Text { text: prompt.trim().to_string() }],
    }];
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mem_path = home().join(".atria").join("memory.json");
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    let result = rt.block_on(async {
        let mut mem = atria_core::MemoryStore::open(&mem_path);
        atria_core::run_agent(&client, messages, &mut mem, stop, |ev| match ev {
            atria_core::AgentEvent::ToolStart { label, input, .. } => {
                eprintln!("[ابزار] {label}: {}", input.to_string().chars().take(200).collect::<String>());
            }
            atria_core::AgentEvent::ToolEnd { label, ok, output, .. } => {
                let mark = if ok { "✓" } else { "✗" };
                eprintln!("[ابزار {mark}] {label}: {}", output.chars().take(300).collect::<String>());
            }
            atria_core::AgentEvent::Stream(atria_core::StreamEvent::Text(delta)) => {
                eprint!("{delta}");
                let _ = std::io::stderr().flush();
            }
            _ => {}
        })
        .await
    });
    match result {
        Ok(out) => {
            if !out.final_text.is_empty() {
                println!("{}", out.final_text);
            }
        }
        Err(e) => {
            eprintln!("خطا: {e}");
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        "atria — CLI هستهٔ آتریا

استفاده:
  atria \"پرسش\"                 یک دور با ابزارها خاموش
  atria --tools \"پرسش\"         با حلقهٔ ابزارهای داخلی
  echo \"متن\" | atria          پرسش از stdin
  atria --init                 ساخت فایل تنظیمات آغازین
  atria --config مسیر/فایل     تنظیمات جایگزین

تنظیمات (~/.atria/cli.json):
  {{ \"base_url\": \"...\", \"model\": \"...\", \"kind\": \"openai|anthropic\" }}

کلید API فقط از متغیر محیطی ATRIA_API_KEY خوانده می‌شود و هرگز روی دیسک ذخیره نمی‌شود."
    );
}
