//! Minimal MCP (Model Context Protocol) client over stdio.
//!
//! Each operation spawns the configured server command, performs the JSON-RPC
//! handshake (initialize → initialized notification), performs the requested
//! call and terminates the process. No long-lived processes are kept, which
//! keeps the desktop app's resource footprint predictable.
//!
//! Tool names are exposed to the model as `mcp__<server>__<tool>` so they can
//! never collide with the built-in tool names.

use crate::tools::ToolOutcome;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const MAX_ARGS: usize = 16;
pub const MAX_ARG_LEN: usize = 4000;
pub const INIT_TIMEOUT: Duration = Duration::from_secs(30);
pub const LIST_TIMEOUT: Duration = Duration::from_secs(30);
pub const CALL_TIMEOUT: Duration = Duration::from_secs(120);
/// The model can see at most this many characters of an MCP tool result.
const MAX_RESULT_CHARS: usize = 16_000;

/// One configured MCP server (stdio transport).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    /// Short unique id, sanitized to `[a-z0-9_]` (used in tool names).
    pub name: String,
    /// Executable to spawn (e.g. `npx`, `node`, `python3`).
    pub command: String,
    /// Arguments for the executable (e.g. `["-y", "@modelcontextprotocol/server-filesystem", "C:/files"]`).
    pub args: Vec<String>,
    /// Extra environment variables as `K=V` strings (MCP presets / API keys).
    /// Atria reads them at spawn time; nothing is written to disk by the engine.
    #[serde(default)]
    pub env: Vec<String>,
}

impl McpServer {
    /// Sanitize the display name into a safe tool-name segment.
    pub fn sanitize_name(raw: &str) -> String {
        let mut out = String::new();
        for c in raw.trim().chars() {
            if c.is_ascii_alphanumeric() && out.len() < 24 {
                out.push(c.to_ascii_lowercase());
            } else if (c == '-' || c == '_' || c == ' ') && out.len() < 24 {
                out.push('_');
            }
        }
        if out.is_empty() {
            "server".to_string()
        } else {
            out
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("نام سرور MCP خالی است".into());
        }
        if self.command.trim().is_empty() {
            return Err("دستور (Command) سرور MCP خالی است".into());
        }
        if self.args.len() > MAX_ARGS {
            return Err(format!("حداکثر {MAX_ARGS} آرگومان مجاز است"));
        }
        if self.args.iter().any(|a| a.chars().count() > MAX_ARG_LEN) {
            return Err(format!("هر آرگومان حداکثر {MAX_ARG_LEN} کاراکتر"));
        }
        if self.env.len() > 16 {
            return Err("حداکثر 16 متغیر محیطی مجاز است".into());
        }
        for e in &self.env {
            if e.chars().count() > 512 {
                return Err("هر متغیر محیطی حداکثر 512 کاراکتر (K=V)".into());
            }
            let (k, _v) = e.split_once('=').ok_or("متغیر محیطی باید به شکل K=V باشد")?;
            if k.is_empty() || k != k.trim() || k.chars().any(|c| c.is_whitespace()) {
                return Err("نام متغیر محیطی نامعتبر است".into());
            }
        }
        Ok(())
    }

    /// Parsed `K=V` pairs (validate() guarantees the shape).
    pub fn env_pairs(&self) -> Vec<(String, String)> {
        self.env
            .iter()
            .filter_map(|e| {
                let (k, v) = e.split_once('=')?;
                Some((k.to_string(), v.to_string()))
            })
            .collect()
    }

    pub fn command_desc(&self) -> String {
        let mut s = self.command.trim().to_string();
        for a in &self.args {
            let t = a.trim();
            if !t.is_empty() {
                s.push(' ');
                s.push_str(t);
            }
        }
        s
    }
}

/// Tool name as advertised to the model.
pub fn tool_name(server: &str, tool: &str) -> String {
    format!("mcp__{}__{}", McpServer::sanitize_name(server), sanitize_tool(tool))
}

