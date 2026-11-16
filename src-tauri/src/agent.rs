use crate::llm::{self, Delta, LlmMessage, LlmRequest, ProviderCtx};
use crate::permissions::{self, Gate};
use crate::{creds, db, fsops, providers, tools};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

pub struct Agents(pub Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>);
pub struct Pending(pub Mutex<HashMap<String, oneshot::Sender<Value>>>);
/// Per-chat "allow for this session" rules chosen in permission dialogs.
pub struct SessionAllow(pub Mutex<HashMap<String, HashSet<String>>>);

pub fn default_workspace() -> PathBuf {
    dirs::document_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Flux Projects")
}

pub fn ensure_default_workspace() {
    let ws = default_workspace();
    if !ws.exists() {
        let _ = std::fs::create_dir_all(&ws);
    }
}

fn ev(app: &AppHandle, event: &str, payload: Value) {
    let _ = app.emit(event, payload);
}

pub fn provider_ctx(app: &AppHandle, provider_id: &str, model: &str) -> Result<ProviderCtx, String> {
    let resolved = providers::resolve(app, provider_id)?;
    let key = creds::get_key(provider_id)?.unwrap_or_default();
    if key.is_empty() {
        return Err(format!(
            "No API key configured for {provider_id} — add one in Settings › Providers"
        ));
    }
    if resolved.base_url.is_empty() {
        return Err("This provider has no base URL — configure it in Settings › Providers".into());
    }
    Ok(ProviderCtx {
        provider_id: provider_id.into(),
        format: resolved.format,
        base_url: resolved.base_url,
        api_key: key,
        model: model.into(),
        extra_headers: resolved.extra_headers,
    })
}

fn validated_effort(effort: &str, provider: &str, model: &str) -> Option<String> {
    if effort.is_empty() {
        return None;
    }
    match llm::effort_values(provider, model) {
        Some(values) if values.iter().any(|v| v == effort) => Some(effort.to_string()),
        _ => None,
    }
}

/// Queue a user message and start the agent loop for this chat.
pub fn send(app: &AppHandle, chat_id: &str, text: &str, attachments: Vec<Value>) -> Result<(), String> {
    {
        let agents = app.state::<Agents>();
        let g = agents.0.lock().map_err(|_| "agents registry locked")?;
        if g.contains_key(chat_id) {
            return Err("This chat is already running a task".into());
        }
    }
    let chat = db::get_chat(app, chat_id)?;
    if chat.provider.is_empty() || chat.model.is_empty() {
        return Err("No model selected — choose a model in the composer first".into());
    }
    let ws = if chat.workspace.is_empty() {
        default_workspace().to_string_lossy().to_string()
    } else {
        chat.workspace.clone()
    };
    std::fs::create_dir_all(&ws).map_err(|e| format!("Cannot create workspace: {e}"))?;
    if chat.workspace.is_empty() {
        let _ = db::update_chat(app, chat_id, &json!({ "workspace": ws }));
    }

    // Inline attached file contents; collect images for vision-capable models.
    let (extra, images) = expand_attachments(&attachments, Path::new(&ws));
    let mut content = text.to_string();
    if !extra.is_empty() {
        content.push_str("\n\n[User attached context]");
        content.push_str(&extra);
    }
    let mut meta = json!({});
    if !images.is_empty() {
        meta["images"] = json!(images);
    }
    if !attachments.is_empty() {
        meta["attachments"] = json!(attachments);
    }
    let _ = db::insert_message(app, chat_id, "user", &content, &meta)?;

    if chat.title == "New chat" {
        let first = text.lines().next().unwrap_or("New chat").trim();
        let title = if first.chars().count() > 48 {
            format!("{}…", first.chars().take(47).collect::<String>())
        } else {
            first.to_string()
        };
        if !title.is_empty() {
            let _ = db::rename_chat(app, chat_id, &title);
            ev(app, "chat:title", json!({ "chatId": chat_id, "title": title }));
        }
    }

    let app2 = app.clone();
    let cid = chat_id.to_string();
    let handle = tauri::async_runtime::spawn(async move {
        let _ = run_turn(app2, cid).await;
    });
    let agents = app.state::<Agents>();
    agents
        .0
        .lock()
        .map_err(|_| "agents registry locked")?
        .insert(chat_id.to_string(), handle);
    Ok(())
}

