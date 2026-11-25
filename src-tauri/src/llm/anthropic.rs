//! Anthropic Messages API adapter with extended-thinking support.

use super::{sse_events, Delta, LlmRequest, ProviderCtx, StreamResult, ToolCall};
use serde_json::{json, Value};
use std::collections::HashMap;

fn budget_for(effort: &str) -> u32 {
    match effort {
        "high" => 20480,
        "medium" => 10240,
        _ => 4096,
    }
}

fn images_of(meta: &Value) -> Vec<Value> {
    meta.get("images")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Convert neutral history to Anthropic message format.
/// Consecutive tool results are merged into a single user message,
/// as the API requires strict user/assistant alternation.
fn build_messages(req: &LlmRequest) -> Vec<Value> {
    let mut out: Vec<Value> = vec![];
    let mut pending: Vec<Value> = vec![];
    for m in &req.messages {
        if m.role == "tool" {
            let id = m
                .meta
                .get("toolCallId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            pending.push(json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": m.content,
            }));
            continue;
        }
        if !pending.is_empty() {
            out.push(json!({ "role": "user", "content": pending.clone() }));
            pending.clear();
        }
        match m.role.as_str() {
            "user" => {
                let images = images_of(&m.meta);
                if images.is_empty() {
                    out.push(json!({ "role": "user", "content": m.content }));
                } else {
                    let mut parts: Vec<Value> = vec![];
                    if !m.content.is_empty() {
                        parts.push(json!({ "type": "text", "text": m.content }));
                    }
                    for img in &images {
                        let mime = img
                            .get("mime")
                            .and_then(|v| v.as_str())
                            .unwrap_or("image/png");
                        let data = img.get("data").and_then(|v| v.as_str()).unwrap_or("");
                        if !data.is_empty() {
                            parts.push(json!({
                                "type": "image",
                                "source": { "type": "base64", "media_type": mime, "data": data }
                            }));
                        }
                    }
                    out.push(json!({ "role": "user", "content": parts }));
                }
            }
            "assistant" => {
                let mut content: Vec<Value> = vec![];
                if let Some(arr) = m.meta.get("rawBlocks").and_then(|r| r.as_array()) {
                    for b in arr {
                        match b.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                            "text" => {
                                let t = b.get("text").and_then(|t| t.as_str()).unwrap_or("");
                                if !t.is_empty() {
                                    content.push(json!({ "type": "text", "text": t }));
                                }
                            }
                            "thinking" => {
                                let th = b.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
                                let sig = b.get("signature").and_then(|s| s.as_str()).unwrap_or("");
                                if !th.is_empty() && !sig.is_empty() {
                                    content.push(b.clone());
                                }
                            }
                            "tool_use" => content.push(b.clone()),
                            _ => {}
                        }
                    }
                } else {
                    if !m.content.is_empty() {
                        content.push(json!({ "type": "text", "text": m.content }));
                    }
                    if let Some(calls) = m.meta.get("toolCalls").and_then(|v| v.as_array()) {
                        for c in calls {
                            let args: Value = serde_json::from_str(
                                c.get("args").and_then(|a| a.as_str()).unwrap_or("{}"),
                            )
                            .unwrap_or(json!({}));
                            content.push(json!({
                                "type": "tool_use",
                                "id": c.get("id").cloned().unwrap_or(json!("")),
                                "name": c.get("name").cloned().unwrap_or(json!("")),
                                "input": args,
                            }));
                        }
                    }
                }
                if content.is_empty() {
                    content.push(json!({ "type": "text", "text": "(no content)" }));
                }
                out.push(json!({ "role": "assistant", "content": content }));
            }
            _ => {}
        }
    }
    if !pending.is_empty() {
        out.push(json!({ "role": "user", "content": pending }));
    }
    out
}

