//! Robust extraction of text-embedded tool calls.
//!
//! DeepSeek-family models (web + DSML harnesses) emit tool calls as *text*:
//! `<tool>{'name': ..., 'arguments': {...}, '_nonce': ...}</tool>` with Python
//! quoting, plus DSML markup families (`<｜DSML｜invoke name="...">`, the
//! double-pipe `<｜｜DSML｜｜ calls>` variant and the ASCII `| DSML | parameter |`
//! debris seen from gateways). All are normalized into `(clean_text, calls)`.

use crate::types::{Block, Message};
use serde_json::{json, Value};

/// Full cleanup: strip every tool-markup family, return (clean, [(name, input)]).
pub fn parse_tool_markup(text: &str) -> (String, Vec<(String, Value)>) {
    let mut calls: Vec<(String, Value)> = Vec::new();

    // Family A: <tool>…</tool> (also <tool_call>) with lenient JSON bodies.
    let mut clean = String::new();
    let mut s = text;
    loop {
        let start = match s.find("<tool") {
            Some(p) => p,
            None => break,
        };
        clean.push_str(&s[..start]);
        let after = &s[start..];
        let open_end = match after.find('>') {
            Some(p) => p + 1,
            None => break,
        };
        let body_all = &after[open_end..];
        let end = body_all.find("</tool>").unwrap_or(body_all.len());
        let body = &body_all[..end];
        let consumed = open_end + end + if end < body_all.len() { "</tool>".len() } else { 0 };
        match parse_tool_body(body) {
            Some(call) => {
                calls.push(call);
                let tail_all = &after[consumed.min(after.len())..];
                let noise_end = noise_len(tail_all);
                s = &tail_all[noise_end..];
            }
            None => {
                let keep_to = consumed.min(after.len());
                clean.push_str(&after[..keep_to]);
                s = &after[keep_to..];
            }
        }
    }
    clean.push_str(s);

    // Family B: DSML invoke grammar (single-, double- and ASCII-pipe dialects).
    let clean = extract_dsml(&clean, &mut calls);

    // Family C: leftover debris lines.
    let clean = strip_noise(&clean);
    let clean = clean.trim().to_string();

    // de-dupe identical (name,input) pairs produced by duplicated markers
    let mut uniq: Vec<(String, Value)> = Vec::new();
    for c in calls {
        if !uniq.iter().any(|u| u.0 == c.0 && u.1 == c.1) {
            uniq.push(c);
        }
    }
    (clean, uniq)
}

/// Length of trailing pipe-noise after a tool block
/// (e.g. ` | | | DSML | | | parameter | invoke | | | | | | | </tool>`).
fn noise_len(s: &str) -> usize {
    let t = s.trim_start();
    let lead = s.len() - t.len();
    let core = t.trim_start_matches(|c| c == '|' || c == '｜' || c == ' ' || c == '\t' || c == '\r');
    let looks_noise = core.starts_with("DSML")
        || core.starts_with("parameter")
        || core.starts_with("invoke")
        || core.starts_with("calls>")
        || core.starts_with("tool_calls");
    if looks_noise {
        let line_end = t.find('\n').map(|p| p + 1).unwrap_or(t.len());
        let mut n = lead + line_end;
        let after = &s[n..];
        let trimmed = after.trim_start();
        for closer in ["</tool>", "tool_call>"] {
            if trimmed.starts_with(closer) {
                n += after.len() - trimmed.len() + closer.len();
                break;
            }
        }
        return n;
    }
    if t.starts_with("</tool>") {
        return lead + "</tool>".len();
    }
    0
}

/// Drop standalone DSML/pipe debris lines (Family C).
fn strip_noise(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for line in s.split_inclusive('\n') {
        let pipes = line.matches('|').count() + line.matches("｜").count();
        let core = line.trim().trim_matches(|c| c == '|' || c == '｜').trim();
        let is_debris = (pipes >= 2
            && (core.contains("DSML")
                || core.contains("parameter name")
                || core.contains("invoke name")
                || core.contains("tool_calls")
                || core.contains("calls>")))
            || core == "</tool>"
            || core == "tool_call>";
        if is_debris {
            continue;
        }
        out.push_str(line);
    }
    out
}

