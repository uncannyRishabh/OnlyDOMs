use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use std::collections::HashMap;
use std::sync::Arc;
use futures_util::{StreamExt, SinkExt};
use serde_json::Value;
use tracing::{info, warn, error};
use std::time::Duration;

pub struct HubState {
    pub tx: Option<mpsc::Sender<Value>>,
    pub pending: HashMap<String, oneshot::Sender<Value>>,
    pub active_port: u16,
}

impl HubState {
    pub fn new() -> Self {
        Self {
            tx: None,
            pending: HashMap::new(),
            active_port: 9222,
        }
    }
}

pub type SharedHub = Arc<Mutex<HubState>>;

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
                info!("WebSocket Hub listening on ws://{}", addr);
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

    let _ = tokio::fs::write("/tmp/chromapilot_active_port.txt", active_port.to_string()).await;

    while let Ok((stream, _)) = listener.accept().await {
        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Ok(ws_stream) = tokio_tungstenite::accept_async(stream).await {
                info!("Extension connected to hub on port {}", active_port);
                let (mut write, mut read) = ws_stream.split();
                
                let (tx, mut rx) = mpsc::channel::<Value>(100);
                
                {
                    let mut lock = state_clone.lock().await;
                    lock.tx = Some(tx);
                }

                let state_for_read = state_clone.clone();

                let write_task = tokio::spawn(async move {
                    while let Some(msg) = rx.recv().await {
                        let msg_str = msg.to_string();
                        if write.send(tokio_tungstenite::tungstenite::Message::Text(msg_str)).await.is_err() {
                            break;
                        }
                    }
                });

                while let Some(Ok(msg)) = read.next().await {
                    if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                        if let Ok(val) = serde_json::from_str::<Value>(&text) {
                            if let Some(id) = val.get("id").and_then(|id| id.as_str()) {
                                let mut lock = state_for_read.lock().await;
                                if let Some(sender) = lock.pending.remove(id) {
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
                    lock.tx = None;
                    lock.pending.clear();
                }
                write_task.abort();
            }
        });
    }
}

pub async fn send_hub_command(state: SharedHub, cmd_type: &str, method: &str, params: Value) -> Result<Value, String> {
    // Wait briefly if extension is reconnecting or server was just started
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
        None => return Err("No active browser session detected. Please ensure the ChromaPilot Chrome extension is installed, enabled, and connected.".into()),
    };
    let (resp_tx, resp_rx) = oneshot::channel::<Value>();
    lock.pending.insert(id.clone(), resp_tx);
    drop(lock);

    let mut msg = serde_json::json!({
        "type": cmd_type,
        "id": id,
    });
    
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
                Err(err.to_string())
            } else if let Some(res) = val.get_mut("result") {
                Ok(res.take())
            } else {
                Ok(Value::Null)
            }
        }
        Ok(Err(_)) => Err("Connection lost while waiting for browser response".into()),
        Err(_) => Err("Request timed out waiting for browser response".into())
    }
}

pub async fn send_cdp_command(state: SharedHub, method: &str, params: Value) -> Result<Value, String> {
    send_hub_command(state, "CDP_CMD", method, params).await
}
