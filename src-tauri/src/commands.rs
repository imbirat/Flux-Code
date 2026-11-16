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

/// Compute the effective connection status for a provider: the persisted
/// status from SQLite, overridden to `not_configured` when no key exists.
fn effective_status(app: &AppHandle, id: &str, has_key: bool) -> (String, String) {
    if !has_key {
        return ("not_configured".into(), String::new());
    }
    let (status, message) = db::status_get(app, id);
    match status.as_str() {
        "connected" | "failed" => (status, message),
        _ => ("untested".into(), message),
    }
}

fn has_credential(id: &str) -> bool {
    crate::creds::get_key(id)
        .map(|k| k.map(|v| !v.trim().is_empty()).unwrap_or(false))
        .unwrap_or(false)
}

/// Build the provider info JSON sent to the frontend for a built-in provider.
fn builtin_info(app: &AppHandle, p: &providers::ProviderDef) -> Value {
    let has_key = has_credential(p.id);
    let (status, message) = effective_status(app, p.id, has_key);
    let config = db::provider_config_get(app, p.id);
    json!({
        "id": p.id,
        "label": p.label,
        "format": p.format,
        "baseUrl": p.base_url,
        "helpUrl": p.help_url,
        "custom": false,
        "status": status,
        "message": message,
        "config": {
            "enabledModels": config.get("enabledModels").cloned().unwrap_or(json!([])),
            "defaultModel": config.get("defaultModel").cloned().unwrap_or(json!(null)),
        },
    })
}

/// Build the provider info JSON for a custom provider row.
fn custom_info(_app: &AppHandle, p: db::CustomProvider) -> Value {
    let has_key = has_credential(&p.id);
    let status = if !has_key {
        "not_configured".to_string()
    } else {
        match p.status.as_str() {
            "connected" | "failed" => p.status.clone(),
            _ => "untested".to_string(),
        }
    };
    json!({
        "id": p.id,
        "label": p.name,
        "format": p.format,
        "baseUrl": p.base_url,
        "helpUrl": "",
        "custom": true,
        "status": status,
        "message": p.message,
        "config": {
            "enabledModels": p.enabled_models,
            "defaultModel": if p.default_model.is_empty() { Value::Null } else { json!(p.default_model) },
            "baseUrl": p.base_url,
            "format": p.format,
            "headers": p.headers,
            "modelsPath": p.models_path,
            "manualModels": p.manual_models,
        },
    })
}

#[tauri::command]
pub fn provider_status(app: AppHandle) -> R<Vec<Value>> {
    let mut out = vec![];
    for p in providers::PROVIDERS {
        out.push(builtin_info(&app, p));
    }
    for c in db::custom_list(&app)? {
        out.push(custom_info(&app, c));
    }
    Ok(out)
}

#[tauri::command(rename_all = "camelCase")]
pub fn provider_set_key(app: AppHandle, id: String, key: String) -> R<()> {
    if key.trim().is_empty() {
        return Err("API key is empty".into());
    }
    if providers::is_builtin(&id) || db::custom_get(&app, &id).is_ok() {
        creds::set_key(&id, key.trim())?;
        // Key changed: previous test results no longer apply.
        if providers::is_builtin(&id) {
            db::status_set(&app, &id, "untested", "")?;
        } else {
            let mut row = db::custom_get(&app, &id)?;
            row.status = "untested".into();
            row.message = String::new();
            db::custom_update(&app, &row)?;
        }
        Ok(())
    } else {
        Err(format!("Unknown provider: {id}"))
    }
}

#[tauri::command(rename_all = "camelCase")]
pub fn provider_remove_key(app: AppHandle, id: String) -> R<()> {
    creds::delete_key(&id)?;
    if providers::is_builtin(&id) {
        db::status_set(&app, &id, "not_configured", "")?;
    } else if let Ok(mut row) = db::custom_get(&app, &id) {
        row.status = "not_configured".into();
        row.message = String::new();
        db::custom_update(&app, &row)?;
    }
    Ok(())
}

