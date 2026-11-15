//! OpenAI-compatible chat completions (OpenAI, Groq, OpenRouter, NVIDIA, custom).

use super::{sse_events, Delta, LlmRequest, ProviderCtx, StreamResult, ToolCall};
use serde_json::{json, Map, Value};

fn images_of(meta: &Value) -> Vec<Value> {
    meta.get("images")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

fn build_messages(req: &LlmRequest) -> Vec<Value> {
    let mut out: Vec<Value> = vec![];
    if !req.system.is_empty() {
        out.push(json!({ "role": "system", "content": req.system }));
    }
    for m in &req.messages {
        let images = images_of(&m.meta);
        match m.role.as_str() {
            "user" => {
                if images.is_empty() {
                    out.push(json!({ "role": "user", "content": m.content }));
                } else {
                    let mut parts: Vec<Value> = vec![];
                    if !m.content.is_empty() {
                        parts.push(json!({ "type": "text", "text": m.content }));
                    }
                    for img in &images {
                        let mime = img.get("mime").and_then(|v| v.as_str()).unwrap_or("image/png");
                        let data = img.get("data").and_then(|v| v.as_str()).unwrap_or("");
                        if !data.is_empty() {
                            parts.push(json!({
                                "type": "image_url",
                                "image_url": { "url": format!("data:{mime};base64,{data}") }
                            }));
                        }
                    }
                    out.push(json!({ "role": "user", "content": parts }));
                }
            }
            "assistant" => {
                let calls = m
                    .meta
                    .get("toolCalls")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut msg = Map::new();
                msg.insert("role".into(), json!("assistant"));
                if calls.is_empty() {
                    msg.insert("content".into(), json!(m.content));
                } else {
                    msg.insert("content".into(), Value::Null);
                    let tc: Vec<Value> = calls
                        .iter()
                        .map(|c| {
                            json!({
                                "type": "function",
                                "id": c.get("id").cloned().unwrap_or(json!("")),
                                "function": {
                                    "name": c.get("name").cloned().unwrap_or(json!("")),
                                    "arguments": c.get("args").cloned().unwrap_or(json!("{}")),
                                }
                            })
                        })
                        .collect();
                    msg.insert("tool_calls".into(), json!(tc));
                }
                out.push(Value::Object(msg));
            }
            "tool" => {
                let id = m.meta.get("toolCallId").and_then(|v| v.as_str()).unwrap_or("");
                out.push(json!({ "role": "tool", "tool_call_id": id, "content": m.content }));
            }
            _ => {}
        }
    }
    out
}

struct Call {
    id: String,
    name: String,
    args: String,
}

pub async fn stream(
    ctx: &ProviderCtx,
    req: &LlmRequest,
    on_delta: &mut (dyn FnMut(&Delta) + Send),
) -> Result<StreamResult, String> {
    let client = super::http_client()?;
    let mut body = json!({
        "model": ctx.model,
        "messages": build_messages(req),
        "stream": true,
    });
    if !req.tools.is_empty() {
        body["tools"] = json!(req.tools);
        body["tool_choice"] = json!("auto");
    }
    let m = ctx.model.to_lowercase();
    if ctx.provider_id == "openai"
        && (m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") || m.starts_with("gpt-5"))
    {
        body["max_completion_tokens"] = json!(16384);
    } else {
        body["max_tokens"] = json!(8192);
    }
    if let Some(effort) = &req.effort {
        if ctx.provider_id == "openrouter" {
            body["reasoning"] = json!({ "effort": effort });
        } else {
            body["reasoning_effort"] = json!(effort);
        }
    }

    let url = format!("{}/chat/completions", super::base(&ctx.base_url));
    let mut http = client.post(&url).json(&body);
    if !ctx.api_key.is_empty() {
        http = http.bearer_auth(&ctx.api_key);
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
        let d = data.trim();
        if d == "[DONE]" {
            return false;
        }
        let v: Value = match serde_json::from_str(d) {
            Ok(v) => v,
            Err(_) => return true,
        };
        if let Some(e) = v.get("error") {
            err = Some(if e.is_string() {
                e.as_str().unwrap_or("unknown error").to_string()
            } else {
                super::format_api_error(500, &e.to_string())
            });
            return false;
        }
        if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
            if let Some(ch) = choices.first() {
                if let Some(fr) = ch.get("finish_reason").and_then(|f| f.as_str()) {
                    if !fr.is_empty() {
                        stop = fr.to_string();
                    }
                }
                if let Some(delta) = ch.get("delta") {
                    for key in ["reasoning_content", "reasoning"] {
                        if let Some(rc) = delta.get(key).and_then(|r| r.as_str()) {
                            if !rc.is_empty() {
                                acc_think.push_str(rc);
                                on_delta(&Delta::Thinking(rc.to_string()));
                            }
                        }
                    }
                    if let Some(t) = delta.get("content").and_then(|c| c.as_str()) {
                        if !t.is_empty() {
                            acc_text.push_str(t);
                            on_delta(&Delta::Text(t.to_string()));
                        }
                    }
                    if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                        for tc in tcs {
                            let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                            while calls.len() <= idx {
                                calls.push(Call { id: String::new(), name: String::new(), args: String::new() });
                            }
                            if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                if !id.is_empty() {
                                    calls[idx].id = id.to_string();
                                }
                            }
                            if let Some(f) = tc.get("function") {
                                if let Some(n) = f.get("name").and_then(|n| n.as_str()) {
                                    if !n.is_empty() {
                                        calls[idx].name = n.to_string();
                                    }
                                }
                                if let Some(a) = f.get("arguments").and_then(|a| a.as_str()) {
                                    calls[idx].args.push_str(a);
                                }
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
        .enumerate()
        .map(|(i, c)| ToolCall {
            id: if c.id.is_empty() { format!("call_{i}") } else { c.id },
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
