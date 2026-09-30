//! Built-in agent tools: calculator, time, memory, file reading, and user-approved staged file writes.

use crate::memory::MemoryStore;
use serde_json::Value;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Result of executing a tool locally.
#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub output: String,
    pub is_error: bool,
}

/// Core tool catalog (math / time / memory).
pub fn tool_catalog() -> Vec<Value> {
    vec![
        serde_json::json!({
            "name": "calculator",
            "description": "Evaluate an arithmetic expression precisely. Supports + - * / % ^ and parentheses. Use for any math instead of computing it yourself.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "expression": { "type": "string", "description": "e.g. (17*24)+3" }
                },
                "required": ["expression"]
            }
        }),
        serde_json::json!({
            "name": "current_time",
            "description": "Get the current date and time (UTC and Tehran). Use whenever the user asks about time/date or for anything time-sensitive.",
            "input_schema": { "type": "object", "properties": {} }
        }),
        serde_json::json!({
            "name": "remember",
            "description": "Save an important note/fact to the user's persistent memory so it can be recalled later.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "short note title" },
                    "content": { "type": "string", "description": "the fact to remember" }
                },
                "required": ["title", "content"]
            }
        }),
        serde_json::json!({
            "name": "recall",
            "description": "Search (or list) notes previously saved with 'remember'. Omit query to list all notes.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "optional search term" }
                }
            }
        }),
    ]
}

/// File-access tools (advertised when a workspace root is configured).
pub fn file_tool_catalog() -> Vec<Value> {
    vec![
        serde_json::json!({
            "name": "list_files",
            "description": "List files and folders. Path can be relative to the workspace folder or an absolute path anywhere on the computer (e.g. 'C:/Users/...' or 'C:\\\\Users\\\\...').",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "relative folder path, e.g. 'notes'" }
                }
            }
        }),
        serde_json::json!({
            "name": "read_file",
            "description": "Read a UTF-8 text file. Path can be relative to the workspace folder or an absolute path anywhere on the computer.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "relative file path, e.g. 'notes/todo.md'" }
                },
                "required": ["path"]
            }
        }),
        serde_json::json!({
            "name": "write_file",
            "description": "Prepare a proposed UTF-8 text file change and show a diff. Do NOT claim it has been applied: the user must review and approve it in Atria. Path can be relative to the workspace folder or an absolute path.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "relative file path" },
                    "content": { "type": "string", "description": "new file content" }
                },
                "required": ["path", "content"]
            }
        }),
    ]
}

/// Persian labels for the UI.
pub fn tool_label(name: &str) -> &'static str {
    match name {
        "calculator" => "ماشین‌حساب",
        "current_time" => "ساعت و تاریخ",
        "remember" => "ذخیره در حافظه",
        "recall" => "جست‌وجوی حافظه",
        "list_files" => "فهرست فایل‌ها",
        "read_file" => "خواندن فایل",
        "write_file" => "نوشتن فایل",
        _ => "ابزار",
    }
}

/// Execute one tool call locally. The compatibility wrapper stores review data
/// beneath a private state folder next to the supplied workspace.
pub fn execute(name: &str, input: &Value, mem: &mut MemoryStore, root: &Path) -> ToolOutcome {
    let data_root = if root.as_os_str().is_empty() {
        user_data_root()
    } else {
        root.join(".atria-state")
    };
    execute_with_data_root(name, input, mem, root, &data_root)
}

