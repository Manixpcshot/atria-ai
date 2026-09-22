//! Live smoke tests against the real Atria gateway.
//!
//! ```sh
//! ATRIA_API_KEY=... cargo test -p atria-core --test live_api -- --ignored --nocapture
//! ```

use atria_core::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn key() -> String {
    std::env::var("ATRIA_API_KEY").unwrap_or_else(|_| "".into())
}

#[tokio::test]
#[ignore = "hits the live API"]
async fn streaming_text_works() {
    let cfg = ClientConfig { api_key: key(), ..Default::default() };
    let client = AtriaClient::new();
    let mut text = String::new();
    let turn = client
        .send(
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
    let client = AtriaClient::new();
    let mut mem = MemoryStore::in_memory();
    let mut saw_tool = false;
    let out = run_agent(
        &client,
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
