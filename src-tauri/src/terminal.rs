use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{oneshot, Mutex as AsyncMutex};

/// Maximum bytes kept from a foreground command's stdout/stderr.
const MAX_COMMAND_BYTES: usize = 256 * 1024;
/// Maximum lines retained per background process for the UI.
const MAX_PROCESS_LINES: usize = 400;
/// Maximum characters kept from one background output line.
const MAX_LINE_CHARS: usize = 4000;
/// Finished process entries older than this are pruned from the registry.
const EXITED_RETENTION_MS: i64 = 30 * 60 * 1000;
const MAX_PROCESS_ENTRIES: usize = 50;

pub struct ProcEntry {
    pub id: u32,
    pub chat_id: String,
    pub name: String,
    pub command: String,
    pub started_at: i64,
    pub buffer: Arc<Mutex<VecDeque<String>>>,
    /// -1 while running, exit code (or -2 when unknown) after exit.
    pub exited: Arc<AtomicI32>,
    pub stopping: Arc<AtomicBool>,
    /// Signalled to request termination. Taken exactly once.
    pub stop: Option<oneshot::Sender<()>>,
}

pub struct Procs(pub Mutex<HashMap<u32, ProcEntry>>);

static FALLBACK_ID: AtomicU32 = AtomicU32::new(90000);

fn powershell(cmd: &str) -> Command {
    let mut c = Command::new("powershell");
    c.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        cmd,
    ]);
    c
}

fn tail_chars(s: &str, n: usize) -> String {
    let c = s.chars().count();
    if c <= n {
        s.to_string()
    } else {
        s.chars().skip(c - n).collect()
    }
}

// ---------- foreground commands ----------

struct CappedBuf {
    data: Vec<u8>,
    truncated: bool,
}

impl CappedBuf {
    fn push(&mut self, bytes: &[u8]) {
        let room = MAX_COMMAND_BYTES.saturating_sub(self.data.len());
        if room == 0 {
            self.truncated = true;
            return;
        }
        let take = room.min(bytes.len());
        self.data.extend_from_slice(&bytes[..take]);
        if take < bytes.len() {
            self.truncated = true;
        }
    }
}

fn spawn_reader(
    app: AppHandle,
    chat_id: String,
    stream: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    sink: Arc<Mutex<CappedBuf>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        let mut s = stream;
        // Live events are capped as well: a chatty command must not flood the
        // UI with unbounded IPC traffic. The pipe is still drained below so the
        // child never blocks on a full buffer.
        let mut emitted = 0usize;
        loop {
            match s.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut g) = sink.lock() {
                        g.push(&buf[..n]);
                    }
                    if emitted < MAX_COMMAND_BYTES {
                        emitted += n;
                        let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = app.emit(
                            "terminal:chunk",
                            json!({ "chatId": chat_id, "data": chunk }),
                        );
                    }
                }
                Err(_) => break,
            }
        }
    })
}

