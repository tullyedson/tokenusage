//! Preserve SSE events while keeping the caller's pool name in completion chunks.
use super::metrics::TokenUsage;
use serde_json::Value;

#[derive(Default)]
pub struct AliasStream {
    pending: Vec<u8>,
    scanned: usize,
    line_start: usize,
    completed: bool,
    failed: bool,
    tokens: Option<TokenUsage>,
}
impl AliasStream {
    pub fn push(&mut self, bytes: &[u8], alias: &str) -> Result<Vec<u8>, std::io::Error> {
        let mut output = Vec::new();
        for byte in bytes {
            self.pending.push(*byte);
            if self.pending.len() > 1024 * 1024 {
                return Err(std::io::Error::other(
                    "Upstream event is too large; request was not replayed.",
                ));
            }
            if *byte == b'\n' {
                let line = &self.pending[self.line_start..self.scanned];
                if line.is_empty() || line == b"\r" {
                    self.observe();
                    output.extend(rewrite(&self.pending, alias));
                    self.pending.clear();
                    self.scanned = 0;
                    self.line_start = 0;
                    continue;
                }
                self.line_start = self.scanned + 1;
            }
            self.scanned += 1;
        }
        Ok(output)
    }
    pub fn finish(&mut self, alias: &str) -> Vec<u8> {
        self.observe();
        rewrite(&std::mem::take(&mut self.pending), alias)
    }
    pub fn completed(&self) -> bool {
        self.completed && !self.failed
    }
    pub fn tokens(&self) -> Option<TokenUsage> {
        self.tokens
    }
    fn observe(&mut self) {
        let Ok(text) = std::str::from_utf8(&self.pending) else {
            return;
        };
        let data = text
            .lines()
            .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
            .collect::<Vec<_>>()
            .join("\n");
        if text.lines().any(|line| {
            line.strip_prefix("event:")
                .is_some_and(|value| value.trim() == "error")
        }) {
            self.failed = true;
        }
        if data.trim() == "[DONE]" {
            self.completed = true;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&data) {
            if value.get("usage").is_some_and(|usage| !usage.is_null()) {
                self.tokens = TokenUsage::from_response(&value);
            }
            if value.get("error").is_some_and(|error| !error.is_null()) {
                self.failed = true;
            }
            if value["choices"].as_array().is_some_and(|choices| {
                choices
                    .iter()
                    .any(|choice| choice["finish_reason"].is_string())
            }) {
                self.completed = true;
            }
        }
    }
}
fn rewrite(event: &[u8], alias: &str) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(event) else {
        return event.to_vec();
    };
    let data = text
        .lines()
        .filter_map(|line| {
            line.strip_prefix("data:")
                .map(|s| s.strip_prefix(' ').unwrap_or(s))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let Ok(mut value) = serde_json::from_str::<Value>(&data) else {
        return event.to_vec();
    };
    if !value.is_object() || value.get("model").is_none() {
        return event.to_vec();
    }
    value["model"] = Value::String(alias.into());
    let mut replaced = false;
    let mut output = String::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("data:") {
            if !replaced {
                output.push_str("data: ");
                output.push_str(&value.to_string());
                output.push_str(if line.ends_with("\r\n") {
                    "\r\n"
                } else if line.ends_with('\n') {
                    "\n"
                } else {
                    ""
                });
                replaced = true;
            }
        } else {
            output.push_str(line);
        }
    }
    output.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_utf8_multiline_events_and_tool_arguments_survive_aliasing() {
        let source = "event: message\r\nid: 7\r\ndata: {\"model\":\"upstream\",\r\ndata: \"choices\":[{\"delta\":{\"content\":\"雪\",\"tool_calls\":[{\"function\":{\"arguments\":\"{\\\"model\\\":\\\"keep\\\"}\"}}]}}]}\r\n\r\ndata: [DONE]\n\n";
        let mut stream = AliasStream::default();
        let mut output = Vec::new();
        for byte in source.as_bytes() {
            output.extend(stream.push(&[*byte], "flash-models").unwrap());
        }
        output.extend(stream.finish("flash-models"));
        let text = String::from_utf8(output).unwrap();
        assert!(text.starts_with("event: message\r\nid: 7\r\n"));
        assert!(text.ends_with("data: [DONE]\n\n"));
        let data = text
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .unwrap();
        let value: Value = serde_json::from_str(data).unwrap();
        assert_eq!(value["model"], "flash-models");
        assert_eq!(value["choices"][0]["delta"]["content"], "雪");
        assert_eq!(
            value["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"],
            "{\"model\":\"keep\"}"
        );
    }
    #[test]
    fn unterminated_events_are_preserved_and_unbounded_events_fail() {
        let mut stream = AliasStream::default();
        assert!(stream
            .push(b"data: {\"model\":\"old\"}", "pool")
            .unwrap()
            .is_empty());
        assert_eq!(stream.finish("pool"), b"data: {\"model\":\"pool\"}");
        assert!(AliasStream::default()
            .push(&vec![b'x'; 1024 * 1024 + 1], "pool")
            .is_err());
    }
}
