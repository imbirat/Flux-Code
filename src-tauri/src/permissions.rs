use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Tools that only read from the workspace and never mutate anything.
const READ_TOOLS: &[&str] = &["list_files", "read_file", "search_files"];
/// Tools that are always free to run.
const FREE_TOOLS: &[&str] = &["todo_write", "stop_process", "list_processes"];

pub enum Gate {
    Allow,
    /// Ask the user. `remember_key` allows "allow for this session" for the same kind of action.
    Ask { summary: String, remember_key: Option<String> },
    Deny(String),
}

fn normalize(p: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(p).ok().or_else(|| {
        // For paths that do not exist yet (create_file), canonicalize the parent.
        if let (Some(parent), Some(file)) = (p.parent(), p.file_name()) {
            std::fs::canonicalize(parent)
                .ok()
                .map(|pp| pp.join(file))
        } else {
            None
        }
    })
}

fn is_protected(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    let protected = ["\\windows\\", "\\program files", "\\programdata\\microsoft"];
    protected.iter().any(|d| s.contains(d)) || s == "c:\\windows" || s.starts_with("c:\\windows\\")
}

fn in_scope(p: &Path, scopes: &[PathBuf]) -> bool {
    scopes.iter().any(|s| p.starts_with(s))
}

/// Heuristic: does this command look destructive or system-touching?
/// Such commands require confirmation even in Full Access mode.
pub fn is_risky_command(cmd: &str) -> bool {
    let l = cmd.to_lowercase();
    let always = [
        "format ", "format.com", "diskpart", "shutdown", "cipher /w", "vssadmin",
        "bcdedit", "dism ", "dism/", "sfc /", "reg add", "reg delete", "regedit /s",
        "net user", "net localgroup", "sc delete", "set-executionpolicy",
        "start-process -verb runas", "remove-item -recurse", "remove-item -force -recurse",
        "del /s", "rd /s", "rmdir /s", "rm -r", "rm -rf", "takeown", "icacls ",
    ];
    if always.iter().any(|p| l.contains(p)) {
        return true;
    }
    if l.contains("remove-item") && l.contains("-recurse") {
        return true;
    }
    if l.contains("c:\\windows") || l.contains("c:/windows") || l.contains("program files") {
        return true;
    }
    false
}

/// Decide whether a tool call may run, must ask, or must be denied.
pub fn gate_tool(
    mode: &str,
    tool: &str,
    args: &Value,
    workspace: &PathBuf,
    extra: &[PathBuf],
    allowed: &HashSet<String>,
) -> Gate {
    if FREE_TOOLS.contains(&tool) {
        return Gate::Allow;
    }
    let full = mode == "full";

    if READ_TOOLS.contains(&tool) {
        let raw = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let target = resolve_path(workspace, raw);
        return match normalize(&target) {
            Some(t) if is_protected(&t) => Gate::Deny("Access to protected system directories is blocked".into()),
            Some(t) if in_scope(&t, &scopes(workspace, extra)) => Gate::Allow,
            Some(t) => {
                let key = format!("read:{}", t.to_string_lossy());
                if allowed.contains(&key) {
                    Gate::Allow
                } else {
                    Gate::Ask {
                        summary: format!("Read files outside the workspace: {}", t.display()),
                        remember_key: Some(key),
                    }
                }
            }
            None => Gate::Allow,
        };
    } else if matches!(tool, "create_file" | "edit_file" | "create_folder" | "rename_file" | "delete_file") {
        let mut targets: Vec<String> = vec![];
        if let Some(p) = args.get("path").and_then(|v| v.as_str()) {
            targets.push(p.to_string());
        }
        if let Some(p) = args.get("new_path").and_then(|v| v.as_str()) {
            targets.push(p.to_string());
        }
        let scopes = scopes(workspace, extra);
        for raw in &targets {
            let target = resolve_path(workspace, raw);
            match normalize(&target) {
                Some(t) if is_protected(&t) => {
                    return Gate::Deny(format!(
                        "Modifying {} is blocked: protected system directory",
                        t.display()
                    ))
                }
                Some(t) if !in_scope(&t, &scopes) => {
                    let key = format!("write:{}", t.to_string_lossy());
                    if allowed.contains(&key) {
                        continue;
                    }
                    return Gate::Ask {
                        summary: format!("Modify files outside the workspace: {}", t.display()),
                        remember_key: Some(key),
                    };
                }
                _ => {}
            }
        }
        // Inside the workspace scope.
        if tool == "delete_file" {
            if full {
                return Gate::Allow;
            }
            let key = "fs-delete".to_string();
            if allowed.contains(&key) {
                return Gate::Allow;
            }
            return Gate::Ask {
                summary: format!(
                    "Delete file: {}",
                    args.get("path").and_then(|v| v.as_str()).unwrap_or("?")
                ),
                remember_key: Some(key),
            };
        }
        if full {
            return Gate::Allow;
        }
        let key = "fs-write".to_string();
        if allowed.contains(&key) {
            return Gate::Allow;
        }
        let op = match tool {
            "create_file" => "Create file",
            "edit_file" => "Edit file",
            "create_folder" => "Create folder",
            _ => "Rename or move",
        };
        return Gate::Ask {
            summary: format!("{}: {}", op, targets.first().cloned().unwrap_or_default()),
            remember_key: Some(key),
        };
    } else if matches!(tool, "run_command" | "start_process") {
        let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(cwd) = args.get("cwd").and_then(|v| v.as_str()) {
            let t = resolve_path(workspace, cwd);
            if let Some(t) = normalize(&t) {
                if !in_scope(&t, &scopes(workspace, extra)) && !allowed.contains(&format!("cwd:{}", t.to_string_lossy())) {
                    return Gate::Ask {
                        summary: format!("Run a command outside the workspace, in {}", t.display()),
                        remember_key: Some(format!("cwd:{}", t.to_string_lossy())),
                    };
                }
            }
        }
        let key = format!("cmd:{}", cmd);
        if allowed.contains(&key) {
            return Gate::Allow;
        }
        if full && !is_risky_command(cmd) {
            return Gate::Allow;
        }
        let reason = if is_risky_command(cmd) && full {
            "Potentially destructive command (requires confirmation even in Full Access)"
        } else {
            "Terminal command"
        };
        return Gate::Ask {
            summary: format!("{reason}: {cmd}"),
            remember_key: Some(key),
        };
    }

    Gate::Allow
}

pub fn scopes(workspace: &PathBuf, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut v = vec![workspace.clone()];
    v.extend(extra.iter().cloned());
    v
}

/// Resolve a possibly-relative path against the workspace.
pub fn resolve_path(workspace: &Path, raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        workspace.join(p)
    }
}