/// Extract DSML `invoke name="x"` + `parameter name="p" string="b">value` blocks.
fn extract_dsml(s: &str, calls: &mut Vec<(String, Value)>) -> String {
    let mut clean = String::new();
    let mut rest = s;
    loop {
        let mp = match rest.find("invoke name=\"") {
            Some(p) => p,
            None => break,
        };
        // prose before the markup wrapper is kept; wrapper junk is dropped
        let before = &rest[..mp];
        let mut cut = mp;
        for (i, _) in before.match_indices('<') {
            let tail = &before[i..];
            if tail.contains("DSML") || tail.contains("calls") || tail.contains("invoke") {
                cut = i;
            }
        }
        let keep = strip_noise(&before[..cut]);
        let keep = keep.trim();
        if !keep.is_empty() {
            if !clean.is_empty() {
                clean.push('\n');
            }
            clean.push_str(keep);
        }
        let after = &rest[mp + "invoke name=\"".len()..];
        let name: String = after.split('"').next().unwrap_or("").to_string();
        let mut region = &after[name.len() + 1..];
        region = match region.find('>') {
            Some(p) => &region[p + 1..],
            None => region,
        };
        let mut params: Vec<(String, String, bool)> = Vec::new();
        loop {
            // parameters may be prefixed by a DSML wrapper token
            let pp = match region.find("parameter name=\"") {
                Some(p) => p,
                None => break,
            };
            if region[..pp].contains("invoke") {
                break;
            }
            region = &region[pp..];
            let pa = &region["parameter name=\"".len()..];
            let pname: String = pa.split('"').next().unwrap_or("").to_string();
            let r2 = &pa[pname.len() + 1..];
            let is_string = r2.contains("string=\"true\"");
            let r2v = match r2.find('>') {
                Some(p) => &r2[p + 1..],
                None => break,
            };
            let value = r2v.split('\n').next().unwrap_or("").trim_end_matches('\r');
            params.push((pname.clone(), value.to_string(), is_string));
            let consumed = "parameter name=\"".len()
                + pname.len()
                + 1
                + (r2.len() - r2v.len())
                + r2v.split('\n').next().unwrap_or("").len();
            region = &region[consumed.min(region.len())..];
        }
        if !name.is_empty() {
            calls.push((name, params_to_input(&params)));
        }
        rest = region;
    }
    clean.push_str(rest);
    strip_noise(&clean)
}

/// Merge DSML parameters into one input object (envelopes unwrapped).
fn params_to_input(params: &[(String, String, bool)]) -> Value {
    let mut obj = serde_json::Map::new();
    for (k, v, is_string) in params {
        if matches!(k.as_str(), "arguments" | "input" | "parameters" | "args") {
            if let Some(val) = lenient_json(v) {
                if let Some(o) = val.as_object() {
                    for (ik, iv) in o {
                        obj.insert(ik.clone(), iv.clone());
                    }
                    continue;
                }
            }
        }
        let val = if *is_string {
            Value::String(v.clone())
        } else {
            lenient_json(v).unwrap_or_else(|| Value::String(v.clone()))
        };
        obj.insert(k.clone(), val);
    }
    Value::Object(obj)
}

/// Lenient body parser for `<tool>…</tool>` payloads.
fn parse_tool_body(body: &str) -> Option<(String, Value)> {
    let b = body.trim();
    if b.is_empty() {
        return None;
    }
    if let Some(v) = lenient_json(b) {
        return from_envelope(&v);
    }
    let name = find_str_field(b, "name")?;
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/')
    {
        return None;
    }
    let mut input = Value::Object(serde_json::Map::new());
    for key in ["input", "arguments", "parameters", "args"] {
        if let Some(obj) = find_obj_field(b, key) {
            input = obj;
            break;
        }
    }
    Some((name, input))
}

fn from_envelope(v: &Value) -> Option<(String, Value)> {
    let name = v.get("name").and_then(Value::as_str)?.to_string();
    if name.is_empty() {
        return None;
    }
    let mut input = v
        .get("input")
        .or_else(|| v.get("arguments"))
        .or_else(|| v.get("parameters"))
        .or_else(|| v.get("args"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    if let Some(o) = input.as_object_mut() {
        o.remove("_nonce");
        o.remove("nonce");
    }
    Some((name, input))
}

/// JSON parse with Python-style single-quote repair and control-char escaping.
fn lenient_json(s: &str) -> Option<Value> {
    let t = s.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Some(v);
    }
    let mut repaired = String::with_capacity(t.len() + 16);
    let mut in_str = false;
    let mut quote = '"';
    let mut prev = ' ';
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            if c == quote && prev != '\\' {
                in_str = false;
                repaired.push('"');
            } else if c == '"' && quote == '\'' {
                repaired.push_str("\\\"");
            } else if c == '\n' {
                repaired.push_str("\\n");
            } else if c == '\r' {
                repaired.push_str("\\r");
            } else if c == '\t' {
                repaired.push_str("\\t");
            } else if c == '\\' {
                let next = chars.get(i + 1).copied().unwrap_or(' ');
                if matches!(next, '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' | 'u') {
                    repaired.push('\\');
                } else {
                    repaired.push_str("\\\\");
                }
            } else {
                repaired.push(c);
            }
        } else if c == '\'' || c == '"' {
            in_str = true;
            quote = c;
            repaired.push('"');
        } else {
            repaired.push(c);
        }
        prev = c;
        i += 1;
    }
    serde_json::from_str::<Value>(&repaired).ok()
}

