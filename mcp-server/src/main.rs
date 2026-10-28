#![recursion_limit = "512"]

use chromapilot::{mcp_protocol, websocket_hub, daemon};

use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::warn;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(author, version, about = "ChromaPilot MCP Server & Daemon", long_about = None)]
struct Cli {
    #[arg(short, long, default_value_t = 9222, global = true)]
    port: u16,

    #[arg(long, default_value_t = 9220, global = true)]
    ipc_port: u16,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the persistent background daemon
    Daemon,
    /// Query the status of the running daemon
    Status,
    /// Stop the running daemon
    Stop,
    /// Open a URL in a new browser tab and attach to it
    Open {
        /// Target URL (e.g. https://www.instagram.com)
        url: String,
    },
    /// List open browser tabs with IDs, titles, and cleaned URLs
    Tabs {
        /// Return detailed raw Chromium properties
        #[arg(long, default_value_t = false)]
        detailed: bool,
    },
    /// Evaluate JavaScript expression in active or specified tab
    Eval {
        /// JavaScript expression to evaluate
        expression: String,
        /// Target tab ID (defaults to active/assigned tab)
        #[arg(long)]
        tab_id: Option<u64>,
    },
    /// Execute a multi-step browser pipeline from JSON string or file
    Pipeline {
        /// JSON array of steps or path to a JSON pipeline file
        steps: String,
        /// Target tab ID (defaults to active/assigned tab)
        #[arg(long)]
        tab_id: Option<u64>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Daemon) => {
            daemon::run_daemon(cli.port, cli.ipc_port).await?;
        }
        Some(Commands::Status) => {
            query_status(cli.ipc_port).await?;
        }
        Some(Commands::Stop) => {
            send_control_cmd(cli.ipc_port, "STOP").await?;
            println!("ChromaPilot daemon stopped.");
        }
        Some(Commands::Open { url }) => {
            ensure_daemon_running(cli.port, cli.ipc_port).await?;
            let res = send_mcp_cmd(cli.ipc_port, "create_tab", serde_json::json!({ "url": url })).await?;
            let text = extract_mcp_text(&res)?;
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(tab_id) = parsed.get("tabId").and_then(|v| v.as_u64()) {
                    println!("Opened tab: ID {} ({})", tab_id, url);
                } else {
                    println!("{}", serde_json::to_string_pretty(&parsed)?);
                }
            } else {
                println!("{}", text);
            }
        }
        Some(Commands::Tabs { detailed }) => {
            ensure_daemon_running(cli.port, cli.ipc_port).await?;
            let res = send_mcp_cmd(cli.ipc_port, "list_browser_sessions", serde_json::json!({ "detailed": detailed })).await?;
            let text = extract_mcp_text(&res)?;
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(tabs) = parsed.get("tabs").and_then(|t| t.as_array()) {
                    println!("{:<12} {:<8} {:<42} {}", "TAB ID", "ACTIVE", "TITLE", "URL");
                    println!("{:-<12} {:-<8} {:-<42} {:-<35}", "", "", "", "");
                    for tab in tabs {
                        let id = tab.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                        let active = if tab.get("active").and_then(|v| v.as_bool()).unwrap_or(false) { "● YES" } else { "  no" };
                        let title = tab.get("title").and_then(|v| v.as_str()).unwrap_or("");
                        let truncated_title = if title.chars().count() > 40 {
                            format!("{}...", title.chars().take(37).collect::<String>())
                        } else {
                            title.to_string()
                        };
                        let url = tab.get("url").and_then(|v| v.as_str()).unwrap_or("");
                        println!("{:<12} {:<8} {:<42} {}", id, active, truncated_title, url);
                    }
                } else {
                    println!("{}", serde_json::to_string_pretty(&parsed)?);
                }
            } else {
                println!("{}", text);
            }
        }
        Some(Commands::Eval { expression, tab_id }) => {
            ensure_daemon_running(cli.port, cli.ipc_port).await?;
            let mut args = serde_json::json!({ "expression": expression });
            if let Some(tid) = tab_id {
                args.as_object_mut().unwrap().insert("tab_id".to_string(), serde_json::json!(tid));
            }
            let res = send_mcp_cmd(cli.ipc_port, "evaluate_expression", args).await?;
            let text = extract_mcp_text(&res)?;
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(val) = parsed.get("result").and_then(|r| r.get("value")) {
                    if let Some(s) = val.as_str() {
                        println!("{}", s);
                    } else {
                        println!("{}", serde_json::to_string_pretty(val)?);
                    }
                } else {
                    println!("{}", serde_json::to_string_pretty(&parsed)?);
                }
            } else {
                println!("{}", text);
            }
        }
        Some(Commands::Pipeline { steps, tab_id }) => {
            ensure_daemon_running(cli.port, cli.ipc_port).await?;
            let raw_steps = if Path::new(&steps).exists() {
                tokio::fs::read_to_string(&steps).await?
            } else {
                steps
            };
            let parsed_steps: serde_json::Value = serde_json::from_str(&raw_steps)
                .map_err(|e| format!("Invalid JSON steps in pipeline: {}", e))?;

            let mut args = if parsed_steps.is_array() {
                serde_json::json!({ "steps": parsed_steps })
            } else {
                parsed_steps
            };
            if let Some(tid) = tab_id {
                args.as_object_mut().unwrap().insert("tab_id".to_string(), serde_json::json!(tid));
            }
            let res = send_mcp_cmd(cli.ipc_port, "execute_browser_pipeline", args).await?;
            let text = extract_mcp_text(&res)?;
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                println!("{}", serde_json::to_string_pretty(&parsed)?);
            } else {
                println!("{}", text);
            }
        }
        None => {
            // Default MCP entrypoint: run as lightweight proxy, auto-starting daemon if needed
            run_proxy(cli.port, cli.ipc_port).await?;
        }
    }

    Ok(())
}

