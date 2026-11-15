use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub const IGNORED_DIRS: &[&str] = &[
    ".git", "node_modules", "target", "dist", "build", ".next", ".venv", "venv",
    "__pycache__", "bin", "obj", ".gradle", ".idea", ".vs", "vendor", ".cache",
    ".turbo", "out", "coverage",
];

/// Directory listing as an indented tree string.
pub fn tree(root: &Path, depth: usize, max_entries: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    walk_tree(root, root, 0, depth, &mut out, &mut count, max_entries);
    out
}

fn walk_tree(root: &Path, dir: &Path, level: usize, max_depth: usize, out: &mut String, count: &mut usize, max: usize) {
    if level > max_depth || *count >= max {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| (!e.path().is_dir(), e.file_name().to_string_lossy().to_lowercase()));
    for e in entries {
        if *count >= max {
            out.push_str("… (truncated)\n");
            return;
        }
        let name = e.file_name().to_string_lossy().to_lowercase();
        let path = e.path();
        if path.is_dir() {
            if IGNORED_DIRS.contains(&name.as_str()) {
                continue;
            }
            let rel = rel_to(root, &path);
            out.push_str(&format!("{rel}/\n"));
            *count += 1;
            walk_tree(root, &path, level + 1, max_depth, out, count, max);
        } else {
            let rel = rel_to(root, &path);
            out.push_str(&format!("{rel}\n"));
            *count += 1;
        }
    }
}

fn rel_to(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Read a text file with optional 1-based line offset. Returns (text, truncated).
pub fn read_file(path: &Path, offset: usize, limit: usize) -> Result<(String, bool), String> {
    let bytes = fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if bytes.len() > 2_000_000 {
        return Err(format!("File too large to read: {} bytes", bytes.len()));
    }
    if bytes.contains(&0) {
        return Err("File appears to be binary".into());
    }
    let text = String::from_utf8_lossy(&bytes);
    let total = text.lines().count();
    let skip = offset.saturating_sub(1);
    let lim = if limit == 0 { 1000 } else { limit };
    let selected: Vec<&str> = text.lines().skip(skip).take(lim).collect();
    let got = selected.len();
    let mut out = selected.join("\n");
    if out.is_empty() {
        out = "(empty file)".into();
    }
    let truncated = skip + got < total;
    if truncated {
        out.push_str(&format!(
            "\n\n[Showing lines {}–{} of {total}. Call again with a higher offset for more.]",
            skip + 1,
            skip + got
        ));
    }
    Ok((out, truncated))
}

pub fn create_file(path: &Path, content: &str) -> Result<Value, String> {
    if path.exists() {
        return Err(format!("{} already exists — use edit_file to change it", path.display()));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, content).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(json!({ "path": path.display().to_string(), "bytes": content.len() }))
}

pub fn edit_file(path: &Path, old: &str, new: &str, replace_all: bool) -> Result<Value, String> {
    if old.is_empty() {
        return Err("old_string must not be empty".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if bytes.contains(&0) {
        return Err("File appears to be binary".into());
    }
    let text = String::from_utf8_lossy(&bytes).to_string();
    let count = text.matches(old).count();
    if count == 0 {
        return Err("old_string not found in the file. Copy the exact text, including whitespace and indentation.".into());
    }
    if count > 1 && !replace_all {
        return Err(format!("old_string appears {count} times — add more surrounding lines to make it unique, or set replace_all to true"));
    }
    let updated = if replace_all {
        text.replace(old, new)
    } else {
        text.replacen(old, new, 1)
    };
    fs::write(path, &updated).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    let diff = diff_lines(&text, &updated);
    Ok(json!({ "path": path.display().to_string(), "diff": diff }))
}

pub fn delete_file(path: &Path) -> Result<Value, String> {
    if path.is_dir() {
        return Err("delete_file only works on single files".into());
    }
    fs::remove_file(path).map_err(|e| format!("Cannot delete {}: {e}", path.display()))?;
    Ok(json!({ "path": path.display().to_string() }))
}

pub fn create_folder(path: &Path) -> Result<Value, String> {
    fs::create_dir_all(path).map_err(|e| format!("Cannot create {}: {e}", path.display()))?;
    Ok(json!({ "path": path.display().to_string() }))
}

pub fn rename_path(path: &Path, new_path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Err(format!("{} does not exist", path.display()));
    }
    if let Some(parent) = new_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(path, new_path).map_err(|e| format!("Cannot rename: {e}"))?;
    Ok(json!({ "path": path.display().to_string(), "newPath": new_path.display().to_string() }))
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>, budget: usize) {
    if out.len() >= budget {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.filter_map(|e| e.ok()) {
        let p = e.path();
        if p.is_dir() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if IGNORED_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect_files(&p, out, budget);
        } else if out.len() < budget {
            out.push(p);
        }
    }
}

/// Search file contents. Plain text or regex, with sensible defaults.
pub fn search(
    root: &Path,
    query: &str,
    is_regex: bool,
    case_sensitive: bool,
    max: usize,
) -> Result<Vec<String>, String> {
    if query.is_empty() {
        return Err("Empty search query".into());
    }
    let max = max.clamp(1, 100);
    let re = if is_regex {
        Some(
            regex::RegexBuilder::new(query)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|e| format!("Invalid regex: {e}"))?,
        )
    } else {
        None
    };
    let needle = if case_sensitive { query.to_string() } else { query.to_lowercase() };
    let mut files = vec![];
    collect_files(root, &mut files, 6000);
    let mut hits: Vec<String> = vec![];
    for f in files {
        if hits.len() >= max {
            break;
        }
        let Ok(md) = fs::metadata(&f) else { continue };
        if md.len() > 2_000_000 {
            continue;
        }
        let Ok(bytes) = fs::read(&f) else { continue };
        if bytes.contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let rel = rel_to(root, &f);
        for (i, line) in text.lines().enumerate() {
            let matched = match &re {
                Some(r) => r.is_match(line),
                None => {
                    let hay = if case_sensitive { line.to_string() } else { line.to_lowercase() };
                    hay.contains(&needle)
                }
            };
            if matched {
                let shown: String = line.trim().chars().take(240).collect();
                hits.push(format!("{rel}:{}: {shown}", i + 1));
                if hits.len() >= max {
                    break;
                }
            }
        }
    }
    Ok(hits)
}

/// A minimal LCS line diff, returned as UI-friendly rows.
pub fn diff_lines(old: &str, new: &str) -> Vec<Value> {
    let a: Vec<&str> = old.lines().take(500).collect();
    let b: Vec<&str> = new.lines().take(500).collect();
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut rows: Vec<Value> = vec![];
    let (mut i, mut j) = (0usize, 0usize);
    let (mut old_no, mut new_no) = (1usize, 1usize);
    while i < n && j < m && rows.len() < 400 {
        if a[i] == b[j] {
            rows.push(json!({ "type": "same", "text": a[i], "oldNo": old_no, "newNo": new_no }));
            i += 1; j += 1; old_no += 1; new_no += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            rows.push(json!({ "type": "del", "text": a[i], "oldNo": old_no }));
            i += 1; old_no += 1;
        } else {
            rows.push(json!({ "type": "add", "text": b[j], "newNo": new_no }));
            j += 1; new_no += 1;
        }
    }
    while i < n && rows.len() < 400 {
        rows.push(json!({ "type": "del", "text": a[i], "oldNo": old_no }));
        i += 1; old_no += 1;
    }
    while j < m && rows.len() < 400 {
        rows.push(json!({ "type": "add", "text": b[j], "newNo": new_no }));
        j += 1; new_no += 1;
    }
    rows.truncate(400);
    rows
}
