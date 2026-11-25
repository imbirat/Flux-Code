pub mod anthropic;
pub mod gemini;
pub mod openai;

use serde_json::Value;

#[derive(Clone, Debug)]
pub enum Delta {
    Text(String),
    Thinking(String),
    // Tool deltas are consumed by provider-specific accumulators; the shared
    // agent loop uses the final `tool_calls` vector instead.
    #[allow(dead_code)]
    ToolStart {
        index: usize,
        id: String,
        name: String,
    },
    #[allow(dead_code)]
    ToolArgs {
        index: usize,
        delta: String,
    },
}

#[derive(Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args_json: String,
}

pub struct StreamResult {
    pub text: String,
    pub thinking: String,
    pub tool_calls: Vec<ToolCall>,
    /// Anthropic-only: the raw content blocks (text/thinking/tool_use) so that
    /// thinking signatures can be replayed on the next request.
    pub raw_blocks: Option<Value>,
    pub stop_reason: String,
}

/// Provider-neutral message as stored in the database.
#[derive(Clone, Debug)]
pub struct LlmMessage {
    pub role: String, // "user" | "assistant" | "tool"
    pub content: String,
    pub meta: Value,
}

pub struct LlmRequest {
    pub system: String,
    pub messages: Vec<LlmMessage>,
    /// OpenAI-style function definitions: {type:"function", function:{name, description, parameters}}
    pub tools: Vec<Value>,
    /// "low" | "medium" | "high" — only set when the model supports it.
    pub effort: Option<String>,
}

pub struct ProviderCtx {
    pub provider_id: String,
    pub format: String, // "openai" | "anthropic" | "gemini"
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub extra_headers: Vec<(String, String)>,
}

pub async fn stream_chat(
    ctx: &ProviderCtx,
    req: &LlmRequest,
    on_delta: &mut (dyn FnMut(&Delta) + Send),
) -> Result<StreamResult, String> {
    match ctx.format.as_str() {
        "anthropic" => anthropic::stream(ctx, req, on_delta).await,
        "gemini" => gemini::stream(ctx, req, on_delta).await,
        _ => openai::stream(ctx, req, on_delta).await,
    }
}

pub fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

pub fn base(base: &str) -> String {
    base.trim_end_matches('/').to_string()
}

/// Upper bound for one buffered SSE line/event. Protects memory against a
/// provider that never sends a newline or emits an enormous event.
const MAX_SSE_BUFFER_BYTES: usize = 8 * 1024 * 1024;

/// Incremental Server-Sent Events parser.
///
/// Bytes are buffered until a complete line terminator is seen, so a UTF-8
/// sequence split across network chunks is never decoded in pieces. Handles
/// LF, CRLF and CR line endings, multi-line `data:` fields, comments and
/// unknown fields, and a final event without a trailing blank line.
pub(crate) struct SseParser {
    buf: Vec<u8>,
    data: String,
    has_data: bool,
}

impl SseParser {
    pub(crate) fn new() -> Self {
        Self {
            buf: Vec::new(),
            data: String::new(),
            has_data: false,
        }
    }

    fn dispatch(&mut self) -> Option<String> {
        if self.has_data {
            self.has_data = false;
            Some(std::mem::take(&mut self.data))
        } else {
            None
        }
    }

    fn handle_line(&mut self, raw: &[u8]) -> Result<Option<String>, String> {
        // A blank line terminates the event.
        if raw.is_empty() {
            return Ok(self.dispatch());
        }
        // Lines are decoded whole, so multi-byte characters are intact here.
        let line = String::from_utf8_lossy(raw);
        if line.starts_with(':') {
            return Ok(None); // comment / keep-alive
        }
        let (field, value) = match line.find(':') {
            Some(i) => (&line[..i], &line[i + 1..]),
            None => (line.as_ref(), ""),
        };
        if field != "data" {
            return Ok(None);
        }
        let value = value.strip_prefix(' ').unwrap_or(value);
        if self.has_data {
            self.data.push('\n');
        }
        self.data.push_str(value);
        self.has_data = true;
        if self.data.len() > MAX_SSE_BUFFER_BYTES {
            return Err("SSE event exceeded the size limit".into());
        }
        Ok(None)
    }

    /// Feed raw bytes; returns every event completed by this chunk.
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|b| *b == b'\n' || *b == b'\r') {
            let is_cr = self.buf[pos] == b'\r';
            // A trailing CR may be the first half of CRLF; wait for more input.
            if is_cr && pos + 1 >= self.buf.len() {
                break;
            }
            let consume = if is_cr {
                if self.buf.get(pos + 1) == Some(&b'\n') {
                    pos + 2
                } else {
                    pos + 1
                }
            } else {
                pos + 1
            };
            let raw: Vec<u8> = self.buf.drain(..consume).collect();
            let line = &raw[..pos];
            if let Some(ev) = self.handle_line(line)? {
                out.push(ev);
            }
        }
        if self.buf.len() > MAX_SSE_BUFFER_BYTES {
            return Err("SSE line exceeded the size limit".into());
        }
        Ok(out)
    }

    /// Flush a final event that was not followed by a blank line.
    pub(crate) fn finish(&mut self) -> Result<Option<String>, String> {
        if !self.buf.is_empty() {
            let raw = std::mem::take(&mut self.buf);
            let raw = raw.strip_suffix(b"\r").unwrap_or(&raw);
            if let Some(ev) = self.handle_line(raw)? {
                return Ok(Some(ev));
            }
        }
        Ok(self.dispatch())
    }
}