/// Execute one tool with an explicit private app-data root (used by the Tauri shell).
pub fn execute_with_data_root(
    name: &str,
    input: &Value,
    mem: &mut MemoryStore,
    root: &Path,
    data_root: &Path,
) -> ToolOutcome {
    match name {
        "calculator" => {
            let expr = input["expression"].as_str().unwrap_or("").trim();
            match calc(expr) {
                Ok(v) => {
                    let out = if v.fract() == 0.0 && v.abs() < 1e15 {
                        format!("{}", v as i64)
                    } else {
                        format!("{v:.8}")
                    };
                    ToolOutcome { output: format!("{expr} = {out}"), is_error: false }
                }
                Err(e) => ToolOutcome { output: format!("calc error: {e}"), is_error: true },
            }
        }
        "current_time" => {
            let now = chrono::Utc::now();
            let tehran = now.with_timezone(&chrono::FixedOffset::east_opt(3 * 3600 + 1800).unwrap());
            ToolOutcome {
                output: format!(
                    "UTC: {}\nTehran (Asia/Tehran, UTC+3:30): {}\nUnix: {}\nISO: {}",
                    now.format("%Y-%m-%d %H:%M:%S"),
                    tehran.format("%Y-%m-%d %H:%M:%S (%A)"),
                    now.timestamp(),
                    now.to_rfc3339(),
                ),
                is_error: false,
            }
        }
        "remember" => {
            let title = input["title"].as_str().unwrap_or("").trim().to_string();
            let content = input["content"].as_str().unwrap_or("").trim().to_string();
            if title.is_empty() || content.is_empty() {
                ToolOutcome { output: "title and content are required".to_string(), is_error: true }
            } else {
                mem.add(title.clone(), content);
                ToolOutcome { output: format!("Saved note \"{title}\" to memory."), is_error: false }
            }
        }
        "recall" => {
            let q = input["query"].as_str().map(str::trim).filter(|s| !s.is_empty());
            let notes = mem.search(q);
            if notes.is_empty() {
                ToolOutcome { output: "(memory is empty)".to_string(), is_error: false }
            } else {
                let mut out = String::new();
                for (i, n) in notes.iter().enumerate().take(20) {
                    out.push_str(&format!("{}. [{}] {} — {}\n", i + 1, n.ts, n.title, n.content));
                }
                ToolOutcome { output: out, is_error: false }
            }
        }
        "list_files" => file_list(input, root),
        "read_file" => file_read(input, root),
        "write_file" => file_write(input, root, data_root),
        other => ToolOutcome { output: format!("unknown tool \"{other}\""), is_error: true },
    }
}

fn user_data_root() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    home.join(".atria")
}

// ---------------------------------------------------------------------------
// Full file access — absolute paths address the whole computer; relative
// paths resolve under the workspace folder (which is only the default base).
// ---------------------------------------------------------------------------

const MAX_READ: usize = 256 * 1024;

/// Resolve a file-tool path. Absolute paths (drive `C:/...`, UNC `//server/...`,
/// or `/...`) can address any location on the computer; relative paths resolve
/// under `root` (the workspace folder) as the default base.
fn sandbox(root: &Path, rel: &str) -> Result<std::path::PathBuf, String> {
    let raw = rel.trim().replace('\\', "/");
    let is_abs = raw.starts_with('/')
        || (raw.len() >= 3
            && raw.as_bytes()[0].is_ascii_alphabetic()
            && raw.as_bytes()[1] == b':'
            && raw.as_bytes()[2] == b'/');
    if is_abs {
        return Ok(std::path::PathBuf::from(raw));
    }
    if root.as_os_str().is_empty() {
        return Err("file tools are disabled (no workspace set)".to_string());
    }
    let mut p = root.to_path_buf();
    for comp in raw.split('/') {
        match comp {
            "" | "." => continue,
            c => p.push(c),
        }
    }
    Ok(p)
}

fn file_list(input: &Value, root: &Path) -> ToolOutcome {
    let rel = input["path"].as_str().unwrap_or("").trim();
    let dir = match sandbox(root, if rel.is_empty() { "." } else { rel }) {
        Ok(p) => p,
        Err(e) => return ToolOutcome { output: e, is_error: true },
    };
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) => return ToolOutcome { output: format!("cannot read folder: {e}"), is_error: true },
    };
    let mut entries: Vec<String> = Vec::new();
    for entry in rd.flatten().take(300) {
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            entries.push(format!("{name}/"));
        } else {
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            entries.push(format!("{name} ({len} bytes)"));
        }
    }
    entries.sort();
    if entries.is_empty() {
        ToolOutcome { output: "(folder is empty)".to_string(), is_error: false }
    } else {
        ToolOutcome { output: entries.join("\n"), is_error: false }
    }
}

