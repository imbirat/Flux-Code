use serde_json::Value;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// Tools that only read from the workspace and never mutate anything.
pub const READ_TOOLS: &[&str] = &["list_files", "read_file", "search_files"];
/// Tools that are always free to run.
pub const FREE_TOOLS: &[&str] = &["todo_write", "stop_process", "list_processes"];

#[derive(Debug)]
pub enum Gate {
    Allow,
    /// Ask the user. `remember_key` allows "allow for this session" for the
    /// same operation on the same target — never a blanket grant.
    Ask {
        summary: String,
        remember_key: Option<String>,
    },
    Deny(String),
}

/// Canonical targets for one tool call, resolved once and shared by the
/// permission gate and the executor so the decision and the operation act on
/// the very same path. `None` fields mean the tool has no such argument.
#[derive(Debug, Clone, Default)]
pub struct ResolvedPaths {
    /// Canonical targets derived from `path` (delete/rename keep one entry).
    pub paths: Vec<PathBuf>,
    /// Canonical `new_path` of rename_file, when present.
    pub new_path: Option<PathBuf>,
    /// Canonical `cwd` of run_command/start_process, when present.
    pub cwd: Option<PathBuf>,
}

const PATH_TOOLS: &[&str] = &[
    "list_files",
    "read_file",
    "create_file",
    "edit_file",
    "create_folder",
    "rename_file",
    "delete_file",
];
const CMD_TOOLS: &[&str] = &["run_command", "start_process"];

/// Resolve every path argument of a tool call to a canonical absolute path.
///
/// Fails closed: an unresolvable path is an error the caller must treat as a
/// denial, never as an implicit allow. This is the single place where raw
/// model-supplied path strings are turned into real filesystem paths.
pub fn resolve_tool_paths(
    tool: &str,
    args: &Value,
    workspace: &Path,
) -> Result<ResolvedPaths, String> {
    let mut out = ResolvedPaths::default();
    if PATH_TOOLS.contains(&tool) {
        // list_files defaults to the workspace root when `path` is absent.
        let raw = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let target = resolve_path(workspace, raw);
        out.paths.push(resolve_real(&target)?);
        if let Some(raw) = args.get("new_path").and_then(|v| v.as_str()) {
            let target = resolve_path(workspace, raw);
            out.new_path = Some(resolve_real(&target)?);
        }
    }
    if CMD_TOOLS.contains(&tool) {
        if let Some(raw) = args.get("cwd").and_then(|v| v.as_str()) {
            if !raw.is_empty() {
                let target = resolve_path(workspace, raw);
                out.cwd = Some(resolve_real(&target)?);
            }
        }
    }
    Ok(out)
}

// ---------- lexical normalization ----------

