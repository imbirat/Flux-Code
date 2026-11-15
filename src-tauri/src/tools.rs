use crate::fsops;
use crate::permissions;
use crate::terminal;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::AppHandle;

pub struct ToolCtx {
    pub app: AppHandle,
    pub chat_id: String,
    pub workspace: PathBuf,
    pub extra: Vec<PathBuf>,
}

fn tool(name: &str, desc: &str, params: Value) -> Value {
    json!({ "type": "function", "function": { "name": name, "description": desc, "parameters": params } })
}

fn obj(props: Value, required: &[&str]) -> Value {
    let mut v = json!({ "type": "object", "properties": props });
    if !required.is_empty() {
        v["required"] = json!(required);
    }
    v
}

pub fn tool_defs() -> Vec<Value> {
    vec![
        tool("list_files", "List files and folders as an indented tree, relative to the workspace. Common build dirs (node_modules, .git, target…) are skipped.", obj(json!({
            "path": { "type": "string", "description": "Relative path inside the workspace, defaults to the workspace root" },
            "depth": { "type": "integer", "description": "How deep to list, default 2" }
        }), &[])),
        tool("read_file", "Read a text file. Long files report which lines were shown.", obj(json!({
            "path": { "type": "string", "description": "File path, relative to the workspace" },
            "offset": { "type": "integer", "description": "1-based line to start from" },
            "limit": { "type": "integer", "description": "Max lines, default 1000" }
        }), &["path"])),
        tool("create_file", "Create a new file with content. Fails if the file exists — use edit_file for existing files.", obj(json!({
            "path": { "type": "string" },
            "content": { "type": "string" }
        }), &["path", "content"])),
        tool("edit_file", "Replace an exact snippet in an existing file. old_string must match the file exactly, including indentation.", obj(json!({
            "path": { "type": "string" },
            "old_string": { "type": "string" },
            "new_string": { "type": "string" },
            "replace_all": { "type": "boolean", "description": "Replace every occurrence instead of failing on multiple matches" }
        }), &["path", "old_string", "new_string"])),
        tool("delete_file", "Delete a single file.", obj(json!({
            "path": { "type": "string" }
        }), &["path"])),
        tool("create_folder", "Create a folder (parents are created too).", obj(json!({
            "path": { "type": "string" }
        }), &["path"])),
        tool("rename_file", "Rename or move a file or folder.", obj(json!({
            "path": { "type": "string" },
            "new_path": { "type": "string" }
        }), &["path", "new_path"])),
        tool("search_files", "Search file contents in the workspace. Plain text by default; set is_regex for a regular expression.", obj(json!({
            "query": { "type": "string" },
            "is_regex": { "type": "boolean" },
            "case_sensitive": { "type": "boolean" }
        }), &["query"])),
        tool("run_command", "Run a PowerShell command in the workspace and return its output and exit code. For builds, tests, dependency installs and quick scripts.", obj(json!({
            "command": { "type": "string" },
            "cwd": { "type": "string", "description": "Working directory, defaults to the workspace" },
            "timeout_secs": { "type": "integer", "description": "Timeout in seconds, default 120" }
        }), &["command"])),
        tool("start_process", "Start a long-running process such as a dev server. Returns a process id; output streams to the Preview panel.", obj(json!({
            "command": { "type": "string" },
            "name": { "type": "string", "description": "Short name, e.g. 'dev server'" },
            "cwd": { "type": "string" }
        }), &["command", "name"])),
        tool("stop_process", "Stop a previously started process by id.", obj(json!({
            "id": { "type": "integer" }
        }), &["id"])),
        tool("list_processes", "List processes started in this conversation.", obj(json!({}), &[])),
        tool("todo_write", "Replace the shared todo list for this task. Keep statuses honest: pending / in_progress / completed. Mark completed only after verification.", obj(json!({
            "todos": { "type": "array", "items": { "type": "object", "properties": {
                "content": { "type": "string" },
                "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] }
            }, "required": ["content", "status"] } }
        }), &["todos"])),
        tool("ask_user", "Ask the user clarifying questions as interactive cards. Use when requirements are ambiguous or a decision matters. At most 4 questions.", obj(json!({
            "questions": { "type": "array", "items": { "type": "object", "properties": {
                "id": { "type": "string" },
                "type": { "type": "string", "enum": ["choice", "multi", "text", "yesno"] },
                "question": { "type": "string" },
                "options": { "type": "array", "items": { "type": "string" } },
                "optional": { "type": "boolean" }
            }, "required": ["id", "type", "question"] } }
        }), &["questions"])),
    ]
}

