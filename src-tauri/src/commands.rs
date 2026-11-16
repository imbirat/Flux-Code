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
            "manualModels": config.get("manualModels").cloned().unwrap_or(json!([])),
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

/// Pick the model used for a connection-test ping: the configured default
/// model first, then manual IDs, then enabled ones. Empty when nothing is
/// configured. Pure so the precedence is unit-testable.
fn ping_candidate(default_model: &str, manual: &[String], enabled: &[String]) -> String {
    let d = default_model.trim();
    if !d.is_empty() {
        return d.to_string();
    }
    manual
        .iter()
        .chain(enabled.iter())
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

/// The ping model for a provider id, reading the correct store per kind:
/// built-ins from the providers config table, customs from their own row.
fn ping_model_for(app: &AppHandle, id: &str) -> String {
    if providers::is_builtin(id) {
        let cfg = db::provider_config_get(app, id);
        let def = cfg.get("defaultModel").and_then(|v| v.as_str()).unwrap_or("");
        let enabled: Vec<String> = cfg
            .get("enabledModels")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        ping_candidate(def, &[], &enabled)
    } else {
        db::custom_get(app, id)
            .map(|row| ping_candidate(&row.default_model, &row.manual_models, &row.enabled_models))
            .unwrap_or_default()
    }
}

/// How many manually configured models a provider has (for status messaging).
fn manual_model_count(app: &AppHandle, id: &str) -> usize {
    if providers::is_builtin(id) {
        db::provider_config_get(app, id)
            .get("manualModels")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0)
    } else {
        db::custom_get(app, id)
            .map(|row| row.manual_models.len())
            .unwrap_or(0)
    }
}

/// Decide the connection verdict from discovery and an optional chat ping.
///
/// Model discovery is deliberately separate from connectivity: an HTTP
/// success always means the key and endpoint are valid, even when the catalog
/// is empty. The ping only runs when listing failed outright (e.g. endpoints
/// without a models route), validating the key against the real chat API.
/// Pure so the whole decision matrix is unit-testable.
fn connection_verdict(
    discovery: Result<Vec<String>, String>,
    ping: Option<Result<String, String>>,
    manual_count: usize,
) -> (String, String) {
    match discovery {
        Ok(models) if !models.is_empty() => (
            "connected".to_string(),
            format!("Connected — {} models available", models.len()),
        ),
        Ok(_) => {
            if manual_count > 0 {
                (
                    "connected".to_string(),
                    format!(
                        "Connected — no models discovered. {manual_count} manual model{} configured.",
                        if manual_count == 1 { "" } else { "s" }
                    ),
                )
            } else {
                (
                    "connected".to_string(),
                    "Connected — no models discovered. Add a model ID manually.".to_string(),
                )
            }
        }
        Err(err) => match ping {
            Some(Ok(model)) => (
                "connected".to_string(),
                format!("Connected — model listing unavailable, using manual model \"{model}\""),
            ),
            Some(Err(ping_err)) => ("failed".to_string(), ping_err),
            None => ("failed".to_string(), err),
        },
    }
}