/// Feed an SSE response body to `f`, one event payload per call.
/// Return `false` from `f` to stop reading early.
pub(crate) async fn sse_events<F>(resp: reqwest::Response, mut f: F) -> Result<(), String>
where
    F: FnMut(&str) -> bool,
{
    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut parser = SseParser::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| e.to_string())?;
        for event in parser.feed(&bytes)? {
            if !f(&event) {
                return Ok(());
            }
        }
    }
    if let Some(event) = parser.finish()? {
        let _ = f(&event);
    }
    Ok(())
}

/// Turn a provider HTTP error into a readable message.
pub fn format_api_error(status: u16, body: &str) -> String {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let msg = v
        .pointer("/error/message")
        .and_then(|x| x.as_str())
        .or_else(|| v.pointer("/error").and_then(|x| x.as_str()))
        .or_else(|| v.pointer("/message").and_then(|x| x.as_str()))
        .unwrap_or(body);
    let msg: String = msg.chars().take(400).collect();
    let hint = match status {
        401 | 403 => " — check the API key in Settings › Providers",
        404 => " — model or endpoint not found",
        429 => " — rate limited, try again shortly",
        _ => "",
    };
    format!("Provider error {status}: {msg}{hint}")
}

/// Which reasoning-effort values a model supports, from a built-in registry.
/// `None` means the model has no reasoning-effort control: the selector is
/// hidden and no parameter is ever sent.
pub fn effort_values(provider_id: &str, model: &str) -> Option<Vec<String>> {
    let m = model.to_lowercase();
    let supported = |values: &[&str]| Some(values.iter().map(|s| s.to_string()).collect());
    match provider_id {
        "openai" => {
            if m.starts_with("o1")
                || m.starts_with("o3")
                || m.starts_with("o4")
                || m.starts_with("gpt-5")
            {
                supported(&["low", "medium", "high"])
            } else {
                None
            }
        }
        "anthropic" => {
            let thinking = m.contains("claude-3-7")
                || m.contains("claude-sonnet-4")
                || m.contains("claude-opus-4")
                || m.contains("claude-haiku-4");
            if thinking {
                supported(&["low", "medium", "high"])
            } else {
                None
            }
        }
        "gemini" => {
            if m.contains("gemini-2.5-pro") || m.contains("gemini-2.5-flash") {
                supported(&["low", "medium", "high"])
            } else {
                None
            }
        }
        "openrouter" => {
            let hints = [
                "o1-",
                "o3-",
                "o4-",
                "gpt-5",
                "deepseek-r1",
                "deepseek-reasoner",
                "qwq",
                "thinking",
                "gemini-2.5",
                "claude-3-7",
                "claude-sonnet-4",
                "claude-opus-4",
            ];
            if hints.iter().any(|h| m.contains(h)) {
                supported(&["low", "medium", "high"])
            } else {
                None
            }
        }
        // Groq runs open reasoning models (qwen3, gpt-oss, qwq, deepseek-r1…)
        // that accept the OpenAI reasoning_effort parameter.
        "groq" => {
            let hints = ["qwq", "qwen3", "gpt-oss", "deepseek-r1", "reasoning"];
            if hints.iter().any(|h| m.contains(h)) {
                supported(&["low", "medium", "high"])
            } else {
                None
            }
        }
        // Custom providers are never assumed to support reasoning effort.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::SseParser;

    fn events(parser: &mut SseParser, bytes: &[u8]) -> Vec<String> {
        parser.feed(bytes).unwrap()
    }

    #[test]
    fn splits_utf8_across_chunks_without_mangling() {
        let mut p = SseParser::new();
        let full = "data: {\"t\":\"héllo 🌊\"}\n\n".as_bytes().to_vec();
        // Split in the middle of the multi-byte emoji.
        let split = full
            .windows(3)
            .position(|w| w == "🌊".as_bytes().get(..3).unwrap())
            .unwrap()
            + 1;
        assert!(events(&mut p, &full[..split]).is_empty());
        let got = events(&mut p, &full[split..]);
        assert_eq!(got, vec!["{\"t\":\"héllo 🌊\"}"]);
    }

    #[test]
    fn handles_crlf_and_multiline_data() {
        let mut p = SseParser::new();
        let got = events(
            &mut p,
            b"event: message\r\ndata: line one\r\ndata: line two\r\n\r\n",
        );
        assert_eq!(got, vec!["line one\nline two"]);
    }

    #[test]
    fn ignores_comments_and_unknown_fields() {
        let mut p = SseParser::new();
        let got = events(&mut p, b": keep-alive\nid: 7\nretry: 10\ndata: real\n\n");
        assert_eq!(got, vec!["real"]);
    }

    #[test]
    fn flushes_a_final_event_without_trailing_blank_line() {
        let mut p = SseParser::new();
        assert!(events(&mut p, b"data: tail").is_empty());
        assert_eq!(p.finish().unwrap().as_deref(), Some("tail"));
    }

    #[test]
    fn preserves_done_sentinel_for_adapters() {
        let mut p = SseParser::new();
        let got = events(&mut p, b"data: [DONE]\n\n");
        assert_eq!(got, vec!["[DONE]"]);
    }

    #[test]
    fn data_without_space_and_empty_data_are_supported() {
        let mut p = SseParser::new();
        let got = events(&mut p, b"data:x\ndata:\n\n");
        assert_eq!(got, vec!["x\n"]);
    }

    #[test]
    fn cr_only_line_endings_are_supported() {
        let mut p = SseParser::new();
        // A trailing CR is ambiguous (it may be half of a CRLF split across
        // chunks), so it is resolved by the next chunk or by finish().
        assert!(events(&mut p, b"data: a\rdata: b\r\r").is_empty());
        assert_eq!(p.finish().unwrap().as_deref(), Some("a\nb"));
    }
}
