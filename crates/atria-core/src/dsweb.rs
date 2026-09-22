//! Unofficial **chat.deepseek.com** web client — signed-in `userToken` auth.
//!
//! This speaks the site's internal API (no paid API key):
//!   1. `POST /api/v0/chat_session/create`            → session id
//!   2. `POST /api/v0/chat/create_pow_challenge`      → DeepSeekHashV1 challenge
//!   3. solve PoW (`keccak23`) → `x-ds-pow-response` header
//!   4. `POST /api/v0/chat/completion`                → SSE stream
//!
//! The PoW hash is DeepSeek's non-standard `keccak23`: SHA3-style 0x06
//! padding over Keccak-f[1600] with **23** rounds (first round-constant
//! skipped), rate 136, 32-byte digest. Cross-checked against independent
//! reverses (pow.py reference + golden vectors in tests).
//!
//! Tool use is prompt-based (`<tool>{json}</tool>` blocks) since the web
//! API has no function-calling contract.
//!
//! ⚠ Unofficial — may break or get accounts limited. Use with care.

use crate::client::{extract_error, ClientConfig, CoreError, StreamEvent, Turn};
use crate::sse::SseDecoder;
use crate::types::{Block, Message, Role, Usage};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

const BASE: &str = "https://chat.deepseek.com";
const API: &str = "https://chat.deepseek.com/api/v0";
const COMPLETION_PATH: &str = "/api/v0/chat/completion";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/132.0.0.0 Safari/537.36";

/* ================= DeepSeekHashV1 (keccak23) ================= */

/// Keccak-f round constants RC[1..=23] (first constant skipped).
const RC: [u64; 23] = [
    0x0000_0000_0000_8082,
    0x8000_0000_0000_808A,
    0x8000_0000_0000_8000,
    0x0000_0000_0000_808B,
    0x8000_0000_0000_0001,
    0x8000_0000_0000_8081,
    0x8000_0000_0000_8009,
    0x0000_0000_0000_008A,
    0x0000_0000_0000_0088,
    0x0000_0000_8000_8009,
    0x0000_0000_8000_000A,
    0x0000_0000_8000_808B,
    0x8000_0000_0000_008B,
    0x8000_0000_0000_8089,
    0x8000_0000_0000_8003,
    0x8000_0000_0000_8002,
    0x8000_0000_0000_0080,
    0x0000_0000_0000_800A,
    0x8000_0000_8000_000A,
    0x8000_0000_8000_8081,
    0x8000_0000_0000_8080,
    0x0000_0000_8000_8001,
    0x8000_0000_8000_8008,
];

const ROT: [[u32; 5]; 5] = [
    [0, 36, 3, 41, 18],
    [1, 44, 10, 45, 2],
    [62, 6, 43, 15, 61],
    [28, 55, 25, 21, 56],
    [27, 20, 39, 8, 14],
];

const MASK: u64 = u64::MAX;

fn rotl(x: u64, n: u32) -> u64 {
    let n = n % 64;
    if n == 0 {
        x & MASK
    } else {
        ((x << n) | (x >> (64 - n))) & MASK
    }
}

fn keccak_f(s: &mut [u64; 25]) {
    let mut a = [[0u64; 5]; 5];
    for x in 0..5 {
        for y in 0..5 {
            a[x][y] = s[x + 5 * y];
        }
    }
    for &rc in RC.iter() {
        let mut c = [0u64; 5];
        for (x, cx) in c.iter_mut().enumerate() {
            *cx = a[x][0] ^ a[x][1] ^ a[x][2] ^ a[x][3] ^ a[x][4];
        }
        let mut d = [0u64; 5];
        for (x, dx) in d.iter_mut().enumerate() {
            *dx = c[(x + 4) % 5] ^ rotl(c[(x + 1) % 5], 1);
        }
        for x in 0..5 {
            let dx = d[x];
            for y in 0..5 {
                a[x][y] ^= dx;
            }
        }
        let mut b = [[0u64; 5]; 5];
        for x in 0..5 {
            for y in 0..5 {
                b[y][(2 * x + 3 * y) % 5] = rotl(a[x][y], ROT[x][y]);
            }
        }
        for x in 0..5 {
            for y in 0..5 {
                a[x][y] = b[x][y] ^ ((!b[(x + 1) % 5][y]) & b[(x + 2) % 5][y]);
            }
        }
        a[0][0] ^= rc;
    }
    for y in 0..5 {
        for x in 0..5 {
            s[x + 5 * y] = a[x][y];
        }
    }
}

