//! Live smoke tests against the real Atria gateway (both dialects).
//!
//! ```sh
//! ATRIA_API_KEY=... cargo test -p atria-core --test live_api -- --ignored --nocapture
//! ```

use atria_core::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn key() -> String {
    std::env::var("ATRIA_API_KEY").expect("set ATRIA_API_KEY to run ignored live_api tests; no key is embedded")
}

#[tokio::test]
#[ignore = "hits the live API"]
async fn streaming_text_works() {
    let cfg = ClientConfig { api_key: key(), ..Default::default() };
    let mut text = String::new();
    let turn = send(
        &cfg,
        &[Message::user_text("Say exactly: HELLO ATRIA")],
        &[],
        |ev| {
            if let StreamEvent::Text(t) = ev {
                text.push_str(&t);
            }
        },
        Arc::new(AtomicBool::new(false)),
    )
    .await
    .expect("stream send");
    assert_eq!(turn.message.role, Role::Assistant);
    assert!(!text.is_empty(), "should have streamed text");
    println!("streamed: {text:?}");
    println!("final:    {:?}", turn.message.plain_text());
}

#[tokio::test]
#[ignore = "hits the live API"]
async fn agent_uses_tools_and_answers() {
    let cfg = ClientConfig { api_key: key(), ..Default::default() };
    let mut mem = MemoryStore::in_memory();
    let mut saw_tool = false;
    let out = run_agent(
        &cfg,
        vec![Message::user_text(
            "Use the calculator tool to compute 17*24+3 and reply with just the number.",
        )],
        &mut mem,
        Arc::new(AtomicBool::new(false)),
        |ev| {
            if let AgentEvent::ToolEnd { name, ok, output, .. } = &ev {
                saw_tool = true;
                println!("tool {name} ok={ok} -> {output}");
            }
        },
    )
    .await
    .expect("agent run");

    println!("rounds={} text={:?}", out.rounds, out.final_text);
    assert!(saw_tool, "model should call a tool");
    assert!(out.rounds >= 2, "needs a tool round + a final round");
    assert!(out.final_text.contains("411"), "final text: {}", out.final_text);
}

#[tokio::test]
#[ignore = "hits the live API"]
async fn chat_completions_streaming_works() {
    let cfg = ClientConfig {
        api_key: key(),
        kind: ApiKind::OpenAi,
        tools: false, // Atria router stalls on tool emission in this dialect
        ..Default::default()
    };
    let mut text = String::new();
    let mut thinking = String::new();
    let turn = send(
        &cfg,
        &[Message::user_text("Say exactly: HELLO ATRIA")],
        &[],
        |ev| match ev {
            StreamEvent::Text(t) => text.push_str(&t),
            StreamEvent::Thinking(t) => thinking.push_str(&t),
            _ => {}
        },
        Arc::new(AtomicBool::new(false)),
    )
    .await
    .expect("chat-completions send");
    assert!(!text.is_empty(), "should have streamed content");
    println!("thinking: {thinking:?}");
    println!("cc streamed: {text:?}");
}

#[tokio::test]
#[ignore = "hits the live API"]
async fn agent_writes_workspace_file() {
    let ws = std::env::temp_dir().join(format!("atria-live-ws-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ws);
    std::fs::create_dir_all(&ws).unwrap();

    let cfg = ClientConfig {
        api_key: key(),
        file_tools: true,
        workspace: ws.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let mut mem = MemoryStore::in_memory();
    let out = run_agent(
        &cfg,
        vec![Message::user_text(
            "Use the write_file tool to create 'hello.txt' with content 'salam donya', then confirm.",
        )],
        &mut mem,
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .await
    .expect("agent run");

    let content = std::fs::read_to_string(ws.join("hello.txt")).unwrap_or_default();
    println!("file: {content:?} final: {:?}", out.final_text);
    assert!(content.contains("salam donya"), "agent should have written the file");
    let _ = std::fs::remove_dir_all(&ws);
}
