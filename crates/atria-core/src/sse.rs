//! Minimal Server-Sent-Events decoder for the Anthropic Messages streaming API.

use serde_json::Value;

/// One decoded SSE event: `event:` name + parsed JSON `data:`.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub name: String,
    pub data: Value,
}

/// Incremental SSE decoder — feed raw bytes, get complete events.
///
/// Handles `event:` / `data:` lines, multi-line data, CRLF endings and
/// comment lines (`:`) per the WHATWG SSE spec. Incomplete lines stay
/// buffered until the next chunk.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
    event_name: String,
    data_lines: Vec<String>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk of the response body; returns all events completed by it.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(chunk);
        let mut events = Vec::new();

        while let Some(nl) = self.buf.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=nl).collect();
            line.pop(); // drop '\n'
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line).into_owned();

            if line.is_empty() {
                // dispatch
                if !self.data_lines.is_empty() {
                    let name = if self.event_name.is_empty() {
                        "message".to_string()
                    } else {
                        std::mem::take(&mut self.event_name)
                    };
                    let data = self.data_lines.join("\n");
                    self.data_lines.clear();
                    self.event_name.clear();
                    if let Ok(v) = serde_json::from_str::<Value>(&data) {
                        events.push(SseEvent { name, data: v });
                    }
                }
            } else if let Some(rest) = line.strip_prefix("event:") {
                self.event_name = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("data:") {
                self.data_lines.push(rest.trim_start().to_string());
            }
            // lines starting with ':' are comments — ignored
        }

        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_simple_events() {
        let mut d = SseDecoder::new();
        let chunk = b"event: message_start\ndata: {\"type\":\"message_start\",\"n\":1}\n\nevent: ping\ndata: {\"type\":\"ping\"}\n\n";
        let evs = d.feed(chunk);
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].name, "message_start");
        assert_eq!(evs[0].data["n"], 1);
        assert_eq!(evs[1].name, "ping");
    }

    #[test]
    fn handles_split_chunks() {
        let mut d = SseDecoder::new();
        let evs1 = d.feed(b"event: content_block_delta\nda");
        assert!(evs1.is_empty());
        let evs2 = d.feed(b"ta: {\"type\":\"content_block_delta\"}\n");
        assert!(evs2.is_empty()); // no blank line yet
        let evs3 = d.feed(b"\n");
        assert_eq!(evs3.len(), 1);
        assert_eq!(evs3[0].name, "content_block_delta");
    }

    #[test]
    fn handles_crlf_and_multiline_data() {
        let mut d = SseDecoder::new();
        let evs = d.feed(b"event: x\r\ndata: {\"a\":\r\ndata: 1}\r\n\r\n");
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].data["a"], 1);
    }

    #[test]
    fn ignores_comments_and_bad_json() {
        let mut d = SseDecoder::new();
        let evs = d.feed(b": keepalive\n\nevent: e\ndata: not-json\n\n");
        assert!(evs.is_empty());
    }

    #[test]
    fn default_event_name_is_message() {
        let mut d = SseDecoder::new();
        let evs = d.feed(b"data: {\"ok\":true}\n\n");
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].name, "message");
    }
}
