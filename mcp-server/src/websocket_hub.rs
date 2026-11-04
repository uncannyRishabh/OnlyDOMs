use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::collections::{HashMap, VecDeque};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use futures_util::{StreamExt, SinkExt};
use serde_json::Value;
use tracing::{info, warn, error};
use std::time::Duration;
use crate::tab_registry::TabRegistry;
use crate::tools::hud_state::AgentTaskState;
use crate::tools::network_harvester::NetworkHarvester;
use crate::filter::hud_sanitizer::sanitize_extension_payload;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ActiveDialog {
    pub tab_id: u64,
    pub dialog_type: String, // "alert" | "confirm" | "prompt" | "beforeunload"
    pub message: String,
    pub url: String,
    pub default_prompt: Option<String>,
    pub opened_at: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpawnedPopup {
    pub tab_id: u64,
    pub opener_tab_id: Option<u64>,
    pub url: String,
    pub is_popup: bool,
}

pub struct HubState {
    pub tx: Option<mpsc::Sender<Value>>,
    pub hud_tx: Option<mpsc::Sender<Value>>,
    pub conn_token: u64,
    pub pending: HashMap<String, oneshot::Sender<Value>>,
    pub active_port: u16,
    pub tab_registry: TabRegistry,
    pub active_dialogs: HashMap<u64, ActiveDialog>,
    pub spawned_popups: HashMap<u64, SpawnedPopup>,
    pub hud_tasks: HashMap<u64, AgentTaskState>,
    pub hud_broadcast_log: VecDeque<Value>,
    pub network_harvester: NetworkHarvester,
}

impl HubState {
    pub fn new() -> Self {
        Self {
            tx: None,
            hud_tx: None,
            conn_token: 0,
            pending: HashMap::new(),
            active_port: 9222,
            tab_registry: TabRegistry::new(),
            active_dialogs: HashMap::new(),
            spawned_popups: HashMap::new(),
            hud_tasks: HashMap::new(),
            hud_broadcast_log: VecDeque::new(),
            network_harvester: NetworkHarvester::new(),
        }
    }
}

pub type SharedHub = Arc<Mutex<HubState>>;

/// Resolves a `/reports/<run_id>/<file>` HTTP target into a validated local path under `reports_root`,
/// returning `(resolved_path, content_type, is_auto_print_query)`.
pub fn resolve_report_file_path(
    request_target: &str,
    reports_root: &Path,
) -> Result<(PathBuf, &'static str, bool), (u16, &'static str)> {
    let (raw_path, query_opt) = match request_target.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (request_target, None),
    };

    let is_auto_print = query_opt
        .map(|q| {
            q.split('&')
                .any(|pair| pair.trim() == "print=1" || pair.trim() == "print=true")
        })
        .unwrap_or(false);

    let stripped = raw_path
        .strip_prefix("/reports/")
        .ok_or((404, "Not Found"))?;

    if stripped.is_empty() || stripped.contains('\0') || stripped.contains('\\') {
        return Err((403, "Forbidden"));
    }

    let rel_path = Path::new(stripped);
    let mut clean_rel = PathBuf::new();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(seg) => {
                let s = seg.to_string_lossy();
                if s.starts_with('.') {
                    return Err((403, "Forbidden"));
                }
                clean_rel.push(seg);
            }
            _ => return Err((403, "Forbidden")),
        }
    }

    if clean_rel.as_os_str().is_empty() {
        return Err((404, "Not Found"));
    }

    // If a trailing slash or directory name was requested (e.g. /reports/<run_id>/), default to overview.html
    let mut full_path = reports_root.join(&clean_rel);
    if raw_path.ends_with('/') || full_path.is_dir() {
        full_path = full_path.join("overview.html");
    }

    let ext = full_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let content_type = match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "pdf" => "application/pdf",
        "webp" => "image/webp",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "json" => "application/json; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        _ => "application/octet-stream",
    };

    Ok((full_path, content_type, is_auto_print))
}