/// DeepSeekHashV1: rate 136, pad 0x06, 23-round Keccak-f, 32-byte digest.
pub fn keccak23(data: &[u8]) -> [u8; 32] {
    const RATE: usize = 136;
    let mut buf = data.to_vec();
    buf.push(0x06);
    while buf.len() % RATE != 0 {
        buf.push(0);
    }
    let last = buf.len() - 1;
    buf[last] ^= 0x80;
    let mut state = [0u64; 25];
    for block in buf.chunks(RATE) {
        for i in 0..(RATE / 8) {
            let mut le = [0u8; 8];
            le.copy_from_slice(&block[i * 8..i * 8 + 8]);
            state[i] ^= u64::from_le_bytes(le);
        }
        keccak_f(&mut state);
    }
    let mut out = [0u8; 32];
    for i in 0..4 {
        out[i * 8..i * 8 + 8].copy_from_slice(&state[i].to_le_bytes());
    }
    out
}

/* ================= small utils ================= */

fn b64encode(data: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TBL[(n >> 18) as usize & 63] as char);
        out.push(TBL[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TBL[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TBL[n as usize & 63] as char } else { '=' });
    }
    out
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    Some(out)
}

/// Accept the raw `localStorage.userToken` JSON or the bare token value.
pub fn normalize_token(raw: &str) -> String {
    let t = raw.trim();
    if t.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(t) {
            if let Some(val) = v.get("value").and_then(Value::as_str) {
                return val.to_string();
            }
        }
    }
    t.trim_matches('"').to_string()
}

/* ================= PoW ================= */

/// Solve one DeepSeekHashV1 challenge (multi-threaded brute force) and
/// build the `x-ds-pow-response` header value.
pub fn solve_pow(challenge: &Value) -> Result<String, CoreError> {
    let c = if challenge.get("challenge").and_then(Value::as_str).is_some() {
        challenge.clone()
    } else {
        challenge.get("challenge").cloned().unwrap_or(Value::Null)
    };
    let fail = |m: &str| CoreError::Api { status: 0, message: m.to_string() };
    let algo = c.get("algorithm").and_then(Value::as_str).unwrap_or("DeepSeekHashV1");
    let chal_hex = c.get("challenge").and_then(Value::as_str).ok_or_else(|| fail("PoW: challenge missing"))?;
    let salt = c.get("salt").and_then(Value::as_str).ok_or_else(|| fail("PoW: salt missing"))?;
    let signature = c.get("signature").and_then(Value::as_str).unwrap_or("");
    let target_path = c.get("target_path").and_then(Value::as_str).unwrap_or(COMPLETION_PATH);
    let expire_at = c.get("expire_at").and_then(Value::as_i64).unwrap_or(0);
    let difficulty = c.get("difficulty").and_then(Value::as_i64).unwrap_or(0).max(0) as i64;
    let target = hex_decode(chal_hex).ok_or_else(|| fail("PoW: challenge is not hex"))?;
    let prefix = format!("{salt}_{expire_at}_");

    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    let found = AtomicI64::new(-1);
    let per = (difficulty + 1 + threads as i64 - 1) / threads as i64;
    std::thread::scope(|scope| {
        for t in 0..threads as i64 {
            let found = &found;
            let prefix = &prefix;
            let target = &target;
            let lo = t * per;
            let hi = (lo + per - 1).min(difficulty);
            scope.spawn(move || {
                for w in lo..=hi {
                    if found.load(Ordering::Relaxed) >= 0 {
                        return;
                    }
                    let mut buf = prefix.as_bytes().to_vec();
                    buf.extend_from_slice(w.to_string().as_bytes());
                    if keccak23(&buf) == target.as_slice() {
                        found.store(w, Ordering::Relaxed);
                        return;
                    }
                }
            });
        }
    });
    let answer = found.load(Ordering::Relaxed);
    if answer < 0 {
        return Err(fail("PoW: no solution found — دوباره تلاش کن"));
    }
    let payload = json!({
        "algorithm": algo,
        "challenge": chal_hex,
        "salt": salt,
        "answer": answer,
        "signature": signature,
        "target_path": target_path,
    });
    let bytes = serde_json::to_vec(&payload).map_err(|_| fail("PoW: encode"))?;
    Ok(b64encode(&bytes))
}