async fn ensure_daemon_running(ws_port: u16, tcp_ipc_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    if !daemon::is_daemon_running().await {
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe)
                .arg("daemon")
                .arg("--port")
                .arg(ws_port.to_string())
                .arg("--ipc-port")
                .arg(tcp_ipc_port.to_string())
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        }

        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(2500) {
            if daemon::is_daemon_running().await {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
    Ok(())
}

fn extract_mcp_text(res: &serde_json::Value) -> Result<String, Box<dyn std::error::Error>> {
    if let Some(err) = res.get("error") {
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown error");
        return Err(format!("ChromaPilot error: {}", msg).into());
    }
    if let Some(content) = res.get("result").and_then(|r| r.get("content")).and_then(|c| c.as_array()) {
        if let Some(first) = content.first() {
            if let Some(text) = first.get("text").and_then(|t| t.as_str()) {
                return Ok(text.to_string());
            }
        }
    }
    Ok(serde_json::to_string(res)?)
}

async fn send_mcp_cmd(tcp_ipc_port: u16, tool_name: &str, arguments: serde_json::Value) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "cli_cmd",
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments
        }
    });
    let req_str = req.to_string();

    #[cfg(unix)]
    if Path::new(daemon::UNIX_SOCKET_PATH).exists() {
        if let Ok(mut stream) = tokio::net::UnixStream::connect(daemon::UNIX_SOCKET_PATH).await {
            stream.write_all(format!("{}\n", req_str).as_bytes()).await?;
            stream.flush().await?;
            let mut reader = BufReader::new(stream).lines();
            if let Ok(Some(line)) = reader.next_line().await {
                let val: serde_json::Value = serde_json::from_str(&line)?;
                return Ok(val);
            }
        }
    }

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", tcp_ipc_port)).await
        .map_err(|_| "ChromaPilot daemon is not running. Please start it with 'chromapilot daemon --port 9222'")?;
    stream.write_all(format!("{}\n", req_str).as_bytes()).await?;
    stream.flush().await?;
    let mut reader = BufReader::new(stream).lines();
    if let Ok(Some(line)) = reader.next_line().await {
        let val: serde_json::Value = serde_json::from_str(&line)?;
        return Ok(val);
    }

    Err("No response from ChromaPilot daemon.".into())
}