/// Parse a prefixed MCP tool name back into (server, tool).
pub fn parse_tool_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once("__")?;
    if server.is_empty() || tool.is_empty() {
        return None;
    }
    Some((server, tool))
}

fn sanitize_tool(raw: &str) -> &str {
    let t = raw.trim();
    // Tools with weird characters are still addressable: keep them verbatim
    // (they never collide because the server segment is sanitized).
    t
}

/// One MCP tool descriptor (as returned by `tools/list`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
}

struct Rpc {
    child: tokio::process::Child,
    stdin: tokio::io::BufWriter<tokio::process::ChildStdin>,
    stdout: tokio::io::BufReader<tokio::process::ChildStdout>,
}

impl Rpc {
    async fn spawn(command: &str, args: &[String], env: &[(String, String)]) -> Result<Self, String> {
        let mut child = tokio::process::Command::new(command)
            .args(args)
            .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("اجرای «{command}» ممکن نبود: {e}"))?;
        let stdin = child.stdin.take().ok_or("stdin در دسترس نیست")?;
        let stdout = child.stdout.take().ok_or("stdout در دسترس نیست")?;
        Ok(Self {
            child,
            stdin: tokio::io::BufWriter::new(stdin),
            stdout: tokio::io::BufReader::new(stdout),
        })
    }

    async fn send(&mut self, v: &serde_json::Value) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        let mut line = serde_json::to_string(v).map_err(|e| e.to_string())?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
        self.stdin.flush().await.map_err(|e| e.to_string())
    }

    /// Read lines until one carries the given jsonrpc id, or the process exits.
    async fn wait(&mut self, id: u64, timeout: Duration) -> Result<serde_json::Value, String> {
        use tokio::io::AsyncBufReadExt;
        let deadline = std::time::Instant::now() + timeout;
        let mut line = String::new();
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err("مهلت پاسخ سرور MCP به پایان رسید (timeout)".into());
            }
            line.clear();
            let n = tokio::time::timeout(remaining, self.stdout.read_line(&mut line))
                .await
                .map_err(|_| "مهلت پاسخ سرور MCP به پایان رسید (timeout)".to_string())?
                .map_err(|e| format!("خواندن خروجی سرور MCP شکست خورد: {e}"))?;
            if n == 0 {
                let out = self.child.wait().await.ok();
                return Err(format!("سرور MCP پیش از پاسخ بسته شد {:?}", out.map(|s| s.code()).flatten()));
            }
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            let v: serde_json::Value = match serde_json::from_str(t) {
                Ok(v) => v,
                Err(_) => continue, // not JSON-RPC (server log noise)
            };
            if v.get("id").and_then(serde_json::Value::as_u64) == Some(id) {
                if let Some(err) = v.get("error") {
                    let msg = err.get("message").and_then(serde_json::Value::as_str).unwrap_or("unknown");
                    return Err(format!("خطای سرور MCP: {msg}"));
                }
                return Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null));
            }
            // Notifications / unexpected responses are skipped.
        }
    }

    async fn close(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

/// `tools/list` — the tools a server advertises.
pub async fn list_tools(server: &McpServer) -> Result<Vec<McpTool>, String> {
    server.validate()?;
    let mut rpc = Rpc::spawn(server.command.trim(), &server.args, &server.env_pairs()).await?;
    let res = (async {
        rpc.send(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "atria-desktop", "version": env!("CARGO_PKG_VERSION") }
            }
        }))
        .await?;
        rpc.wait(1, INIT_TIMEOUT).await?;
        let _ = rpc.send(&serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
        rpc.send(&serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }))
            .await?;
        rpc.wait(2, LIST_TIMEOUT).await
    })
    .await;
    rpc.close().await;
    let result = res?;
    let items = result.get("tools").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
    Ok(items
        .iter()
        .filter_map(|t| {
            Some(McpTool {
                name: t.get("name").and_then(serde_json::Value::as_str)?.to_string(),
                description: t.get("description").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
                input_schema: t.get("inputSchema").cloned().unwrap_or(serde_json::json!({"type": "object"})),
            })
        })
        .collect())
}