fn resolve(ws: &Path, raw: &str) -> PathBuf {
    permissions::resolve_path(ws, raw)
}

fn tail(s: &str, n: usize) -> String {
    let c = s.chars().count();
    if c <= n { s.to_string() } else { s.chars().skip(c - n).collect() }
}

/// Execute a tool. `ask_user` is handled by the agent loop, not here.
pub async fn execute(ctx: &ToolCtx, name: &str, args: &Value) -> Result<(String, Value), String> {
    let ws = ctx.workspace.clone();
    match name {
        "list_files" => {
            let raw = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
            let p = resolve(&ws, raw);
            if !p.exists() {
                return Err(format!("Path does not exist: {}", p.display()));
            }
            let t = fsops::tree(&p, depth.min(4), 500);
            Ok((
                if t.trim().is_empty() { "(empty)".to_string() } else { t },
                json!({ "path": p.display().to_string() }),
            ))
        }
        "read_file" => {
            let raw = args.get("path").and_then(|v| v.as_str()).ok_or("path is required")?;
            let p = resolve(&ws, raw);
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let (content, truncated) = fsops::read_file(&p, offset, limit)?;
            Ok((content, json!({ "path": p.display().to_string(), "truncated": truncated })))
        }
        "create_file" => {
            let raw = args.get("path").and_then(|v| v.as_str()).ok_or("path is required")?;
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let p = resolve(&ws, raw);
            let meta = fsops::create_file(&p, content)?;
            Ok((format!("Created {} ({} bytes)", p.display(), content.len()), meta))
        }
        "edit_file" => {
            let raw = args.get("path").and_then(|v| v.as_str()).ok_or("path is required")?;
            let old = args.get("old_string").and_then(|v| v.as_str()).ok_or("old_string is required")?;
            let new = args.get("new_string").and_then(|v| v.as_str()).ok_or("new_string is required")?;
            let all = args.get("replace_all").and_then(|v| v.as_bool()).unwrap_or(false);
            let p = resolve(&ws, raw);
            let meta = fsops::edit_file(&p, old, new, all)?;
            Ok((format!("Edited {}", p.display()), meta))
        }
        "delete_file" => {
            let raw = args.get("path").and_then(|v| v.as_str()).ok_or("path is required")?;
            let p = resolve(&ws, raw);
            let meta = fsops::delete_file(&p)?;
            Ok((format!("Deleted {}", p.display()), meta))
        }
        "create_folder" => {
            let raw = args.get("path").and_then(|v| v.as_str()).ok_or("path is required")?;
            let p = resolve(&ws, raw);
            let meta = fsops::create_folder(&p)?;
            Ok((format!("Created folder {}", p.display()), meta))
        }
        "rename_file" => {
            let raw = args.get("path").and_then(|v| v.as_str()).ok_or("path is required")?;
            let new_raw = args.get("new_path").and_then(|v| v.as_str()).ok_or("new_path is required")?;
            let p = resolve(&ws, raw);
            let np = resolve(&ws, new_raw);
            let meta = fsops::rename_path(&p, &np)?;
            Ok((format!("Renamed {} → {}", p.display(), np.display()), meta))
        }
        "search_files" => {
            let q = args.get("query").and_then(|v| v.as_str()).ok_or("query is required")?;
            let is_regex = args.get("is_regex").and_then(|v| v.as_bool()).unwrap_or(false);
            let cs = args.get("case_sensitive").and_then(|v| v.as_bool()).unwrap_or(false);
            let hits = fsops::search(&ws, q, is_regex, cs, 50)?;
            let out = if hits.is_empty() { "No matches found".to_string() } else { hits.join("\n") };
            Ok((out, json!({ "count": hits.len() })))
        }
        "run_command" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).ok_or("command is required")?;
            let cwd = match args.get("cwd").and_then(|v| v.as_str()) {
                Some(c) if !c.is_empty() => resolve(&ws, c),
                _ => ws.clone(),
            };
            if !cwd.exists() {
                return Err(format!("Working directory does not exist: {}", cwd.display()));
            }
            let timeout = args.get("timeout_secs").and_then(|v| v.as_u64()).unwrap_or(120).clamp(10, 900);
            let (output, code, timed_out) =
                terminal::run_command(&ctx.app, &ctx.chat_id, &cwd, cmd, timeout).await?;
            let mut result = output.clone();
            if timed_out {
                result.push_str(&format!("\n[Timed out after {timeout}s — the command was stopped]"));
            } else {
                result.push_str(&format!("\nExit code: {}", code.unwrap_or(-1)));
            }
            let meta = json!({
                "command": cmd,
                "cwd": cwd.display().to_string(),
                "exitCode": code,
                "timedOut": timed_out,
                "output": tail(&output, 6000),
            });
            Ok((result, meta))
        }
        "start_process" => {
            let cmd = args.get("command").and_then(|v| v.as_str()).ok_or("command is required")?;
            let name_s = args.get("name").and_then(|v| v.as_str()).unwrap_or("process");
            let cwd = match args.get("cwd").and_then(|v| v.as_str()) {
                Some(c) if !c.is_empty() => resolve(&ws, c),
                _ => ws.clone(),
            };
            let id = terminal::start_process(&ctx.app, &ctx.chat_id, &cwd, name_s, cmd).await?;
            Ok((
                format!("Started process #{id}: {cmd}\nOutput streams to the Preview panel. Use list_processes or stop_process as needed."),
                json!({ "processId": id, "name": name_s, "command": cmd }),
            ))
        }
        "stop_process" => {
            let id = args.get("id").and_then(|v| v.as_u64()).ok_or("id is required")? as u32;
            terminal::stop(&ctx.app, id)?;
            Ok((format!("Stopped process #{id}"), json!({ "processId": id })))
        }
        "list_processes" => {
            let list = terminal::list_json(&ctx.app)?;
            let lines: Vec<String> = list
                .iter()
                .map(|p| format!("#{} {} — {}", p["id"], p["name"], p["command"]))
                .collect();
            Ok((
                if lines.is_empty() { "No running processes".into() } else { lines.join("\n") },
                json!({ "processes": list }),
            ))
        }
        "todo_write" => {
            let items = args.get("todos").and_then(|v| v.as_array()).ok_or("todos is required")?;
            let mut todos = vec![];
            for (i, t) in items.iter().enumerate() {
                let content = t.get("content").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                if content.is_empty() {
                    continue;
                }
                let status = match t.get("status").and_then(|v| v.as_str()).unwrap_or("pending") {
                    "in_progress" => "in_progress",
                    "completed" => "completed",
                    _ => "pending",
                };
                todos.push(crate::db::TodoItem {
                    id: String::new(),
                    content,
                    status: status.into(),
                    sort: i as i64,
                });
            }
            crate::db::set_todos(&ctx.app, &ctx.chat_id, &todos)?;
            let n = todos.len();
            let inprog = todos.iter().filter(|t| t.status == "in_progress").count();
            let done = todos.iter().filter(|t| t.status == "completed").count();
            Ok((
                format!("Todo list updated: {n} tasks ({done} completed, {inprog} in progress)"),
                json!({}),
            ))
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}