/// Handles an HTTP `GET /reports/<run_id>/...` or `HEAD /reports/<run_id>/...` request on port 9222.
pub async fn handle_http_report_stream<S>(mut stream: S, reports_root: &Path)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut req_buf = vec![0u8; 4096];
    let n = match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut req_buf)).await {
        Ok(Ok(n)) if n > 0 => n,
        _ => return,
    };

    let req_str = String::from_utf8_lossy(&req_buf[..n]);
    let first_line = req_str.lines().next().unwrap_or("");
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");

    if method != "GET" && method != "HEAD" {
        let resp = b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 18\r\nConnection: close\r\n\r\nMethod Not Allowed";
        let _ = stream.write_all(resp).await;
        return;
    }

    match resolve_report_file_path(target, reports_root) {
        Ok((file_path, content_type, _is_auto_print)) => {
            match tokio::fs::read(&file_path).await {
                Ok(bytes) => {
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
                        content_type,
                        bytes.len()
                    );
                    let _ = stream.write_all(header.as_bytes()).await;
                    if method == "GET" {
                        let _ = stream.write_all(&bytes).await;
                    }
                    let _ = stream.flush().await;
                }
                Err(_) => {
                    let body = b"Report artifact not found";
                    let header = format!(
                        "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(header.as_bytes()).await;
                    let _ = stream.write_all(body).await;
                }
            }
        }
        Err((code, status_text)) => {
            let body = status_text.as_bytes();
            let header = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                code,
                status_text,
                body.len()
            );
            let _ = stream.write_all(header.as_bytes()).await;
            let _ = stream.write_all(body).await;
        }
    }
}

