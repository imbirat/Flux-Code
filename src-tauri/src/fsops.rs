use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".venv",
    "venv",
    "__pycache__",
    "bin",
    "obj",
    ".gradle",
    ".idea",
    ".vs",
    "vendor",
    ".cache",
    ".turbo",
    "out",
    "coverage",
];

/// Text files larger than this are refused instead of being read whole.
pub const MAX_TEXT_BYTES: u64 = 2_000_000;

/// True when the entry is a directory symlink/junction/reparse point: such
/// entries are listed but never traversed, so traversal cannot follow links
/// out of the workspace or into a cycle.
fn is_link_dir(e: &fs::DirEntry) -> bool {
    match e.file_type() {
        Ok(ft) => ft.is_symlink(),
        Err(_) => true, // unreadable type: do not traverse
    }
}

/// Directory listing as an indented tree string.
pub fn tree(root: &Path, depth: usize, max_entries: usize) -> String {
    // A linked root would make every relative path ambiguous and could walk
    // outside the intended workspace.
    if let Ok(md) = fs::symlink_metadata(root) {
        if md.file_type().is_symlink() {
            return "(refusing to list a linked directory)\n".into();
        }
    }
    let mut out = String::new();
    let mut count = 0usize;
    walk_tree(root, root, 0, depth, &mut out, &mut count, max_entries);
    out
}