/// `tools/call` — run one tool and collapse its content into text.
pub async fn call_tool(server: &McpServer, tool: &str, args: &serde_json::Value) -> ToolOutcome {
    match call_tool_impl(server, tool, args).await {
        Ok(text) => ToolOutcome { output: truncate_chars(&text, MAX_RESULT_CHARS), is_error: false },
        Err(e) => ToolOutcome { output: e, is_error: true },
    }
}

async fn call_tool_impl(server: &McpServer, tool: &str, args: &serde_json::Value) -> Result<String, String> {
    server.validate()?;
    let mut rpc = Rpc::spawn(server.command.trim(), &server.args, &server.env_pairs()).await?;
    let res = (async {
        rpc.send(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "atria-desktop", "version": env!("CARGO_PKG_VERSION") }
            }
        }))
        .await?;
        rpc.wait(1, INIT_TIMEOUT).await?;
        let _ = rpc.send(&serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await;
        rpc.send(&serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": tool, "arguments": args } }))
            .await?;
        rpc.wait(2, CALL_TIMEOUT).await
    })
    .await;
    rpc.close().await;
    let result = res?;
    if result.get("isError").and_then(serde_json::Value::as_bool) == Some(true) {
        return Err(collapse_content(&result).unwrap_or("سرور MCP خطا گزارش کرد".into()));
    }
    Ok(collapse_content(&result).unwrap_or_default())
}

/// MCP tool content is an array of typed parts; collapse text (and a marker
/// for non-text parts) into one string.
fn collapse_content(result: &serde_json::Value) -> Option<String> {
    let content = result.get("content")?;
    let mut out = String::new();
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(items) = content.as_array() {
        for item in items {
            match item.get("type").and_then(serde_json::Value::as_str) {
                Some("text") => {
                    if let Some(t) = item.get("text").and_then(serde_json::Value::as_str) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(t);
                    }
                }
                Some(other) => {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&format!("[محتوای {other} — فقط متن پشتیبانی می‌شود]"));
                }
                _ => {}
            }
        }
    }
    Some(out)
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut} … [متن ابزار بیشتر از {max} کاراکتر بود و کوتاه شد]")
}