fn file_read(input: &Value, root: &Path) -> ToolOutcome {
    let rel = input["path"].as_str().unwrap_or("").trim();
    let path = match sandbox(root, rel) {
        Ok(p) => p,
        Err(e) => return ToolOutcome { output: e, is_error: true },
    };
    match std::fs::read(&path) {
        Ok(bytes) => {
            let slice = &bytes[..bytes.len().min(MAX_READ)];
            match std::str::from_utf8(slice) {
                Ok(s) => {
                    let mut out = s.to_string();
                    if bytes.len() > MAX_READ {
                        out.push_str("\n… (truncated)");
                    }
                    ToolOutcome { output: out, is_error: false }
                }
                Err(_) => ToolOutcome {
                    output: "file is not valid UTF-8 text".to_string(),
                    is_error: true,
                },
            }
        }
        Err(e) => ToolOutcome { output: format!("cannot read file: {e}"), is_error: true },
    }
}

const MAX_WRITE: usize = 2 * 1024 * 1024;
static NEXT_EDIT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingEdit {
    path: PathBuf,
    expected_old: Option<String>,
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileBackup {
    path: PathBuf,
    backup_path: Option<PathBuf>,
    existed: bool,
    applied_content: String,
}

fn new_id(prefix: &str) -> String {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis()).unwrap_or(0);
    format!("{prefix}-{n}-{}", NEXT_EDIT_ID.fetch_add(1, Ordering::Relaxed))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 96 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn diff_preview(old: &str, new: &str) -> String {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let mut prefix = 0;
    while prefix < a.len().min(b.len()) && a[prefix] == b[prefix] { prefix += 1; }
    let mut suffix = 0;
    while suffix < a.len().saturating_sub(prefix).min(b.len().saturating_sub(prefix))
        && a[a.len() - 1 - suffix] == b[b.len() - 1 - suffix] { suffix += 1; }
    let mut out = String::from("--- current\n+++ proposed\n");
    for line in a.iter().take(prefix).rev().take(3).rev() { out.push_str(&format!(" {line}\n")); }
    for line in &a[prefix..a.len() - suffix] { out.push_str(&format!("-{line}\n")); }
    for line in &b[prefix..b.len() - suffix] { out.push_str(&format!("+{line}\n")); }
    if suffix > 0 {
        for line in a.iter().rev().take(suffix.min(3)).collect::<Vec<_>>().into_iter().rev() {
            out.push_str(&format!(" {line}\n"));
        }
    }
    if out.len() > 12_000 { out.truncate(12_000); out.push_str("\n… diff truncated"); }
    out
}

fn file_write(input: &Value, root: &Path, data_root: &Path) -> ToolOutcome {
    let rel = input["path"].as_str().unwrap_or("").trim();
    let content = input["content"].as_str().unwrap_or("");
    if content.len() > MAX_WRITE {
        return ToolOutcome { output: format!("proposed file is larger than {} bytes", MAX_WRITE), is_error: true };
    }
    let path = match sandbox(root, rel) {
        Ok(p) => p,
        Err(e) => return ToolOutcome { output: e, is_error: true },
    };
    let (old, expected_old) = match std::fs::read(&path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => (text.clone(), Some(text)),
            Err(_) => return ToolOutcome { output: "refusing to replace a non-UTF-8/binary file with a text edit".into(), is_error: true },
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), None),
        Err(e) => return ToolOutcome { output: format!("cannot inspect file: {e}"), is_error: true },
    };
    if old == content {
        return ToolOutcome { output: "proposed content is identical; no change needed".into(), is_error: false };
    }
    let id = new_id("edit");
    let dir = data_root.join("pending-edits");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return ToolOutcome { output: format!("cannot stage edit: {e}"), is_error: true };
    }
    let pending = PendingEdit { path: path.clone(), expected_old, content: content.to_string() };
    let file = dir.join(format!("{id}.json"));
    let serialized = match serde_json::to_vec(&pending) {
        Ok(v) => v,
        Err(e) => return ToolOutcome { output: format!("cannot stage edit: {e}"), is_error: true },
    };
    if let Err(e) = std::fs::write(file, serialized) {
        return ToolOutcome { output: format!("cannot stage edit: {e}"), is_error: true };
    }
    ToolOutcome {
        output: format!(
            "This edit is staged only and has NOT been applied. Review the diff in Atria and wait for the user to approve or reject it.\n[ATRIA_PENDING_EDIT:{id}]\n{}",
            diff_preview(&old, content)
        ),
        is_error: false,
    }
}

