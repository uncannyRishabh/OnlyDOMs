use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use futures_util::{StreamExt, SinkExt};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

struct DaemonState {
    tx: Option<mpsc::Sender<Value>>,
    pending: HashMap<String, oneshot::Sender<Value>>,
    _active_tab: Option<u64>,
}

type SharedState = Arc<Mutex<DaemonState>>;

async fn send_ws_cmd(state: SharedState, cmd_type: &str, params: Value) -> Result<Value, String> {
    let mut lock = state.lock().await;
    let mut wait_count = 0;
    while lock.tx.is_none() && wait_count < 30 {
        drop(lock);
        tokio::time::sleep(Duration::from_millis(200)).await;
        lock = state.lock().await;
        wait_count += 1;
    }

    let tx = match &lock.tx {
        Some(tx) => tx.clone(),
        None => return Err("Chrome extension is not connected to ChromaPilot hub.".into()),
    };

    let id = uuid::Uuid::new_v4().to_string();
    let (resp_tx, resp_rx) = oneshot::channel::<Value>();
    lock.pending.insert(id.clone(), resp_tx);
    drop(lock);

    let mut msg = serde_json::json!({
        "type": cmd_type,
        "id": id
    });

    if cmd_type == "CDP_CMD" {
        msg.as_object_mut().unwrap().insert("method".into(), params.get("method").cloned().unwrap_or(serde_json::json!("Runtime.evaluate")));
        msg.as_object_mut().unwrap().insert("params".into(), params.get("params").cloned().unwrap_or(serde_json::json!({})));
    } else if cmd_type == "ATTACH_TAB" {
        msg.as_object_mut().unwrap().insert("tabId".into(), params.get("tabId").cloned().unwrap_or(serde_json::json!(0)));
    }

    if tx.send(msg).await.is_err() {
        return Err("Failed to dispatch to Chrome extension WebSocket.".into());
    }

    match tokio::time::timeout(Duration::from_secs(30), resp_rx).await {
        Ok(Ok(val)) => {
            if let Some(err) = val.get("error") {
                Err(err.to_string())
            } else if let Some(res) = val.get("result") {
                Ok(res.clone())
            } else {
                Ok(val)
            }
        }
        Ok(Err(_)) => Err("Connection dropped while waiting for response.".into()),
        Err(_) => Err("Timeout waiting for Chrome response.".into())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let state: SharedState = Arc::new(Mutex::new(DaemonState {
        tx: None,
        pending: HashMap::new(),
        _active_tab: None,
    }));

    // Find and bind WebSocket port
    let ws_candidates = [9222, 9922, 9223, 9923, 9224, 9924];
    let mut ws_listener = None;
    let mut ws_port = 9222;
    for p in ws_candidates {
        if let Ok(l) = TcpListener::bind(format!("127.0.0.1:{}", p)).await {
            ws_port = p;
            ws_listener = Some(l);
            break;
        }
    }
    let ws_listener = ws_listener.expect("Cannot bind WebSocket port");
    println!("🟢 ChromaPilot WS Hub listening on ws://127.0.0.1:{}", ws_port);
    let _ = tokio::fs::write("/tmp/chromapilot_active_port.txt", ws_port.to_string()).await;

    // Find and bind HTTP port
    let http_candidates = [9225, 9925, 9226, 9926, 9227];
    let mut http_listener = None;
    let mut http_port = 9225;
    for p in http_candidates {
        if let Ok(l) = TcpListener::bind(format!("127.0.0.1:{}", p)).await {
            http_port = p;
            http_listener = Some(l);
            break;
        }
    }
    let http_listener = http_listener.expect("Cannot bind HTTP port");
    println!("🚀 ChromaPilot HTTP API listening on http://127.0.0.1:{}", http_port);
    let _ = tokio::fs::write("/tmp/chromapilot_http_port.txt", http_port.to_string()).await;

    // Background WebSocket Hub Acceptor
    let ws_state = state.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = ws_listener.accept().await {
            let state_clone = ws_state.clone();
            tokio::spawn(async move {
                if let Ok(ws_stream) = tokio_tungstenite::accept_async(stream).await {
                    println!("🔌 Chrome extension connected to hub!");
                    let (mut write, mut read) = ws_stream.split();
                    let (tx, mut rx) = mpsc::channel::<Value>(100);

                    {
                        let mut lock = state_clone.lock().await;
                        lock.tx = Some(tx);
                    }

                    let state_for_read = state_clone.clone();

                    let write_task = tokio::spawn(async move {
                        while let Some(msg) = rx.recv().await {
                            if write.send(tokio_tungstenite::tungstenite::Message::Text(msg.to_string())).await.is_err() {
                                break;
                            }
                        }
                    });

                    // Keepalive ping task every 15s to keep MV3 service worker active
                    let ping_tx = {
                        let lock = state_clone.lock().await;
                        lock.tx.clone()
                    };
                    let ping_task = tokio::spawn(async move {
                        if let Some(tx) = ping_tx {
                            loop {
                                tokio::time::sleep(Duration::from_secs(15)).await;
                                if tx.send(serde_json::json!({"type": "PING", "id": "keepalive"})).await.is_err() {
                                    break;
                                }
                            }
                        }
                    });

                    while let Some(Ok(msg)) = read.next().await {
                        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                                if let Some(id) = val.get("id").and_then(|i| i.as_str()) {
                                    if id != "keepalive" {
                                        let mut lock = state_for_read.lock().await;
                                        if let Some(sender) = lock.pending.remove(id) {
                                            let _ = sender.send(val);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    {
                        let mut lock = state_clone.lock().await;
                        lock.tx = None;
                        lock.pending.clear();
                    }
                    write_task.abort();
                    ping_task.abort();
                    println!("❌ Chrome extension disconnected from hub.");
                }
            });
        }
    });

    // HTTP Request Handler
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    while let Ok((mut stream, _)) = http_listener.accept().await {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            let n = match stream.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };
            let req_str = String::from_utf8_lossy(&buf[..n]);
            let first_line = req_str.lines().next().unwrap_or("");
            let parts: Vec<&str> = first_line.split_whitespace().collect();
            if parts.len() < 2 {
                return;
            }
            let method = parts[0];
            let path = parts[1];

            // Extract body
            let body = if let Some(idx) = req_str.find("\r\n\r\n") {
                &req_str[idx + 4..]
            } else {
                ""
            };

            let response_body = if path == "/tabs" {
                match send_ws_cmd(state_clone, "LIST_TABS", serde_json::json!({})).await {
                    Ok(res) => serde_json::to_string(&res).unwrap_or("{}".into()),
                    Err(e) => format!("{{\"error\": {}}}", serde_json::json!(e)),
                }
            } else if path.starts_with("/attach") {
                let tab_id = if let Some(q_idx) = path.find("tabId=") {
                    path[q_idx + 6..].split('&').next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
                } else {
                    body.trim().parse::<u64>().unwrap_or(0)
                };
                match send_ws_cmd(state_clone, "ATTACH_TAB", serde_json::json!({"tabId": tab_id})).await {
                    Ok(res) => serde_json::to_string(&res).unwrap_or("{}".into()),
                    Err(e) => format!("{{\"error\": {}}}", serde_json::json!(e)),
                }
            } else if path == "/eval" && method == "POST" {
                let expr = body.trim();
                let params = serde_json::json!({
                    "method": "Runtime.evaluate",
                    "params": {
                        "expression": expr,
                        "returnByValue": true,
                        "awaitPromise": true
                    }
                });
                match send_ws_cmd(state_clone, "CDP_CMD", params).await {
                    Ok(res) => serde_json::to_string(&res).unwrap_or("{}".into()),
                    Err(e) => format!("{{\"error\": {}}}", serde_json::json!(e)),
                }
            } else {
                "{\"status\":\"ok\",\"service\":\"ChromaPilot Daemon\"}".to_string()
            };

            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(resp.as_bytes()).await;
        });
    }

    Ok(())
}