/// Terminate a process and every child it spawned. On Windows a plain
/// `Child::kill` leaves PowerShell's children (npm, node, dev servers) running;
/// `taskkill /T /F` walks the process tree.
async fn kill_tree(pid: Option<u32>) {
    #[cfg(windows)]
    {
        if let Some(pid) = pid {
            let _ = tokio::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(pid) = pid {
            let _ = tokio::process::Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
        }
    }
}

/// Execute a PowerShell command and return (output, exit_code, timed_out).
/// The timeout covers the whole command lifetime — waiting for the process,
/// not just closing the streams. On timeout the entire process tree is killed
/// and reaped so no orphan keeps running.
pub async fn run_command(
    app: &AppHandle,
    chat_id: &str,
    cwd: &PathBuf,
    cmd: &str,
    timeout_secs: u64,
) -> Result<(String, Option<i32>, bool), String> {
    if !cwd.is_dir() {
        return Err(format!(
            "Working directory does not exist: {}",
            cwd.display()
        ));
    }
    let mut child = powershell(cmd)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to start PowerShell: {e}"))?;
    let pid = child.id();

    let stdout = child.stdout.take().ok_or_else(|| "No stdout".to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| "No stderr".to_string())?;

    let out_sink: Arc<Mutex<CappedBuf>> = Arc::new(Mutex::new(CappedBuf {
        data: vec![],
        truncated: false,
    }));
    let err_sink: Arc<Mutex<CappedBuf>> = Arc::new(Mutex::new(CappedBuf {
        data: vec![],
        truncated: false,
    }));
    let so = spawn_reader(app.clone(), chat_id.to_string(), stdout, out_sink.clone());
    let se = spawn_reader(app.clone(), chat_id.to_string(), stderr, err_sink.clone());

    let (code, timed_out) = match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs.max(1)),
        child.wait(),
    )
    .await
    {
        Ok(Ok(status)) => (status.code(), false),
        Ok(Err(e)) => {
            kill_tree(pid).await;
            let _ = child.start_kill();
            return Err(format!("Command failed while running: {e}"));
        }
        Err(_) => {
            // Kill the tree, then reap the direct child so it cannot linger.
            kill_tree(pid).await;
            let _ = child.start_kill();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;
            (None, true)
        }
    };

    // After the child exits its pipes close. Give the readers a moment to
    // drain, then abort so a misbehaving grandchild cannot block the turn.
    for handle in [so, se] {
        if tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .is_err()
        {
            // Timed out waiting; the join handle was consumed by timeout, so
            // the reader task is detached and will end on EOF/error.
        }
    }

    let (out, out_trunc) = out_sink
        .lock()
        .map(|g| (String::from_utf8_lossy(&g.data).to_string(), g.truncated))
        .unwrap_or_default();
    let (err, err_trunc) = err_sink
        .lock()
        .map(|g| (String::from_utf8_lossy(&g.data).to_string(), g.truncated))
        .unwrap_or_default();
    let mut output = out;
    if !err.trim().is_empty() {
        output.push_str("\n[stderr]\n");
        output.push_str(&err);
    }
    if out_trunc || err_trunc {
        output.push_str("\n… [output truncated]");
    }
    // Keep the tail so huge outputs stay manageable to the model as well.
    let output = if output.chars().count() > 24_000 {
        format!("… [output truncated]\n{}", tail_chars(&output, 24_000))
    } else {
        output
    };
    Ok((output, code, timed_out))
}

// ---------- background processes ----------

/// Maximum bytes retained for one background output line. A process that
/// prints a huge line without a newline must not grow memory without bound.
const MAX_LINE_BYTES: usize = 64 * 1024;

/// Read newline-delimited output with a hard cap on the retained partial line.
/// The stream is always drained, so a chatty child never blocks on a full pipe.
async fn read_lines_bounded(
    mut stream: impl tokio::io::AsyncRead + Unpin,
    mut on_line: impl FnMut(String),
) {
    let mut buf = [0u8; 8192];
    let mut pending: Vec<u8> = Vec::new();
    loop {
        match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let mut start = 0;
                for (i, byte) in buf[..n].iter().enumerate() {
                    if *byte == b'\n' {
                        pending.extend_from_slice(&buf[start..i]);
                        let line = String::from_utf8_lossy(&pending);
                        on_line(line.trim_end_matches('\r').to_string());
                        pending.clear();
                        start = i + 1;
                    }
                }
                if start < n {
                    pending.extend_from_slice(&buf[start..n]);
                    // Keep the tail: the end of a line carries the useful part
                    // (errors, URLs) and memory stays bounded.
                    if pending.len() > MAX_LINE_BYTES * 2 {
                        let cut = pending.len() - MAX_LINE_BYTES;
                        pending.drain(..cut);
                    }
                }
            }
            Err(_) => break,
        }
    }
    if !pending.is_empty() {
        let line = String::from_utf8_lossy(&pending);
        on_line(line.trim_end_matches('\r').to_string());
    }
}