pub fn cancel(app: &AppHandle, chat_id: &str) {
    let handle = {
        let agents = app.state::<Agents>();
        agents
            .0
            .lock()
            .ok()
            .and_then(|mut g| g.remove(chat_id))
    };
    if let Some(h) = handle {
        h.abort();
    }
}

pub fn resolve_permission(app: &AppHandle, id: &str, allow: bool, remember: bool) {
    let tx = {
        let p = app.state::<Pending>();
        p.0.lock().ok().and_then(|mut g| g.remove(id))
    };
    if let Some(tx) = tx {
        let _ = tx.send(json!({ "allow": allow, "remember": remember }));
    }
}

pub fn resolve_questions(app: &AppHandle, id: &str, answers: Value) {
    let tx = {
        let p = app.state::<Pending>();
        p.0.lock().ok().and_then(|mut g| g.remove(id))
    };
    if let Some(tx) = tx {
        let _ = tx.send(json!({ "answers": answers }));
    }
}

async fn run_turn(app: AppHandle, chat_id: String) {
    let result = turn(&app, &chat_id).await;
    if let Err(e) = result {
        ev(&app, "chat:error", json!({ "chatId": chat_id, "error": e }));
        ev(&app, "chat:done", json!({ "chatId": chat_id }));
    }
    if let Ok(mut g) = app.state::<Agents>().0.lock() {
        g.remove(&chat_id);
    }
}