/// Run model discovery for a provider. Returns (models, discovery_ok, error).
async fn discover(app: &AppHandle, id: &str) -> (Vec<String>, bool, String) {
    let resolved = match providers::resolve(app, id) {
        Ok(r) => r,
        Err(e) => return (vec![], false, e),
    };
    let key = crate::creds::get_key(id).ok().flatten().unwrap_or_default();
    let models = providers::list_models_http(
        &resolved.format,
        &resolved.base_url,
        if key.is_empty() { None } else { Some(key.as_str()) },
        &resolved.extra_headers,
        &resolved.models_path,
    )
    .await;
    match models {
        Ok(m) => (m, true, String::new()),
        Err(e) => (vec![], false, e),
    }
}

/// Validate the connection: try model discovery first; for custom OpenAI-format
/// providers without a listing endpoint, fall back to a real chat ping using
/// the configured default model. The verdict is persisted.
async fn test_provider(app: &AppHandle, id: &str) -> R<Value> {
    let (models, discovery_ok, err) = discover(app, id).await;
    let (status, message) = if discovery_ok {
        (
            "connected".to_string(),
            format!("Connected — {} models available", models.len()),
        )
    } else {
        // Fallback: discovery may be unavailable on some endpoints while real
        // chat still works. Ping with the provider's native chat shape using
        // the configured default model.
        let resolved = providers::resolve(app, id).ok();
        let can_ping = resolved
            .as_ref()
            .map(|r| !r.base_url.is_empty())
            .unwrap_or(false);
        let key = crate::creds::get_key(id).ok().flatten().unwrap_or_default();
        let mut ping_ok = false;
        let mut ping_err = String::new();
        let mut ping_model = String::new();
        if can_ping && !key.is_empty() {
            ping_model = db::provider_config_get(app, id)
                .get("defaultModel")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !ping_model.is_empty() {
                if let Some(r) = &resolved {
                    match providers::ping_chat(&r.format, &r.base_url, &key, &r.extra_headers, &ping_model)
                        .await
                    {
                        Ok(()) => ping_ok = true,
                        Err(e) => ping_err = e,
                    }
                }
            }
        }
        if ping_ok {
            (
                "connected".to_string(),
                format!(
                    "Connected — model listing unavailable, using manual model \"{ping_model}\""
                ),
            )
        } else {
            let msg = if ping_err.is_empty() { err } else { ping_err };
            ("failed".to_string(), msg)
        }
    };
    if providers::is_builtin(id) {
        db::status_set(app, id, &status, &message)?;
    } else if db::custom_get(app, id).is_ok() {
        db::custom_status_set(app, id, &status, &message)?;
    }
    Ok(json!({
        "ok": status == "connected",
        "status": status,
        "message": message,
        "models": models,
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn provider_test(app: AppHandle, id: String) -> R<Value> {
    test_provider(&app, &id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn provider_list_models(app: AppHandle, id: String) -> R<Vec<String>> {
    let (models, ok, err) = discover(&app, &id).await;
    if ok {
        Ok(models)
    } else {
        Err(err)
    }
}

#[tauri::command(rename_all = "camelCase")]
pub fn provider_get_config(app: AppHandle, id: String) -> R<Value> {
    Ok(db::provider_config_get(&app, &id))
}

/// Persist enabled/default model selection. Built-ins go to the providers
/// table; custom providers update their own row.
#[tauri::command(rename_all = "camelCase")]
pub fn provider_set_config(app: AppHandle, id: String, config: Value) -> R<()> {
    if providers::is_builtin(&id) {
        db::provider_config_set(&app, &id, &config)
    } else if let Ok(mut row) = db::custom_get(&app, &id) {
        if let Some(m) = config.get("enabledModels").and_then(|v| v.as_array()) {
            row.enabled_models = m
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
        }
        if let Some(m) = config.get("defaultModel") {
            row.default_model = m.as_str().unwrap_or("").to_string();
        }
        db::custom_update(&app, &row)
    } else {
        Err(format!("Unknown provider: {id}"))
    }
}

/// Create or update a custom provider. Validation, unique id generation and
/// duplicate-name rejection happen here. The key, when supplied, is stored in
/// Windows Credential Manager under the provider's own credential id.
#[tauri::command(rename_all = "camelCase")]
#[allow(non_snake_case)]
pub async fn custom_provider_save(
    app: AppHandle,
    provider: Value,
    apiKey: Option<String>,
) -> R<Value> {
    let name = provider
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return Err("Provider name is required".into());
    }
    if name.len() > 40 {
        return Err("Provider name is too long (max 40 characters)".into());
    }
    let format = provider
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("openai")
        .to_string();
    if !["openai", "anthropic", "gemini"].contains(&format.as_str()) {
        return Err(format!("Unsupported API format: {format}"));
    }
    let base_url = provider
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .trim_end_matches('/')
        .to_string();
    if !base_url.is_empty() && !base_url.starts_with("http://") && !base_url.starts_with("https://")
    {
        return Err("Base URL must start with http:// or https://".into());
    }
    let headers = provider.get("headers").cloned().unwrap_or_else(|| json!({}));
    if !headers.is_object() {
        return Err("Headers must be a JSON object".into());
    }
    let models_path = provider
        .get("modelsPath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let manual_models_opt: Option<Vec<String>> = provider
        .get("manualModels")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect());
    let enabled_models: Vec<String> = provider
        .get("enabledModels")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let mut existing = db::custom_list(&app)?;
    let id_opt = provider.get("id").and_then(|v| v.as_str()).unwrap_or("");

    // Reject duplicate names across custom providers (case-insensitive).
    if let Some(p) = existing.iter().find(|p| p.name.eq_ignore_ascii_case(&name)) {
        if p.id != id_opt {
            return Err(format!("A custom provider named \"{name}\" already exists"));
        }
    }

    let row = match existing.iter_mut().find(|p| p.id == id_opt) {
        Some(p) => {
            p.name = name;
            p.format = format;
            p.base_url = base_url;
            p.headers = headers;
            p.models_path = models_path;
            // Explicit manualModels wins — the UI always sends the field, so
            // an emptied list clears previously saved manual ids.
            if let Some(mm) = &manual_models_opt {
                p.manual_models = providers::dedupe_sort(mm.clone());
            }
            // Editing the endpoint invalidates a previous "connected" verdict.
            if p.status == "connected" {
                p.status = "untested".into();
                p.message.clear();
            }
            p.clone()
        }
        None => {
            let slug: String = {
                let s: String = name
                    .to_lowercase()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                    .collect();
                s.trim_matches('-').to_string()
            };
            let ids: Vec<String> = existing.iter().map(|p| p.id.clone()).collect();
            db::CustomProvider {
                id: providers::unique_custom_id(&slug, &ids),
                name,
                base_url,
                format,
                headers,
                models_path,
                manual_models: providers::dedupe_sort(manual_models_opt.unwrap_or_default()),
                enabled_models,
                default_model: String::new(),
                status: "untested".into(),
                message: String::new(),
                sort: existing.len() as i64,
            }
        }
    };

    let is_update = existing.iter().any(|p| p.id == row.id);
    if is_update {
        db::custom_update(&app, &row)?;
    } else {
        db::custom_insert(&app, &row)?;
    }

    if let Some(key) = apiKey {
        if !key.trim().is_empty() {
            crate::creds::set_key(&row.id, key.trim())?;
            let mut r2 = row.clone();
            r2.status = "untested".into();
            r2.message.clear();
            db::custom_update(&app, &r2)?;
        }
    }

    Ok(custom_info(&app, db::custom_get(&app, &row.id)?))
}

/// Remove a custom provider and its credential. Chats are kept — the composer
/// will warn when a chat still references the removed provider.
#[tauri::command(rename_all = "camelCase")]
pub fn custom_provider_delete(app: AppHandle, id: String) -> R<()> {
    if db::custom_get(&app, &id).is_err() {
        return Err(format!("Unknown custom provider: {id}"));
    }
    db::custom_delete(&app, &id)?;
    let _ = crate::creds::delete_key(&id);
    Ok(())
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