fn walk_tree(
    root: &Path,
    dir: &Path,
    level: usize,
    max_depth: usize,
    out: &mut String,
    count: &mut usize,
    max: usize,
) {
    if level > max_depth || *count >= max {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| {
        (
            !e.path().is_dir(),
            e.file_name().to_string_lossy().to_lowercase(),
        )
    });
    for e in entries {
        if *count >= max {
            out.push_str("… (truncated)\n");
            return;
        }
        let name = e.file_name().to_string_lossy().to_lowercase();
        let path = e.path();
        if path.is_dir() {
            if IGNORED_DIRS.contains(&name.as_str()) || is_link_dir(&e) {
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
/// The size is checked from metadata *before* any bytes are read, and the read
/// itself is bounded, so an oversized file never enters memory.
pub fn read_file(path: &Path, offset: usize, limit: usize) -> Result<(String, bool), String> {
    let md = fs::metadata(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if md.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "File too large to read: {} bytes (max {})",
            md.len(),
            MAX_TEXT_BYTES
        ));
    }
    let bytes = bounded_read(path, MAX_TEXT_BYTES as usize + 1)?;
    if bytes.len() as u64 > MAX_TEXT_BYTES {
        return Err(format!(
            "File too large to read (max {MAX_TEXT_BYTES} bytes)"
        ));
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

/// Read at most `cap` bytes without ever loading more into memory.
pub fn bounded_read(path: &Path, cap: usize) -> Result<Vec<u8>, String> {
    let mut f = fs::File::open(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let mut buf = Vec::with_capacity(cap.min(64 * 1024));
    std::io::Read::take(&mut f, cap as u64)
        .read_to_end(&mut buf)
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    Ok(buf)
}

/// Fail closed unless the path can still be modified safely (no symlink or
/// junction appeared since permission was granted).
fn ensure_mutable(path: &Path) -> Result<(), String> {
    crate::permissions::validate_mutation_target(path)
}

pub fn create_file(path: &Path, content: &str) -> Result<Value, String> {
    ensure_mutable(path)?;
    if path.exists() {
        return Err(format!(
            "{} already exists — use edit_file to change it",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, content).map_err(|e| format!("Cannot write {}: {e}", path.display()))?;
    Ok(json!({ "path": path.display().to_string(), "bytes": content.len() }))
}

pub fn edit_file(path: &Path, old: &str, new: &str, replace_all: bool) -> Result<Value, String> {
    ensure_mutable(path)?;
    if old.is_empty() {
        return Err("old_string must not be empty".into());
    }
    let md = fs::metadata(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    if md.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "File too large to edit (max {MAX_TEXT_BYTES} bytes)"
        ));
    }
    let bytes = bounded_read(path, MAX_TEXT_BYTES as usize + 1)?;
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
    ensure_mutable(path)?;
    if path.is_dir() {
        return Err("delete_file only works on single files".into());
    }
    fs::remove_file(path).map_err(|e| format!("Cannot delete {}: {e}", path.display()))?;
    Ok(json!({ "path": path.display().to_string() }))
}

pub fn create_folder(path: &Path) -> Result<Value, String> {
    ensure_mutable(path)?;
    fs::create_dir_all(path).map_err(|e| format!("Cannot create {}: {e}", path.display()))?;
    Ok(json!({ "path": path.display().to_string() }))
}

pub fn rename_path(path: &Path, new_path: &Path) -> Result<Value, String> {
    ensure_mutable(path)?;
    ensure_mutable(new_path)?;
    if !path.exists() {
        return Err(format!("{} does not exist", path.display()));
    }
    if new_path.exists() {
        return Err(format!("{} already exists", new_path.display()));
    }
    if let Some(parent) = new_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::rename(path, new_path).map_err(|e| format!("Cannot rename: {e}"))?;
    Ok(json!({ "path": path.display().to_string(), "newPath": new_path.display().to_string() }))
}

/// Collect up to `budget` file paths under `dir`. The budget counts
/// directories as well as files, ignored directories are skipped, and
/// directory symlinks/junctions are never followed — a junction cycle burns
/// budget instead of the stack. The result is sorted for determinism.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>, budget: &mut usize) {
    if *budget == 0 {
        return;
    }
    *budget = budget.saturating_sub(1);
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<fs::DirEntry> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for e in entries {
        if *budget == 0 {
            return;
        }
        let p = e.path();
        if p.is_dir() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            if IGNORED_DIRS.contains(&name.as_str()) || is_link_dir(&e) {
                continue;
            }
            *budget = budget.saturating_sub(1);
            collect_files(&p, out, budget);
        } else if *budget > 0 {
            // File symlinks are skipped as well: reading through one could
            // expose content outside the workspace.
            let is_link = e.file_type().map(|ft| ft.is_symlink()).unwrap_or(true);
            if is_link {
                continue;
            }
            out.push(p);
            *budget -= 1;
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
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };
    let mut files = vec![];
    let mut budget = 6000usize;
    collect_files(root, &mut files, &mut budget);
    files.sort();
    let mut hits: Vec<String> = vec![];
    for f in files {
        if hits.len() >= max {
            break;
        }
        let Ok(md) = fs::symlink_metadata(&f) else {
            continue;
        };
        if md.file_type().is_symlink() || md.len() > MAX_TEXT_BYTES {
            continue;
        }
        let Ok(bytes) = bounded_read(&f, MAX_TEXT_BYTES as usize) else {
            continue;
        };
        if bytes.contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let rel = rel_to(root, &f);
        for (i, line) in text.lines().enumerate() {
            let matched = match &re {
                Some(r) => r.is_match(line),
                None => {
                    let hay = if case_sensitive {
                        line.to_string()
                    } else {
                        line.to_lowercase()
                    };
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
            i += 1;
            j += 1;
            old_no += 1;
            new_no += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            rows.push(json!({ "type": "del", "text": a[i], "oldNo": old_no }));
            i += 1;
            old_no += 1;
        } else {
            rows.push(json!({ "type": "add", "text": b[j], "newNo": new_no }));
            j += 1;
            new_no += 1;
        }
    }
    while i < n && rows.len() < 400 {
        rows.push(json!({ "type": "del", "text": a[i], "oldNo": old_no }));
        i += 1;
        old_no += 1;
    }
    while j < m && rows.len() < 400 {
        rows.push(json!({ "type": "add", "text": b[j], "newNo": new_no }));
        j += 1;
        new_no += 1;
    }
    rows.truncate(400);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_dir;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TMP: AtomicUsize = AtomicUsize::new(0);

    fn tmp(name: &str) -> PathBuf {
        let n = NEXT_TMP.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!("fluxfs-{}-{}-{n}", std::process::id(), name));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn read_file_refuses_oversized_files_without_reading_them() {
        let ws = tmp("big");
        let p = ws.join("big.txt");
        // Write just over the cap.
        let line = "x".repeat(1000);
        fs::write(&p, line.repeat(2100)).unwrap(); // ~2.1 MB
        let err = read_file(&p, 1, 100).unwrap_err();
        assert!(err.contains("too large"), "unexpected error: {err}");
    }

    #[test]
    fn read_file_reports_truncation_accurately() {
        let ws = tmp("trunc");
        let p = ws.join("many.txt");
        fs::write(
            &p,
            (1..=50)
                .map(|i| format!("line {i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        let (text, truncated) = read_file(&p, 1, 10).unwrap();
        assert!(truncated);
        assert!(text.contains("line 10"));
        assert!(text.contains("Showing lines 1–10 of 50"));
        let (text, truncated) = read_file(&p, 40, 100).unwrap();
        assert!(!truncated);
        assert!(text.contains("line 50"));
    }

    #[test]
    fn read_file_detects_binary() {
        let ws = tmp("bin");
        let p = ws.join("b.bin");
        fs::write(&p, [0u8, 1, 2, 3]).unwrap();
        assert!(read_file(&p, 1, 10).is_err());
    }

    #[test]
    fn tree_skips_ignored_and_symlink_directories() {
        let ws = tmp("skips");
        fs::create_dir_all(ws.join(".git").join("objects")).unwrap();
        fs::create_dir_all(ws.join("node_modules").join("x")).unwrap();
        fs::create_dir_all(ws.join("src")).unwrap();
        fs::write(ws.join("src").join("main.rs"), "fn main() {}").unwrap();
        fs::write(ws.join(".git").join("config"), "x").unwrap();
        let t = tree(&ws, 3, 100);
        assert!(t.contains("src/"), "tree should list src: {t}");
        assert!(t.contains("main.rs"));
        assert!(!t.contains(".git"), "ignored dir must not appear: {t}");
        assert!(!t.contains("node_modules"));
    }

    #[cfg(windows)]
    #[test]
    fn tree_and_collect_files_survive_junction_cycles() {
        let ws = tmp("cycle");
        let inner = ws.join("inner");
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("f.txt"), "x").unwrap();
        // A junction pointing back at the parent creates a directory cycle.
        match symlink_dir(&ws, inner.join("loop")) {
            Ok(_) => {
                // Must complete without a stack overflow, bounded output.
                let t = tree(&ws, 10, 200);
                assert!(
                    t.contains("… (truncated)") || t.len() < 20_000,
                    "cycle produced runaway output"
                );
                let mut files = vec![];
                let mut budget = 500usize;
                collect_files(&ws, &mut files, &mut budget);
                assert!(files.len() <= 500);
                assert!(files.iter().all(|f| f.starts_with(&ws)));
                // The junction must never be traversed: no duplicated f.txt
                let count = files.iter().filter(|f| f.ends_with("f.txt")).count();
                assert_eq!(count, 1, "junction cycle duplicated files: {files:?}");
                fs::remove_dir(inner.join("loop")).unwrap();
            }
            Err(e) => {
                // Junction creation can require privileges on some systems;
                // skip rather than fail the suite.
                eprintln!("skipping junction cycle test: {e}");
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn search_does_not_follow_junctions_out_of_the_workspace() {
        let ws = tmp("junction-escape");
        let outside = std::env::temp_dir().join(format!("fluxfs-outside-{}", std::process::id()));
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "needle outside").unwrap();
        match symlink_dir(&outside, ws.join("link")) {
            Ok(_) => {
                let hits = search(&ws, "needle outside", false, false, 10).unwrap();
                assert!(
                    hits.is_empty(),
                    "search followed a junction out of the workspace: {hits:?}"
                );
                fs::remove_dir(ws.join("link")).unwrap();
            }
            Err(e) => eprintln!("skipping junction escape test: {e}"),
        }
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn collect_files_budget_counts_directories_too() {
        let ws = tmp("budget");
        for i in 0..5 {
            fs::create_dir_all(ws.join(format!("d{i}"))).unwrap();
            fs::write(ws.join(format!("d{i}")).join("f.txt"), "x").unwrap();
        }
        // Budget 6: root visit + d0 entry + d0 visit + f + d1 entry + d1 visit.
        // Everything after that is refused — files in d2..d4 never appear.
        let mut files = vec![];
        let mut budget = 6usize;
        collect_files(&ws, &mut files, &mut budget);
        assert_eq!(
            files.len(),
            1,
            "small budget must stop before later files: {files:?}"
        );
        assert!(files[0].ends_with(r"d0\f.txt"), "unexpected: {files:?}");
        // Generous budget finds all five files.
        let mut files = vec![];
        let mut budget = 100usize;
        collect_files(&ws, &mut files, &mut budget);
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn collect_files_is_deterministic_and_sorted() {
        let ws = tmp("deterministic");
        for i in 0..10 {
            fs::write(ws.join(format!("f{i:02}.txt")), "x").unwrap();
        }
        let mut files = vec![];
        let mut budget = 100usize;
        collect_files(&ws, &mut files, &mut budget);
        files.sort();
        let mut files2 = vec![];
        let mut budget = 100usize;
        collect_files(&ws, &mut files2, &mut budget);
        files2.sort();
        assert_eq!(files, files2);
    }

    #[test]
    fn edit_file_refuses_oversized_files() {
        let ws = tmp("editbig");
        let p = ws.join("big.txt");
        fs::write(&p, "y".repeat(2_100_000)).unwrap();
        let err = edit_file(&p, "y", "z", false).unwrap_err();
        assert!(err.contains("too large"), "unexpected error: {err}");
    }
}
