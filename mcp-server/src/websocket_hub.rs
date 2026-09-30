use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use std::collections::HashMap;
use std::sync::Arc;
use futures_util::{StreamExt, SinkExt};
use serde_json::Value;
use tracing::info;
use std::time::Duration;

pub struct HubState {
    pub tx: Option<mpsc::Sender<Value>>,
    pub pending: HashMap<String, oneshot::Sender<Value>>,
}

impl HubState {
    pub fn new() -> Self {
        Self {
            tx: None,
            pending: HashMap::new(),
        }
    }
}

pub type SharedHub = Arc<Mutex<HubState>>;

pub async fn start_hub(addr: &str, state: SharedHub) {
    let listener = TcpListener::bind(addr).await.expect("Failed to bind");
    info!("WebSocket Hub listening on ws://{}", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let state_clone = state.clone();
        tokio::spawn(async move {
            if let Ok(ws_stream) = tokio_tungstenite::accept_async(stream).await {
                info!("Extension connected to hub");
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
                                // Fallback just in case
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
    let (id, tx, rx) = {
        let mut lock = state.lock().await;
        let id = uuid::Uuid::new_v4().to_string();
        let tx = match &lock.tx {
            Some(tx) => tx.clone(),
            None => return Err("No active browser session detected. Please install and toggle the extension ON in your target browser.".into()),
        };
        let (resp_tx, resp_rx) = oneshot::channel();
        lock.pending.insert(id.clone(), resp_tx);
        (id, tx, resp_rx)
    };

    let mut msg = serde_json::json!({
        "type": cmd_type,
        "id": id,
    });
    
    if cmd_type == "CDP_CMD" {
        msg.as_object_mut().unwrap().insert("method".to_string(), Value::String(method.to_string()));
        msg.as_object_mut().unwrap().insert("params".to_string(), params);
    } else if cmd_type == "INJECT_SCRIPT" {
        msg.as_object_mut().unwrap().insert("file".to_string(), params);
    } else if cmd_type == "ATTACH_TAB" {
        msg.as_object_mut().unwrap().insert("tabId".to_string(), params);
    }

    if tx.send(msg).await.is_err() {
        return Err("Failed to send".into());
    }

    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(mut val)) => {
            if let Some(err) = val.get("error") {
                Err(err.to_string())
            } else if let Some(res) = val.get_mut("result") {
                Ok(res.take())
            } else {
                Ok(Value::Null)
            }
        }
        Ok(Err(_)) => Err("Connection lost".into()),
        Err(_) => Err("Request timed out".into())
    }
}

pub async fn send_cdp_command(state: SharedHub, method: &str, params: Value) -> Result<Value, String> {
    send_hub_command(state, "CDP_CMD", method, params).await
}