/// Validate the connection: try model discovery first; for endpoints without
/// a usable listing route, fall back to a real chat ping using the provider's
/// own configured model (custom providers read their own row, built-ins the
/// providers config table). The verdict is persisted.
async fn test_provider(app: &AppHandle, id: &str) -> R<Value> {
    let (models, discovery_ok, err) = discover(app, id).await;
    let discovery = if discovery_ok {
        Ok(models.clone())
    } else {
        Err(err.clone())
    };

    // Fallback ping: only when listing failed. Ping with the provider's
    // native chat shape using the model the user actually configured.
    let mut ping: Option<Result<String, String>> = None;
    if !discovery_ok {
        if let Some(r) = providers::resolve(app, id).ok() {
            if !r.base_url.is_empty() {
                let key = crate::creds::get_key(id).ok().flatten().unwrap_or_default();
                let ping_model = ping_model_for(app, id);
                if !key.is_empty() && !ping_model.is_empty() {
                    match providers::ping_chat(&r.format, &r.base_url, &key, &r.extra_headers, &ping_model)
                        .await
                    {
                        Ok(()) => ping = Some(Ok(ping_model)),
                        Err(e) => ping = Some(Err(e)),
                    }
                }
            }
        }
    }

    let manual_count = manual_model_count(app, id);
    let (status, message) = connection_verdict(discovery, ping, manual_count);
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
/// table; custom providers update their own row. An optional `manualModels`
/// list (the union persisted by Select Models) is honored for both kinds so
/// manually added IDs survive reopening the app.
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
        if let Some(m) = config.get("manualModels").and_then(|v| v.as_array()) {
            row.manual_models = providers::dedupe_sort(
                m.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- connection verdicts (issue: status vs discovery) ----------

    #[test]
    fn verdict_connected_with_models() {
        let (s, m) = connection_verdict(
            Ok(vec!["a".into(), "b".into()]),
            None,
            0,
        );
        assert_eq!((s.as_str(), m.as_str()), ("connected", "Connected — 2 models available"));
    }

    #[test]
    fn verdict_connected_when_catalog_empty() {
        // An authenticated HTTP success is "connected" even with no models.
        let (s, m) = connection_verdict(Ok(vec![]), None, 0);
        assert_eq!(
            (s.as_str(), m.as_str()),
            ("connected", "Connected — no models discovered. Add a model ID manually.")
        );
    }

    #[test]
    fn verdict_connected_mentions_manual_models() {
        let (s, m) = connection_verdict(Ok(vec![]), None, 2);
        assert_eq!(
            (s.as_str(), m.as_str()),
            ("connected", "Connected — no models discovered. 2 manual models configured.")
        );
        let (s, m) = connection_verdict(Ok(vec![]), None, 1);
        assert_eq!(
            (s.as_str(), m.as_str()),
            ("connected", "Connected — no models discovered. 1 manual model configured.")
        );
    }

    #[test]
    fn verdict_ping_success_overrides_listing_failure() {
        let (s, m) = connection_verdict(
            Err("Provider error 404: no such route".into()),
            Some(Ok("my-model".into())),
            3,
        );
        assert_eq!(
            (s.as_str(), m.as_str()),
            ("connected", "Connected — model listing unavailable, using manual model \"my-model\"")
        );
    }

    #[test]
    fn verdict_ping_failure_is_reported_not_the_listing_error() {
        let (s, m) = connection_verdict(
            Err("Provider error 404: no such route".into()),
            Some(Err("Provider error 401: invalid key — check the API key in Settings › Providers".into())),
            3,
        );
        assert_eq!(s, "failed");
        assert!(m.contains("Provider error 401"));
        assert!(!m.contains("no such route"));
    }

    #[test]
    fn verdict_no_ping_reports_discovery_error() {
        let (s, m) = connection_verdict(Err("Network error: connection refused".into()), None, 0);
        assert_eq!((s.as_str(), m.as_str()), ("failed", "Network error: connection refused"));
    }

    #[test]
    fn verdict_messages_never_leak_secrets() {
        // Whatever the inputs, the verdict text must not contain a key-like
        // token; errors are provider-provided status lines only.
        let (_, m) = connection_verdict(
            Err("Provider error 401: invalid key".into()),
            Some(Err("Provider error 403: forbidden".into())),
            5,
        );
        assert!(!m.contains("sk-"));
        assert!(!m.contains("Bearer"));
    }

    // ---------- ping model precedence (issue: custom default model) ----------

    #[test]
    fn ping_candidate_prefers_default_then_manual_then_enabled() {
        assert_eq!(ping_candidate("default-m", &["manual-m".into()], &["enabled-m".into()]), "default-m");
        assert_eq!(ping_candidate("", &["manual-m".into()], &["enabled-m".into()]), "manual-m");
        assert_eq!(ping_candidate("", &[], &["enabled-m".into()]), "enabled-m");
        assert_eq!(ping_candidate("", &[], &[]), "");
        // Blank entries are skipped, not returned.
        assert_eq!(ping_candidate("  ", &["  ".into(), "real".into()], &[]), "real");
    }
}