/// Remove `.` and resolve `..` lexically. A `..` that would climb above the
/// path's anchor (drive root, UNC share root, or a relative path's start) is
/// rejected instead of silently clamped: clamping would make `C:\..\Windows`
/// look like `C:\Windows` while still acting on a different string.
/// The result is only used for resolution and comparison, never passed to
/// the OS unchanged.
fn normalize_lexical(p: &Path) -> Result<PathBuf, String> {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                // Refuse to pop a prefix or root: only real path components
                // may be undone. `pop` also fails for an empty relative path.
                let is_root = matches!(
                    out.components().next_back(),
                    Some(Component::Prefix(_)) | Some(Component::RootDir)
                );
                if is_root || !out.pop() {
                    return Err(format!("path escapes its root: {}", p.display()));
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

/// Canonicalize `p`. When the path itself does not exist yet (create_file,
/// create_folder, rename destination), canonicalize the nearest existing
/// ancestor and re-append the remaining components verbatim.
///
/// Fails closed: an unresolvable path is an error, so callers can never treat
/// a normalization failure as an implicit allow.
pub fn resolve_real(p: &Path) -> Result<PathBuf, String> {
    let lex = normalize_lexical(p)?;
    if lex.as_os_str().is_empty() {
        return Err("path resolves to nothing".into());
    }
    if let Ok(c) = std::fs::canonicalize(&lex) {
        return Ok(c);
    }
    // Walk up to the nearest existing ancestor, then rebuild the tail.
    let mut rest: Vec<OsString> = Vec::new();
    let mut anc = lex;
    loop {
        let parent = match anc.parent() {
            Some(par) if par != anc => par.to_path_buf(),
            _ => return Err(format!("cannot resolve path: {}", p.display())),
        };
        match anc.file_name() {
            Some(name) => rest.push(name.to_os_string()),
            None => return Err(format!("cannot resolve path: {}", p.display())),
        }
        anc = parent;
        if let Ok(c) = std::fs::canonicalize(&anc) {
            let mut out = c;
            for comp in rest.iter().rev() {
                out.push(comp);
            }
            return Ok(out);
        }
    }
}

/// Plain (non-verbatim) form of a canonical Windows path:
/// `\\?\C:\x` → `C:\x`, `\\?\UNC\srv\share\x` → `\\srv\share\x`.
/// All comparisons and displays use this form.
pub fn to_plain(p: &Path) -> PathBuf {
    let s = p.as_os_str().to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest.to_string())
    } else {
        p.to_path_buf()
    }
}

fn fold(c: &Component) -> String {
    c.as_os_str().to_string_lossy().to_lowercase()
}

/// Component-based containment check — never a string prefix comparison.
/// Case-insensitive so `C:\WS` and `c:\ws` agree on Windows-style filesystems.
pub fn path_within(child: &Path, root: &Path) -> bool {
    let child = to_plain(child);
    let root = to_plain(root);
    if child.starts_with(&root) {
        return true;
    }
    let rc: Vec<Component> = root.components().collect();
    let cc: Vec<Component> = child.components().collect();
    if cc.len() < rc.len() {
        return false;
    }
    rc.iter().zip(cc.iter()).all(|(a, b)| fold(a) == fold(b))
}

fn in_scope(p: &Path, scopes: &[PathBuf]) -> bool {
    scopes.iter().any(|s| path_within(p, s))
}

// ---------- protected system locations ----------

const PROTECTED_SYSTEM_DIRS: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
];

/// True when `p` is inside a Windows system folder. Checked component-wise so
/// it covers every drive (`D:\Windows`, `E:\Program Files\…`) and UNC shares
/// (`\\srv\share\Windows`), while a workspace folder that merely *contains*
/// a directory with one of these names deeper in the tree is not affected:
/// only a directory directly under the drive root (or share root) counts.
pub fn is_protected(p: &Path) -> bool {
    let plain = to_plain(p);
    let comps: Vec<Component> = plain.components().collect();
    // A drive root, UNC share root, or bare relative root is system-critical.
    if comps.iter().all(|c| {
        matches!(
            c,
            Component::Prefix(_) | Component::RootDir | Component::CurDir
        )
    }) {
        return true;
    }
    let first_normal = plain.components().find_map(|c| match c {
        Component::Normal(n) => Some(n.to_string_lossy().to_lowercase()),
        _ => None,
    });
    match first_normal {
        Some(name) => PROTECTED_SYSTEM_DIRS.contains(&name.as_str()),
        None => false,
    }
}

/// Re-check immediately before a filesystem mutation that the target is still
/// the canonical path the permission gate approved. A symlink or junction that
/// appeared in the final component or an ancestor would make the operation
/// act on a different location than the one the user approved, so it fails
/// closed. Roots and protected system locations are always refused.
pub fn validate_mutation_target(p: &Path) -> Result<(), String> {
    if is_protected(p) {
        return Err(format!(
            "Refusing to modify a protected system location: {}",
            to_plain(p).display()
        ));
    }
    if let Ok(md) = std::fs::symlink_metadata(p) {
        if md.file_type().is_symlink() {
            return Err(format!(
                "Refusing to modify a symlink or junction: {}",
                to_plain(p).display()
            ));
        }
    }
    let expected = to_plain(&normalize_lexical(p)?);
    let resolved = to_plain(&resolve_real(p)?);
    if !path_within(&resolved, &expected) || !path_within(&expected, &resolved) {
        return Err(format!(
            "Path changed after approval — refusing to continue: {}",
            to_plain(p).display()
        ));
    }
    Ok(())
}