fn line_reader(
    app: AppHandle,
    id: u32,
    chat_id: String,
    stream: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    buffer: Arc<Mutex<VecDeque<String>>>,
) {
    tokio::spawn(async move {
        read_lines_bounded(stream, |line| {
            let line = tail_chars(&line, MAX_LINE_CHARS);
            if let Ok(mut b) = buffer.lock() {
                b.push_back(line.clone());
                while b.len() > MAX_PROCESS_LINES {
                    b.pop_front();
                }
            }
            let _ = app.emit("process:output", json!({ "id": id, "data": line }));
            let _ = app.emit(
                "terminal:chunk",
                json!({ "chatId": chat_id, "data": format!("{line}\n") }),
            );
            for cap in url_re().captures_iter(&line) {
                if let Some(m) = cap.get(0) {
                    let _ = app.emit(
                        "preview:server",
                        json!({ "processId": id, "chatId": chat_id, "url": m.as_str() }),
                    );
                }
            }
        })
        .await;
    });
}

static URL_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

fn url_re() -> &'static regex::Regex {
    URL_RE.get_or_init(|| {
        regex::Regex::new(r#"https?://(?:localhost|127\.0\.0\.1):\d+[^\s"']*"#).unwrap()
    })
}

fn prune_registry(map: &mut HashMap<u32, ProcEntry>) {
    let now = crate::db::now_ms();
    map.retain(|_, e| {
        e.exited.load(Ordering::Relaxed) < 0
            || now.saturating_sub(e.started_at) < EXITED_RETENTION_MS
    });
    if map.len() > MAX_PROCESS_ENTRIES {
        let mut finished: Vec<(u32, i64)> = map
            .iter()
            .filter(|(_, e)| e.exited.load(Ordering::Relaxed) >= 0)
            .map(|(id, e)| (*id, e.started_at))
            .collect();
        finished.sort_by_key(|(_, t)| *t);
        let excess = map.len().saturating_sub(MAX_PROCESS_ENTRIES);
        for (id, _) in finished.into_iter().take(excess) {
            map.remove(&id);
        }
    }
}

/// Start a long-running process (dev server etc.). Returns its id.
pub async fn start_process(
    app: &AppHandle,
    chat_id: &str,
    cwd: &PathBuf,
    name: &str,
    cmd: &str,
) -> Result<u32, String> {
    if !cwd.is_dir() {
        return Err(format!(
            "Working directory does not exist: {}",
            cwd.display()
        ));
    }
    let child = powershell(cmd)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to start process: {e}"))?;

    let pid = child.id();
    let id = pid.unwrap_or_else(|| FALLBACK_ID.fetch_add(1, Ordering::Relaxed));
    let buffer: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let exited = Arc::new(AtomicI32::new(-1));
    let stopping = Arc::new(AtomicBool::new(false));
    let child_arc = Arc::new(AsyncMutex::new(child));
    {
        let mut guard = child_arc.lock().await;
        let so = guard.stdout.take();
        let se = guard.stderr.take();
        drop(guard);
        if let Some(s) = so {
            line_reader(app.clone(), id, chat_id.to_string(), s, buffer.clone());
        }
        if let Some(s) = se {
            line_reader(app.clone(), id, chat_id.to_string(), s, buffer.clone());
        }
    }

    let (stop_tx, mut stop_rx) = oneshot::channel::<()>();
    let wchild = child_arc.clone();
    let wapp = app.clone();
    let wid = id;
    let wexited = exited.clone();
    let wstopping = stopping.clone();
    let wchat = chat_id.to_string();
    tokio::spawn(async move {
        let mut guard = wchild.lock().await;
        let mut status = tokio::select! {
            s = guard.wait() => Some(s),
            _ = &mut stop_rx => None,
        };
        if status.is_none() {
            wstopping.store(true, Ordering::Relaxed);
            kill_tree(pid).await;
            let _ = guard.start_kill();
            status = Some(
                tokio::time::timeout(std::time::Duration::from_secs(5), guard.wait())
                    .await
                    .map_err(|_| {
                        std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "process did not exit after kill",
                        )
                    })
                    .and_then(|r| r),
            );
        }
        let code = match status {
            Some(Ok(st)) => st.code(),
            _ => None,
        };
        wexited.store(code.unwrap_or(-2), Ordering::Relaxed);
        let _ = wapp.emit(
            "process:exit",
            json!({ "id": wid, "chatId": wchat, "code": code }),
        );
    });

    let procs = app.state::<Procs>();
    let mut g = procs.0.lock().map_err(|_| "process registry locked")?;
    prune_registry(&mut g);
    g.insert(
        id,
        ProcEntry {
            id,
            chat_id: chat_id.to_string(),
            name: name.to_string(),
            command: cmd.to_string(),
            started_at: crate::db::now_ms(),
            buffer,
            exited,
            stopping,
            stop: Some(stop_tx),
        },
    );
    Ok(id)
}

