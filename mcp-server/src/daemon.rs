use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, warn};
use std::path::Path;
use serde_json::Value;

use crate::websocket_hub::{HubState, SharedHub, start_hub};
use crate::mcp_protocol::{handle_mcp_request, McpRequest};

#[allow(dead_code)]
pub const DEFAULT_WS_PORT: u16 = 9222;
pub const DEFAULT_TCP_IPC_PORT: u16 = 9220;
pub const UNIX_SOCKET_PATH: &str = "/tmp/onlydoms.sock";
pub const PID_FILE_PATH: &str = "/tmp/onlydoms.pid";

pub fn get_ipc_port_path() -> std::path::PathBuf {
    std::env::temp_dir().join("onlydoms_ipc_port.txt")
}

pub fn get_pid_file_path() -> std::path::PathBuf {
    std::env::temp_dir().join("onlydoms.pid")
}

pub async fn is_daemon_running() -> bool {
    // Check unix socket if available
    #[cfg(unix)]
    {
        if Path::new(UNIX_SOCKET_PATH).exists() {
            if let Ok(mut stream) = tokio::net::UnixStream::connect(UNIX_SOCKET_PATH).await {
                let _ = stream.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":\"ping\",\"method\":\"ping\"}\n").await;
                return true;
            }
            // Socket file exists but not responding; remove stale socket file
            let _ = tokio::fs::remove_file(UNIX_SOCKET_PATH).await;
        }
    }

    // Check TCP IPC port fallback
    if let Ok(mut stream) = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", DEFAULT_TCP_IPC_PORT)).await {
        let _ = stream.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":\"ping\",\"method\":\"ping\"}\n").await;
        return true;
    }

    false
}

pub async fn run_daemon(ws_port: u16, tcp_ipc_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting OnlyDOMs Daemon (WS port: {}, IPC port: {})...", ws_port, tcp_ipc_port);

    // Write current PID to lockfile
    let pid = std::process::id();
    let _ = tokio::fs::write(PID_FILE_PATH, pid.to_string()).await;
    let _ = tokio::fs::write(get_pid_file_path(), pid.to_string()).await;

    let state = Arc::new(Mutex::new(HubState::new()));
    let state_for_ws = state.clone();

    // 1. Start WebSocket Hub for Chrome Extension
    tokio::spawn(async move {
        start_hub(ws_port, state_for_ws).await;
    });

    // 2. Start TCP IPC listener (cross-platform & CLI)
    let tcp_listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{}", tcp_ipc_port)).await {
        Ok(l) => {
            info!("OnlyDOMs IPC TCP listener active on 127.0.0.1:{}", tcp_ipc_port);
            let _ = tokio::fs::write(get_ipc_port_path(), tcp_ipc_port.to_string()).await;
            Some(l)
        }
        Err(e) => {
            warn!("Could not bind TCP IPC port {}: {}", tcp_ipc_port, e);
            None
        }
    };

    // 3. Start Unix Domain Socket listener on Unix platforms
    #[cfg(unix)]
    let unix_listener = {
        if Path::new(UNIX_SOCKET_PATH).exists() {
            let _ = tokio::fs::remove_file(UNIX_SOCKET_PATH).await;
        }
        match tokio::net::UnixListener::bind(UNIX_SOCKET_PATH) {
            Ok(l) => {
                info!("OnlyDOMs IPC Unix Socket active on {}", UNIX_SOCKET_PATH);
                Some(l)
            }
            Err(e) => {
                warn!("Could not bind Unix Socket {}: {}", UNIX_SOCKET_PATH, e);
                None
            }
        }
    };

    if tcp_listener.is_none() {
        #[cfg(unix)]
        if unix_listener.is_none() {
            return Err("Failed to bind both TCP and Unix IPC listeners".into());
        }
        #[cfg(not(unix))]
        return Err("Failed to bind TCP IPC listener".into());
    }

    // 4. Accept TCP connections
    if let Some(listener) = tcp_listener {
        let state_for_tcp = state.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let client_id = format!("agent_{}", &uuid::Uuid::new_v4().to_string()[..8]);
                let state_clone = state_for_tcp.clone();
                tokio::spawn(async move {
                    handle_generic_stream(stream, client_id.clone(), state_clone.clone()).await;
                    let mut lock = state_clone.lock().await;
                    lock.tab_registry.release_all_client_tabs(&client_id);
                });
            }
        });
    }

    // 5. Accept Unix socket connections on Unix
    #[cfg(unix)]
    if let Some(listener) = unix_listener {
        let state_for_unix = state.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let client_id = format!("agent_{}", &uuid::Uuid::new_v4().to_string()[..8]);
                let state_clone = state_for_unix.clone();
                tokio::spawn(async move {
                    handle_generic_stream(stream, client_id.clone(), state_clone.clone()).await;
                    let mut lock = state_clone.lock().await;
                    lock.tab_registry.release_all_client_tabs(&client_id);
                });
            }
        });
    }

    info!("OnlyDOMs Daemon fully initialized and ready for connections.");

    // Wait for shutdown signal (SIGINT or SIGTERM)
    tokio::signal::ctrl_c().await?;
    info!("Shutting down OnlyDOMs Daemon...");

    // Cleanup files
    #[cfg(unix)]
    let _ = tokio::fs::remove_file(UNIX_SOCKET_PATH).await;
    let _ = tokio::fs::remove_file(PID_FILE_PATH).await;
    let _ = tokio::fs::remove_file(get_pid_file_path()).await;
    let _ = tokio::fs::remove_file(get_ipc_port_path()).await;

    Ok(())
}

async fn handle_generic_stream<S>(
    stream: S,
    client_id: String,
    state: SharedHub,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (read_half, mut write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half).lines();

    info!("Client connected: {}", client_id);

    while let Ok(Some(line)) = reader.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Support simple control commands
        if trimmed == "STATUS" || trimmed == "{\"method\":\"status\"}" {
            let mut lock = state.lock().await;
            let status = serde_json::json!({
                "service": "OnlyDOMs Daemon",
                "status": "running",
                "active_port": lock.active_port,
                "extension_connected": lock.tx.is_some(),
                "active_locks": lock.tab_registry.list_active_locks()
            });
            let _ = write_half.write_all(format!("{}\n", status).as_bytes()).await;
            let _ = write_half.flush().await;
            continue;
        }

        if trimmed == "STOP" {
            let _ = write_half.write_all(b"{\"status\":\"stopping\"}\n").await;
            let _ = write_half.flush().await;
            std::process::exit(0);
        }

        match serde_json::from_str::<McpRequest>(trimmed) {
            Ok(req) => {
                if let Some(res) = handle_mcp_request(&client_id, req, state.clone()).await {
                    if let Ok(json_res) = serde_json::to_string(&res) {
                        let _ = write_half.write_all(json_res.as_bytes()).await;
                        let _ = write_half.write_all(b"\n").await;
                        let _ = write_half.flush().await;
                    }
                }
            }
            Err(e) => {
                let err_res = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                });
                let _ = write_half.write_all(format!("{}\n", err_res).as_bytes()).await;
                let _ = write_half.flush().await;
            }
        }
    }

    info!("Client disconnected: {}", client_id);
}
