//! Built-in agent tools: `calculator`, `current_time`, `remember`, `recall`.

use crate::memory::MemoryStore;
use serde_json::Value;

/// Result of executing a tool locally.
#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub output: String,
    pub is_error: bool,
}

/// Anthropic-format tool schemas advertised to the model.
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

/// Persian labels for the UI.
pub fn tool_label(name: &str) -> &'static str {
    match name {
        "calculator" => "ماشین‌حساب",
        "current_time" => "ساعت و تاریخ",
        "remember" => "ذخیره در حافظه",
        "recall" => "جست‌وجوی حافظه",
        _ => "ابزار",
    }
}

/// Execute one tool call locally.
pub fn execute(name: &str, input: &Value, mem: &mut MemoryStore) -> ToolOutcome {
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
        other => ToolOutcome { output: format!("unknown tool \"{other}\""), is_error: true },
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
        let out = execute("calculator", &serde_json::json!({"expression": "17*24+3"}), &mut mem);
        assert!(!out.is_error);
        assert!(out.output.contains("411"));

        let out = execute("remember", &serde_json::json!({"title": "t", "content": "c"}), &mut mem);
        assert!(!out.is_error);
        let out = execute("recall", &serde_json::json!({"query": "t"}), &mut mem);
        assert!(out.output.contains("c"));

        let out = execute("nope", &serde_json::json!({}), &mut mem);
        assert!(out.is_error);
    }
}
