//! Google Gemini streamGenerateContent adapter (SSE).

use super::{sse_events, Delta, LlmRequest, ProviderCtx, StreamResult, ToolCall};
use serde_json::{json, Value};

fn images_of(meta: &Value) -> Vec<Value> {
    meta.get("images")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

struct Call {
    id: String,
    name: String,
    args: String,
}

fn build_contents(req: &LlmRequest) -> Vec<Value> {
    let mut contents: Vec<Value> = vec![];
    let mut pending_fn: Vec<Value> = vec![];
    for m in &req.messages {
        if m.role == "tool" {
            let name = m.meta.get("toolName").and_then(|v| v.as_str()).unwrap_or("tool");
            pending_fn.push(json!({
                "functionResponse": { "name": name, "response": { "result": m.content } }
            }));
            continue;
        }
        if !pending_fn.is_empty() {
            contents.push(json!({ "role": "user", "parts": pending_fn.clone() }));
            pending_fn.clear();
        }
        match m.role.as_str() {
            "user" => {
                let images = images_of(&m.meta);
                let mut parts: Vec<Value> = vec![];
                if !m.content.is_empty() {
                    parts.push(json!({ "text": m.content }));
                }
                for img in &images {
                    let mime = img.get("mime").and_then(|v| v.as_str()).unwrap_or("image/png");
                    let data = img.get("data").and_then(|v| v.as_str()).unwrap_or("");
                    if !data.is_empty() {
                        parts.push(json!({ "inlineData": { "mimeType": mime, "data": data } }));
                    }
                }
                if parts.is_empty() {
                    parts.push(json!({ "text": "" }));
                }
                contents.push(json!({ "role": "user", "parts": parts }));
            }
            "assistant" => {
                let mut parts: Vec<Value> = vec![];
                if !m.content.is_empty() {
                    parts.push(json!({ "text": m.content }));
                }
                if let Some(calls) = m.meta.get("toolCalls").and_then(|v| v.as_array()) {
                    for c in calls {
                        let args: Value = serde_json::from_str(
                            c.get("args").and_then(|a| a.as_str()).unwrap_or("{}"),
                        )
                        .unwrap_or(json!({}));
                        parts.push(json!({
                            "functionCall": {
                                "name": c.get("name").cloned().unwrap_or(json!("")),
                                "args": args,
                            }
                        }));
                    }
                }
                if parts.is_empty() {
                    parts.push(json!({ "text": "" }));
                }
                contents.push(json!({ "role": "model", "parts": parts }));
            }
            _ => {}
        }
    }
    if !pending_fn.is_empty() {
        contents.push(json!({ "role": "user", "parts": pending_fn }));
    }
    contents
}

pub async fn stream(
    ctx: &ProviderCtx,
    req: &LlmRequest,
    on_delta: &mut (dyn FnMut(&Delta) + Send),
) -> Result<StreamResult, String> {
    let client = super::http_client()?;
    let mut body = json!({ "contents": build_contents(req) });
    if !req.system.is_empty() {
        body["systemInstruction"] = json!({ "parts": [{ "text": req.system }] });
    }
    if !req.tools.is_empty() {
        let decls: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                let f = t.get("function").cloned().unwrap_or(json!({}));
                json!({
                    "name": f.get("name").cloned().unwrap_or(json!("")),
                    "description": f.get("description").cloned().unwrap_or(json!("")),
                    "parameters": f.get("parameters").cloned().unwrap_or(json!({"type":"object"})),
                })
            })
            .collect();
        body["tools"] = json!([{ "functionDeclarations": decls }]);
    }
    let mut gen = json!({ "maxOutputTokens": 8192 });
    if let Some(e) = &req.effort {
        if ctx.model.to_lowercase().contains("gemini-2.5") {
            let budget = match e.as_str() {
                "high" => 24576,
                "medium" => 8192,
                _ => 2048,
            };
            gen["thinkingConfig"] = json!({ "thinkingBudget": budget });
        }
    }
    body["generationConfig"] = gen;

    let url = format!(
        "{}/models/{}:streamGenerateContent?alt=sse",
        super::base(&ctx.base_url),
        ctx.model
    );
    let mut http = client.post(&url).json(&body);
    if !ctx.api_key.is_empty() {
        http = http.header("x-goog-api-key", &ctx.api_key);
    }
    for (k, v) in &ctx.extra_headers {
        http = http.header(k.as_str(), v.as_str());
    }
    let resp = http.send().await.map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(super::format_api_error(status.as_u16(), &text));
    }

    let mut acc_text = String::new();
    let mut acc_think = String::new();
    let mut calls: Vec<Call> = vec![];
    let mut err: Option<String> = None;
    let mut stop = String::new();

    sse_events(resp, |data| {
        let v: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => return true,
        };
        if let Some(e) = v.get("error") {
            err = Some(
                e.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Provider error")
                    .to_string(),
            );
            return false;
        }
        if let Some(candidates) = v.get("candidates").and_then(|c| c.as_array()) {
            if let Some(c) = candidates.first() {
                if let Some(fr) = c.get("finishReason").and_then(|f| f.as_str()) {
                    if !fr.is_empty() {
                        stop = fr.to_string();
                    }
                }
                if let Some(parts) = c.pointer("/content/parts").and_then(|p| p.as_array()) {
                    for part in parts {
                        if part.get("thought").and_then(|t| t.as_bool()).unwrap_or(false) {
                            if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                                if !t.is_empty() {
                                    acc_think.push_str(t);
                                    on_delta(&Delta::Thinking(t.to_string()));
                                }
                            }
                            continue;
                        }
                        if let Some(fc) = part.get("functionCall") {
                            let idx = calls.len();
                            let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                            let id = format!("fc_{idx}");
                            let args = fc.get("args").cloned().unwrap_or(json!({})).to_string();
                            calls.push(Call { id: id.clone(), name: name.clone(), args: args.clone() });
                            on_delta(&Delta::ToolStart { index: idx, id, name });
                            on_delta(&Delta::ToolArgs { index: idx, delta: args });
                            continue;
                        }
                        if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                            if !t.is_empty() {
                                acc_text.push_str(t);
                                on_delta(&Delta::Text(t.to_string()));
                            }
                        }
                    }
                }
            }
        }
        true
    })
    .await?;

    if let Some(e) = err {
        return Err(e);
    }
    let tool_calls = calls
        .into_iter()
        .map(|c| ToolCall {
            id: c.id,
            name: c.name,
            args_json: if c.args.trim().is_empty() { "{}".into() } else { c.args },
        })
        .collect();
    Ok(StreamResult {
        text: acc_text,
        thinking: acc_think,
        tool_calls,
        raw_blocks: None,
        stop_reason: stop,
    })
}