async fn run_proxy(ws_port: u16, tcp_ipc_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    // 1. If daemon is not running, attempt auto-spawn
    let _ = ensure_daemon_running(ws_port, tcp_ipc_port).await;

    // 2. Connect to daemon via Unix domain socket
    #[cfg(unix)]
    {
        if Path::new(daemon::UNIX_SOCKET_PATH).exists() {
            if let Ok(stream) = tokio::net::UnixStream::connect(daemon::UNIX_SOCKET_PATH).await {
                proxy_stream(stream).await;
                return Ok(());
            }
        }
    }

    // Fallback: connect via TCP IPC
    if let Ok(stream) = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", tcp_ipc_port)).await {
        proxy_stream(stream).await;
        return Ok(());
    }

    // 3. Last-resort fallback: in-process execution (e.g. strict sandbox preventing detached subprocesses)
    warn!("ChromaPilot daemon could not be reached or auto-spawned; falling back to in-process mode.");
    run_in_process_fallback(ws_port).await
}

async fn proxy_stream<S>(stream: S)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + 'static,
{
    let (sock_read, mut sock_write) = tokio::io::split(stream);

    let mut stdin_reader = BufReader::new(tokio::io::stdin()).lines();
    let mut sock_reader = BufReader::new(sock_read).lines();
    let mut stdout = tokio::io::stdout();

    let client_to_daemon = tokio::spawn(async move {
        while let Ok(Some(line)) = stdin_reader.next_line().await {
            if sock_write.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if sock_write.write_all(b"\n").await.is_err() {
                break;
            }
            if sock_write.flush().await.is_err() {
                break;
            }
        }
    });

    let daemon_to_client = tokio::spawn(async move {
        while let Ok(Some(line)) = sock_reader.next_line().await {
            if stdout.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if stdout.write_all(b"\n").await.is_err() {
                break;
            }
            if stdout.flush().await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = client_to_daemon => {},
        _ = daemon_to_client => {},
    }
}

async fn run_in_process_fallback(ws_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let state = Arc::new(Mutex::new(websocket_hub::HubState::new()));
    let state_for_hub = state.clone();

    tokio::spawn(async move {
        websocket_hub::start_hub(ws_port, state_for_hub).await;
    });

    let client_id = "in_process_client".to_string();
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = reader.next_line().await {
        if let Ok(req) = serde_json::from_str::<mcp_protocol::McpRequest>(&line) {
            if let Some(res) = mcp_protocol::handle_mcp_request(&client_id, req, state.clone()).await {
                if let Ok(json_res) = serde_json::to_string(&res) {
                    let _ = stdout.write_all(json_res.as_bytes()).await;
                    let _ = stdout.write_all(b"\n").await;
                    let _ = stdout.flush().await;
                }
            }
        }
    }

    Ok(())
}

async fn query_status(tcp_ipc_port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let status_str = send_control_cmd(tcp_ipc_port, "STATUS").await?;
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&status_str) {
        println!("{}", serde_json::to_string_pretty(&val)?);
    } else {
        println!("{}", status_str);
    }
    Ok(())
}

async fn send_control_cmd(tcp_ipc_port: u16, cmd: &str) -> Result<String, Box<dyn std::error::Error>> {
    #[cfg(unix)]
    if Path::new(daemon::UNIX_SOCKET_PATH).exists() {
        if let Ok(mut stream) = tokio::net::UnixStream::connect(daemon::UNIX_SOCKET_PATH).await {
            stream.write_all(format!("{}\n", cmd).as_bytes()).await?;
            stream.flush().await?;
            let mut reader = BufReader::new(stream).lines();
            if let Ok(Some(line)) = reader.next_line().await {
                return Ok(line);
            }
        }
    }

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", tcp_ipc_port)).await
        .map_err(|_| "ChromaPilot daemon is not running.")?;
    stream.write_all(format!("{}\n", cmd).as_bytes()).await?;
    stream.flush().await?;
    let mut reader = BufReader::new(stream).lines();
    if let Ok(Some(line)) = reader.next_line().await {
        return Ok(line);
    }

    Ok("OK".to_string())
}