pub async fn start_hub(initial_port: u16, state: SharedHub) {
    let mut candidate_ports = vec![initial_port];
    for p in &[9922, 9222, 9223, 9923, 9224, 9924] {
        if !candidate_ports.contains(p) {
            candidate_ports.push(*p);
        }
    }
    for offset in 1..=5 {
        let p = initial_port.saturating_add(offset);
        if !candidate_ports.contains(&p) {
            candidate_ports.push(p);
        }
    }

    let mut listener = None;
    let mut active_port = initial_port;

    for port in candidate_ports {
        let addr = format!("127.0.0.1:{}", port);
        match TcpListener::bind(&addr).await {
            Ok(l) => {
                info!("WebSocket & Report Hub listening on http://{}", addr);
                active_port = port;
                listener = Some(l);
                break;
            }
            Err(e) => {
                warn!("Port {} unavailable ({}). Trying next candidate...", port, e);
            }
        }
    }

    let listener = match listener {
        Some(l) => l,
        None => {
            error!("Failed to bind any WebSocket port candidate. Hub cannot start.");
            return;
        }
    };

    {
        let mut lock = state.lock().await;
        lock.active_port = active_port;
    }

    let _ = tokio::fs::write("/tmp/onlydoms_active_port.txt", active_port.to_string()).await;
    let _ = tokio::fs::write(std::env::temp_dir().join("onlydoms_active_port.txt"), active_port.to_string()).await;

    while let Ok((stream, _)) = listener.accept().await {
        let state_clone = state.clone();
        tokio::spawn(async move {
            // Peek initial bytes to distinguish HTTP `/reports/<run_id>/...` requests from WebSocket upgrades
            let mut peek_buf = [0u8; 256];
            if let Ok(Ok(n)) = tokio::time::timeout(Duration::from_secs(2), stream.peek(&mut peek_buf)).await {
                let prefix = String::from_utf8_lossy(&peek_buf[..n]);
                let is_report_http = (prefix.starts_with("GET /reports/") || prefix.starts_with("HEAD /reports/"))
                    && !prefix.to_ascii_lowercase().contains("upgrade: websocket");
                if is_report_http {
                    let reports_root = std::env::var("ONLYDOMS_REPORTS_DIR")
                        .map(PathBuf::from)
                        .unwrap_or_else(|_| PathBuf::from("test_reports"));
                    handle_http_report_stream(stream, &reports_root).await;
                    return;
                }
            }

            if let Ok(ws_stream) = tokio_tungstenite::accept_async(stream).await {
                info!("Extension connected to hub on port {}", active_port);
                let (mut write, mut read) = ws_stream.split();

                let (tx, mut rx) = mpsc::channel::<Value>(100);
                let current_token = {
                    let mut lock = state_clone.lock().await;
                    lock.conn_token = lock.conn_token.wrapping_add(1);
                    lock.tx = Some(tx.clone());
                    lock.conn_token
                };

                let state_for_read = state_clone.clone();

                let write_task = tokio::spawn(async move {
                    while let Some(msg) = rx.recv().await {
                        let msg_str = msg.to_string();
                        if write.send(tokio_tungstenite::tungstenite::Message::Text(msg_str)).await.is_err() {
                            break;
                        }
                    }
                });

                // Keepalive heartbeat ping task every 15s to keep Chrome MV3 service worker alive
                let ping_tx = tx.clone();
                let ping_task = tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(15)).await;
                        if ping_tx.send(serde_json::json!({"type": "PING", "id": "keepalive"})).await.is_err() {
                            break;
                        }
                    }
                });

                while let Some(Ok(msg)) = read.next().await {
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                        if let Ok(mut val) = serde_json::from_str::<Value>(&text) {
                            // Check for unsolicited EVENT / NETWORK_CAPTURE / REPLAY_CURL messages
                            if let Some(msg_type) = val.get("type").and_then(|t| t.as_str()).map(|s| s.to_string()) {
                                if msg_type == "NETWORK_CAPTURE" {
                                    let mut lock = state_for_read.lock().await;
                                    lock.network_harvester.ingest_extension_event(&val);
                                    continue;
                                }
                                if msg_type == "REPLAY_CURL" {
                                    let req_id_str = val.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                    let tab_id = val.get("tabId").or_else(|| val.get("tab_id")).and_then(|t| t.as_u64());
                                    let st = state_for_read.clone();
                                    let reply_tx = tx.clone();
                                    let payload = val.clone();
                                    tokio::spawn(async move {
                                        let resp = match crate::tools::curl_runner::replay_curl(st, tab_id, payload).await {
                                            Ok(r) => serde_json::json!({
                                                "type": "REPLAY_CURL_RESULT",
                                                "id": req_id_str,
                                                "result": r
                                            }),
                                            Err(e) => serde_json::json!({
                                                "type": "REPLAY_CURL_RESULT",
                                                "id": req_id_str,
                                                "error": e
                                            }),
                                        };
                                        let _ = reply_tx.send(resp).await;
                                    });
                                    continue;
                                }
                                if msg_type == "EVENT" {
                                    let event_name = val.get("event").and_then(|e| e.as_str()).unwrap_or("");
                                    let tab_id = val.get("tabId").and_then(|t| t.as_u64()).unwrap_or(0);
                                    let mut lock = state_for_read.lock().await;
                                    match event_name {
                                        "Page.javascriptDialogOpening" => {
                                            if let Some(payload) = val.get("payload") {
                                                let dialog = ActiveDialog {
                                                    tab_id,
                                                    dialog_type: payload.get("type").and_then(|v| v.as_str()).unwrap_or("alert").to_string(),
                                                    message: payload.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                                    url: payload.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                                    default_prompt: payload.get("defaultPrompt").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                                    opened_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
                                                };
                                                lock.active_dialogs.insert(tab_id, dialog);
                                            }
                                        }
                                        "Page.javascriptDialogClosed" => {
                                            lock.active_dialogs.remove(&tab_id);
                                        }
                                        "Target.targetCreated" | "chrome.tabs.onCreated" => {
                                            let opener_tab_id = val.get("openerTabId").and_then(|o| o.as_u64());
                                            let url = val.get("url").and_then(|u| u.as_str()).unwrap_or("").to_string();
                                            let is_popup = val.get("isPopup").and_then(|p| p.as_bool()).unwrap_or(opener_tab_id.is_some());
                                            if tab_id > 0 {
                                                lock.spawned_popups.insert(tab_id, SpawnedPopup {
                                                    tab_id,
                                                    opener_tab_id,
                                                    url,
                                                    is_popup,
                                                });
                                            }
                                        }
                                        "Network.requestWillBeSent"
                                        | "Network.responseReceived"
                                        | "NETWORK_CAPTURE" => {
                                            lock.network_harvester.ingest_extension_event(&val);
                                        }
                                        _ => {}
                                    }
                                    continue;
                                }
                            }

                            // Multi-Layer Agent Blindspot Protection: sanitize extension payload before resolving channel
                            if let Some(res_field) = val.get_mut("result") {
                                sanitize_extension_payload(res_field);
                            }

                            if let Some(id) = val.get("id").and_then(|id| id.as_str()).map(|s| s.to_string()) {
                                if id == "keepalive" {
                                    continue;
                                }
                                let mut lock = state_for_read.lock().await;
                                if let Some(sender) = lock.pending.remove(&id) {
                                    let _ = sender.send(val);
                                }
                            } else if let Some(id_num) = val.get("id").and_then(|id| id.as_u64()) {
                                let mut lock = state_for_read.lock().await;
                                if let Some(sender) = lock.pending.remove(&id_num.to_string()) {
                                    let _ = sender.send(val);
                                }
                            }
                        }
                    }
                }

                {
                    let mut lock = state_clone.lock().await;
                    if lock.conn_token == current_token {
                        lock.tx = None;
                        lock.pending.clear();
                        lock.active_dialogs.clear();
                        lock.spawned_popups.clear();
                    }
                }
                write_task.abort();
                ping_task.abort();
                info!("Extension connection closed (token {})", current_token);
            }
        });
    }
}