async fn turn(app: &AppHandle, chat_id: &str) -> Result<(), String> {
    let chat = db::get_chat(app, chat_id)?;
    let ws = PathBuf::from(if chat.workspace.is_empty() {
        default_workspace().to_string_lossy().to_string()
    } else {
        chat.workspace.clone()
    });
    if !ws.exists() {
        std::fs::create_dir_all(&ws).map_err(|e| e.to_string())?;
    }
    let settings = crate::commands::get_settings_value(app);
    let extra: Vec<PathBuf> = settings
        .get("extraFolders")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(PathBuf::from).collect())
        .unwrap_or_default();

    let pctx = provider_ctx(app, &chat.provider, &chat.model)?;
    let effort = validated_effort(&chat.effort, &chat.provider, &chat.model);
    let allowed: HashSet<String> = {
        let s = app.state::<SessionAllow>();
        let guard = s.0.lock().map_err(|_| "session allowlist locked")?;
        guard.get(chat_id).cloned().unwrap_or_default()
    };

    let tree = fsops::tree(&ws, 2, 300);
    let mode_desc = if chat.permission_mode == "full" {
        "full (you may work autonomously inside the workspace; destructive or system-touching operations still require confirmation)"
    } else {
        "ask (the user confirms file modifications and terminal commands)"
    };
    let system = system_prompt(&ws, mode_desc, &tree);

    let stored = db::list_messages(app, chat_id)?;
    let mut msgs: Vec<LlmMessage> = stored
        .iter()
        .map(|m| LlmMessage { role: m.role.clone(), content: m.content.clone(), meta: m.meta.clone() })
        .collect();
    for m in msgs.iter_mut() {
        if m.content.chars().count() > 30_000 {
            m.content = format!("{}\n…[truncated]…\n{}", head(&m.content, 4000), tail(&m.content, 4000));
        }
    }
    if msgs.len() > 160 {
        msgs = msgs[msgs.len() - 160..].to_vec();
    }

    let tool_defs = tools::tool_defs();
    let tctx = tools::ToolCtx {
        app: app.clone(),
        chat_id: chat_id.to_string(),
        workspace: ws.clone(),
        extra: extra.clone(),
    };

    for _ in 0..40 {
        ev(app, "chat:status", json!({ "chatId": chat_id, "status": "thinking" }));
        let msg_id = db::uid();
        ev(app, "chat:start", json!({ "chatId": chat_id, "messageId": msg_id }));

        let req = LlmRequest {
            system: system.clone(),
            messages: msgs.clone(),
            tools: tool_defs.clone(),
            effort: effort.clone(),
        };

        let mut text = String::new();
        let mut think = String::new();
        let mut streamed = false;
        let appc = app.clone();
        let cid = chat_id.to_string();
        let mid = msg_id.clone();
        let mut on_delta = |d: &Delta| {
            let (kind, value) = match d {
                Delta::Text(t) => {
                    text.push_str(t);
                    streamed = true;
                    ("text", t.clone())
                }
                Delta::Thinking(t) => {
                    think.push_str(t);
                    ("thinking", t.clone())
                }
                _ => return,
            };
            let _ = appc.emit("chat:delta", json!({ "chatId": cid, "messageId": mid, "kind": kind, "value": value }));
        };

        let result = match llm::stream_chat(&pctx, &req, &mut on_delta).await {
            Ok(r) => r,
            Err(e) => {
                // One automatic retry without the reasoning-effort parameter if the
                // provider rejected it — protects against registry misses.
                let retried = if effort.is_some() {
                    let el = e.to_lowercase();
                    if el.contains("reasoning") || el.contains("thinking") || el.contains("effort") {
                        let req2 = LlmRequest { effort: None, ..req };
                        Some(llm::stream_chat(&pctx, &req2, &mut on_delta).await)
                    } else {
                        None
                    }
                } else {
                    None
                };
                match retried {
                    Some(Ok(r2)) => r2,
                    _ => return Err(e),
                }
            }
        };

        let mut meta = json!({});
        if !think.is_empty() {
            meta["thinking"] = json!(think);
        }
        if let Some(rb) = &result.raw_blocks {
            meta["rawBlocks"] = rb.clone();
        }
        if !result.tool_calls.is_empty() {
            meta["toolCalls"] = json!(result
                .tool_calls
                .iter()
                .map(|c| json!({ "id": c.id, "name": c.name, "args": c.args_json }))
                .collect::<Vec<_>>());
        }
        let am = db::insert_message(app, chat_id, "assistant", &result.text, &meta)?;
        ev(app, "chat:message", json!({ "chatId": chat_id, "message": am }));
        msgs.push(LlmMessage { role: "assistant".into(), content: result.text.clone(), meta: meta.clone() });

        if result.tool_calls.is_empty() {
            ev(app, "chat:done", json!({ "chatId": chat_id, "messageId": msg_id }));
            return Ok(());
        }

        for call in &result.tool_calls {
            let status = match call.name.as_str() {
                "read_file" | "list_files" | "search_files" => "reading",
                "create_file" | "edit_file" | "delete_file" | "create_folder" | "rename_file" => "writing",
                "run_command" | "start_process" | "stop_process" => "running",
                "ask_user" => "asking",
                _ => "working",
            };
            ev(app, "chat:status", json!({ "chatId": chat_id, "status": status }));

            let args: Value = serde_json::from_str(&call.args_json).unwrap_or(Value::Null);
            ev(app, "chat:tool", json!({
                "chatId": chat_id, "callId": call.id, "name": call.name,
                "args": args, "state": "running"
            }));
            let (state, result_text, tool_meta) = if args.is_null() {
                ("error".to_string(), format!("Tool arguments were not valid JSON: {}", call.args_json), json!({}))
            } else if call.name == "ask_user" {
                handle_ask_user(app, chat_id, &args).await
            } else {
                gate_and_execute(app, chat_id, &tctx, &chat.permission_mode, &call.name, &args, &allowed).await
            };

            let mut tmeta = json!({ "toolCallId": call.id, "toolName": call.name, "state": state });
            if let (Some(src), Some(dst)) = (tool_meta.as_object(), tmeta.as_object_mut()) {
                for (k, v) in src {
                    dst.insert(k.clone(), v.clone());
                }
            }
            let result_text = with_tail_note(&result_text, 16_000);
            let tm = db::insert_message(app, chat_id, "tool", &result_text, &tmeta)?;
            ev(app, "chat:tool", json!({
                "chatId": chat_id, "callId": call.id, "name": call.name,
                "args": args, "state": state, "result": result_text, "messageId": tm.id
            }));
            msgs.push(LlmMessage {
                role: "tool".into(),
                content: result_text,
                meta: json!({ "toolCallId": call.id, "toolName": call.name }),
            });

            if call.name == "todo_write" {
                let todos = db::list_todos(app, chat_id)?;
                ev(app, "todo:update", json!({ "chatId": chat_id, "todos": todos }));
            }
        }
    }

    let am = db::insert_message(
        app,
        chat_id,
        "assistant",
        "Stopped after 40 steps — ask me to continue if the task is not finished.",
        &json!({}),
    )?;
    ev(app, "chat:message", json!({ "chatId": chat_id, "message": am }));
    ev(app, "chat:done", json!({ "chatId": chat_id, "messageId": am.id }));
    Ok(())
}

