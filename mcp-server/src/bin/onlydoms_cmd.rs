use tokio::net::TcpListener;
use futures_util::{StreamExt, SinkExt};
use serde_json::Value;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: onlydoms_cmd <eval_expr|list_tabs|attach_tab <id>>");
        std::process::exit(1);
    }

    let cmd = &args[1];
    let candidate_ports = [9222, 9922, 9223, 9923, 9224, 9924];
    let mut listener = None;
    let mut active_port = 9222;

    for port in candidate_ports {
        let addr = format!("127.0.0.1:{}", port);
        if let Ok(l) = TcpListener::bind(&addr).await {
            active_port = port;
            listener = Some(l);
            break;
        }
    }

    let listener = match listener {
        Some(l) => l,
        None => {
            eprintln!("Failed to bind any candidate port.");
            std::process::exit(1);
        }
    };

    println!("[Hub] Listening on 127.0.0.1:{}. Waiting for browser extension...", active_port);

    // Accept connection with timeout
    let (stream, _) = match tokio::time::timeout(Duration::from_secs(12), listener.accept()).await {
        Ok(Ok(conn)) => conn,
        _ => {
            eprintln!("Timed out waiting for Chrome extension to connect on port {}", active_port);
            std::process::exit(1);
        }
    };

    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    println!("[Hub] Extension connected!");
    let (mut write, mut read) = ws_stream.split();

    let req_id = "cli_req_1";
    let req_msg = match cmd.as_str() {
        "list_tabs" => {
            serde_json::json!({
                "type": "LIST_TABS",
                "id": req_id
            })
        }
        "attach_tab" => {
            let tab_id = args.get(2).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            serde_json::json!({
                "type": "ATTACH_TAB",
                "id": req_id,
                "tabId": tab_id
            })
        }
        "eval" => {
            let expr = args.get(2).cloned().unwrap_or_else(|| "window.location.href".to_string());
            serde_json::json!({
                "type": "CDP_CMD",
                "id": req_id,
                "method": "Runtime.evaluate",
                "params": {
                    "expression": expr,
                    "returnByValue": true,
                    "awaitPromise": true
                }
            })
        }
        _ => {
            eprintln!("Unknown command: {}", cmd);
            std::process::exit(1);
        }
    };

    write.send(tokio_tungstenite::tungstenite::Message::Text(req_msg.to_string())).await?;

    let timeout_duration = Duration::from_secs(20);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout_duration {
        if let Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text)))) = tokio::time::timeout(Duration::from_secs(5), read.next()).await {
            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                if val.get("id").and_then(|i| i.as_str()) == Some(req_id) {
                    println!("{}", serde_json::to_string_pretty(&val)?);
                    return Ok(());
                }
            }
        }
    }

    eprintln!("Timed out waiting for response.");
    std::process::exit(1);
}
