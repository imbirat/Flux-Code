use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
        "#,
    )
    .map_err(|e| e.to_string())
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
    })
}

const CHAT_COLS: &str =
    "id,title,workspace,provider,model,effort,permission_mode,created_at,updated_at";

pub fn list_chats(app: &AppHandle) -> Result<Vec<ChatSummary>, String> {
    with(app, |c| {
        let sql = format!("SELECT {CHAT_COLS} FROM chats ORDER BY updated_at DESC");
        let mut st = c.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = st
            .query_map([], row_to_chat)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    })
}

pub fn create_chat(
    app: &AppHandle,
    workspace: &str,
    provider: &str,
    model: &str,
    effort: &str,
    mode: &str,
) -> Result<ChatSummary, String> {
    let id = uid();
    let t = now_ms();
    with(app, |c| {
        c.execute(
            "INSERT INTO chats(id,title,workspace,provider,model,effort,permission_mode,created_at,updated_at)
             VALUES(?1,'New chat',?2,?3,?4,?5,?6,?7,?7)",
            rusqlite::params![id, workspace, provider, model, effort, mode, t],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })?;
    Ok(ChatSummary {
        id,
        title: "New chat".into(),
        workspace: workspace.into(),
        provider: provider.into(),
        model: model.into(),
        effort: effort.into(),
        permission_mode: mode.into(),
        created_at: t,
        updated_at: t,
    })
}

pub fn update_chat(app: &AppHandle, id: &str, patch: &Value) -> Result<ChatSummary, String> {
    with(app, |c| {
        let cols = [
            ("title", "title"),
            ("workspace", "workspace"),
            ("provider", "provider"),
            ("model", "model"),
            ("effort", "effort"),
            ("permissionMode", "permission_mode"),
        ];
        let mut sets: Vec<String> = vec![];
        let mut vals: Vec<String> = vec![];
        for (key, col) in cols {
            if let Some(v) = patch.get(key).and_then(|v| v.as_str()) {
                sets.push(format!("{col} = ?"));
                vals.push(v.to_string());
            }
        }
        if !sets.is_empty() {
            sets.push("updated_at = ?".into());
            vals.push(now_ms().to_string());
            vals.push(id.to_string());
            let sql = format!("UPDATE chats SET {} WHERE id = ?", sets.join(", "));
            c.execute(&sql, rusqlite::params_from_iter(vals.iter()))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    get_chat(app, id)
}

pub fn get_chat(app: &AppHandle, id: &str) -> Result<ChatSummary, String> {
    with(app, |c| {
        let sql = format!("SELECT {CHAT_COLS} FROM chats WHERE id = ?1");
        let mut st = c.prepare(&sql).map_err(|e| e.to_string())?;
        st.query_row([id], row_to_chat)
            .map_err(|e| format!("chat not found: {e}"))
    })
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
        c.execute("DELETE FROM chats WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        c.execute("DELETE FROM messages WHERE chat_id = ?1", [id])
            .map_err(|e| e.to_string())?;
        c.execute("DELETE FROM todos WHERE chat_id = ?1", [id])
            .map_err(|e| e.to_string())?;
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
        c.execute(
            "INSERT INTO messages(id,chat_id,role,content,meta,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![id, chat_id, role, content, meta_s, t],
        )
        .map_err(|e| e.to_string())?;
        c.execute(
            "UPDATE chats SET updated_at = ?1 WHERE id = ?2",
            rusqlite::params![t, chat_id],
        )
        .map_err(|e| e.to_string())?;
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

pub fn update_message_content(
    app: &AppHandle,
    id: &str,
    content: &str,
    meta: &Value,
) -> Result<(), String> {
    let meta_s = serde_json::to_string(meta).unwrap_or_else(|_| "{}".into());
    with(app, |c| {
        c.execute(
            "UPDATE messages SET content = ?1, meta = ?2 WHERE id = ?3",
            rusqlite::params![content, meta_s, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
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
            let id = if item.id.is_empty() { uid() } else { item.id.clone() };
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
        let mut st = c.prepare("SELECT value FROM settings WHERE key = ?1").map_err(|e| e.to_string())?;
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
        let mut st = c.prepare("SELECT config FROM providers WHERE id = ?1").map_err(|e| e.to_string())?;
        let v: String = st.query_row([id], |r| r.get(0)).unwrap_or_else(|_| "{}".into());
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