/* ================= prompt tools (agent via text protocol) ================= */

/// Describe the tool catalog as text instructions (web API has no tools).
pub fn tool_prompt(tools: &[Value], enabled: bool) -> String {
    if !enabled || tools.is_empty() {
        return String::new();
    }
    let mut s = String::from(
        "شما یک دستیار ایجنت هستید و به ابزارهای محلی زیر دسترسی دارید. وقتی لازم شد، \
         به‌جای توضیحِ کار، دقیقاً یک بلوک فراخوانی ابزار بنویس؛ هیچ متن دیگری با آن قاطی نکن. \
         پس از دریافت «نتیجهٔ ابزار»، ادامه بده تا به پاسخ نهایی برسی.\n\nابزارها:\n",
    );
    for t in tools {
        let name = t.get("name").and_then(Value::as_str).unwrap_or("");
        let desc = t.get("description").and_then(Value::as_str).unwrap_or("");
        s.push_str(&format!("- {name}: {desc}\n"));
    }
    s.push_str(
        "\nقالب دقیق فراخوانی (JSON فشرده، بدون توضیح اضافه):\n\
         <tool>{\"name\":\"نام_ابزار\",\"input\":{...}}</tool>\n\
         برای چند ابزار پشت سر هم، چند بلوک جدا بنویس.\n",
    );
    s
}

/// Split tool calls out of the visible text (robust: DSML + `<tool>` + quoting).
/// Returns (clean_text, [(name, input)]).
pub fn split_tool_calls(text: &str) -> (String, Vec<(String, Value)>) {
    crate::toolfmt::parse_tool_markup(text)
}

/// Flatten history into DeepSeek's `role: text` prompt shape.
pub fn flatten_prompt(cfg: &ClientConfig, messages: &[Message], tools: &[Value]) -> String {
    let mut out = String::new();
    let mut head = String::new();
    if !cfg.system.trim().is_empty() {
        head.push_str(cfg.system.trim());
        head.push_str("\n\n");
    }
    head.push_str(&tool_prompt(tools, cfg.tools || cfg.file_tools));
    if !head.is_empty() {
        out.push_str(head.trim_end());
        out.push_str("\n\n");
    }
    for m in messages {
        let role = match m.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        let mut parts: Vec<String> = Vec::new();
        for b in &m.content {
            if let Block::Image { .. } = b {
                parts.push("[تصویر پیوست]".to_string());
                continue;
            }
            match b {
                Block::Image { .. } => {}
                Block::Text { text } => {
                    if !text.trim().is_empty() {
                        parts.push(text.clone());
                    }
                }
                Block::ToolUse { name, input, .. } => {
                    parts.push(format!("[ابزار {name} با ورودی {input} اجرا شد]"));
                }
                Block::ToolResult { content, is_error, .. } => {
                    let c = content.clone().unwrap_or_default();
                    let tag = if is_error.unwrap_or(false) { "خطا" } else { "نتیجهٔ ابزار" };
                    parts.push(format!("[{tag}: {}]", c.chars().take(6000).collect::<String>()));
                }
                Block::Thinking { .. } => {}
            }
        }
        let body = parts.join("\n");
        if !body.trim().is_empty() {
            out.push_str(&format!("{role}: {body}\n"));
        }
    }
    out.trim_end().to_string()
}

/* ================= SSE payload parser ================= */