fn find_str_field(s: &str, key: &str) -> Option<String> {
    for pat in [format!("\"{key}\""), format!("'{key}'")] {
        if let Some(p) = s.find(&pat) {
            let after = &s[p + pat.len()..];
            let after = after.trim_start();
            let after = after.strip_prefix(':')?.trim_start();
            let q = after.chars().next()?;
            if q == '"' || q == '\'' {
                let inner = &after[1..];
                let end = inner.find(q)?;
                return Some(inner[..end].to_string());
            }
        }
    }
    None
}

fn find_obj_field(s: &str, key: &str) -> Option<Value> {
    for pat in [format!("\"{key}\""), format!("'{key}'")] {
        if let Some(p) = s.find(&pat) {
            let after = &s[p + pat.len()..].trim_start().strip_prefix(':')?.trim_start();
            if !after.starts_with('{') {
                continue;
            }
            let mut depth = 0usize;
            let mut in_str = false;
            let mut quote = '"';
            let mut prev = ' ';
            for (i, c) in after.char_indices() {
                if in_str {
                    if c == quote && prev != '\\' {
                        in_str = false;
                    }
                } else if c == '"' || c == '\'' {
                    in_str = true;
                    quote = c;
                } else if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        if let Some(v) = lenient_json(&after[..=i]) {
                            return Some(v);
                        }
                        break;
                    }
                }
                prev = c;
            }
        }
    }
    None
}

/// Rewrite a message: pull text-embedded tool calls into real ToolUse blocks.
pub fn extract_tool_calls(msg: &mut Message) {
    let mut collected: Vec<(String, Value)> = Vec::new();
    let mut new_blocks: Vec<Block> = Vec::new();
    for b in msg.content.drain(..) {
        match b {
            Block::Text { text } => {
                let (clean, calls) = parse_tool_markup(&text);
                collected.extend(calls);
                if !clean.is_empty() {
                    new_blocks.push(Block::Text { text: clean });
                }
            }
            other => new_blocks.push(other),
        }
    }
    for (i, (name, input)) in collected.into_iter().enumerate() {
        new_blocks.push(Block::ToolUse { id: format!("call_tfm_{i}"), name, input });
    }
    msg.content = new_blocks;
}

/// Live stream filter: hides tool markup from the visible text as it streams.
#[derive(Default)]
pub struct LiveFilter {
    holding: bool,
    buf: String,
}