/// Heuristic: does this command look destructive or system-touching?
/// Such commands require confirmation even in Full Access mode.
pub fn is_risky_command(cmd: &str) -> bool {
    let l = cmd.to_lowercase();
    let always = [
        "format ",
        "format.com",
        "diskpart",
        "shutdown",
        "cipher /w",
        "vssadmin",
        "bcdedit",
        "dism ",
        "dism/",
        "sfc /",
        "reg add",
        "reg delete",
        "regedit /s",
        "net user",
        "net localgroup",
        "sc delete",
        "set-executionpolicy",
        "start-process -verb runas",
        "remove-item -recurse",
        "remove-item -force -recurse",
        "del /s",
        "rd /s",
        "rmdir /s",
        "rm -r",
        "rm -rf",
        "takeown",
        "icacls ",
    ];
    if always.iter().any(|p| l.contains(p)) {
        return true;
    }
    if l.contains("remove-item") && l.contains("-recurse") {
        return true;
    }
    // System locations on any drive: `\windows\`, `\program files`,
    // `\program files (x86)`, `\programdata` — with a separator so a workspace
    // folder named `windows-tools` is not a false positive.
    let sys_hits = [
        r"\windows\",
        r"\program files\",
        r"\program files (x86)\",
        r"\programdata\",
    ];
    let ends_sys = [
        r"\windows",
        r"\program files",
        r"\program files (x86)",
        r"\programdata",
    ];
    if sys_hits.iter().any(|d| l.contains(d)) || ends_sys.iter().any(|d| l.ends_with(d)) {
        return true;
    }
    if l.contains("system32") {
        return true;
    }
    false
}

// ---------- gate ----------