/// Request termination. `chat_id` scopes agent-initiated stops so one
/// conversation can never stop another's process; the UI passes `None`.
pub fn stop(app: &AppHandle, id: u32, chat_id: Option<&str>) -> Result<(), String> {
    let procs = app.state::<Procs>();
    let mut g = procs.0.lock().map_err(|_| "process registry locked")?;
    let entry = g
        .get_mut(&id)
        .ok_or_else(|| format!("Unknown process #{id}"))?;
    if let Some(cid) = chat_id {
        if entry.chat_id != cid {
            return Err(format!("Process #{id} belongs to another conversation"));
        }
    }
    if entry.exited.load(Ordering::Relaxed) >= 0 {
        return Ok(());
    }
    entry.stopping.store(true, Ordering::Relaxed);
    if let Some(tx) = entry.stop.take() {
        let _ = tx.send(());
    }
    Ok(())
}

/// Stop every process owned by a chat (used when the chat is deleted).
pub fn stop_all_for_chat(app: &AppHandle, chat_id: &str) -> usize {
    let Some(procs) = app.try_state::<Procs>() else {
        return 0;
    };
    let Ok(mut g) = procs.0.lock() else { return 0 };
    let mut n = 0;
    for entry in g.values_mut() {
        if entry.chat_id != chat_id || entry.exited.load(Ordering::Relaxed) >= 0 {
            continue;
        }
        entry.stopping.store(true, Ordering::Relaxed);
        if let Some(tx) = entry.stop.take() {
            let _ = tx.send(());
            n += 1;
        }
    }
    n
}

pub fn list_json(app: &AppHandle) -> Result<Vec<Value>, String> {
    let procs = app.state::<Procs>();
    let g = procs.0.lock().map_err(|_| "process registry locked")?;
    let mut out: Vec<Value> = g
        .values()
        .map(|p| {
            let exited = p.exited.load(Ordering::Relaxed);
            let running = exited < 0;
            let stopping = p.stopping.load(Ordering::Relaxed);
            let state = if running {
                if stopping {
                    "stopping"
                } else {
                    "running"
                }
            } else {
                "exited"
            };
            let tail: Vec<String> = p
                .buffer
                .lock()
                .map(|b| b.iter().rev().take(15).rev().cloned().collect())
                .unwrap_or_default();
            json!({
                "id": p.id,
                "chatId": p.chat_id,
                "name": p.name,
                "command": p.command,
                "startedAt": p.started_at,
                "exited": exited,
                "running": running,
                "state": state,
                "recentOutput": tail,
            })
        })
        .collect();
    out.sort_by_key(|p| p["startedAt"].as_i64().unwrap_or(0));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bounded_line_reader_handles_crlf_and_final_line() {
        let input = b"a\r\nb\nc";
        let mut got = Vec::new();
        read_lines_bounded(&input[..], |line| got.push(line)).await;
        assert_eq!(got, vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn bounded_line_reader_caps_giant_lines() {
        let mut input = vec![b'x'; MAX_LINE_BYTES * 3];
        input.push(b'\n');
        let mut got = String::new();
        read_lines_bounded(&input[..], |line| got = line).await;
        // The retained tail stays bounded even though the line was 3x the cap.
        assert!(got.len() > MAX_LINE_BYTES);
        assert!(got.len() <= MAX_LINE_BYTES * 2);
    }
}