#[derive(Debug, Clone, PartialEq)]
enum Chunk {
    Think(String),
    Text(String),
}

#[derive(Debug, Default)]
struct DsFeed {
    active_path: String,
    seeded: bool,
}

impl DsFeed {
    fn push_fragment(&self, frag: &Value, out: &mut Vec<Chunk>, seed: bool) {
        let c = frag.get("content").and_then(Value::as_str).unwrap_or("");
        if c.is_empty() {
            return;
        }
        match frag.get("type").and_then(Value::as_str) {
            Some("THINK") => out.push(Chunk::Think(c.to_string())),
            Some("RESPONSE") | Some("TEMPLATE_RESPONSE") | None => {
                if seed {
                    if !self.seeded {
                        out.push(Chunk::Text(c.to_string()));
                    }
                } else {
                    out.push(Chunk::Text(c.to_string()));
                }
            }
            _ => {} // TOOL_SEARCH / TOOL_OPEN / TIP metadata — not visible text
        }
    }

    fn apply_item(&mut self, item: &Value, out: &mut Vec<Chunk>) -> Result<(), CoreError> {
        let v = item.get("v");
        let p_owned = item.get("p").and_then(Value::as_str).unwrap_or("").to_string();
        let o = item.get("o").and_then(Value::as_str).unwrap_or("");
        let p: &str = if p_owned.is_empty() { &self.active_path } else { &p_owned };

        if let Some(vv) = v {
            if vv.get("response").is_some() {
                self.capture_snapshot(vv, out);
                return Ok(());
            }
        }
        if let Some(s) = v.and_then(Value::as_str) {
            if !s.is_empty() {
                if p.contains("thinking") {
                    out.push(Chunk::Think(s.to_string()));
                } else if p.ends_with("content") {
                    out.push(Chunk::Text(s.to_string()));
                } else if p.ends_with("message_id") {
                    // captured implicitly — ids unused (fresh session per turn)
                }
            }
            return Ok(());
        }
        if !p_owned.is_empty() {
            self.active_path = p_owned;
        }
        let _ = o;
        Ok(())
    }

    fn capture_snapshot(&mut self, snap: &Value, out: &mut Vec<Chunk>) {
        let resp = &snap["response"];
        let frags = resp
            .get("fragments")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for frag in frags {
            self.push_fragment(frag, out, true);
        }
    }

    fn feed_frame(&mut self, obj: &Value, out: &mut Vec<Chunk>) -> Result<(), CoreError> {
        if obj.get("type").and_then(Value::as_str) == Some("error") {
            let content = obj.get("content").and_then(Value::as_str).unwrap_or("");
            let fr = obj.get("finish_reason").and_then(Value::as_str).unwrap_or("");
            let msg = if fr == "rate_limit_reached" {
                format!("محدودیت نرخ سایت دیپ‌سیک — کمی بعد تلاش کن ({content})")
            } else {
                format!("خطای سایت دیپ‌سیک: {content}")
            };
            return Err(CoreError::Api { status: 429, message: msg });
        }
        let v = obj.get("v");
        let p = obj.get("p").and_then(Value::as_str).unwrap_or("");
        let o = obj.get("o").and_then(Value::as_str).unwrap_or("");

        // BATCH patch collection
        if o == "BATCH" {
            if let Some(Value::Array(items)) = v {
                for item in items {
                    let mut it = item.clone();
                    if it.get("p").is_none() && !p.is_empty() {
                        if let Some(obj) = it.as_object_mut() {
                            obj.insert("p".into(), Value::String(p.to_string()));
                        }
                    }
                    self.apply_item(&it, out)?;
                }
                return Ok(());
            }
        }
        // pathless array: fragment batch or patch batch
        if p.is_empty() {
            if let Some(Value::Array(items)) = v {
                for item in items {
                    if item.get("type").is_some() && item.get("content").is_some() {
                        self.push_fragment(item, out, false);
                    } else if item.get("p").is_some() {
                        self.apply_item(item, out)?;
                    }
                }
                return Ok(());
            }
        }
        // initial snapshot envelope
        if let Some(vv) = v {
            if vv.get("response").is_some() {
                self.capture_snapshot(vv, out);
                self.seeded = true;
                return Ok(());
            }
        }
        if !p.is_empty() {
            self.active_path = p.to_string();
            if o == "APPEND" {
                if let Some(s) = v.and_then(Value::as_str) {
                    if !s.is_empty() {
                        if p.contains("thinking") {
                            out.push(Chunk::Think(s.to_string()));
                        } else if p.ends_with("content") {
                            out.push(Chunk::Text(s.to_string()));
                        }
                    }
                }
                if let Some(Value::Array(items)) = v {
                    if p.ends_with("fragments") {
                        for frag in items {
                            self.push_fragment(frag, out, false);
                        }
                    }
                }
            }
            return Ok(());
        }
        // bare token on the active path
        if let Some(s) = v.and_then(Value::as_str) {
            if !s.is_empty() && !self.active_path.is_empty() {
                if self.active_path.contains("thinking") {
                    out.push(Chunk::Think(s.to_string()));
                } else if self.active_path.ends_with("content")
                    && self.active_path.starts_with("response/fragments")
                {
                    out.push(Chunk::Text(s.to_string()));
                }
            }
        }
        Ok(())
    }
}

