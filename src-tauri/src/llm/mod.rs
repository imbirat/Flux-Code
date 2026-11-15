pub mod anthropic;
pub mod gemini;
pub mod openai;

use serde_json::Value;

#[derive(Clone, Debug)]
pub enum Delta {
    Text(String),
    Thinking(String),
    ToolStart { index: usize, id: String, name: String },
    ToolArgs { index: usize, delta: String },
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

/// Feed an SSE response body to `f`, one `data:` payload per call.
/// Return `false` from `f` to stop reading early.
pub(crate) async fn sse_events<F>(resp: reqwest::Response, mut f: F) -> Result<(), String>
where
    F: FnMut(&str) -> bool,
{
    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| e.to_string())?;
        buf.push_str(&String::from_utf8_lossy(&bytes));
        while let Some(pos) = buf.find("\n\n") {
            let msg: String = buf.drain(..pos + 2).collect();
            for line in msg.lines() {
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.strip_prefix(' ').unwrap_or(data);
                    if !f(data) {
                        return Ok(());
                    }
                }
            }
        }
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
            if m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") || m.starts_with("gpt-5") {
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
                "o1-", "o3-", "o4-", "gpt-5", "deepseek-r1", "deepseek-reasoner", "qwq",
                "thinking", "gemini-2.5", "claude-3-7", "claude-sonnet-4", "claude-opus-4",
            ];
            if hints.iter().any(|h| m.contains(h)) {
                supported(&["low", "medium", "high"])
            } else {
                None
            }
        }
        _ => None,
    }
}