pub async fn send_hub_command(
    state: SharedHub,
    tab_id: Option<u64>,
    cmd_type: &str,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let mut lock = state.lock().await;
    let mut wait_count = 0;
    while lock.tx.is_none() && wait_count < 35 {
        drop(lock);
        tokio::time::sleep(Duration::from_millis(200)).await;
        lock = state.lock().await;
        wait_count += 1;
    }

    let id = uuid::Uuid::new_v4().to_string();
    let tx = match &lock.tx {
        Some(tx) => tx.clone(),
        None => return Err("No active browser session detected. Please ensure the OnlyDOMs Chrome extension is installed, enabled, and connected.".into()),
    };
    let (resp_tx, resp_rx) = oneshot::channel::<Value>();
    lock.pending.insert(id.clone(), resp_tx);
    drop(lock);

    let mut msg = serde_json::json!({
        "type": cmd_type,
        "id": id,
    });

    if let Some(tid) = tab_id {
        msg.as_object_mut().unwrap().insert("tabId".to_string(), Value::Number(tid.into()));
    }

    if cmd_type == "CDP_CMD" {
        msg.as_object_mut().unwrap().insert("method".to_string(), Value::String(method.to_string()));
        msg.as_object_mut().unwrap().insert("params".to_string(), params);
    } else if cmd_type == "INJECT_SCRIPT" {
        msg.as_object_mut().unwrap().insert("file".to_string(), params);
    } else if cmd_type == "ATTACH_TAB" || cmd_type == "DETACH_TAB" {
        msg.as_object_mut().unwrap().insert("tabId".to_string(), params);
    } else if let Some(obj) = params.as_object() {
        for (k, v) in obj {
            msg.as_object_mut().unwrap().insert(k.clone(), v.clone());
        }
    }

    if tx.send(msg).await.is_err() {
        return Err("Failed to send command to browser".into());
    }

    match tokio::time::timeout(Duration::from_secs(30), resp_rx).await {
        Ok(Ok(mut val)) => {
            if let Some(err) = val.get("error") {
                let err_str = err.as_str().map(|s| s.to_string()).unwrap_or_else(|| err.to_string());
                Err(err_str)
            } else if let Some(res) = val.get_mut("result") {
                let mut taken = res.take();
                // Always run application-layer HUD blindspot filter before returning to any tool caller
                sanitize_extension_payload(&mut taken);
                Ok(taken)
            } else {
                Ok(Value::Null)
            }
        }
        Ok(Err(_)) => Err("Connection lost while waiting for browser response".into()),
        Err(_) => Err("Request timed out waiting for browser response".into())
    }
}

pub async fn send_cdp_command(
    state: SharedHub,
    tab_id: Option<u64>,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    send_hub_command(state, tab_id, "CDP_CMD", method, params).await
}