/* ================= HTTP client ================= */

/// HTTP client for the chat.deepseek.com web API.
pub struct DsWebClient {
    http: reqwest::Client,
}

impl Default for DsWebClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DsWebClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(12))
            .read_timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("build http client");
        Self { http }
    }

    fn base_headers(token: &str) -> reqwest::header::HeaderMap {
        use reqwest::header::*;
        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {token}")).unwrap_or(HeaderValue::from_static("")));
        h.insert(ACCEPT, HeaderValue::from_static("*/*"));
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h.insert(USER_AGENT, HeaderValue::from_static(UA));
        h.insert("origin", HeaderValue::from_static(BASE));
        h.insert("referer", HeaderValue::from_static("https://chat.deepseek.com/"));
        h.insert("x-app-version", HeaderValue::from_static("2.0.0"));
        h.insert("x-client-version", HeaderValue::from_static("2.0.0"));
        h.insert("x-client-platform", HeaderValue::from_static("web"));
        h.insert("x-client-locale", HeaderValue::from_static("en_US"));
        h.insert("x-client-bundle-id", HeaderValue::from_static("com.deepseek.chat"));
        h
    }

    async fn post_json(
        &self,
        path: &str,
        body: &Value,
        headers: &reqwest::header::HeaderMap,
        url_hint: &str,
    ) -> Result<Value, CoreError> {
        let resp = self
            .http
            .post(format!("{API}{path}"))
            .headers(headers.clone())
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        if status == 401 {
            return Err(CoreError::Api {
                status: 401,
                message: format!("یوزر توکن نامعتبر یا منقضی شده — دوباره از chat.deepseek.com بگیر [{url_hint}]"),
            });
        }
        if status == 429 {
            return Err(CoreError::Api {
                status: 429,
                message: format!("محدودیت نرخ دیپ‌سیک [{url_hint}]"),
            });
        }
        if text.contains("Just a moment") && text.contains("<!DOCTYPE html>") {
            return Err(CoreError::Api {
                status: 403,
                message: format!("صفحهٔ بررسی انسانی Cloudflare — دوباره تلاش کن [{url_hint}]"),
            });
        }
        if status != 200 {
            return Err(CoreError::Api {
                status,
                message: format!("{} [{url_hint}]", extract_error(&text)),
            });
        }
        let v: Value = serde_json::from_str(&text)
            .map_err(|_| CoreError::Api { status, message: format!("پاسخ نامعتبر [{url_hint}]") })?;
        if v.get("code").and_then(Value::as_i64) != Some(0) {
            let msg = v.get("msg").and_then(Value::as_str).unwrap_or("خطای ناشناخته");
            return Err(CoreError::Api {
                status,
                message: format!("دیپ‌سیک: {msg} [{url_hint}]"),
            });
        }
        Ok(v)
    }

    /// Create a fresh chat session on the site.
    async fn create_session(&self, headers: &reqwest::header::HeaderMap) -> Result<Value, CoreError> {
        let sess = self
            .post_json("/chat_session/create", &json!({}), headers, "chat_session/create")
            .await?;
        let sid = sess
            .pointer("/data/biz_data/chat_session/id")
            .or_else(|| sess.pointer("/data/biz_data/id"))
            .cloned()
            .unwrap_or(Value::Null);
        if sid.is_null() {
            return Err(CoreError::Api {
                status: 0,
                message: format!("شناسهٔ نشست برنگشت [{BASE}/api/v0/chat_session/create]"),
            });
        }
        Ok(sid)
    }

    /// One agentic turn against the web API (session reused across turns so the
    /// chat stays alive on chat.deepseek.com; full history folded into the prompt).
    pub async fn send(
        &self,
        cfg: &ClientConfig,
        messages: &[Message],
        tools: &[Value],
        mut on_event: impl FnMut(StreamEvent),
        stop: Arc<AtomicBool>,
    ) -> Result<Turn, CoreError> {
        let token = normalize_token(&cfg.api_key);
        if token.is_empty() {
            return Err(CoreError::Api {
                status: 401,
                message: "یوزر توکن دیپ‌سیک را وارد کن (از localStorage سایت)".into(),
            });
        }
        let headers = Self::base_headers(&token);

        // 1..4) session (reused when provided) + PoW + completion — one
        // fresh-session retry if a reused session is rejected.
        let mut last_prompt_len = 0usize;
        let mut sid = Value::String(cfg.web_session.trim().to_string());
        let mut allow_retry = !cfg.web_session.trim().is_empty();
        let resp = loop {
            if sid.as_str().unwrap_or("").is_empty() {
                sid = self.create_session(&headers).await?;
                allow_retry = false;
            }
            if let Some(out) = &cfg.web_session_out {
                if let Ok(mut g) = out.lock() {
                    *g = sid.as_str().unwrap_or("").to_string();
                }
            }

            // PoW challenge + solve (CPU-bound → blocking pool)
            let chal_resp = self
                .post_json(
                    "/chat/create_pow_challenge",
                    &json!({ "target_path": COMPLETION_PATH }),
                    &headers,
                    "chat/create_pow_challenge",
                )
                .await?;
            let challenge = chal_resp
                .pointer("/data/biz_data/challenge")
                .or_else(|| chal_resp.pointer("/data/biz_data"))
                .cloned()
                .unwrap_or(Value::Null);
            if challenge.is_null() {
                return Err(CoreError::Api {
                    status: 0,
                    message: format!("چالش PoW برنگشت [{BASE}/api/v0/chat/create_pow_challenge]"),
                });
            }
            let pow_header = tokio::task::spawn_blocking(move || solve_pow(&challenge))
                .await
                .map_err(|e| CoreError::Api { status: 0, message: e.to_string() })??;

            // prompt + body
            let prompt = flatten_prompt(cfg, messages, tools);
            last_prompt_len = prompt.len();
            let model_type = match cfg.model.trim().to_ascii_lowercase().as_str() {
                "deepseek-expert" | "expert" | "deepseek-v4-pro" | "pro" => "expert",
                "deepseek-vision" | "vision" => "vision",
                _ => "default",
            };
            let body = json!({
                "chat_session_id": sid,
                "parent_message_id": Value::Null,
                "prompt": prompt,
                "ref_file_ids": [],
                "thinking_enabled": cfg.web_thinking,
                "search_enabled": cfg.web_search,
                "action": Value::Null,
                "preempt": false,
                "model_type": model_type,
            });

            // stream completion
            let mut stream_headers = headers.clone();
            stream_headers.insert(
                "x-ds-pow-response",
                reqwest::header::HeaderValue::from_str(&pow_header)
                    .map_err(|_| CoreError::Api { status: 0, message: "PoW header".into() })?,
            );
            let url = format!("{API}{COMPLETION_PATH}");
            let resp = self.http.post(&url).headers(stream_headers).json(&body).send().await?;
            if resp.status().is_success() {
                break resp;
            }
            let text = resp.text().await.unwrap_or_default();
            if allow_retry {
                allow_retry = false;
                sid = Value::Null;
                continue;
            }
            return Err(CoreError::Api {
                status: 500,
                message: format!("{} [{url}]", extract_error(&text)),
            });
        };
        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut dec = SseDecoder::new();
        let mut feed = DsFeed::default();
        let mut text = String::new();
        let mut thinking = String::new();
        let mut tool_seq = 0usize;
        let mut pending_tool: Option<String> = None;

        while let Some(chunk) = stream.next().await {
            if stop.load(Ordering::Relaxed) {
                return Err(CoreError::Stopped);
            }
            let chunk = chunk?;
            for ev in dec.feed(&chunk) {
                if ev.name != "message" {
                    continue; // ready / update_session / finish / title / close
                }
                let mut out = Vec::new();
                feed.feed_frame(&ev.data, &mut out)?;
                for c in out {
                    match c {
                        Chunk::Think(s) => {
                            thinking.push_str(&s);
                            on_event(StreamEvent::Thinking(s));
                        }
                        Chunk::Text(s) => {
                            text.push_str(&s);
                            on_event(StreamEvent::Text(s.clone()));
                            // early tool card: once <tool> opens
                            if pending_tool.is_none() {
                                if let Some(pos) = text.rfind("<tool>") {
                                    if !text[pos..].contains("</tool>") {
                                        let id = format!("call_ds_{tool_seq}");
                                        pending_tool = Some(id.clone());
                                        on_event(StreamEvent::ToolStart { id, name: String::new() });
                                    }
                                }
                            } else {
                                let id = pending_tool.clone().unwrap_or_default();
                                on_event(StreamEvent::ToolArgs { id, n: s.len() });
                                if text.contains("</tool>") {
                                    tool_seq += 1;
                                    pending_tool = None;
                                }
                            }
                        }
                    }
                }
            }
        }
        if stop.load(Ordering::Relaxed) {
            return Err(CoreError::Stopped);
        }

        // 5) split tool calls out of the visible text
        let (clean, calls) = split_tool_calls(&text);
        let mut blocks: Vec<Block> = Vec::new();
        if !clean.is_empty() {
            blocks.push(Block::Text { text: clean });
        }
        let has_tools = !calls.is_empty();
        for (i, (name, input)) in calls.into_iter().enumerate() {
            blocks.push(Block::ToolUse { id: format!("call_ds_{i}"), name, input });
        }
        if blocks.is_empty() {
            let fallback = if thinking.trim().is_empty() {
                "(پاسخ خالی)".to_string()
            } else {
                // thinking-only reply (e.g. content filtered) — surface something
                thinking.chars().take(2000).collect()
            };
            blocks.push(Block::Text { text: fallback });
        }

        let usage = Usage {
            input_tokens: (last_prompt_len / 4) as u32,
            output_tokens: ((text.len() + thinking.len()) / 4) as u32,
        };
        let _ = thinking; // surfaced live; not echoed back (text protocol)
        Ok(Turn {
            message: Message { role: Role::Assistant, content: blocks },
            stop_reason: if has_tools { "tool_use".to_string() } else { "end_turn".to_string() },
            usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> String {
        keccak23(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn keccak23_golden_vectors() {
        // generated from the reference pure-Python implementation
        assert_eq!(hex(""), "3c64f696a71a1f3ae815ae7c73a2c87570f24e91cb10d3b3f7025760371853a4");
        assert_eq!(hex("abc"), "8a977ec7439f151f3347ca651ced5553add7b0a1513017bfe4c69f2862762b9d");
        assert_eq!(
            hex("salt_1730000000_5"),
            "e5faaad70c836199f9d6ab593dafdfd59b9368cb57f1762b41252174e78817ba"
        );
        assert_eq!(
            hex("salt_1730000000_12345"),
            "d94cf187a55377756962eacc2acffd9bf12262c84768987059077b3ad00869a6"
        );
    }

    #[test]
    fn solve_pow_roundtrip() {
        let target = hex("mysalt_1700000000_42");
        let chal = json!({
            "algorithm": "DeepSeekHashV1",
            "challenge": target,
            "salt": "mysalt",
            "difficulty": 200,
            "signature": "sig",
            "target_path": "/api/v0/chat/completion",
            "expire_at": 1700000000i64
        });
        let header = solve_pow(&chal).expect("solve");
        // decode base64 manually to check the answer field
        fn b64decode(s: &str) -> Vec<u8> {
            const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let idx = |c: u8| TBL.iter().position(|&t| t == c).unwrap_or(0) as u32;
            let raw: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
            let mut out = Vec::new();
            for ch in raw.chunks(4) {
                let n = (idx(ch[0]) << 18) | (idx(ch[1]) << 12) | (idx(*ch.get(2).unwrap_or(&b'A')) << 6) | idx(*ch.get(3).unwrap_or(&b'A'));
                out.push((n >> 16) as u8);
                if ch.len() > 2 { out.push((n >> 8) as u8); }
                if ch.len() > 3 { out.push(n as u8); }
            }
            out
        }
        let v: Value = serde_json::from_slice(&b64decode(&header)).expect("json");
        assert_eq!(v["answer"], 42);
        assert_eq!(v["algorithm"], "DeepSeekHashV1");
    }

    #[test]
    fn token_normalizes_both_shapes() {
        assert_eq!(normalize_token("  abc.def.ghi  "), "abc.def.ghi");
        assert_eq!(
            normalize_token(r#"{"value":"tok123","ttl":86400}"#),
            "tok123"
        );
    }

    #[test]
    fn splits_tool_calls_and_keeps_text() {
        let (txt, calls) = split_tool_calls(
            "خب بریم فایل بسازم <tool>{\"name\":\"write_file\",\"input\":{\"path\":\"a.txt\",\"content\":\"hi\"}}</tool> تمام شد",
        );
        assert!(txt.contains("خب بریم"));
        assert!(txt.contains("تمام شد"));
        assert!(!txt.contains("<tool>"));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "write_file");
        assert_eq!(calls[0].1["path"], "a.txt");
    }

    #[test]
    fn sse_frames_route_think_and_text() {
        let mut f = DsFeed::default();
        let mut out = Vec::new();
        // snapshot with THINK + RESPONSE
        f.feed_frame(
            &json!({"v":{"response":{"fragments":[
                {"id":1,"type":"THINK","content":"دارم فکر می‌کنم"},
                {"id":2,"type":"RESPONSE","content":"سلام"}]}}}),
            &mut out,
        )
        .unwrap();
        assert_eq!(out, vec![Chunk::Think("دارم فکر می‌کنم".into()), Chunk::Text("سلام".into())]);
        out.clear();
        // patch appends
        f.feed_frame(
            &json!({"p":"response/fragments/-1/content","o":"APPEND","v":" عزیز"}),
            &mut out,
        )
        .unwrap();
        f.feed_frame(&json!({"v":"!"}), &mut out).unwrap();
        assert_eq!(out, vec![Chunk::Text(" عزیز".into()), Chunk::Text("!".into())]);
        out.clear();
        // fragment container append
        f.feed_frame(
            &json!({"p":"response/fragments","o":"APPEND","v":[
                {"id":3,"type":"RESPONSE","content":"ادامه"}]}),
            &mut out,
        )
        .unwrap();
        assert_eq!(out, vec![Chunk::Text("ادامه".into())]);
    }

    #[test]
    fn error_frames_become_api_errors() {
        let mut f = DsFeed::default();
        let mut out = Vec::new();
        let err = f
            .feed_frame(
                &json!({"type":"error","finish_reason":"rate_limit_reached","content":"slow down"}),
                &mut out,
            )
            .unwrap_err();
        match err {
            CoreError::Api { status, message } => {
                assert_eq!(status, 429);
                assert!(message.contains("محدودیت نرخ"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