/// Decide whether a tool call may run, must ask, or must be denied.
///
/// `resolved` carries the canonical targets produced by `resolve_tool_paths`
/// for this exact call — the gate never re-reads path strings from `args`.
pub fn gate_tool(
    mode: &str,
    tool: &str,
    args: &Value,
    resolved: &ResolvedPaths,
    workspace: &Path,
    extra: &[PathBuf],
    allowed: &HashSet<String>,
) -> Gate {
    if FREE_TOOLS.contains(&tool) {
        return Gate::Allow;
    }
    let full = mode == "full";

    if READ_TOOLS.contains(&tool) {
        return match resolved.paths.first() {
            None => Gate::Deny("Path could not be resolved safely".into()),
            Some(t) if is_protected(t) => {
                Gate::Deny("Access to protected system directories is blocked".into())
            }
            Some(t) if in_scope(t, &scopes_resolved(workspace, extra)) => Gate::Allow,
            Some(t) => {
                let key = format!("read:{}", to_plain(t).display());
                if allowed.contains(&key) {
                    Gate::Allow
                } else {
                    Gate::Ask {
                        summary: format!(
                            "Read files outside the workspace: {}",
                            to_plain(t).display()
                        ),
                        remember_key: Some(key),
                    }
                }
            }
        };
    }

    if PATH_TOOLS.contains(&tool) {
        let scopes = scopes_resolved(workspace, extra);
        // Validate every target, including the rename destination.
        let mut all_targets: Vec<&PathBuf> = resolved.paths.iter().collect();
        if let Some(np) = &resolved.new_path {
            all_targets.push(np);
        }
        for t in &all_targets {
            if is_protected(t) {
                return Gate::Deny(format!(
                    "Modifying {} is blocked: protected system directory",
                    to_plain(t).display()
                ));
            }
        }
        for t in &all_targets {
            if !in_scope(t, &scopes) {
                let key = format!("write:{}", to_plain(t).display());
                if !allowed.contains(&key) {
                    return Gate::Ask {
                        summary: format!(
                            "Modify files outside the workspace: {}",
                            to_plain(t).display()
                        ),
                        remember_key: Some(key),
                    };
                }
            }
        }
        // Inside the authorized scope. Remember keys stay per-target so one
        // approval never covers unrelated future writes.
        let delete = tool == "delete_file";
        let op = match tool {
            "create_file" => "Create file",
            "edit_file" => "Edit file",
            "create_folder" => "Create folder",
            "delete_file" => "Delete file",
            _ => "Rename or move",
        };
        let first_plain = resolved
            .paths
            .first()
            .map(|t| to_plain(t).display().to_string())
            .or_else(|| {
                resolved
                    .new_path
                    .as_ref()
                    .map(|t| to_plain(t).display().to_string())
            })
            .unwrap_or_else(|| "?".into());
        if full {
            return Gate::Allow;
        }
        let prefix = if delete { "delete" } else { "write" };
        let key = format!("{prefix}:{first_plain}");
        if allowed.contains(&key) {
            return Gate::Allow;
        }
        let summary = if delete {
            format!("Delete file: {first_plain}")
        } else {
            format!("{op}: {first_plain}")
        };
        return Gate::Ask {
            summary,
            remember_key: Some(key),
        };
    }

    if CMD_TOOLS.contains(&tool) {
        let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(cwd) = &resolved.cwd {
            if !in_scope(cwd, &scopes_resolved(workspace, extra)) {
                let key = format!("cwd:{}", to_plain(cwd).display());
                if !allowed.contains(&key) {
                    return Gate::Ask {
                        summary: format!(
                            "Run a command outside the workspace, in {}",
                            to_plain(cwd).display()
                        ),
                        remember_key: Some(key),
                    };
                }
            }
        }
        let key = format!("cmd:{cmd}");
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

/// Workspace + explicitly authorized extra folders, resolved canonically.
/// A scope that cannot be resolved (missing folder) authorizes nothing —
/// it is dropped rather than trusted.
pub fn scopes_resolved(workspace: &Path, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut v = vec![];
    if let Ok(w) = resolve_real(workspace) {
        v.push(to_plain(&w));
    }
    for e in extra {
        // Extra folders are user-added authorization scopes: a path that does
        // not exist is not a usable scope and must never widen access.
        if e.is_dir() {
            if let Ok(c) = resolve_real(e) {
                v.push(to_plain(&c));
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TMP: AtomicUsize = AtomicUsize::new(0);

    fn tmp(name: &str) -> PathBuf {
        // Tests run in parallel: each workspace needs its own directory even
        // when two tests ask for the same logical name.
        let n = NEXT_TMP.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("fluxperm-{}-{}-{n}", std::process::id(), name));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn set_of(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // ---------- resolve_real ----------

    #[test]
    fn resolve_real_canonicalizes_existing_paths() {
        let ws = tmp("canon");
        let dir = ws.join("sub");
        fs::create_dir_all(&dir).unwrap();
        let got = resolve_real(&dir).unwrap();
        assert!(path_within(&got, &ws), "{got:?} should be inside {ws:?}");
        assert!(got.is_absolute());
        // No verbatim prefix leaks into comparisons: to_plain strips \\?\
        assert!(
            !got.to_string_lossy().starts_with(r"\\?\")
                || !to_plain(&got).to_string_lossy().starts_with(r"\\?\")
        );
    }

    #[test]
    fn resolve_real_handles_not_yet_existing_nested_paths() {
        let ws = tmp("nested");
        fs::create_dir_all(&ws).unwrap();
        let target = ws.join("a").join("b").join("c.txt");
        let got = resolve_real(&target).unwrap();
        assert!(path_within(&got, &ws), "{got:?} should be inside {ws:?}");
        assert!(got.ends_with("c.txt"));
    }

    #[test]
    fn resolve_real_rejects_paths_beyond_the_filesystem_root() {
        // A path whose lexical .. pops above the drive root cannot resolve.
        let got = resolve_real(Path::new(r"C:\..\..\..\x"));
        assert!(got.is_err());
    }

    #[test]
    fn resolve_real_keeps_component_traversal_contained() {
        let ws = tmp("dots");
        fs::create_dir_all(&ws).unwrap();
        // .. stays inside the workspace after lexical resolution.
        let got = resolve_real(&ws.join("a").join("..").join("b.txt")).unwrap();
        assert!(path_within(&got, &ws));
        assert!(got.ends_with("b.txt"));
    }

    // ---------- containment ----------

    #[test]
    fn containment_is_component_based_not_string_prefix() {
        let root = Path::new(r"C:\ws");
        // A sibling whose name shares a string prefix is NOT inside.
        assert!(!path_within(Path::new(r"C:\ws-evil\x"), root));
        assert!(!path_within(Path::new(r"C:\ws2\x"), root));
        assert!(path_within(Path::new(r"C:\ws\a"), root));
        assert!(path_within(Path::new(r"C:\ws"), root));
        assert!(!path_within(Path::new(r"C:\ws.txt"), Path::new(r"C:\ws")));
    }

    #[test]
    fn containment_is_case_insensitive() {
        assert!(path_within(
            Path::new(r"c:\WS\sub\f.txt"),
            Path::new(r"C:\ws")
        ));
        assert!(!path_within(Path::new(r"C:\OTHER"), Path::new(r"C:\ws")));
    }

    // ---------- protected locations ----------

    #[test]
    fn protected_covers_windows_directories_on_every_drive() {
        assert!(is_protected(Path::new(r"C:\Windows\System32\config")));
        assert!(is_protected(Path::new(r"D:\Windows\System32")));
        assert!(is_protected(Path::new(r"E:\Program Files\App")));
        assert!(is_protected(Path::new(r"C:\Program Files (x86)\App")));
        assert!(is_protected(Path::new(r"C:\ProgramData\Microsoft")));
        assert!(is_protected(Path::new(r"\\srv\share\Windows")));
    }

    #[test]
    fn protected_covers_filesystem_roots() {
        assert!(is_protected(Path::new(r"C:\")));
        assert!(is_protected(Path::new(r"\\srv\share")));
    }

    // ---------- mutation revalidation ----------

    #[test]
    fn mutation_target_rejects_roots_and_protected_locations() {
        assert!(validate_mutation_target(Path::new(r"C:\")).is_err());
        assert!(validate_mutation_target(Path::new(r"C:\Windows\System32\evil.dll")).is_err());
    }

    #[test]
    fn mutation_target_accepts_unchanged_workspace_paths() {
        let ws = tmp("revalidate");
        let p = ws.join("new-file.txt");
        validate_mutation_target(&p).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn mutation_target_rejects_a_symlink_appearing_after_approval() {
        use std::os::windows::fs::symlink_file;
        let ws = tmp("revalidate-link");
        let outside = std::env::temp_dir().join(format!("fluxperm-out-{}", std::process::id()));
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();
        let target = outside.join("secret.txt");
        fs::write(&target, "x").unwrap();
        let link = ws.join("link.txt");
        match symlink_file(&target, &link) {
            Ok(_) => {
                let err = validate_mutation_target(&link).unwrap_err();
                assert!(err.contains("symlink") || err.contains("changed"), "{err}");
                let _ = fs::remove_file(&link);
            }
            Err(e) => eprintln!("skipping symlink revalidation test: {e}"),
        }
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn protect_ignores_same_named_subfolders_deeper_in_a_tree() {
        // A workspace that contains a folder named "Windows" deeper down is
        // NOT a system directory.
        assert!(!is_protected(Path::new(r"C:\Users\me\projects\Windows")));
        assert!(!is_protected(Path::new(
            r"C:\Users\me\projects\Windows\System32"
        )));
        assert!(!is_protected(Path::new(r"C:\ws\windows-tools")));
    }

    // ---------- scopes ----------

    #[test]
    fn scope_containment_ignores_verbatim_prefix_differences() {
        let ws = tmp("scope");
        fs::create_dir_all(&ws).unwrap();
        // Whatever form canonicalize produced, the plain forms must agree.
        let canonical = resolve_real(&ws).unwrap(); // verbatim on Windows
        assert!(path_within(&canonical, &ws) && path_within(&ws, &canonical));
    }

    #[test]
    fn unresolvable_extra_folder_authorizes_nothing() {
        let ws = tmp("scopes-missing");
        fs::create_dir_all(&ws).unwrap();
        let scopes = scopes_resolved(&ws, &[PathBuf::from(r"C:\definitely\missing\folder")]);
        // Workspace still present; the missing extra folder is dropped.
        assert_eq!(scopes.len(), 1);
        assert!(path_within(&resolve_real(&ws).unwrap(), &scopes[0]));
    }

    // ---------- gate_tool ----------

    fn ws_with_file() -> PathBuf {
        let ws = tmp("gate");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("a.txt"), "hi").unwrap();
        ws
    }

    fn gate_args(tool: &str, path: &Path) -> (Value, ResolvedPaths) {
        let args = serde_json::json!({ "path": path.display().to_string() });
        let resolved = resolve_tool_paths(tool, &args, Path::new("")).unwrap_or_default();
        (args, resolved)
    }

    #[test]
    fn gate_asks_for_in_workspace_writes_in_ask_mode() {
        let ws = ws_with_file();
        let (args, resolved) = gate_args("edit_file", &ws.join("a.txt"));
        match gate_tool("ask", "edit_file", &args, &resolved, &ws, &[], &set_of(&[])) {
            Gate::Ask { remember_key, .. } => {
                // Per-target key, not a blanket fs-write grant.
                let key = remember_key.unwrap();
                assert!(key.starts_with("write:"), "unexpected key: {key}");
                assert!(key.to_lowercase().contains("a.txt"));
            }
            other => panic!("expected Ask, got {other:?}"),
        }
    }

    #[test]
    fn gate_remembered_grant_is_scoped_to_the_same_target() {
        let ws = ws_with_file();
        fs::write(ws.join("b.txt"), "x").unwrap();
        let a_plain = to_plain(&resolve_real(&ws.join("a.txt")).unwrap())
            .display()
            .to_string();
        let allowed = set_of(&[&format!("write:{a_plain}")]);
        // Same target: allowed.
        let (args, resolved) = gate_args("edit_file", &ws.join("a.txt"));
        assert!(matches!(
            gate_tool("ask", "edit_file", &args, &resolved, &ws, &[], &allowed),
            Gate::Allow
        ));
        // Unrelated target: must still ask.
        let (args, resolved) = gate_args("edit_file", &ws.join("b.txt"));
        assert!(matches!(
            gate_tool("ask", "edit_file", &args, &resolved, &ws, &[], &allowed),
            Gate::Ask { .. }
        ));
    }

    #[test]
    fn gate_full_mode_still_blocks_protected_paths() {
        let ws = ws_with_file();
        let (args, resolved) = gate_args("edit_file", Path::new(r"C:\Windows\System32\config.ini"));
        assert!(matches!(
            gate_tool(
                "full",
                "edit_file",
                &args,
                &resolved,
                &ws,
                &[],
                &set_of(&[])
            ),
            Gate::Deny(_)
        ));
    }

    #[test]
    fn gate_full_mode_still_asks_outside_authorized_scopes() {
        let ws = ws_with_file();
        let (args, resolved) = gate_args("edit_file", Path::new(r"C:\Users\Public\other\out.txt"));
        match gate_tool(
            "full",
            "edit_file",
            &args,
            &resolved,
            &ws,
            &[],
            &set_of(&[]),
        ) {
            Gate::Ask { remember_key, .. } => assert!(remember_key.unwrap().starts_with("write:")),
            other => panic!("expected Ask, got {other:?}"),
        }
    }

    #[test]
    fn gate_denies_when_paths_cannot_be_resolved() {
        let ws = ws_with_file();
        let args = serde_json::json!({ "path": r"C:\..\..\..\evil.txt" });
        let resolved = resolve_tool_paths("create_file", &args, &ws);
        assert!(resolved.is_err(), "resolve must fail closed");
    }

    #[test]
    fn gate_read_outside_workspace_asks_per_target() {
        let ws = ws_with_file();
        let (args, resolved) = gate_args("read_file", Path::new(r"C:\Windows\win.ini"));
        // win.ini sits in a protected directory -> deny outright.
        assert!(matches!(
            gate_tool("ask", "read_file", &args, &resolved, &ws, &[], &set_of(&[])),
            Gate::Deny(_)
        ));
        let (args, resolved) = gate_args("read_file", Path::new(r"C:\Users\Public\readme.txt"));
        match gate_tool("ask", "read_file", &args, &resolved, &ws, &[], &set_of(&[])) {
            Gate::Ask { remember_key, .. } => assert!(remember_key.unwrap().starts_with("read:")),
            other => panic!("expected Ask, got {other:?}"),
        }
    }

    #[test]
    fn gate_rename_validates_both_source_and_destination() {
        let ws = ws_with_file();
        let args = serde_json::json!({
            "path": ws.join("a.txt").display().to_string(),
            "new_path": r"C:\Windows\System32\evil.txt",
        });
        let resolved = resolve_tool_paths("rename_file", &args, &ws).unwrap();
        assert!(matches!(
            gate_tool(
                "full",
                "rename_file",
                &args,
                &resolved,
                &ws,
                &[],
                &set_of(&[])
            ),
            Gate::Deny(_)
        ));
    }

    #[test]
    fn gate_cwd_outside_workspace_requires_approval() {
        let ws = ws_with_file();
        let args = serde_json::json!({
            "command": "npm run build",
            "cwd": r"C:\Users\Public\project",
        });
        let resolved = resolve_tool_paths("run_command", &args, &ws).unwrap();
        match gate_tool(
            "full",
            "run_command",
            &args,
            &resolved,
            &ws,
            &[],
            &set_of(&[]),
        ) {
            Gate::Ask { remember_key, .. } => assert!(remember_key.unwrap().starts_with("cwd:")),
            other => panic!("expected Ask, got {other:?}"),
        }
    }

    #[test]
    fn gate_unresolvable_cwd_asks_even_in_full_mode() {
        let ws = ws_with_file();
        let args = serde_json::json!({
            "command": "dir",
            "cwd": r"C:\..\..\nope",
        });
        let resolved = resolve_tool_paths("run_command", &args, &ws);
        // Resolution fails -> the caller denies before gating; assert that.
        assert!(resolved.is_err());
    }

    // ---------- risky commands ----------

    #[test]
    fn risky_commands_detected_on_any_drive() {
        assert!(is_risky_command(
            r"dism /online /cleanup-image /restorehealth"
        ));
        assert!(is_risky_command(r"Remove-Item D:\Windows\Temp\* -Recurse"));
        assert!(is_risky_command(r"del /s C:\temp\*"));
        assert!(!is_risky_command(r"npm run build"));
        assert!(!is_risky_command(r"C:\ws\windows-tools\build.cmd"));
        assert!(is_risky_command(r"C:\Windows\System32\cmd.exe /c whoami"));
    }
}