/// Build the Anthropic-style tool descriptors for every configured server
/// (only the servers that answer are included).
pub async fn catalog(servers: &[McpServer]) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for server in servers {
        if server.validate().is_err() {
            continue;
        }
        match list_tools(server).await {
            Ok(tools) => {
                for t in tools {
                    out.push(serde_json::json!({
                        "name": tool_name(&server.name, &t.name),
                        "description": format!("[MCP:{}] {}", server.name, t.description),
                        "input_schema": t.input_schema,
                    }));
                }
            }
            Err(_) => {} // unreachable server contributes no tools
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny fake MCP server implemented in Node (Node is available wherever
    /// the UI harness runs: local dev, CI). Skipped when `node` is missing.
    const FAKE_SERVER_JS: &str = r#"
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin });
const send = (o) => process.stdout.write(JSON.stringify(o) + '\n');
rl.on('line', (line) => {
  let req; try { req = JSON.parse(line); } catch { return; }
  if (req.method === 'initialize') {
    send({ jsonrpc: '2.0', id: req.id, result: { protocolVersion: req.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: 'fake', version: '0.0.1' } } });
  } else if (req.method === 'tools/list') {
    send({ jsonrpc: '2.0', id: req.id, result: { tools: [ { name: 'echo', description: 'echo the arguments', inputSchema: { type: 'object', properties: { msg: { type: 'string' } }, required: ['msg'] } } ] } });
  } else if (req.method === 'tools/call') {
    const p = req.params || {};
    if (p.name === 'echo') send({ jsonrpc: '2.0', id: req.id, result: { content: [{ type: 'text', text: 'echo:' + JSON.stringify(p.arguments || {}) }], isError: false } });
    else if (p.name === 'fail') send({ jsonrpc: '2.0', id: req.id, result: { content: [{ type: 'text', text: 'boom' }], isError: true } });
    else send({ jsonrpc: '2.0', id: req.id, error: { code: -32601, message: 'unknown tool' } });
  }
});
"#;

    fn node() -> Option<String> {
        std::env::var("PATH")
            .ok()
            .and_then(|p| {
                p.split(':').chain(p.split(';')).find_map(|dir| {
                    let cand = format!("{dir}/node");
                    if std::path::Path::new(&cand).exists() {
                        Some(cand)
                    } else {
                        None
                    }
                })
            })
    }

    fn fake_server() -> McpServer {
        McpServer {
            name: "fake".into(),
            command: node().unwrap_or_else(|| "node".into()),
            args: vec!["-e".into(), FAKE_SERVER_JS.to_string()],
            env: vec![],
        }
    }

    #[tokio::test]
    async fn list_tools_roundtrip() {
        if node().is_none() {
            eprintln!("skip: node not found");
            return;
        }
        let tools = list_tools(&fake_server()).await.expect("list");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
        assert!(tools[0].description.contains("echo"));
    }

    #[tokio::test]
    async fn call_tool_echoes_args() {
        if node().is_none() {
            eprintln!("skip: node not found");
            return;
        }
        let out = call_tool(&fake_server(), "echo", &serde_json::json!({ "msg": "hello" })).await;
        assert!(!out.is_error);
        assert!(out.output.contains("hello"), "got: {}", out.output);
    }

    #[tokio::test]
    async fn call_tool_reports_server_error() {
        if node().is_none() {
            eprintln!("skip: node not found");
            return;
        }
        let out = call_tool(&fake_server(), "fail", &serde_json::json!({})).await;
        assert!(out.is_error);
        assert!(out.output.contains("boom"));
    }

    #[tokio::test]
    async fn call_unknown_tool_is_an_error() {
        if node().is_none() {
            eprintln!("skip: node not found");
            return;
        }
        let out = call_tool(&fake_server(), "nope", &serde_json::json!({})).await;
        assert!(out.is_error);
    }

    #[tokio::test]
    async fn missing_command_is_a_clean_error() {
        let s = McpServer { name: "x".into(), command: "definitely-not-a-command-12345".into(), args: vec![], env: vec![] };
        let res = list_tools(&s).await;
        assert!(res.is_err());
    }

    #[test]
    fn name_parsing_and_sanitizing() {
        assert_eq!(McpServer::sanitize_name("My-Server 2!"), "my_server_2");
        assert_eq!(McpServer::sanitize_name("///"), "server");
        assert_eq!(tool_name("My-Server", "read_file"), "mcp__my_server__read_file");
        assert_eq!(parse_tool_name("mcp__my_server__read_file"), Some(("my_server", "read_file")));
        assert_eq!(parse_tool_name("mcp__onlyserver"), None);
        assert_eq!(parse_tool_name("read_file"), None);
        assert_eq!(parse_tool_name("mcp___x"), None);
    }

    #[test]
    fn validation_rejects_empty_or_huge() {
        let s = McpServer { name: "".into(), command: "node".into(), args: vec![], env: vec![] };
        assert!(s.validate().is_err());
        let s = McpServer { name: "a".into(), command: "".into(), args: vec![], env: vec![] };
        assert!(s.validate().is_err());
        let mut s = McpServer { name: "a".into(), command: "node".into(), args: (0..20).map(|i| i.to_string()).collect(), env: vec![] };
        assert!(s.validate().is_err());
        s.args = vec!["x".repeat(5000)];
        assert!(s.validate().is_err());
        s.args = vec!["ok".into()];
        assert!(s.validate().is_ok());
    }

    #[test]
    fn long_results_are_truncated_with_a_marker() {
        let s = "a".repeat(20_000);
        let t = truncate_chars(&s, 16_000);
        assert!(t.chars().count() < 16_200);
        assert!(t.contains("کوتاه شد"));
    }
}
