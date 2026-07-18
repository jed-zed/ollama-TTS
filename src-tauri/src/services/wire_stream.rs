//! OpenAI SSE 与 Ollama NDJSON 的增量线路解析。
//!
//! 网络分片不保证 UTF-8 字符或逻辑记录边界。解析器先按字节保留不完整行，
//! 再按 SSE 事件块或 NDJSON 行发布完整载荷，避免分片中文被替换或尾记录丢失。

const MAX_BUFFERED_EVENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireEvent {
    Data(String),
    Done,
}

#[derive(Debug, Default)]
pub struct WireStreamDecoder {
    line_buffer: Vec<u8>,
    sse_data_lines: Vec<String>,
    sse_data_bytes: usize,
}

impl WireStreamDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<WireEvent>, String> {
        self.line_buffer.extend_from_slice(bytes);
        let mut events = Vec::new();

        while let Some(newline) = self.line_buffer.iter().position(|byte| *byte == b'\n') {
            let line = self.line_buffer[..newline].to_vec();
            self.line_buffer.drain(..=newline);
            self.process_line(&line, &mut events)?;
        }

        if self.line_buffer.len() > MAX_BUFFERED_EVENT_BYTES {
            return Err("上游流单行数据超过 8 MiB 限制".to_string());
        }

        Ok(events)
    }

    /// 流结束时发布没有换行符的尾记录。
    pub fn finish(&mut self) -> Result<Vec<WireEvent>, String> {
        let mut events = Vec::new();
        if !self.line_buffer.is_empty() {
            let line = std::mem::take(&mut self.line_buffer);
            self.process_line(&line, &mut events)?;
        }
        self.flush_sse(&mut events);
        Ok(events)
    }

    fn process_line(&mut self, raw_line: &[u8], events: &mut Vec<WireEvent>) -> Result<(), String> {
        let raw_line = raw_line.strip_suffix(b"\r").unwrap_or(raw_line);
        let line = std::str::from_utf8(raw_line)
            .map_err(|error| format!("上游流包含无效 UTF-8: {}", error))?;

        if line.is_empty() {
            self.flush_sse(events);
            return Ok(());
        }
        if line.starts_with(':') {
            return Ok(());
        }

        let (field, raw_value) = line.split_once(':').unwrap_or((line, ""));
        if field == "data" {
            let value = raw_value.strip_prefix(' ').unwrap_or(raw_value);
            self.sse_data_bytes = self.sse_data_bytes.saturating_add(value.len() + 1);
            if self.sse_data_bytes > MAX_BUFFERED_EVENT_BYTES {
                return Err("上游 SSE 事件超过 8 MiB 限制".to_string());
            }
            self.sse_data_lines.push(value.to_string());
            return Ok(());
        }

        // 其余标准 SSE 字段不承载业务数据。
        if matches!(field, "event" | "id" | "retry") {
            return Ok(());
        }

        // 没有 SSE 字段前缀时按 Ollama NDJSON 记录处理。
        if !self.sse_data_lines.is_empty() {
            self.flush_sse(events);
        }
        self.emit_payload(line, events);
        Ok(())
    }

    fn flush_sse(&mut self, events: &mut Vec<WireEvent>) {
        if self.sse_data_lines.is_empty() {
            return;
        }

        let payload = self.sse_data_lines.join("\n");
        self.sse_data_lines.clear();
        self.sse_data_bytes = 0;
        self.emit_payload(&payload, events);
    }

    fn emit_payload(&self, payload: &str, events: &mut Vec<WireEvent>) {
        let payload = payload.trim();
        if payload.is_empty() {
            return;
        }
        if payload == "[DONE]" {
            events.push(WireEvent::Done);
        } else {
            events.push(WireEvent::Data(payload.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_split_utf8_and_unterminated_ndjson_tail() {
        let source = "{\"response\":\"你\"}\n{\"response\":\"好\"}".as_bytes();
        let split = source
            .windows("你".len())
            .position(|window| window == "你".as_bytes())
            .unwrap()
            + 1;
        let mut decoder = WireStreamDecoder::new();

        assert!(decoder.push(&source[..split]).unwrap().is_empty());
        let first = decoder.push(&source[split..]).unwrap();
        let tail = decoder.finish().unwrap();

        assert_eq!(
            first,
            vec![WireEvent::Data("{\"response\":\"你\"}".to_string())]
        );
        assert_eq!(
            tail,
            vec![WireEvent::Data("{\"response\":\"好\"}".to_string())]
        );
    }

    #[test]
    fn parses_sse_without_optional_space_and_with_multiline_data() {
        let mut decoder = WireStreamDecoder::new();
        let events = decoder
            .push(b"data:{\"value\":\r\ndata: \"ok\"}\r\n\r\n")
            .unwrap();

        assert_eq!(
            events,
            vec![WireEvent::Data("{\"value\":\n\"ok\"}".to_string())]
        );
    }

    #[test]
    fn parses_done_marker_with_crlf() {
        let mut decoder = WireStreamDecoder::new();
        assert_eq!(
            decoder.push(b"data: [DONE]\r\n\r\n").unwrap(),
            vec![WireEvent::Done]
        );
    }
}
