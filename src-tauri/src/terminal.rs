use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex as AsyncMutex;

pub struct ProcEntry {
    pub id: u32,
    pub name: String,
    pub command: String,
    pub started_at: i64,
    pub buffer: Arc<Mutex<Vec<String>>>,
    pub child: Arc<AsyncMutex<tokio::process::Child>>,
    pub exited: Arc<AtomicI32>,
}

pub struct Procs(pub Mutex<HashMap<u32, ProcEntry>>);

static FALLBACK_ID: AtomicU32 = AtomicU32::new(90000);

fn powershell(cmd: &str) -> Command {
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", cmd]);
    c
}

fn spawn_reader(
    app: AppHandle,
    chat_id: String,
    stream: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    sink: Arc<Mutex<Vec<u8>>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        let mut s = stream;
        loop {
            match s.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    if let Ok(mut g) = sink.lock() {
                        g.extend_from_slice(&buf[..n]);
                    }
                    let _ = app.emit("terminal:chunk", json!({ "chatId": chat_id, "data": chunk }));
                }
                Err(_) => break,
            }
        }
    })
}

/// Execute a PowerShell command and return (output, exit_code, timed_out).
pub async fn run_command(
    app: &AppHandle,
    chat_id: &str,
    cwd: &PathBuf,
    cmd: &str,
    timeout_secs: u64,
) -> Result<(String, Option<i32>, bool), String> {
    let mut child = powershell(cmd)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to start PowerShell: {e}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "No stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "No stderr".to_string())?;

    let out_sink: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(vec![]));
    let err_sink: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(vec![]));
    let so = spawn_reader(app.clone(), chat_id.to_string(), stdout, out_sink.clone());
    let se = spawn_reader(app.clone(), chat_id.to_string(), stderr, err_sink.clone());

    let streams_done = async {
        let _ = so.await;
        let _ = se.await;
    };
    let (code, timed_out) = tokio::select! {
        _ = streams_done => (child.wait().await.ok().and_then(|s| s.code()), false),
        _ = tokio::time::sleep(std::time::Duration::from_secs(timeout_secs)) => {
            let _ = child.start_kill();
            (None, true)
        }
    };

    let out = String::from_utf8_lossy(&out_sink.lock().map(|g| g.clone()).unwrap_or_default()).to_string();
    let err = String::from_utf8_lossy(&err_sink.lock().map(|g| g.clone()).unwrap_or_default()).to_string();
    let mut output = out;
    if !err.trim().is_empty() {
        output.push_str("\n[stderr]\n");
        output.push_str(&err);
    }
    // keep the tail so huge outputs stay manageable
    let chars: Vec<char> = output.chars().collect();
    if chars.len() > 24_000 {
        output = chars[chars.len() - 24_000..].iter().collect();
        output.insert_str(0, "… [output truncated]\n");
    }
    Ok((output, code, timed_out))
}

fn line_reader(
    app: AppHandle,
    id: u32,
    chat_id: String,
    stream: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    buffer: Arc<Mutex<Vec<String>>>,
) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if let Ok(mut b) = buffer.lock() {
                        b.push(line.clone());
                        if b.len() > 400 {
                            b.remove(0);
                        }
                    }
                    let _ = app.emit("process:output", json!({ "id": id, "data": line }));
                    let _ = app.emit("terminal:chunk", json!({ "chatId": chat_id, "data": format!("{line}\n") }));
                    for cap in url_re().captures_iter(&line) {
                        if let Some(m) = cap.get(0) {
                            let _ = app.emit(
                                "preview:server",
                                json!({ "processId": id, "chatId": chat_id, "url": m.as_str() }),
                            );
                        }
                    }
                }
                _ => break,
            }
        }
    });
}

static URL_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();

fn url_re() -> &'static regex::Regex {
    URL_RE.get_or_init(|| {
        regex::Regex::new(r#"https?://(?:localhost|127\.0\.0\.1):\d+[^\s"']*"#).unwrap()
    })
}

/// Start a long-running process (dev server etc.). Returns its id.
pub async fn start_process(
    app: &AppHandle,
    chat_id: &str,
    cwd: &PathBuf,
    name: &str,
    cmd: &str,
) -> Result<u32, String> {
    let child = powershell(cmd)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to start process: {e}"))?;

    let id = child.id().unwrap_or_else(|| FALLBACK_ID.fetch_add(1, Ordering::Relaxed));
    let buffer: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
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

    let exited = Arc::new(AtomicI32::new(-1));
    let wchild = child_arc.clone();
    let wapp = app.clone();
    let wid = id;
    let wexited = exited.clone();
    tokio::spawn(async move {
        let code = {
            let mut guard = wchild.lock().await;
            guard.wait().await.ok().and_then(|s| s.code())
        };
        wexited.store(code.unwrap_or(-2), Ordering::Relaxed);
        let _ = wapp.emit("process:exit", json!({ "id": wid, "code": code }));
    });

    let procs = app.state::<Procs>();
    procs.0.lock().map_err(|_| "process registry locked")?.insert(
        id,
        ProcEntry {
            id,
            name: name.to_string(),
            command: cmd.to_string(),
            started_at: crate::db::now_ms(),
            buffer,
            child: child_arc,
            exited,
        },
    );
    Ok(id)
}

pub fn stop(app: &AppHandle, id: u32) -> Result<(), String> {
    let procs = app.state::<Procs>();
    let g = procs.0.lock().map_err(|_| "process registry locked")?;
    if let Some(entry) = g.get(&id) {
        // start_kill is synchronous; block briefly on the async mutex via a helper task
        let child = entry.child.clone();
        drop(g);
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let mut guard = child.lock().await;
                let _ = guard.start_kill();
            });
        });
    }
    Ok(())
}

pub fn list_json(app: &AppHandle) -> Result<Vec<Value>, String> {
    let procs = app.state::<Procs>();
    let g = procs.0.lock().map_err(|_| "process registry locked")?;
    Ok(g.values()
        .map(|p| {
            let tail: Vec<String> = p.buffer.lock().map(|b| b.iter().rev().take(15).rev().cloned().collect()).unwrap_or_default();
            json!({
                "id": p.id,
                "name": p.name,
                "command": p.command,
                "startedAt": p.started_at,
                "exited": p.exited.load(Ordering::Relaxed),
                "recentOutput": tail,
            })
        })
        .collect())
}
