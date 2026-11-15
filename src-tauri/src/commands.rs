use crate::{agent, creds, db, llm, providers, terminal};
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::AppHandle;

type R<T> = Result<T, String>;

// ---------- settings ----------

fn default_settings() -> Value {
    json!({
        "theme": "dark",
        "accent": "flux",
        "language": "default",
        "reducedMotion": false,
        "workspace": "",
        "extraFolders": [],
        "defaultPermissionMode": "ask",
        "effortPrefs": {}
    })
}

pub fn get_settings_value(app: &AppHandle) -> Value {
    let mut v = db::setting_get(app, "app", default_settings());
    if let (Some(cur), Some(def)) = (v.as_object_mut(), default_settings().as_object()) {
        for (k, val) in def {
            cur.entry(k.clone()).or_insert_with(|| val.clone());
        }
    }
    v
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> R<Value> {
    Ok(get_settings_value(&app))
}

#[tauri::command]
pub fn set_settings(app: AppHandle, patch: Value) -> R<Value> {
    let mut cur = db::setting_get(&app, "app", default_settings());
    if let (Some(c), Some(p)) = (cur.as_object_mut(), patch.as_object()) {
        for (k, v) in p {
            c.insert(k.clone(), v.clone());
        }
    }
    db::setting_set(&app, "app", &cur)?;
    Ok(cur)
}

// ---------- chats ----------

#[tauri::command]
pub fn list_chats(app: AppHandle) -> R<Vec<db::ChatSummary>> {
    db::list_chats(&app)
}

#[tauri::command(rename_all = "camelCase")]
pub fn create_chat(
    app: AppHandle,
    workspace: String,
    provider: String,
    model: String,
    effort: String,
    permission_mode: String,
) -> R<db::ChatSummary> {
    db::create_chat(&app, &workspace, &provider, &model, &effort, &permission_mode)
}

#[tauri::command(rename_all = "camelCase")]
pub fn rename_chat(app: AppHandle, chat_id: String, title: String) -> R<()> {
    db::rename_chat(&app, &chat_id, &title)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_chat(app: AppHandle, chat_id: String) -> R<()> {
    db::delete_chat(&app, &chat_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn update_chat(app: AppHandle, chat_id: String, patch: Value) -> R<db::ChatSummary> {
    db::update_chat(&app, &chat_id, &patch)
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_messages(app: AppHandle, chat_id: String) -> R<Vec<db::StoredMessage>> {
    db::list_messages(&app, &chat_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn send_message(app: AppHandle, chat_id: String, text: String, attachments: Vec<Value>) -> R<()> {
    agent::send(&app, &chat_id, &text, attachments)
}

#[tauri::command(rename_all = "camelCase")]
pub fn cancel_chat(app: AppHandle, chat_id: String) -> R<()> {
    agent::cancel(&app, &chat_id);
    Ok(())
}

// ---------- permissions + questions ----------

#[tauri::command]
pub fn respond_permission(app: AppHandle, id: String, allow: bool, remember: bool) -> R<()> {
    agent::resolve_permission(&app, &id, allow, remember);
    Ok(())
}

#[tauri::command]
pub fn respond_questions(app: AppHandle, id: String, answers: Value) -> R<()> {
    agent::resolve_questions(&app, &id, answers);
    Ok(())
}

// ---------- todos ----------

#[tauri::command(rename_all = "camelCase")]
pub fn get_todos(app: AppHandle, chat_id: String) -> R<Vec<db::TodoItem>> {
    db::list_todos(&app, &chat_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_todos(app: AppHandle, chat_id: String, todos: Vec<db::TodoItem>) -> R<()> {
    db::set_todos(&app, &chat_id, &todos)
}

// ---------- providers ----------

#[tauri::command]
pub fn provider_status(app: AppHandle) -> R<Vec<Value>> {
    let mut out = vec![];
    for p in providers::PROVIDERS {
        let connected = creds::get_key(p.id)?.map(|k| !k.is_empty()).unwrap_or(false);
        let config = db::provider_config_get(&app, p.id);
        out.push(json!({
            "id": p.id,
            "label": p.label,
            "format": p.format,
            "baseUrl": p.base_url,
            "connected": connected,
            "config": config,
        }));
    }
    Ok(out)
}

#[tauri::command(rename_all = "camelCase")]
pub fn provider_set_key(app: AppHandle, id: String, key: String) -> R<()> {
    let _ = app;
    if key.trim().is_empty() {
        return Err("API key is empty".into());
    }
    creds::set_key(&id, key.trim())
}

#[tauri::command(rename_all = "camelCase")]
pub fn provider_remove_key(app: AppHandle, id: String) -> R<()> {
    let _ = app;
    creds::delete_key(&id)
}

async fn discover_models(app: &AppHandle, id: &str) -> R<Vec<String>> {
    let config = db::provider_config_get(app, id);
    let format = providers::format_for(id, &config);
    let base = providers::base_url_for(id, &config);
    let headers = providers::extra_headers_for(id, &config);
    let key = creds::get_key(id)?.unwrap_or_default();
    let key_ref = if key.is_empty() { None } else { Some(key.as_str()) };
    providers::list_models_http(&format, &base, key_ref, &headers).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn provider_test(app: AppHandle, id: String) -> R<Value> {
    match discover_models(&app, &id).await {
        Ok(models) => Ok(json!({
            "ok": true,
            "message": format!("Connected — {} models available", models.len()),
            "models": models,
        })),
        Err(e) => Ok(json!({ "ok": false, "message": e })),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn provider_list_models(app: AppHandle, id: String) -> R<Vec<String>> {
    discover_models(&app, &id).await
}

#[tauri::command(rename_all = "camelCase")]
pub fn provider_get_config(app: AppHandle, id: String) -> R<Value> {
    Ok(db::provider_config_get(&app, &id))
}

#[tauri::command(rename_all = "camelCase")]
pub fn provider_set_config(app: AppHandle, id: String, config: Value) -> R<()> {
    db::provider_config_set(&app, &id, &config)
}

#[tauri::command]
pub fn model_capabilities(provider: String, model: String) -> Value {
    match llm::effort_values(&provider, &model) {
        Some(values) => json!({ "supports": true, "values": values, "default": "medium" }),
        None => json!({ "supports": false, "values": [], "default": "" }),
    }
}

// ---------- workspace + files ----------

#[tauri::command]
pub fn get_default_workspace(_app: AppHandle) -> R<String> {
    agent::ensure_default_workspace();
    Ok(agent::default_workspace().to_string_lossy().to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_dir_tree(app: AppHandle, path: String, depth: u32) -> R<Vec<Value>> {
    let _ = app;
    let root = PathBuf::from(&path);
    if !root.exists() {
        return Err("Folder does not exist".into());
    }
    let mut out = vec![];
    walk_nodes(&root, 0, depth.min(6) as usize, &mut out);
    Ok(out)
}

fn walk_nodes(dir: &std::path::Path, level: usize, max_depth: usize, out: &mut Vec<Value>) {
    if level > max_depth || out.len() > 2000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| (!e.path().is_dir(), e.file_name().to_string_lossy().to_lowercase()));
    for e in entries {
        let p = e.path();
        let kind = if p.is_dir() { "dir" } else { "file" };
        out.push(json!({
            "name": e.file_name().to_string_lossy(),
            "path": p.to_string_lossy(),
            "kind": kind,
            "depth": level,
        }));
        if kind == "dir" {
            walk_nodes(&p, level + 1, max_depth, out);
        }
    }
}

#[tauri::command(rename_all = "camelCase")]
pub fn read_text_file(path: String, max_bytes: Option<u64>) -> R<Value> {
    let cap = max_bytes.unwrap_or(200_000).min(2_000_000) as usize;
    let p = PathBuf::from(&path);
    let md = std::fs::metadata(&p).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    if bytes.contains(&0) {
        return Err("Binary file".into());
    }
    let take = bytes.len().min(cap);
    let content = String::from_utf8_lossy(&bytes[..take]).to_string();
    Ok(json!({ "content": content, "truncated": take < bytes.len(), "size": md.len() }))
}

#[tauri::command(rename_all = "camelCase")]
pub fn read_file_base64(path: String) -> R<String> {
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    if bytes.len() > 8_000_000 {
        return Err("File too large (max 8 MB)".into());
    }
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[tauri::command]
pub fn open_external(app: AppHandle, target: String) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    if target.starts_with("http://") || target.starts_with("https://") {
        app.opener().open_url(target, None::<&str>).map_err(|e| e.to_string())
    } else {
        app.opener().open_path(target, None::<&str>).map_err(|e| e.to_string())
    }
}

// ---------- processes ----------

#[tauri::command]
pub fn list_processes(app: AppHandle) -> R<Vec<Value>> {
    terminal::list_json(&app)
}

#[tauri::command]
pub fn stop_process(app: AppHandle, id: u32) -> R<()> {
    terminal::stop(&app, id)
}
