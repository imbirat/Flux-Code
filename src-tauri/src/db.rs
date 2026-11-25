use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct Db(pub Mutex<Connection>);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChatSummary {
    pub id: String,
    pub title: String,
    pub workspace: String,
    pub provider: String,
    pub model: String,
    pub effort: String,
    pub permission_mode: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Project the session belongs to; `None` for legacy rows not yet matched
    /// to a project or after their project was removed.
    pub project_id: Option<String>,
}

/// A saved project: a stable identity for a workspace directory.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    /// Canonical absolute filesystem path (unique, case-insensitive on
    /// Windows-style filesystems).
    pub path: String,
    pub sort: i64,
    pub created_at: i64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub id: String,
    pub chat_id: String,
    pub role: String,
    pub content: String,
    pub meta: Value,
    pub created_at: i64,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    #[serde(default)]
    pub id: String,
    pub content: String,
    pub status: String,
    #[serde(default)]
    pub sort: i64,
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn uid() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn init(app: &AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("flux.sqlite");
    let conn = Connection::open(&path).map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| e.to_string())?;
    migrate(&conn)?;
    seed_projects(&conn, &crate::agent::default_workspace().to_string_lossy())?;
    app.manage(Db(Mutex::new(conn)));
    Ok(())
}

fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS chats(
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT 'New chat',
            workspace TEXT NOT NULL DEFAULT '',
            provider TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            effort TEXT NOT NULL DEFAULT '',
            permission_mode TEXT NOT NULL DEFAULT 'ask',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS messages(
            id TEXT PRIMARY KEY,
            chat_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            meta TEXT NOT NULL DEFAULT '{}',
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_messages_chat ON messages(chat_id);
        CREATE TABLE IF NOT EXISTS todos(
            id TEXT PRIMARY KEY,
            chat_id TEXT NOT NULL,
            content TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            sort INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS providers(id TEXT PRIMARY KEY, config TEXT NOT NULL DEFAULT '{}');
        CREATE TABLE IF NOT EXISTS custom_providers(
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            base_url TEXT NOT NULL DEFAULT '',
            format TEXT NOT NULL DEFAULT 'openai',
            headers TEXT NOT NULL DEFAULT '{}',
            models_path TEXT NOT NULL DEFAULT '',
            manual_models TEXT NOT NULL DEFAULT '[]',
            enabled_models TEXT NOT NULL DEFAULT '[]',
            default_model TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'untested',
            message TEXT NOT NULL DEFAULT '',
            sort INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS provider_status(
            provider_id TEXT PRIMARY KEY,
            status TEXT NOT NULL DEFAULT 'untested',
            message TEXT NOT NULL DEFAULT '',
            checked_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS projects(
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE COLLATE NOCASE,
            sort INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );
        "#,
    )
    .map_err(|e| e.to_string())?;
    ensure_chat_project_column(conn)
}

/// Add `chats.project_id` for databases created before v1.2.3. Existing rows
/// keep `NULL` until `seed_projects` assigns them.
fn ensure_chat_project_column(conn: &Connection) -> Result<(), String> {
    let has_column: bool = conn
        .prepare("PRAGMA table_info(chats)")
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .any(|c| c.map(|n| n == "project_id").unwrap_or(false));
    if !has_column {
        conn.execute_batch("ALTER TABLE chats ADD COLUMN project_id TEXT")
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// One-time project seeding for databases from before v1.2.3. Creates one
/// project per distinct chat workspace (plus the default workspace when there
/// is no project at all) and links every chat to the project matching its
/// workspace. A persisted marker guarantees this never runs again, so removing
/// the last project does not silently recreate it on the next launch.
pub fn seed_projects(conn: &Connection, default_workspace: &str) -> Result<(), String> {
    let already: bool = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'projects_seeded'",
            [],
            |r| r.get::<_, String>(0),
        )
        .map(|v| v.trim() == "true" || v.trim() == "1")
        .unwrap_or(false);
    if already {
        return Ok(());
    }

    let project_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM projects", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if project_count == 0 {
        // Projects from the distinct workspaces of existing chats.
        let mut workspaces: Vec<String> = {
            let mut st = conn
                .prepare(
                    "SELECT DISTINCT workspace FROM chats WHERE workspace != '' ORDER BY workspace",
                )
                .map_err(|e| e.to_string())?;
            let rows = st
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        };
        // Always seed the default workspace project when nothing exists yet.
        if workspaces.is_empty() {
            workspaces.push(default_workspace.to_string());
        }
        for (i, ws) in workspaces.iter().enumerate() {
            let path = normalize_project_path(ws);
            if path.is_empty() {
                continue;
            }
            let name = std::path::Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            conn.execute(
                "INSERT OR IGNORE INTO projects(id,name,path,sort,created_at) VALUES(?1,?2,?3,?4,?5)",
                rusqlite::params![uid(), name, path, i as i64, now_ms()],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    // Link chats to the project matching their workspace (case-insensitive).
    conn.execute(
        "UPDATE chats SET project_id = (
             SELECT p.id FROM projects p
             WHERE p.path = chats.workspace COLLATE NOCASE
             ORDER BY p.sort LIMIT 1
         )
         WHERE project_id IS NULL AND workspace != ''",
        [],
    )
    .map_err(|e| e.to_string())?;
    let default_project: Option<String> = conn
        .query_row(
            "SELECT id FROM projects WHERE path = ?1 COLLATE NOCASE ORDER BY sort LIMIT 1",
            [normalize_project_path(default_workspace)],
            |r| r.get(0),
        )
        .ok();
    if let Some(pid) = default_project {
        conn.execute(
            "UPDATE chats SET project_id = ?1 WHERE project_id IS NULL",
            [pid],
        )
        .map_err(|e| e.to_string())?;
    }
    conn.execute(
        "INSERT INTO settings(key,value) VALUES('projects_seeded','true')
         ON CONFLICT(key) DO UPDATE SET value = 'true'",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Normalize a project path for storage: trimmed, without a trailing
/// separator except for drive roots, where `C:` would be an invalid
/// drive-relative path and must stay `C:\`.
pub fn normalize_project_path(path: &str) -> String {
    let trimmed = path.trim();
    let stripped = trimmed.trim_end_matches(['/', '\\']);
    if stripped.len() == 2 && stripped.as_bytes().get(1) == Some(&b':') {
        format!("{stripped}\\")
    } else if stripped.is_empty() {
        trimmed.to_string()
    } else {
        stripped.to_string()
    }
}

/// Run a closure with the database connection. Never call `.await` inside `f`.
pub fn with<T>(
    app: &AppHandle,
    f: impl FnOnce(&mut Connection) -> Result<T, String>,
) -> Result<T, String> {
    let db = app.state::<Db>();
    let mut c = db.0.lock().map_err(|_| "database locked".to_string())?;
    f(&mut c)
}

fn row_to_chat(r: &rusqlite::Row) -> rusqlite::Result<ChatSummary> {
    Ok(ChatSummary {
        id: r.get(0)?,
        title: r.get(1)?,
        workspace: r.get(2)?,
        provider: r.get(3)?,
        model: r.get(4)?,
        effort: r.get(5)?,
        permission_mode: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
        project_id: r.get(9)?,
    })
}

const CHAT_COLS: &str =
    "id,title,workspace,provider,model,effort,permission_mode,created_at,updated_at,project_id";

fn chats_list_conn(c: &Connection) -> Result<Vec<ChatSummary>, String> {
    let sql = format!("SELECT {CHAT_COLS} FROM chats ORDER BY updated_at DESC");
    let mut st = c.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = st
        .query_map([], row_to_chat)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn list_chats(app: &AppHandle) -> Result<Vec<ChatSummary>, String> {
    with(app, |c| chats_list_conn(c))
}

pub fn create_chat(
    app: &AppHandle,
    workspace: &str,
    provider: &str,
    model: &str,
    effort: &str,
    mode: &str,
    project_id: Option<&str>,
) -> Result<ChatSummary, String> {
    let id = uid();
    let t = now_ms();
    let mut final_workspace = workspace.to_string();
    let mut final_project: Option<String> = None;
    with(app, |c| {
        // A session's workspace always agrees with its project: when a project
        // is supplied, the project's canonical path wins. A dangling project id
        // is dropped rather than stored.
        if let Some(pid) = project_id {
            if let Ok(path) = c.query_row("SELECT path FROM projects WHERE id = ?1", [pid], |r| {
                r.get::<_, String>(0)
            }) {
                final_workspace = path;
                final_project = Some(pid.to_string());
            }
        }
        c.execute(
            "INSERT INTO chats(id,title,workspace,provider,model,effort,permission_mode,created_at,updated_at,project_id)
             VALUES(?1,'New chat',?2,?3,?4,?5,?6,?7,?7,?8)",
            rusqlite::params![id, final_workspace, provider, model, effort, mode, t, final_project],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })?;
    Ok(ChatSummary {
        id,
        title: "New chat".into(),
        workspace: final_workspace,
        provider: provider.into(),
        model: model.into(),
        effort: effort.into(),
        permission_mode: mode.into(),
        created_at: t,
        updated_at: t,
        project_id: final_project,
    })
}

pub fn update_chat(app: &AppHandle, id: &str, patch: &Value) -> Result<ChatSummary, String> {
    with(app, |c| {
        let current = chat_get_conn(c, id)?;
        let mut sets: Vec<(String, Option<String>)> = vec![];
        for (key, col) in [
            ("title", "title"),
            ("provider", "provider"),
            ("model", "model"),
            ("effort", "effort"),
            ("permissionMode", "permission_mode"),
        ] {
            if let Some(v) = patch.get(key).and_then(|v| v.as_str()) {
                sets.push((format!("{col} = ?"), Some(v.to_string())));
            }
        }

        let mut new_workspace = patch
            .get("workspace")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let mut project_patch: Option<Option<String>> = None;

        if let Some(pv) = patch.get("projectId") {
            if pv.is_null() {
                project_patch = Some(None);
            } else if let Some(pid) = pv.as_str() {
                // Refuse a dangling association; the workspace follows the
                // project's canonical path.
                let path = c
                    .query_row("SELECT path FROM projects WHERE id = ?1", [pid], |r| {
                        r.get::<_, String>(0)
                    })
                    .map_err(|_| format!("Unknown project: {pid}"))?;
                project_patch = Some(Some(pid.to_string()));
                new_workspace = Some(path);
            }
        } else if let Some(ws) = &new_workspace {
            // A workspace change leaves the project when the folder no longer
            // matches it; keeping the old association would be a silent lie.
            if let Some(pid) = &current.project_id {
                let same = c
                    .query_row("SELECT path FROM projects WHERE id = ?1", [pid], |r| {
                        r.get::<_, String>(0)
                    })
                    .map(|p| p.eq_ignore_ascii_case(ws))
                    .unwrap_or(false);
                if !same {
                    project_patch = Some(None);
                }
            }
        }

        if let Some(ws) = new_workspace {
            let normalized = crate::permissions::resolve_real(std::path::Path::new(&ws))
                .map(|p| {
                    crate::permissions::to_plain(&p)
                        .to_string_lossy()
                        .to_string()
                })
                .unwrap_or(ws);
            sets.push(("workspace = ?".into(), Some(normalized)));
        }
        match project_patch {
            Some(Some(pid)) => sets.push(("project_id = ?".into(), Some(pid))),
            Some(None) => sets.push(("project_id = NULL".into(), None)),
            None => {}
        }

        if !sets.is_empty() {
            sets.push(("updated_at = ?".into(), Some(now_ms().to_string())));
            let mut parts: Vec<String> = vec![];
            let mut vals: Vec<String> = vec![];
            for (expr, val) in &sets {
                parts.push(expr.clone());
                if let Some(v) = val {
                    vals.push(v.clone());
                }
            }
            vals.push(id.to_string());
            let sql = format!("UPDATE chats SET {} WHERE id = ?", parts.join(", "));
            c.execute(&sql, rusqlite::params_from_iter(vals.iter()))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    get_chat(app, id)
}

fn chat_get_conn(c: &Connection, id: &str) -> Result<ChatSummary, String> {
    let sql = format!("SELECT {CHAT_COLS} FROM chats WHERE id = ?1");
    let mut st = c.prepare(&sql).map_err(|e| e.to_string())?;
    st.query_row([id], row_to_chat)
        .map_err(|e| format!("chat not found: {e}"))
}

pub fn get_chat(app: &AppHandle, id: &str) -> Result<ChatSummary, String> {
    with(app, |c| chat_get_conn(c, id))
}

pub fn rename_chat(app: &AppHandle, id: &str, title: &str) -> Result<(), String> {
    with(app, |c| {
        c.execute(
            "UPDATE chats SET title = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![title, now_ms(), id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn delete_chat(app: &AppHandle, id: &str) -> Result<(), String> {
    with(app, |c| {
        let tx = c.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM messages WHERE chat_id = ?1", [id])
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM todos WHERE chat_id = ?1", [id])
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM chats WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn insert_message(
    app: &AppHandle,
    chat_id: &str,
    role: &str,
    content: &str,
    meta: &Value,
) -> Result<StoredMessage, String> {
    let id = uid();
    let t = now_ms();
    let meta_s = serde_json::to_string(meta).unwrap_or_else(|_| "{}".into());
    with(app, |c| {
        let tx = c.transaction().map_err(|e| e.to_string())?;
        // Fail closed for a deleted chat: a cancelled or late turn must never
        // create orphan message rows that could reappear under a reused id.
        let exists: i64 = tx
            .query_row("SELECT COUNT(*) FROM chats WHERE id = ?1", [chat_id], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())?;
        if exists == 0 {
            return Err("Chat no longer exists — message was not saved".into());
        }
        tx.execute(
            "INSERT INTO messages(id,chat_id,role,content,meta,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![id, chat_id, role, content, meta_s, t],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE chats SET updated_at = ?1 WHERE id = ?2",
            rusqlite::params![t, chat_id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })?;
    Ok(StoredMessage {
        id,
        chat_id: chat_id.into(),
        role: role.into(),
        content: content.into(),
        meta: meta.clone(),
        created_at: t,
    })
}

pub fn list_messages(app: &AppHandle, chat_id: &str) -> Result<Vec<StoredMessage>, String> {
    with(app, |c| {
        let mut st = c
            .prepare("SELECT id,chat_id,role,content,meta,created_at FROM messages WHERE chat_id = ?1 ORDER BY rowid")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([chat_id], |r| {
                let meta_s: String = r.get(4)?;
                Ok(StoredMessage {
                    id: r.get(0)?,
                    chat_id: r.get(1)?,
                    role: r.get(2)?,
                    content: r.get(3)?,
                    meta: serde_json::from_str(&meta_s).unwrap_or(Value::Null),
                    created_at: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    })
}

pub fn set_todos(app: &AppHandle, chat_id: &str, items: &[TodoItem]) -> Result<(), String> {
    with(app, |c| {
        let tx = c.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM todos WHERE chat_id = ?1", [chat_id])
            .map_err(|e| e.to_string())?;
        for (i, item) in items.iter().enumerate() {
            let id = if item.id.is_empty() {
                uid()
            } else {
                item.id.clone()
            };
            tx.execute(
                "INSERT INTO todos(id,chat_id,content,status,sort) VALUES(?1,?2,?3,?4,?5)",
                rusqlite::params![id, chat_id, item.content, item.status, i as i64],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn list_todos(app: &AppHandle, chat_id: &str) -> Result<Vec<TodoItem>, String> {
    with(app, |c| {
        let mut st = c
            .prepare("SELECT id,content,status,sort FROM todos WHERE chat_id = ?1 ORDER BY sort")
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map([chat_id], |r| {
                Ok(TodoItem {
                    id: r.get(0)?,
                    content: r.get(1)?,
                    status: r.get(2)?,
                    sort: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    })
}

pub fn setting_get(app: &AppHandle, key: &str, default: Value) -> Value {
    with(app, |c| {
        let default = default.clone();
        let mut st = c
            .prepare("SELECT value FROM settings WHERE key = ?1")
            .map_err(|e| e.to_string())?;
        let v: String = st
            .query_row([key], |r| r.get(0))
            .unwrap_or_else(|_| default.to_string());
        let parsed = serde_json::from_str::<Value>(&v).unwrap_or(default);
        Ok(parsed)
    })
    .unwrap_or(default)
}

pub fn setting_set(app: &AppHandle, key: &str, value: &Value) -> Result<(), String> {
    let s = serde_json::to_string(value).map_err(|e| e.to_string())?;
    with(app, |c| {
        c.execute(
            "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            rusqlite::params![key, s],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn provider_config_get(app: &AppHandle, id: &str) -> Value {
    with(app, |c| {
        let mut st = c
            .prepare("SELECT config FROM providers WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let v: String = st
            .query_row([id], |r| r.get(0))
            .unwrap_or_else(|_| "{}".into());
        let parsed = serde_json::from_str::<Value>(&v).unwrap_or(Value::Null);
        Ok(parsed)
    })
    .unwrap_or(Value::Null)
}

pub fn provider_config_set(app: &AppHandle, id: &str, config: &Value) -> Result<(), String> {
    let s = serde_json::to_string(config).map_err(|e| e.to_string())?;
    with(app, |c| {
        c.execute(
            "INSERT INTO providers(id,config) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET config = ?2",
            rusqlite::params![id, s],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

// ---------- custom providers ----------

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CustomProvider {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default)]
    pub headers: Value,
    #[serde(default)]
    pub models_path: String,
    #[serde(default)]
    pub manual_models: Vec<String>,
    #[serde(default)]
    pub enabled_models: Vec<String>,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub sort: i64,
}

fn default_format() -> String {
    "openai".into()
}

const CUSTOM_COLS: &str = "id,name,base_url,format,headers,models_path,manual_models,enabled_models,default_model,status,message,sort";

fn row_to_custom(r: &rusqlite::Row) -> rusqlite::Result<CustomProvider> {
    let headers: String = r.get(4)?;
    let manual: String = r.get(6)?;
    let enabled: String = r.get(7)?;
    Ok(CustomProvider {
        id: r.get(0)?,
        name: r.get(1)?,
        base_url: r.get(2)?,
        format: r.get(3)?,
        headers: serde_json::from_str(&headers).unwrap_or_else(|_| json!({})),
        models_path: r.get(5)?,
        manual_models: serde_json::from_str(&manual).unwrap_or_default(),
        enabled_models: serde_json::from_str(&enabled).unwrap_or_default(),
        default_model: r.get(8)?,
        status: r.get(9)?,
        message: r.get(10)?,
        sort: r.get(11)?,
    })
}

pub fn custom_list(app: &AppHandle) -> Result<Vec<CustomProvider>, String> {
    with(app, |c| {
        let sql = format!("SELECT {CUSTOM_COLS} FROM custom_providers ORDER BY sort, created_at");
        let mut st = c.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], row_to_custom)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    })
}

pub fn custom_get(app: &AppHandle, id: &str) -> Result<CustomProvider, String> {
    with(app, |c| {
        let sql = format!("SELECT {CUSTOM_COLS} FROM custom_providers WHERE id = ?1");
        let mut st = c.prepare(&sql).map_err(|e| e.to_string())?;
        st.query_row([id], row_to_custom)
            .map_err(|e| format!("custom provider not found: {e}"))
    })
}

pub fn custom_insert(app: &AppHandle, p: &CustomProvider) -> Result<(), String> {
    let headers = serde_json::to_string(&p.headers).map_err(|e| e.to_string())?;
    let manual = serde_json::to_string(&p.manual_models).map_err(|e| e.to_string())?;
    let enabled = serde_json::to_string(&p.enabled_models).map_err(|e| e.to_string())?;
    with(app, |c| {
        c.execute(
            "INSERT INTO custom_providers(id,name,base_url,format,headers,models_path,manual_models,enabled_models,default_model,status,message,sort,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            rusqlite::params![
                p.id, p.name, p.base_url, p.format, headers, p.models_path, manual, enabled,
                p.default_model, p.status, p.message, p.sort, now_ms()
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn custom_update(app: &AppHandle, p: &CustomProvider) -> Result<(), String> {
    let headers = serde_json::to_string(&p.headers).map_err(|e| e.to_string())?;
    let manual = serde_json::to_string(&p.manual_models).map_err(|e| e.to_string())?;
    let enabled = serde_json::to_string(&p.enabled_models).map_err(|e| e.to_string())?;
    with(app, |c| {
        c.execute(
            "UPDATE custom_providers SET name=?2, base_url=?3, format=?4, headers=?5, models_path=?6,
             manual_models=?7, enabled_models=?8, default_model=?9, status=?10, message=?11 WHERE id=?1",
            rusqlite::params![
                p.id, p.name, p.base_url, p.format, headers, p.models_path, manual, enabled,
                p.default_model, p.status, p.message
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

pub fn custom_delete(app: &AppHandle, id: &str) -> Result<(), String> {
    with(app, |c| {
        c.execute("DELETE FROM custom_providers WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// Persist a connection status for a custom provider (status without the key).
pub fn custom_status_set(
    app: &AppHandle,
    id: &str,
    status: &str,
    message: &str,
) -> Result<(), String> {
    with(app, |c| {
        c.execute(
            "UPDATE custom_providers SET status = ?2, message = ?3 WHERE id = ?1",
            rusqlite::params![id, status, message],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

// ---------- projects ----------

fn row_to_project(r: &rusqlite::Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        path: r.get(2)?,
        sort: r.get(3)?,
        created_at: r.get(4)?,
    })
}

fn projects_list_conn(c: &Connection) -> Result<Vec<Project>, String> {
    let mut st = c
        .prepare("SELECT id,name,path,sort,created_at FROM projects ORDER BY sort, created_at")
        .map_err(|e| e.to_string())?;
    let rows = st
        .query_map([], row_to_project)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn list_projects(app: &AppHandle) -> Result<Vec<Project>, String> {
    with(app, |c| projects_list_conn(c))
}

/// Insert or return the existing project for a canonical path. Safe against
/// duplicates (the path is UNIQUE COLLATE NOCASE) and races (INSERT OR IGNORE
/// then re-read). An empty display name falls back to the folder basename.
fn project_upsert_conn(c: &Connection, path: &str, name: &str) -> Result<Project, String> {
    let path = normalize_project_path(path);
    if path.is_empty() {
        return Err("Project path is empty".into());
    }
    let name = {
        let n = name.trim();
        if !n.is_empty() {
            n.to_string()
        } else {
            std::path::Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone())
        }
    };
    if let Ok(existing) = c
        .prepare("SELECT id,name,path,sort,created_at FROM projects WHERE path = ?1 COLLATE NOCASE")
        .and_then(|mut st| st.query_row([&path], row_to_project))
    {
        return Ok(existing);
    }
    let sort: i64 = c
        .query_row(
            "SELECT COALESCE(MAX(sort), -1) + 1 FROM projects",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    conn_execute_ignored(
        c,
        "INSERT OR IGNORE INTO projects(id,name,path,sort,created_at) VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params![uid(), name, path, sort, now_ms()],
    )?;
    c.prepare("SELECT id,name,path,sort,created_at FROM projects WHERE path = ?1 COLLATE NOCASE")
        .and_then(|mut st| st.query_row([&path], row_to_project))
        .map_err(|e| format!("project insert failed: {e}"))
}

fn conn_execute_ignored(
    c: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<(), String> {
    c.execute(sql, params)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Register (or return) a project for a directory path. The caller must pass
/// an existing, accessible directory path.
pub fn project_add(app: &AppHandle, path: &str, name: &str) -> Result<Project, String> {
    let path = normalize_project_path(path);
    if path.is_empty() {
        return Err("Project path is empty".into());
    }
    if !std::path::Path::new(&path).is_dir() {
        return Err(format!(
            "Folder does not exist or is not accessible: {path}"
        ));
    }
    let name = name.trim();
    if !name.is_empty() && name.len() > 60 {
        return Err("Project name is too long (max 60 characters)".into());
    }
    with(app, |c| project_upsert_conn(c, &path, name))
}

pub fn project_rename(app: &AppHandle, id: &str, name: &str) -> Result<Project, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Project name is required".into());
    }
    if name.len() > 60 {
        return Err("Project name is too long (max 60 characters)".into());
    }
    with(app, |c| {
        let n = c
            .execute(
                "UPDATE projects SET name = ?1 WHERE id = ?2",
                rusqlite::params![name, id],
            )
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("Unknown project: {id}"));
        }
        c.prepare("SELECT id,name,path,sort,created_at FROM projects WHERE id = ?1")
            .and_then(|mut st| st.query_row([id], row_to_project))
            .map_err(|e| format!("project not found: {e}"))
    })
}

/// Remove a project row without touching its sessions (their `project_id`
/// becomes NULL) or the files on disk.
pub fn project_remove(app: &AppHandle, id: &str) -> Result<(), String> {
    with(app, |c| {
        let tx = c.transaction().map_err(|e| e.to_string())?;
        let n = tx
            .execute("DELETE FROM projects WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err(format!("Unknown project: {id}"));
        }
        tx.execute(
            "UPDATE chats SET project_id = NULL WHERE project_id = ?1",
            [id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    })
}

// ---------- built-in provider connection status ----------

pub fn status_get(app: &AppHandle, id: &str) -> (String, String) {
    with(app, |c| {
        let mut st = c
            .prepare("SELECT status, message FROM provider_status WHERE provider_id = ?1")
            .map_err(|e| e.to_string())?;
        let (s, m): (String, String) = st
            .query_row([id], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap_or_else(|_| ("untested".into(), "".into()));
        Ok((s, m))
    })
    .unwrap_or(("untested".into(), "".into()))
}

pub fn status_set(app: &AppHandle, id: &str, status: &str, message: &str) -> Result<(), String> {
    with(app, |c| {
        c.execute(
            "INSERT INTO provider_status(provider_id,status,message,checked_at) VALUES(?1,?2,?3,?4)
             ON CONFLICT(provider_id) DO UPDATE SET status=?2, message=?3, checked_at=?4",
            rusqlite::params![id, status, message, now_ms()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// One-time migration of the legacy single "custom" provider row (config JSON
/// with a nested `custom` object) into the `custom_providers` table. The API
/// key stored under the legacy credential account is copied to the new id.
pub fn migrate_legacy_custom(app: &AppHandle) -> Result<(), String> {
    let config = provider_config_get(app, "custom");
    if config.is_null() {
        return Ok(());
    }
    let nested = config.get("custom");
    let name = nested
        .and_then(|c| c.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let base = nested
        .and_then(|c| c.get("baseUrl"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let format = nested
        .and_then(|c| c.get("format"))
        .and_then(|v| v.as_str())
        .unwrap_or("openai")
        .to_string();
    let headers = nested
        .and_then(|c| c.get("headers"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let enabled: Vec<String> = config
        .get("enabledModels")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let default_model = config
        .get("defaultModel")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Only migrate when something meaningful was configured.
    if name.is_empty() && base.is_empty() && enabled.is_empty() {
        return Ok(());
    }

    let name = if name.is_empty() {
        "Custom".to_string()
    } else {
        name
    };
    let existing = custom_list(app)?;
    let ids: Vec<String> = existing.iter().map(|p| p.id.clone()).collect();
    let id = crate::providers::unique_custom_id(&slug(&name), &ids);

    let p = CustomProvider {
        id: id.clone(),
        name,
        base_url: base,
        format,
        headers,
        models_path: String::new(),
        manual_models: vec![],
        enabled_models: enabled,
        default_model,
        status: "untested".into(),
        message: String::new(),
        sort: existing.len() as i64,
    };
    custom_insert(app, &p)?;

    // Copy the credential from the legacy account to the new provider id so
    // the migrated provider keeps working without re-entering the key.
    if let Ok(Some(key)) = crate::creds::get_key("custom") {
        if !key.is_empty() {
            let _ = crate::creds::set_key(&id, &key);
            let _ = crate::creds::delete_key("custom");
        }
    }

    // Drop the legacy row so this migration runs only once.
    with(app, |c| {
        c.execute("DELETE FROM providers WHERE id = 'custom'", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "provider".to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A fresh in-memory database with the full v1.2.3 schema.
    fn fresh() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        conn
    }

    /// Simulate a pre-1.2.3 database after `migrate()` ran: old chats table
    /// without project_id, plus the projects table migrate() creates.
    fn legacy() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE projects(
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE COLLATE NOCASE,
                sort INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE chats(
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT 'New chat',
                workspace TEXT NOT NULL DEFAULT '',
                provider TEXT NOT NULL DEFAULT '',
                model TEXT NOT NULL DEFAULT '',
                effort TEXT NOT NULL DEFAULT '',
                permission_mode TEXT NOT NULL DEFAULT 'ask',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            INSERT INTO chats(id,workspace,created_at,updated_at)
                VALUES('c1','C:\code\alpha', 1000, 2000),
                      ('c2','C:\code\beta',  1001, 2001),
                      ('c3','',                1002, 2002);
            "#,
        )
        .unwrap();
        conn
    }

    #[test]
    fn migration_adds_project_column_and_preserves_sessions() {
        let conn = legacy();
        ensure_chat_project_column(&conn).unwrap();
        // The column now exists and every legacy chat row survived untouched.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM chats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
        let titles: Vec<String> = {
            let mut st = conn.prepare("SELECT title FROM chats ORDER BY id").unwrap();
            st.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(titles.len(), 3);
        // Column is real: selecting it works and all values start as NULL.
        let nulls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chats WHERE project_id IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(nulls, 3);
    }

    #[test]
    fn migration_is_idempotent() {
        let conn = fresh();
        ensure_chat_project_column(&conn).unwrap();
        ensure_chat_project_column(&conn).unwrap(); // must not error
    }

    #[test]
    fn seeding_creates_project_per_distinct_workspace() {
        let conn = legacy();
        ensure_chat_project_column(&conn).unwrap();
        seed_projects(&conn, "C:\\code\\alpha").unwrap();
        let projects = projects_list_conn(&conn).unwrap();
        let mut paths: Vec<String> = projects.iter().map(|p| p.path.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec!["C:\\code\\alpha", "C:\\code\\beta"]);
        // Session names derive from the folder.
        let alpha = projects
            .iter()
            .find(|p| p.path == "C:\\code\\alpha")
            .unwrap();
        assert_eq!(alpha.name, "alpha");
    }

    #[test]
    fn seeding_assigns_chats_to_matching_projects_and_default() {
        let conn = legacy();
        ensure_chat_project_column(&conn).unwrap();
        seed_projects(&conn, "C:\\code\\alpha").unwrap();
        let chats = chats_list_conn(&conn).unwrap();
        let by_id = |id: &str| chats.iter().find(|c| c.id == id).unwrap();
        // Matched by workspace path (case-insensitive fallback included).
        assert!(by_id("c1").project_id.as_deref().is_some());
        assert!(by_id("c2").project_id.as_deref().is_some());
        // Empty workspace chat lands on the default project.
        assert!(by_id("c3").project_id.as_deref().is_some());
        let alpha = projects_list_conn(&conn)
            .unwrap()
            .into_iter()
            .find(|p| p.path == "C:\\code\\alpha")
            .unwrap();
        assert_eq!(by_id("c1").project_id.as_deref(), Some(alpha.id.as_str()));
        assert_eq!(by_id("c3").project_id.as_deref(), Some(alpha.id.as_str()));
        assert_ne!(by_id("c1").project_id, by_id("c2").project_id);
    }

    #[test]
    fn seeding_with_no_chats_creates_default_project_only() {
        let conn = fresh();
        seed_projects(&conn, "C:\\code\\default").unwrap();
        let projects = projects_list_conn(&conn).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].path, "C:\\code\\default");
        assert_eq!(projects[0].name, "default");
    }

    #[test]
    fn seeding_does_not_run_twice() {
        let conn = fresh();
        project_upsert_conn(&conn, "C:\\code\\existing", "existing").unwrap();
        seed_projects(&conn, "C:\\code\\default").unwrap();
        // Existing projects are never replaced by the default workspace.
        let projects = projects_list_conn(&conn).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].path, "C:\\code\\existing");
    }

    #[test]
    fn project_upsert_dedupes_by_path_case_insensitively() {
        let conn = fresh();
        let a = project_upsert_conn(&conn, "C:\\code\\Alpha", "Alpha").unwrap();
        // Same path with different casing returns the same project row.
        let b = project_upsert_conn(&conn, "c:\\code\\alpha", "Whatever").unwrap();
        assert_eq!(a.id, b.id);
        let all = projects_list_conn(&conn).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn project_upsert_rejects_empty_path() {
        let conn = fresh();
        assert!(project_upsert_conn(&conn, "  ", "x").is_err());
    }

    #[test]
    fn chat_rows_roundtrip_project_id() {
        let conn = fresh();
        conn.execute_batch(
            "INSERT INTO projects(id,name,path,sort,created_at) VALUES('p1','Alpha','C:\\a',0,1)",
        )
        .unwrap();
        let chat = ChatSummary {
            id: "ch1".into(),
            title: "T".into(),
            workspace: "C:\\a".into(),
            provider: "groq".into(),
            model: "m".into(),
            effort: "".into(),
            permission_mode: "ask".into(),
            created_at: 1,
            updated_at: 1,
            project_id: Some("p1".into()),
        };
        conn.execute(
            "INSERT INTO chats(id,title,workspace,provider,model,effort,permission_mode,created_at,updated_at,project_id)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            rusqlite::params![
                chat.id, chat.title, chat.workspace, chat.provider, chat.model, chat.effort,
                chat.permission_mode, chat.created_at, chat.updated_at, chat.project_id
            ],
        )
        .unwrap();
        let got = chat_get_conn(&conn, "ch1").unwrap();
        assert_eq!(got.project_id.as_deref(), Some("p1"));
        assert_eq!(got.title, "T");
    }

    #[test]
    fn removing_project_unlinks_but_keeps_its_sessions() {
        let conn = fresh();
        conn.execute_batch(
            "INSERT INTO projects(id,name,path,sort,created_at) VALUES('p1','Alpha','C:\\a',0,1);
             INSERT INTO chats(id,workspace,created_at,updated_at,project_id)
                 VALUES('ch1','C:\\a',1,2,'p1');",
        )
        .unwrap();
        // Run the migration once so the one-time marker is persisted.
        seed_projects(&conn, "C:\\a").unwrap();
        // Simulate project removal (transactional delete + unlink).
        conn.execute("DELETE FROM projects WHERE id = 'p1'", [])
            .unwrap();
        conn.execute(
            "UPDATE chats SET project_id = NULL WHERE project_id = 'p1'",
            [],
        )
        .unwrap();
        // The session survives with no project association.
        let chats = chats_list_conn(&conn).unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].project_id, None);
        // Seeding is a one-time migration: it must NOT recreate the removed
        // project or silently relink the session on the next launch.
        seed_projects(&conn, "C:\\a").unwrap();
        assert!(projects_list_conn(&conn).unwrap().is_empty());
        let chats = chats_list_conn(&conn).unwrap();
        assert_eq!(chats[0].project_id, None);
        seed_projects(&conn, "C:\\a").unwrap();
        assert!(projects_list_conn(&conn).unwrap().is_empty());
    }

    #[test]
    fn seed_marker_prevents_default_project_recreation_after_removal() {
        let conn = fresh();
        seed_projects(&conn, "C:\\code\\default").unwrap();
        assert_eq!(projects_list_conn(&conn).unwrap().len(), 1);
        conn.execute("DELETE FROM projects", []).unwrap();
        seed_projects(&conn, "C:\\code\\default").unwrap();
        // Removing the last project is user intent, not a reason to seed again.
        assert!(projects_list_conn(&conn).unwrap().is_empty());
    }

    #[test]
    fn project_path_normalization_preserves_drive_roots() {
        assert_eq!(normalize_project_path(r"C:\"), r"C:\");
        assert_eq!(normalize_project_path(r"C:\\"), r"C:\");
        assert_eq!(normalize_project_path(r"C:\code\alpha\\"), r"C:\code\alpha");
        assert_eq!(normalize_project_path("  "), "");
        assert_eq!(normalize_project_path(r"\\srv\share\"), r"\\srv\share");
    }

    #[test]
    fn project_name_falls_back_to_folder_basename() {
        let conn = fresh();
        let p = project_upsert_conn(&conn, "C:\\code\\my-project", "").unwrap();
        assert_eq!(p.name, "my-project");
    }

    #[test]
    fn chat_summary_serializes_project_id() {
        let c = ChatSummary {
            id: "1".into(),
            title: "t".into(),
            workspace: "w".into(),
            provider: "p".into(),
            model: "m".into(),
            effort: String::new(),
            permission_mode: "ask".into(),
            created_at: 0,
            updated_at: 0,
            project_id: None,
        };
        let v = serde_json::to_value(&c).unwrap();
        assert!(v.get("projectId").unwrap().is_null());
    }

    // Sanity: Path-based basename extraction used by seeding.
    #[test]
    fn basename_extraction() {
        let p = Path::new("C:\\code\\alpha");
        assert_eq!(p.file_name().unwrap().to_string_lossy(), "alpha");
    }
}