pub async fn stream(
    ctx: &ProviderCtx,
    req: &LlmRequest,
    on_delta: &mut (dyn FnMut(&Delta) + Send),
) -> Result<StreamResult, String> {
    let client = super::http_client()?;
    let mut body = json!({
        "model": ctx.model,
        "max_tokens": 16384,
        "stream": true,
        "messages": build_messages(req),
    });
    if !req.system.is_empty() {
        body["system"] = json!(req.system);
    }
    if !req.tools.is_empty() {
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                let f = t.get("function").cloned().unwrap_or(json!({}));
                json!({
                    "name": f.get("name").cloned().unwrap_or(json!("")),
                    "description": f.get("description").cloned().unwrap_or(json!("")),
                    "input_schema": f.get("parameters").cloned().unwrap_or(json!({"type":"object"})),
                })
            })
            .collect();
        body["tools"] = json!(tools);
    }
    if let Some(e) = &req.effort {
        let budget = budget_for(e);
        body["thinking"] = json!({ "type": "enabled", "budget_tokens": budget });
        body["max_tokens"] = json!(budget + 12288);
    } else if ctx.model.to_lowercase().contains("haiku")
        && !ctx.model.to_lowercase().contains("haiku-4")
    {
        body["max_tokens"] = json!(8192);
    }

    let url = format!("{}/messages", super::base(&ctx.base_url));
    let mut http = client
        .post(&url)
        .header("anthropic-version", "2023-06-01")
        .json(&body);
    if !ctx.api_key.is_empty() {
        http = http.header("x-api-key", &ctx.api_key);
    }
    for (k, v) in &ctx.extra_headers {
        http = http.header(k.as_str(), v.as_str());
    }
    let resp = http
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(super::format_api_error(status.as_u16(), &text));
    }

    let mut text_acc = String::new();
    let mut think_acc = String::new();
    let mut blocks: Vec<Value> = vec![];
    let mut arg_raw: HashMap<usize, String> = HashMap::new();
    let mut err: Option<String> = None;
    let mut stop = String::new();

    sse_events(resp, |data| {
        let v: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => return true,
        };
        match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "error" => {
                err = Some(
                    v.pointer("/error/message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Provider error")
                        .to_string(),
                );
                return false;
            }
            "content_block_start" => {
                let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let cb = v.get("content_block").cloned().unwrap_or(json!({}));
                while blocks.len() <= idx {
                    blocks.push(json!({ "type": "text", "text": "" }));
                }
                match cb.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "text" => blocks[idx] = json!({ "type": "text", "text": "" }),
                    "thinking" => blocks[idx] = json!({ "type": "thinking", "thinking": "" }),
                    "tool_use" => {
                        let id = cb
                            .get("id")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = cb
                            .get("name")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        blocks[idx] =
                            json!({ "type": "tool_use", "id": id, "name": name, "input": {} });
                        arg_raw.insert(idx, String::new());
                        on_delta(&Delta::ToolStart {
                            index: idx,
                            id,
                            name,
                        });
                    }
                    _ => {}
                }
            }
            "content_block_delta" => {
                let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let d = v.get("delta").cloned().unwrap_or(json!({}));
                match d.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "text_delta" => {
                        let s = d.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        if !s.is_empty() {
                            text_acc.push_str(s);
                            on_delta(&Delta::Text(s.to_string()));
                            if let Some(b) = blocks.get_mut(idx) {
                                let cur = b.get("text").and_then(|t| t.as_str()).unwrap_or("");
                                b["text"] = json!(format!("{cur}{s}"));
                            }
                        }
                    }
                    "thinking_delta" => {
                        let s = d.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
                        if !s.is_empty() {
                            think_acc.push_str(s);
                            on_delta(&Delta::Thinking(s.to_string()));
                            if let Some(b) = blocks.get_mut(idx) {
                                let cur = b.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
                                b["thinking"] = json!(format!("{cur}{s}"));
                            }
                        }
                    }
                    "signature_delta" => {
                        let s = d.get("signature").and_then(|t| t.as_str()).unwrap_or("");
                        if !s.is_empty() {
                            if let Some(b) = blocks.get_mut(idx) {
                                b["signature"] = json!(s);
                            }
                        }
                    }
                    "input_json_delta" => {
                        let s = d.get("partial_json").and_then(|t| t.as_str()).unwrap_or("");
                        if !s.is_empty() {
                            arg_raw.entry(idx).or_default().push_str(s);
                            on_delta(&Delta::ToolArgs {
                                index: idx,
                                delta: s.to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            "message_delta" => {
                if let Some(sr) = v.pointer("/delta/stop_reason").and_then(|x| x.as_str()) {
                    stop = sr.to_string();
                }
            }
            "message_stop" => return false,
            _ => {}
        }
        true
    })
    .await?;

    if let Some(e) = err {
        return Err(e);
    }

    // Finalize tool blocks with parsed input.
    for (i, b) in blocks.iter_mut().enumerate() {
        if b.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
            let raw = arg_raw.get(&i).cloned().unwrap_or_default();
            let input: Value = serde_json::from_str(&raw).unwrap_or(json!({}));
            b["input"] = input;
        }
    }
    let mut tool_calls = vec![];
    for (i, b) in blocks.iter().enumerate() {
        if b.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
            let args_raw = arg_raw.get(&i).cloned().unwrap_or_default();
            tool_calls.push(ToolCall {
                id: b
                    .get("id")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                name: b
                    .get("name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                args_json: if args_raw.trim().is_empty() {
                    "{}".into()
                } else {
                    args_raw
                },
            });
        }
    }
    Ok(StreamResult {
        text: text_acc,
        thinking: think_acc,
        tool_calls,
        raw_blocks: Some(Value::Array(blocks)),
        stop_reason: stop,
    })
}