async fn gate_and_execute(
    app: &AppHandle,
    chat_id: &str,
    tctx: &tools::ToolCtx,
    mode: &str,
    name: &str,
    args: &Value,
    allowed: &HashSet<String>,
) -> (String, String, Value) {
    match permissions::gate_tool(mode, name, args, &tctx.workspace, &tctx.extra, allowed) {
        Gate::Allow => match tools::execute(tctx, name, args).await {
            Ok((r, m)) => ("done".into(), r, m),
            Err(e) => ("error".into(), format!("Error: {e}"), json!({})),
        },
        Gate::Deny(reason) => ("denied".into(), format!("Blocked: {reason}"), json!({})),
        Gate::Ask { summary, remember_key } => {
            ev(app, "chat:status", json!({ "chatId": chat_id, "status": "waiting-for-permission" }));
            let detail = permission_detail(name, args, &tctx.workspace);
            let decision = request_permission(app, chat_id, name, &summary, detail).await;
            let allow = decision
                .get("allow")
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            let remember = decision
                .get("remember")
                .and_then(|x| x.as_bool())
                .unwrap_or(false);
            if !allow {
                return (
                    "denied".into(),
                    "User denied this operation. Continue with a different approach, or ask the user how to proceed.".into(),
                    json!({}),
                );
            }
            if remember {
                if let Some(k) = remember_key {
                    if let Ok(mut g) = app.state::<SessionAllow>().0.lock() {
                        g.entry(chat_id.to_string()).or_default().insert(k);
                    }
                }
            }
            match tools::execute(tctx, name, args).await {
                Ok((r, m)) => ("done".into(), r, m),
                Err(e) => ("error".into(), format!("Error: {e}"), json!({})),
            }
        }
    }
}

fn permission_detail(name: &str, args: &Value, ws: &Path) -> Value {
    match name {
        "run_command" | "start_process" => {
            let cwd = args
                .get("cwd")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| ws.to_string_lossy().to_string());
            json!({
                "kind": "command",
                "command": args.get("command").and_then(|v| v.as_str()).unwrap_or(""),
                "cwd": cwd,
            })
        }
        _ => json!({
            "kind": "file",
            "path": args.get("path").and_then(|v| v.as_str()).unwrap_or(""),
            "newPath": args.get("new_path").and_then(|v| v.as_str()).unwrap_or(""),
        }),
    }
}

async fn request_permission(
    app: &AppHandle,
    chat_id: &str,
    tool: &str,
    summary: &str,
    detail: Value,
) -> Value {
    let id = db::uid();
    let (tx, rx) = oneshot::channel::<Value>();
    {
        if let Ok(mut g) = app.state::<Pending>().0.lock() {
            g.insert(id.clone(), tx);
        }
    }
    ev(app, "permission:request", json!({
        "id": id, "chatId": chat_id, "tool": tool, "summary": summary, "detail": detail
    }));
    match tokio::time::timeout(std::time::Duration::from_secs(600), rx).await {
        Ok(Ok(v)) => v,
        _ => {
            if let Ok(mut g) = app.state::<Pending>().0.lock() {
                g.remove(&id);
            }
            json!({ "allow": false, "remember": false, "timeout": true })
        }
    }
}