impl LiveFilter {
    pub fn feed(&mut self, delta: &str) -> Vec<String> {
        self.buf.push_str(delta);
        let mut out = Vec::new();
        loop {
            if !self.holding {
                let markers = ["<tool", "DSML", "<｜"];
                let mut at: Option<usize> = None;
                for m in markers {
                    if let Some(p) = self.buf.find(m) {
                        at = Some(match at {
                            Some(a) if a < p => a,
                            _ => p,
                        });
                    }
                }
                match at {
                    Some(0) => {
                        self.holding = true;
                    }
                    Some(p) => {
                        out.push(self.buf[..p].to_string());
                        self.buf = self.buf[p..].to_string();
                        self.holding = true;
                    }
                    None => {
                        let keep = 8.min(self.buf.len());
                        let mut release = self.buf.len() - keep;
                        // never split a multibyte char (Persian text!)
                        while release > 0 && !self.buf.is_char_boundary(release) {
                            release -= 1;
                        }
                        if release > 0 {
                            out.push(self.buf[..release].to_string());
                            self.buf = self.buf[release..].to_string();
                        }
                        break;
                    }
                }
            } else {
                let ends = ["</tool>", "tool_call>", "end▁of", "calls>"];
                let mut done = false;
                for e in ends {
                    if self.buf.contains(e) {
                        done = true;
                        break;
                    }
                }
                if self.buf.matches('\n').count() >= 2 && self.buf.contains("invoke") {
                    done = true;
                }
                if done {
                    let (clean, _calls) = parse_tool_markup(&self.buf);
                    let clean = clean.trim().to_string();
                    if !clean.is_empty() {
                        out.push(clean);
                    }
                    self.buf.clear();
                    self.holding = false;
                } else {
                    break;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_tool_with_quoted_json_and_dsml_debris() {
        let t = "<tool>{'name': 'list_files', 'arguments': {'path': 'NewFolder'}, '_nonce': 'u1w0kedo'}</tool> | | | DSML | | | parameter | invoke | | | | | | | </tool>";
        let (clean, calls) = parse_tool_markup(t);
        assert!(clean.trim().is_empty(), "clean was: {clean:?}");
        assert_eq!(calls.len(), 1, "calls: {calls:?}");
        assert_eq!(calls[0].0, "list_files");
        assert_eq!(calls[0].1["path"], "NewFolder");
        assert!(calls[0].1.get("_nonce").is_none());
    }

    #[test]
    fn dsml_single_pipe_unicode() {
        let t = "قبل\n<｜DSML｜tool_calls>\n<｜DSML｜invoke name=\"write\">\n<｜DSML｜parameter name=\"file_path\" string=\"true\">C:\\a\\b.txt\n<｜DSML｜parameter name=\"content\" string=\"true\">سلام دنیا\n\n";
        let (clean, calls) = parse_tool_markup(t);
        assert!(clean.contains("قبل"));
        assert!(!clean.contains("DSML"), "clean: {clean:?}");
        assert_eq!(calls.len(), 1, "calls: {calls:?}");
        assert_eq!(calls[0].0, "write");
        assert_eq!(calls[0].1["file_path"], "C:\\a\\b.txt");
        assert_eq!(calls[0].1["content"], "سلام دنیا");
    }

    #[test]
    fn dsml_double_pipe_with_json_param() {
        let t = "<｜｜DSML｜｜ calls>\n<｜｜DSML｜｜ invoke name=\"calc\">\n<｜｜DSML｜｜ parameter name=\"expression\" string=\"true\">2+2\n<｜｜DSML｜｜ parameter name=\"count\" string=\"false\">5";
        let (_clean, calls) = parse_tool_markup(t);
        assert_eq!(calls.len(), 1, "calls: {calls:?}");
        assert_eq!(calls[0].0, "calc");
        assert_eq!(calls[0].1["expression"], "2+2");
        assert_eq!(calls[0].1["count"], 5);
    }

    #[test]
    fn strict_json_tool_still_works() {
        let t = "خب:\n<tool>{\"name\":\"bash\",\"input\":{\"command\":\"ls\"}}</tool>";
        let (clean, calls) = parse_tool_markup(t);
        assert_eq!(clean, "خب:");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1["command"], "ls");
    }

    #[test]
    fn long_code_payload_survives() {
        let code = "x = 1\n".repeat(1600);
        let t = format!(
            "<tool>{{'name': 'write_file', 'arguments': {{'path': 'a.py', 'content': '{code}'}}}}</tool>"
        );
        let (_clean, calls) = parse_tool_markup(&t);
        assert_eq!(calls.len(), 1, "calls len");
        assert_eq!(calls[0].0, "write_file");
        let c = calls[0].1["content"].as_str().unwrap_or("");
        assert_eq!(c.matches("x = 1").count(), 1600);
    }

    #[test]
    fn prose_without_tools_untouched() {
        let (clean, calls) = parse_tool_markup("سلام! حالت چطوره؟ <toolx> این برچسب نیست.");
        assert!(calls.is_empty());
        assert!(clean.contains("سلام"));
    }

    #[test]
    fn live_filter_hides_markup_char_by_char() {
        let t = "خب بریم <tool>{'name': 'list_files', 'arguments': {'path': '.'}}</tool> تمام";
        let mut f = LiveFilter::default();
        let mut shown = String::new();
        for ch in t.chars() {
            for p in f.feed(&ch.to_string()) {
                shown.push_str(&p);
            }
        }
        assert!(!shown.contains("<tool"), "leaked: {shown:?}");
        assert!(shown.contains("خب بریم"), "shown: {shown:?}");
    }
}