/// Apply a previously staged text edit, creating a restorable backup first.
pub fn apply_pending_edit(id: &str, data_root: &Path) -> Result<String, String> {
    if !valid_id(id) { return Err("invalid edit id".into()); }
    let pending_path = data_root.join("pending-edits").join(format!("{id}.json"));
    let pending: PendingEdit = serde_json::from_slice(&std::fs::read(&pending_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let current = match std::fs::read(&pending.path) {
        Ok(bytes) => Some(String::from_utf8(bytes).map_err(|_| "file is no longer UTF-8 text".to_string())?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot re-check target before applying: {e}")),
    };
    if current != pending.expected_old {
        return Err("file changed after the preview was created; refresh the diff before applying".into());
    }
    if let Some(parent) = pending.path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
    let backup_id = new_id("backup");
    let backup_dir = data_root.join("backups");
    std::fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
    let existed = pending.path.exists();
    let backup_path = if existed {
        let p = backup_dir.join(format!("{backup_id}.bak"));
        std::fs::copy(&pending.path, &p).map_err(|e| format!("cannot back up current file: {e}"))?;
        Some(p)
    } else { None };
    let backup = FileBackup { path: pending.path.clone(), backup_path, existed, applied_content: pending.content.clone() };
    std::fs::write(backup_dir.join(format!("{backup_id}.json")), serde_json::to_vec(&backup).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::write(&pending.path, pending.content.as_bytes()) {
        return Err(format!("cannot apply edit (backup is preserved): {e}"));
    }
    let _ = std::fs::remove_file(pending_path);
    Ok(format!("Edit applied to {}. Undo is available in Atria.\n[ATRIA_BACKUP:{backup_id}]", pending.path.display()))
}

pub fn reject_pending_edit(id: &str, data_root: &Path) -> Result<(), String> {
    if !valid_id(id) { return Err("invalid edit id".into()); }
    let path = data_root.join("pending-edits").join(format!("{id}.json"));
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

pub fn restore_file_backup(id: &str, data_root: &Path) -> Result<String, String> {
    if !valid_id(id) { return Err("invalid backup id".into()); }
    let dir = data_root.join("backups");
    let file = dir.join(format!("{id}.json"));
    let meta: FileBackup = serde_json::from_slice(&std::fs::read(file).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let current = match std::fs::read_to_string(&meta.path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot verify file before undo: {e}")),
    };
    if current.as_deref() != Some(meta.applied_content.as_str()) {
        return Err("file has changed since this edit was applied; refusing to overwrite newer changes".into());
    }
    if meta.existed {
        let backup = meta.backup_path.ok_or_else(|| "backup data missing".to_string())?;
        let bytes = std::fs::read(backup).map_err(|e| e.to_string())?;
        if let Some(parent) = meta.path.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        std::fs::write(&meta.path, bytes).map_err(|e| e.to_string())?;
        Ok(format!("Restored previous contents of {}.", meta.path.display()))
    } else {
        match std::fs::remove_file(&meta.path) {
            Ok(()) => Ok(format!("Removed newly-created file {}.", meta.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("File was already absent.".into()),
            Err(e) => Err(e.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Tiny safe arithmetic interpreter (no eval, no deps).
// expr   := term (('+' | '-') term)*
// term   := unary (('*' | '/' | '%') unary)*
// unary  := ('-' | '+') unary | power
// power  := primary ('^' unary)?          (right-associative)
// primary:= number | '(' expr ')'
// ---------------------------------------------------------------------------

struct Parser<'a> {
    s: &'a [char],
    i: usize,
}

/// Evaluate an arithmetic expression. Errors are user-safe strings.
pub fn calc(expr: &str) -> Result<f64, String> {
    let chars: Vec<char> = expr.chars().collect();
    if chars.is_empty() {
        return Err("empty expression".to_string());
    }
    let mut p = Parser { s: &chars, i: 0 };
    let v = p.expr()?;
    p.ws();
    if p.i < p.s.len() {
        return Err(format!("unexpected \"{}\"", p.s[p.i]));
    }
    if !v.is_finite() {
        return Err("result is not a finite number".to_string());
    }
    Ok(v)
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Result<f64, String> {
        let mut v = self.term()?;
        loop {
            if self.eat('+') {
                v += self.term()?;
            } else if self.eat('-') {
                v -= self.term()?;
            } else {
                return Ok(v);
            }
        }
    }

    fn term(&mut self) -> Result<f64, String> {
        let mut v = self.unary()?;
        loop {
            if self.eat('*') {
                v *= self.unary()?;
            } else if self.eat('/') {
                let d = self.unary()?;
                if d == 0.0 {
                    return Err("division by zero".to_string());
                }
                v /= d;
            } else if self.eat('%') {
                let d = self.unary()?;
                if d == 0.0 {
                    return Err("division by zero".to_string());
                }
                v %= d;
            } else {
                return Ok(v);
            }
        }
    }

    fn unary(&mut self) -> Result<f64, String> {
        if self.eat('-') {
            Ok(-self.unary()?)
        } else if self.eat('+') {
            self.unary()
        } else {
            self.power()
        }
    }

    fn power(&mut self) -> Result<f64, String> {
        let base = self.primary()?;
        if self.eat('^') {
            let exp = self.unary()?;
            Ok(base.powf(exp))
        } else {
            Ok(base)
        }
    }

    fn primary(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some('(') => {
                self.i += 1;
                let v = self.expr()?;
                if !self.eat(')') {
                    return Err("missing \")\"".to_string());
                }
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() || c == '.' => {
                let start = self.i;
                while self.i < self.s.len()
                    && (self.s[self.i].is_ascii_digit() || self.s[self.i] == '.')
                {
                    self.i += 1;
                }
                let num: String = self.s[start..self.i].iter().collect();
                num.parse::<f64>().map_err(|_| format!("bad number \"{num}\""))
            }
            other => Err(format!("unexpected {:?}", other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calc_basics() {
        assert_eq!(calc("17*24+3").unwrap(), 411.0);
        assert_eq!(calc("2+2*2").unwrap(), 6.0);
        assert_eq!(calc("(2+2)*2").unwrap(), 8.0);
        assert_eq!(calc("2^10").unwrap(), 1024.0);
        assert_eq!(calc("10/4").unwrap(), 2.5);
        assert_eq!(calc("-3+5").unwrap(), 2.0);
        assert_eq!(calc("2^3^2").unwrap(), 512.0); // right-assoc
        assert_eq!(calc("7%3").unwrap(), 1.0);
        assert_eq!(calc("1.5*2").unwrap(), 3.0);
        assert_eq!(calc("((1+2)*(3+4))").unwrap(), 21.0);
    }

    #[test]
    fn calc_errors() {
        assert!(calc("").is_err());
        assert!(calc("2+").is_err());
        assert!(calc("2++").is_err());
        assert!(calc("abc").is_err());
        assert!(calc("(1+2").is_err());
        assert!(calc("1/0").is_err());
        assert_eq!(calc("1  +  1").unwrap(), 2.0);
    }

    #[test]
    fn tool_end_to_end() {
        let mut mem = MemoryStore::in_memory();
        let none = Path::new("");
        let out = execute("calculator", &serde_json::json!({"expression": "17*24+3"}), &mut mem, none);
        assert!(!out.is_error);
        assert!(out.output.contains("411"));

        let out = execute("remember", &serde_json::json!({"title": "t", "content": "c"}), &mut mem, none);
        assert!(!out.is_error);
        let out = execute("recall", &serde_json::json!({"query": "t"}), &mut mem, none);
        assert!(out.output.contains("c"));

        let out = execute("nope", &serde_json::json!({}), &mut mem, none);
        assert!(out.is_error);
    }

    #[test]
    fn staged_file_edits_require_approval_and_are_reversible() {
        let dir = std::env::temp_dir().join(format!("atria-fa-{}", std::process::id()));
        let data = dir.join("app-data");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut mem = MemoryStore::in_memory();

        let target = dir.join("notes/a.txt");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "old line\nkeep").unwrap();
        let stale = execute_with_data_root(
            "write_file",
            &serde_json::json!({"path": "notes/a.txt", "content": "staged but now stale"}),
            &mut mem,
            &dir,
            &data,
        );
        let stale_id = stale.output.split("[ATRIA_PENDING_EDIT:").nth(1).unwrap().split(']').next().unwrap();
        std::fs::write(&target, "user edited after preview").unwrap();
        assert!(apply_pending_edit(stale_id, &data).unwrap_err().contains("changed after the preview"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "user edited after preview");
        reject_pending_edit(stale_id, &data).unwrap();
        std::fs::write(&target, "old line\nkeep").unwrap();
        let out = execute_with_data_root(
            "write_file",
            &serde_json::json!({"path": "notes/a.txt", "content": "new line\nkeep"}),
            &mut mem,
            &dir,
            &data,
        );
        assert!(!out.is_error, "{}", out.output);
        assert!(out.output.contains("NOT been applied"));
        assert!(out.output.contains("-old line") && out.output.contains("+new line"));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old line\nkeep");
        let edit_id = out.output.split("[ATRIA_PENDING_EDIT:").nth(1).unwrap().split(']').next().unwrap();
        let applied = apply_pending_edit(edit_id, &data).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new line\nkeep");
        let backup_id = applied.split("[ATRIA_BACKUP:").nth(1).unwrap().split(']').next().unwrap();
        restore_file_backup(backup_id, &data).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "old line\nkeep");

        // New files are staged too, and undo removes them rather than leaving an empty file.
        let new_target = dir.join("abs.txt");
        let abs_s = new_target.to_string_lossy().replace('\\', "/");
        let out = execute_with_data_root(
            "write_file",
            &serde_json::json!({"path": abs_s, "content": "full"}),
            &mut mem,
            Path::new(""),
            &data,
        );
        let edit_id = out.output.split("[ATRIA_PENDING_EDIT:").nth(1).unwrap().split(']').next().unwrap();
        let applied = apply_pending_edit(edit_id, &data).unwrap();
        assert_eq!(std::fs::read_to_string(&new_target).unwrap(), "full");
        let backup_id = applied.split("[ATRIA_BACKUP:").nth(1).unwrap().split(']').next().unwrap();
        restore_file_backup(backup_id, &data).unwrap();
        assert!(!new_target.exists());

        // Explicit rejection deletes the proposal without touching the target.
        let out = execute_with_data_root(
            "write_file",
            &serde_json::json!({"path": "notes/rejected.txt", "content": "no"}),
            &mut mem,
            &dir,
            &data,
        );
        let edit_id = out.output.split("[ATRIA_PENDING_EDIT:").nth(1).unwrap().split(']').next().unwrap();
        reject_pending_edit(edit_id, &data).unwrap();
        assert!(!dir.join("notes/rejected.txt").exists());

        // ".." remains supported for relative paths; no writes happen before approval.
        let out = execute_with_data_root(
            "write_file",
            &serde_json::json!({"path": "../atria-fa-sibling.txt", "content": "x"}),
            &mut mem,
            &dir,
            &data,
        );
        let edit_id = out.output.split("[ATRIA_PENDING_EDIT:").nth(1).unwrap().split(']').next().unwrap();
        apply_pending_edit(edit_id, &data).unwrap();
        let sib = dir.parent().unwrap().join("atria-fa-sibling.txt");
        assert!(sib.exists());
        let _ = std::fs::remove_file(&sib);

        let out = execute("list_files", &serde_json::json!({}), &mut mem, Path::new(""));
        assert!(out.is_error);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