async fn handle_ask_user(app: &AppHandle, chat_id: &str, args: &Value) -> (String, String, Value) {
    let questions = args.get("questions").cloned().unwrap_or(json!([]));
    let id = db::uid();
    let (tx, rx) = oneshot::channel::<Value>();
    {
        if let Ok(mut g) = app.state::<Pending>().0.lock() {
            g.insert(id.clone(), tx);
        }
    }
    ev(app, "question:request", json!({ "id": id, "chatId": chat_id, "questions": questions }));
    let answers = match tokio::time::timeout(std::time::Duration::from_secs(900), rx).await {
        Ok(Ok(v)) => v,
        _ => {
            if let Ok(mut g) = app.state::<Pending>().0.lock() {
                g.remove(&id);
            }
            json!({ "answers": {} })
        }
    };
    let ans = answers.get("answers").cloned().unwrap_or(json!({}));
    let result = if ans.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
        format!("The user answered:\n{}", serde_json::to_string_pretty(&ans).unwrap_or_default())
    } else {
        "The user did not answer (skipped or timed out). Proceed with reasonable defaults and state your assumptions.".to_string()
    };
    let meta = json!({ "card": { "questions": questions, "answers": ans.clone() } });
    ("done".into(), result, meta)
}

fn expand_attachments(attachments: &[Value], ws: &Path) -> (String, Vec<Value>) {
    let mut extra = String::new();
    let mut images: Vec<Value> = vec![];
    for a in attachments {
        let kind = a.get("kind").and_then(|v| v.as_str()).unwrap_or("file");
        let path_s = a.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path_s.is_empty() {
            continue;
        }
        let p = PathBuf::from(path_s);
        match kind {
            "image" => {
                let data = a.get("data").and_then(|v| v.as_str()).unwrap_or("");
                if !data.is_empty() && images.len() < 4 {
                    let mime = a.get("mime").and_then(|v| v.as_str()).unwrap_or("image/png");
                    images.push(json!({
                        "name": a.get("name").cloned().unwrap_or(json!("image")),
                        "mime": mime,
                        "data": data,
                    }));
                    extra.push_str(&format!("\n[Image attached: {path_s}]"));
                }
            }
            "folder" => {
                if p.exists() {
                    let t = fsops::tree(&p, 2, 200);
                    extra.push_str(&format!("\n[Folder attached: {path_s}]\n{t}"));
                } else {
                    extra.push_str(&format!("\n[Folder attached but missing: {path_s}]"));
                }
            }
            _ => match fsops::read_file(&p, 1, 600) {
                Ok((content, _)) => {
                    extra.push_str(&format!("\n[Attached file: {path_s}]\n```\n{content}\n```"));
                }
                Err(e) => extra.push_str(&format!("\n[Attached file: {path_s} — could not be read: {e}]")),
            },
        }
    }
    let _ = ws;
    (extra, images)
}

fn system_prompt(ws: &Path, mode: &str, tree: &str) -> String {
    format!(
        r#"You are Flux Code, an AI coding agent running inside a Windows desktop application. You work directly in the user's workspace using tools.

Environment
- OS: Windows, shell: PowerShell
- Workspace: {}
- Permission mode: {}
- The UI shows your tool calls, so keep prose between them short.

How to work
- If key requirements are ambiguous, use ask_user before building. Do not ask about details you can reasonably decide yourself.
- Keep a plan with todo_write and keep it current. Mark a task completed only after the related work is verified.
- Inspect before you change: list_files / read_file / search_files first.
- Modify code with edit_file (targeted replacement) or create_file (only for new files).
- run_command executes a PowerShell command in the workspace. Use it for builds, tests, and dependency installs. Use start_process for long-running dev servers, and stop_process to stop them.
- After changes, verify: run the project's build or test command when one exists. Report errors honestly and fix them.
- Use relative paths inside the workspace. Never invent file contents or tool results.

Workspace layout (depth 2):
{tree}"#,
        ws.display(),
        mode
    )
}

fn head(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn tail(s: &str, n: usize) -> String {
    let c = s.chars().count();
    if c <= n {
        s.to_string()
    } else {
        s.chars().skip(c - n).collect()
    }
}

fn with_tail_note(s: &str, n: usize) -> String {
    let c = s.chars().count();
    if c <= n {
        s.to_string()
    } else {
        format!("…{}\n[output truncated]", tail(s, n))
    }
}
