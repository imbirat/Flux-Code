use crate::llm::{self, Delta, LlmMessage, LlmRequest, ProviderCtx};
use crate::permissions::{self, Gate};
use crate::{creds, db, fsops, providers, tools};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

/// One active agent turn per chat. The token identifies the turn so an old
/// task can never remove or overwrite a newer reservation during cleanup.
pub struct AgentEntry {
    pub token: u64,
    pub cancel: Arc<AtomicBool>,
    pub handle: Option<tauri::async_runtime::JoinHandle<()>>,
}

#[derive(Default)]
pub struct Agents {
    pub map: Mutex<HashMap<String, AgentEntry>>,
    pub next_token: AtomicU64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PendingKind {
    Permission,
    Question,
}

pub struct PendingEntry {
    pub chat_id: String,
    pub kind: PendingKind,
    pub tx: oneshot::Sender<Value>,
}

/// Pending permission/question requests keyed by request id, each stamped with
/// the owning chat and kind so a response can never resolve the wrong modal.
#[derive(Default)]
pub struct Pending(pub Mutex<HashMap<String, PendingEntry>>);

/// Per-chat "allow for this session" rules chosen in permission dialogs.
#[derive(Default)]
pub struct SessionAllow(pub Mutex<HashMap<String, HashSet<String>>>);

const CANCELLED: &str = "__flux_cancelled__";

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

pub fn provider_ctx(
    app: &AppHandle,
    provider_id: &str,
    model: &str,
) -> Result<ProviderCtx, String> {
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
pub fn send(
    app: &AppHandle,
    chat_id: &str,
    text: &str,
    attachments: Vec<Value>,
) -> Result<(), String> {
    if text.chars().count() > 500_000 {
        return Err("Message is too long (max 500,000 characters)".into());
    }
    if attachments.len() > 64 {
        return Err("Too many attachments (max 64)".into());
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

    // Reserve this chat's single turn before doing any work that could race
    // with another send. The reservation is released on every error path.
    let (token, cancel) = reserve_turn(app, chat_id)?;
    let result = (|| -> Result<(), String> {
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
            // Persist attachment *references* only: the image payload already
            // lives in `meta.images`, so storing `data` here would duplicate
            // megabytes of base64 per message.
            let refs: Vec<Value> = attachments
                .iter()
                .map(|a| {
                    let mut r = a.clone();
                    if let Some(o) = r.as_object_mut() {
                        o.remove("data");
                    }
                    r
                })
                .collect();
            meta["attachments"] = json!(refs);
        }
        db::insert_message(app, chat_id, "user", &content, &meta)?;

        if chat.title == "New chat" {
            let first = text.lines().next().unwrap_or("New chat").trim();
            let title = if first.chars().count() > 48 {
                format!("{}…", first.chars().take(47).collect::<String>())
            } else {
                first.to_string()
            };
            if !title.is_empty() {
                let _ = db::rename_chat(app, chat_id, &title);
                ev(
                    app,
                    "chat:title",
                    json!({ "chatId": chat_id, "title": title }),
                );
            }
        }

        let app2 = app.clone();
        let cid = chat_id.to_string();
        let cancel2 = cancel.clone();
        let handle = tauri::async_runtime::spawn(async move {
            run_turn(app2, cid, token, cancel2).await;
        });
        let agents = app.state::<Agents>();
        if let Ok(mut g) = agents.map.lock() {
            if let Some(entry) = g.get_mut(chat_id) {
                if entry.token == token {
                    entry.handle = Some(handle);
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        release_turn(app, chat_id, token);
        return Err(e);
    }
    Ok(())
}

/// Atomically claim the chat's single active turn.
fn reserve_turn(app: &AppHandle, chat_id: &str) -> Result<(u64, Arc<AtomicBool>), String> {
    let agents = app.state::<Agents>();
    let mut g = agents.map.lock().map_err(|_| "agents registry locked")?;
    if g.contains_key(chat_id) {
        return Err("This chat is already running a task".into());
    }
    let token = agents.next_token.fetch_add(1, Ordering::SeqCst);
    let cancel = Arc::new(AtomicBool::new(false));
    g.insert(
        chat_id.to_string(),
        AgentEntry {
            token,
            cancel: cancel.clone(),
            handle: None,
        },
    );
    Ok((token, cancel))
}

fn release_turn(app: &AppHandle, chat_id: &str, token: u64) {
    let agents = app.state::<Agents>();
    if let Ok(mut g) = agents.map.lock() {
        if g.get(chat_id).map(|e| e.token) == Some(token) {
            g.remove(chat_id);
        }
    };
}

fn is_cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::SeqCst)
}

/// Stop at the next safe point. Every DB write after an await is preceded by
/// this check so a cancelled turn cannot leave partial assistant/tool rows.
fn ensure_live(cancel: &AtomicBool) -> Result<(), String> {
    if is_cancelled(cancel) {
        Err(CANCELLED.into())
    } else {
        Ok(())
    }
}

pub fn cancel(app: &AppHandle, chat_id: &str) {
    let entry = {
        let agents = app.state::<Agents>();
        agents.map.lock().ok().and_then(|mut g| g.remove(chat_id))
    };
    let had_turn = entry.is_some();
    if let Some(e) = entry {
        e.cancel.store(true, Ordering::SeqCst);
        if let Some(h) = e.handle {
            h.abort();
        }
    }
    clear_pending_for_chat(app, chat_id);
    if had_turn {
        ev(
            app,
            "chat:done",
            json!({ "chatId": chat_id, "cancelled": true }),
        );
    }
}

/// Cancel a chat and forget all transient per-chat state. Used before a chat
/// row is deleted so no late write or modal can reference it.
pub fn forget_chat(app: &AppHandle, chat_id: &str) {
    cancel(app, chat_id);
    clear_pending_for_chat(app, chat_id);
    if let Ok(mut g) = app.state::<SessionAllow>().0.lock() {
        g.remove(chat_id);
    }
    crate::terminal::stop_all_for_chat(app, chat_id);
}

fn register_pending(
    app: &AppHandle,
    id: &str,
    chat_id: &str,
    kind: PendingKind,
) -> Result<oneshot::Receiver<Value>, String> {
    let (tx, rx) = oneshot::channel::<Value>();
    let pending = app.state::<Pending>();
    let mut g = pending.0.lock().map_err(|_| "pending registry locked")?;
    g.insert(
        id.to_string(),
        PendingEntry {
            chat_id: chat_id.to_string(),
            kind,
            tx,
        },
    );
    Ok(rx)
}

fn take_pending(app: &AppHandle, id: &str, kind: PendingKind) -> Option<oneshot::Sender<Value>> {
    let pending = app.state::<Pending>();
    let mut g = pending.0.lock().ok()?;
    if g.get(id).map(|e| e.kind == kind).unwrap_or(false) {
        g.remove(id).map(|e| e.tx)
    } else {
        None
    }
}

fn clear_pending_for_chat(app: &AppHandle, chat_id: &str) {
    if let Ok(mut g) = app.state::<Pending>().0.lock() {
        g.retain(|_, e| e.chat_id != chat_id);
    }
}

pub fn resolve_permission(app: &AppHandle, id: &str, allow: bool, remember: bool) {
    if let Some(tx) = take_pending(app, id, PendingKind::Permission) {
        let _ = tx.send(json!({ "allow": allow, "remember": remember }));
    }
}

pub fn resolve_questions(app: &AppHandle, id: &str, answers: Value) {
    if let Some(tx) = take_pending(app, id, PendingKind::Question) {
        let _ = tx.send(json!({ "answers": answers }));
    }
}

/// Releases the chat's reservation even if the turn panics. A panic must not
/// leave the session permanently marked as running.
struct TurnGuard {
    app: AppHandle,
    chat_id: String,
    token: u64,
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        release_turn(&self.app, &self.chat_id, self.token);
    }
}

async fn run_turn(app: AppHandle, chat_id: String, token: u64, cancel: Arc<AtomicBool>) {
    let _guard = TurnGuard {
        app: app.clone(),
        chat_id: chat_id.clone(),
        token,
    };
    let result = turn(&app, &chat_id, &cancel).await;
    // Drop any modal still waiting on this turn; their receivers now treat the
    // closed channel as a cancellation, never as approval.
    clear_pending_for_chat(&app, &chat_id);
    if !is_cancelled(&cancel) {
        if let Err(e) = result {
            if e != CANCELLED {
                ev(&app, "chat:error", json!({ "chatId": chat_id, "error": e }));
                ev(&app, "chat:done", json!({ "chatId": chat_id }));
            }
        }
    }
}

async fn turn(app: &AppHandle, chat_id: &str, cancel: &AtomicBool) -> Result<(), String> {
    ensure_live(cancel)?;
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
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .map(PathBuf::from)
                .collect()
        })
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
        .map(|m| LlmMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            meta: m.meta.clone(),
        })
        .collect();
    for m in msgs.iter_mut() {
        if m.content.chars().count() > 30_000 {
            m.content = format!(
                "{}\n…[truncated]…\n{}",
                head(&m.content, 4000),
                tail(&m.content, 4000)
            );
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
        ensure_live(cancel)?;
        ev(
            app,
            "chat:status",
            json!({ "chatId": chat_id, "status": "thinking" }),
        );
        let msg_id = db::uid();
        ev(
            app,
            "chat:start",
            json!({ "chatId": chat_id, "messageId": msg_id }),
        );

        let req = LlmRequest {
            system: system.clone(),
            messages: msgs.clone(),
            tools: tool_defs.clone(),
            effort: effort.clone(),
        };

        let mut text = String::new();
        let mut think = String::new();
        let appc = app.clone();
        let cid = chat_id.to_string();
        let mid = msg_id.clone();
        let mut on_delta = |d: &Delta| {
            let (kind, value) = match d {
                Delta::Text(t) => {
                    text.push_str(t);
                    ("text", t.clone())
                }
                Delta::Thinking(t) => {
                    think.push_str(t);
                    ("thinking", t.clone())
                }
                _ => return,
            };
            let _ = appc.emit(
                "chat:delta",
                json!({ "chatId": cid, "messageId": mid, "kind": kind, "value": value }),
            );
        };

        let result = match llm::stream_chat(&pctx, &req, &mut on_delta).await {
            Ok(r) => r,
            Err(e) => {
                // One automatic retry without the reasoning-effort parameter if the
                // provider rejected it — protects against registry misses.
                let retried = if effort.is_some() {
                    let el = e.to_lowercase();
                    if el.contains("reasoning") || el.contains("thinking") || el.contains("effort")
                    {
                        let req2 = LlmRequest {
                            effort: None,
                            ..req
                        };
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

        ensure_live(cancel)?;
        if think.is_empty() && !result.thinking.is_empty() {
            think = result.thinking.clone();
        }
        let mut meta = json!({});
        if !think.is_empty() {
            meta["thinking"] = json!(think);
        }
        if !result.stop_reason.is_empty() {
            meta["stopReason"] = json!(result.stop_reason);
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
        ev(
            app,
            "chat:message",
            json!({ "chatId": chat_id, "message": am }),
        );
        msgs.push(LlmMessage {
            role: "assistant".into(),
            content: result.text.clone(),
            meta: meta.clone(),
        });

        if result.tool_calls.is_empty() {
            ev(
                app,
                "chat:done",
                json!({ "chatId": chat_id, "messageId": msg_id }),
            );
            return Ok(());
        }

        for call in &result.tool_calls {
            ensure_live(cancel)?;
            let status = match call.name.as_str() {
                "read_file" | "list_files" | "search_files" => "reading",
                "create_file" | "edit_file" | "delete_file" | "create_folder" | "rename_file" => {
                    "writing"
                }
                "run_command" | "start_process" | "stop_process" => "running",
                "ask_user" => "asking",
                _ => "working",
            };
            ev(
                app,
                "chat:status",
                json!({ "chatId": chat_id, "status": status }),
            );

            let args: Value = serde_json::from_str(&call.args_json).unwrap_or(Value::Null);
            ev(
                app,
                "chat:tool",
                json!({
                    "chatId": chat_id, "callId": call.id, "name": call.name,
                    "args": args, "state": "running"
                }),
            );
            let (state, result_text, tool_meta) = if args.is_null() {
                (
                    "error".to_string(),
                    format!("Tool arguments were not valid JSON: {}", call.args_json),
                    json!({}),
                )
            } else if call.name == "ask_user" {
                handle_ask_user(app, chat_id, &args, cancel).await
            } else {
                gate_and_execute(
                    &tctx,
                    &chat.permission_mode,
                    &call.name,
                    &args,
                    &allowed,
                    cancel,
                )
                .await
            };

            ensure_live(cancel)?;
            let mut tmeta = json!({ "toolCallId": call.id, "toolName": call.name, "state": state });
            if let (Some(src), Some(dst)) = (tool_meta.as_object(), tmeta.as_object_mut()) {
                for (k, v) in src {
                    dst.insert(k.clone(), v.clone());
                }
            }
            let result_text = with_tail_note(&result_text, 16_000);
            let tm = db::insert_message(app, chat_id, "tool", &result_text, &tmeta)?;
            ev(
                app,
                "chat:tool",
                json!({
                    "chatId": chat_id, "callId": call.id, "name": call.name,
                    "args": args, "state": state, "result": result_text, "messageId": tm.id
                }),
            );
            msgs.push(LlmMessage {
                role: "tool".into(),
                content: result_text,
                meta: json!({ "toolCallId": call.id, "toolName": call.name }),
            });

            if call.name == "todo_write" {
                let todos = db::list_todos(app, chat_id)?;
                ev(
                    app,
                    "todo:update",
                    json!({ "chatId": chat_id, "todos": todos }),
                );
            }
        }
    }

    ensure_live(cancel)?;
    let am = db::insert_message(
        app,
        chat_id,
        "assistant",
        "Stopped after 40 steps — ask me to continue if the task is not finished.",
        &json!({}),
    )?;
    ev(
        app,
        "chat:message",
        json!({ "chatId": chat_id, "message": am }),
    );
    ev(
        app,
        "chat:done",
        json!({ "chatId": chat_id, "messageId": am.id }),
    );
    Ok(())
}

async fn gate_and_execute(
    tctx: &tools::ToolCtx,
    mode: &str,
    name: &str,
    args: &Value,
    allowed: &HashSet<String>,
    cancel: &AtomicBool,
) -> (String, String, Value) {
    let app = &tctx.app;
    let chat_id = tctx.chat_id.as_str();
    if is_cancelled(cancel) {
        return ("cancelled".into(), "Cancelled".into(), json!({}));
    }
    // Resolve every target once, before the gate runs. The same canonical
    // paths are then used for the decision and the operation, so approval and
    // execution can never disagree about what is being touched, and a
    // resolution failure denies the call instead of bypassing the check.
    let resolved = match permissions::resolve_tool_paths(name, args, &tctx.workspace) {
        Ok(r) => r,
        Err(e) => {
            return (
                "denied".into(),
                format!("Blocked: could not resolve the target path safely: {e}"),
                json!({}),
            )
        }
    };
    let execute = |resolved: permissions::ResolvedPaths| async move {
        if is_cancelled(cancel) {
            return ("cancelled".to_string(), "Cancelled".to_string(), json!({}));
        }
        match tools::execute(tctx, name, args, &resolved).await {
            Ok((r, m)) => ("done".into(), r, m),
            Err(e) => ("error".into(), format!("Error: {e}"), json!({})),
        }
    };
    match permissions::gate_tool(
        mode,
        name,
        args,
        &resolved,
        &tctx.workspace,
        &tctx.extra,
        allowed,
    ) {
        Gate::Allow => execute(resolved).await,
        Gate::Deny(reason) => ("denied".into(), format!("Blocked: {reason}"), json!({})),
        Gate::Ask {
            summary,
            remember_key,
        } => {
            ev(
                app,
                "chat:status",
                json!({ "chatId": chat_id, "status": "waiting-for-permission" }),
            );
            let detail = permission_detail(name, args, &tctx.workspace);
            let decision = request_permission(app, chat_id, name, &summary, detail, cancel).await;
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
            execute(resolved).await
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
    cancel: &AtomicBool,
) -> Value {
    if is_cancelled(cancel) {
        return json!({ "allow": false, "cancelled": true });
    }
    let id = db::uid();
    let rx = match register_pending(app, &id, chat_id, PendingKind::Permission) {
        Ok(rx) => rx,
        Err(e) => return json!({ "allow": false, "error": e }),
    };
    ev(
        app,
        "permission:request",
        json!({
            "id": id, "chatId": chat_id, "tool": tool, "summary": summary, "detail": detail
        }),
    );
    match tokio::time::timeout(std::time::Duration::from_secs(600), rx).await {
        Ok(Ok(v)) => v,
        Ok(Err(_)) => json!({ "allow": false, "remember": false, "cancelled": true }),
        Err(_) => {
            // Timeout: drop the stale sender if it is still ours.
            let _ = take_pending(app, &id, PendingKind::Permission);
            json!({ "allow": false, "remember": false, "timeout": true })
        }
    }
}

async fn handle_ask_user(
    app: &AppHandle,
    chat_id: &str,
    args: &Value,
    cancel: &AtomicBool,
) -> (String, String, Value) {
    if is_cancelled(cancel) {
        return ("cancelled".into(), "Cancelled".into(), json!({}));
    }
    let questions = args.get("questions").cloned().unwrap_or(json!([]));
    let id = db::uid();
    let rx = match register_pending(app, &id, chat_id, PendingKind::Question) {
        Ok(rx) => rx,
        Err(e) => {
            return (
                "error".into(),
                format!("Could not open the question dialog: {e}"),
                json!({}),
            )
        }
    };
    ev(
        app,
        "question:request",
        json!({ "id": id, "chatId": chat_id, "questions": questions }),
    );
    let answers = match tokio::time::timeout(std::time::Duration::from_secs(900), rx).await {
        Ok(Ok(v)) => v,
        Ok(Err(_)) => json!({ "answers": {} }),
        Err(_) => {
            let _ = take_pending(app, &id, PendingKind::Question);
            json!({ "answers": {} })
        }
    };
    let ans = answers.get("answers").cloned().unwrap_or(json!({}));
    let result = if ans.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
        format!(
            "The user answered:\n{}",
            serde_json::to_string_pretty(&ans).unwrap_or_default()
        )
    } else {
        "The user did not answer (skipped or timed out). Proceed with reasonable defaults and state your assumptions.".to_string()
    };
    let meta = json!({ "card": { "questions": questions, "answers": ans.clone() } });
    ("done".into(), result, meta)
}

/// Total base64 image payload allowed per message (base64 chars). base64 is
/// ~4/3 of the binary size, so this admits roughly 18 MB of real image data.
const MAX_IMAGE_PAYLOAD_CHARS: usize = 24_000_000;
/// Per-image cap (base64 chars), ~9 MB of binary data.
const MAX_IMAGE_CHARS: usize = 12_000_000;
/// Total characters of inline file/folder context per message — the prompt
/// itself stays bounded no matter how much the user drags in.
const MAX_EXTRA_CONTEXT_CHARS: usize = 400_000;

fn expand_attachments(attachments: &[Value], _ws: &Path) -> (String, Vec<Value>) {
    let mut extra = String::new();
    let mut images: Vec<Value> = vec![];
    // Identical image payloads are sent once, not once per attachment.
    let mut seen_data: HashSet<&str> = HashSet::new();
    let mut image_payload = 0usize;
    let mut context_overflow = false;
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
                if data.is_empty() || images.len() >= 4 {
                    continue;
                }
                if data.len() > MAX_IMAGE_CHARS {
                    extra.push_str(&format!("\n[Image skipped — image too large: {path_s}]"));
                    continue;
                }
                if !seen_data.insert(data) {
                    continue;
                }
                if image_payload + data.len() > MAX_IMAGE_PAYLOAD_CHARS {
                    extra.push_str(&format!(
                        "\n[Image skipped — message payload limit: {path_s}]"
                    ));
                    continue;
                }
                image_payload += data.len();
                let mime = a
                    .get("mime")
                    .and_then(|v| v.as_str())
                    .unwrap_or("image/png");
                images.push(json!({
                    "name": a.get("name").cloned().unwrap_or(json!("image")),
                    "mime": mime,
                    "data": data,
                }));
                extra.push_str(&format!("\n[Image attached: {path_s}]"));
            }
            "folder" => {
                if context_overflow {
                    continue;
                }
                if p.exists() {
                    let t = fsops::tree(&p, 2, 200);
                    let block = format!("\n[Folder attached: {path_s}]\n{t}");
                    if extra.len() + block.len() > MAX_EXTRA_CONTEXT_CHARS {
                        context_overflow = true;
                        extra.push_str("\n[Attachment context limit reached — remaining attachments are not inlined]");
                    } else {
                        extra.push_str(&block);
                    }
                } else {
                    extra.push_str(&format!("\n[Folder attached but missing: {path_s}]"));
                }
            }
            _ => {
                if context_overflow {
                    continue;
                }
                match fsops::read_file(&p, 1, 600) {
                    Ok((content, _)) => {
                        let block = format!("\n[Attached file: {path_s}]\n```\n{content}\n```");
                        if extra.len() + block.len() > MAX_EXTRA_CONTEXT_CHARS {
                            context_overflow = true;
                            extra.push_str(
                                "\n[Attachment context limit reached — remaining attachments are not inlined]",
                            );
                        } else {
                            extra.push_str(&block);
                        }
                    }
                    Err(e) => extra.push_str(&format!(
                        "\n[Attached file: {path_s} — could not be read: {e}]"
                    )),
                }
            }
        }
    }
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
