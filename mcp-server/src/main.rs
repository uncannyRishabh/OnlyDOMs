mod mcp_protocol;
mod websocket_hub;
mod tools;

use clap::Parser;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tracing::{info, error};
use std::sync::Arc;
use tokio::sync::Mutex;
use serde_json::Value;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 9222)]
    port: u16,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let args = Args::parse();
    
    info!("MagiBot MCP Server starting...");

    let state = Arc::new(Mutex::new(websocket_hub::HubState::new()));
    let state_for_hub = state.clone();

    // Start WebSocket Hub in background
    let addr = format!("127.0.0.1:{}", args.port);
    tokio::spawn(async move {
        websocket_hub::start_hub(&addr, state_for_hub).await;
    });

    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    while let Ok(Some(line)) = reader.next_line().await {
        match serde_json::from_str::<mcp_protocol::McpRequest>(&line) {
            Ok(req) => {
                info!("Received MCP request: {:?}", req);
                
                let mut result_value = None;
                let mut error_val = None;

                let res_result: Result<Value, String> = match req.method.as_str() {
                    "initialize" => {
                        Ok(serde_json::json!({
                            "protocolVersion": "2024-11-05",
                            "capabilities": {
                                "tools": {}
                            },
                            "serverInfo": {
                                "name": "MagiBot",
                                "version": "1.0.0"
                            },
                            "instructions": "⚠️ USER DISCRETIONARY WARNING: This agent possesses capabilities to actively inspect, navigate, execute arbitrary JavaScript, manipulate the DOM, and control your active browser session. It can view sensitive information on open pages. Use strictly in trusted environments and operate responsibly."
                        }))
                    }
                    "notifications/initialized" => {
                        Ok(serde_json::json!({}))
                    }
                    "tools/list" => {
                        Ok(serde_json::json!({
                            "tools": [
                                {
                                    "name": "list_browser_sessions",
                                    "description": "List connected browser sessions",
                                    "inputSchema": { "type": "object", "properties": {} }
                                },
                                {
                                    "name": "select_session",
                                    "description": "Select a browser session to control",
                                    "inputSchema": { "type": "object", "properties": { "session_id": { "type": "string" } } }
                                },
                                {
                                    "name": "capture_viewport",
                                    "description": "Capture a screenshot of the viewport with an overlay grid",
                                    "inputSchema": { "type": "object", "properties": { "cols": { "type": "number" } } }
                                },
                                {
                                    "name": "execute_browser_pipeline",
                                    "description": "Execute a series of browser steps",
                                    "inputSchema": { "type": "object", "properties": { "steps": { "type": "array" } } }
                                },
                                {
                                    "name": "click_quadrant",
                                    "description": "Click a cell on the grid",
                                    "inputSchema": { "type": "object", "properties": { "cell": { "type": "string" } } }
                                },
                                {
                                    "name": "fill_inputs",
                                    "description": "Fill inputs in the page",
                                    "inputSchema": { "type": "object", "properties": { "fields": { "type": "array" } } }
                                },
                                {
                                    "name": "interact",
                                    "description": "Interact with the page using raw coordinates",
                                    "inputSchema": { "type": "object", "properties": { "action": { "type": "string" }, "x_ratio": { "type": "number" }, "y_ratio": { "type": "number" } } }
                                },
                                {
                                    "name": "evaluate_expression",
                                    "description": "Evaluate JS expression",
                                    "inputSchema": { "type": "object", "properties": { "expression": { "type": "string" } } }
                                },
                                {
                                    "name": "add_breakpoint",
                                    "description": "Add a debugger breakpoint",
                                    "inputSchema": { "type": "object", "properties": { "url_regex": { "type": "string" }, "line_number": { "type": "number" } } }
                                },
                                {
                                    "name": "pause_execution",
                                    "description": "Pause JS execution",
                                    "inputSchema": { "type": "object", "properties": {} }
                                },
                                {
                                    "name": "resume_execution",
                                    "description": "Resume JS execution",
                                    "inputSchema": { "type": "object", "properties": {} }
                                },
                                {
                                    "name": "step_over",
                                    "description": "Step over in debugger",
                                    "inputSchema": { "type": "object", "properties": {} }
                                },
                                {
                                    "name": "step_into",
                                    "description": "Step into in debugger",
                                    "inputSchema": { "type": "object", "properties": {} }
                                }
                            ]
                        }))
                    }
                    "tools/call" => {
                        let name = req.params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()).unwrap_or("");
                        let args = req.params.as_ref().and_then(|p| p.get("arguments")).cloned().unwrap_or(serde_json::json!({}));
                        
                        match name {
                            "list_browser_sessions" => { tools::session::list_browser_sessions(state.clone()).await }
                            "select_session" => {
                                let id = args.get("session_id").and_then(|s| s.as_u64()).unwrap_or(0);
                                tools::session::select_session(state.clone(), id).await
                            }
                            "capture_viewport" => {
                                let cols = args.get("cols").and_then(|c| c.as_u64()).unwrap_or(16) as u32;
                                tools::visual::capture_viewport(state.clone(), cols).await
                            }
                            "execute_browser_pipeline" => {
                                tools::navigation::execute_browser_pipeline(state.clone(), args).await
                            }
                            "click_quadrant" => {
                                tools::input::click_quadrant(state.clone(), args).await
                            }
                            "fill_inputs" => {
                                tools::input::fill_inputs(state.clone(), args).await
                            }
                            "interact" => {
                                tools::input::interact(state.clone(), args).await
                            }
                            "evaluate_expression" => {
                                tools::debugger::evaluate_expression(state.clone(), args).await
                            }
                            "add_breakpoint" => {
                                tools::debugger::add_breakpoint(state.clone(), args).await
                            }
                            "pause_execution" => {
                                tools::debugger::pause_execution(state.clone()).await
                            }
                            "resume_execution" => {
                                tools::debugger::resume_execution(state.clone()).await
                            }
                            "step_over" => {
                                tools::debugger::step_over(state.clone()).await
                            }
                            "step_into" => {
                                tools::debugger::step_into(state.clone()).await
                            }
                            _ => Err(format!("Unknown tool: {}", name)),
                        }
                    }
                    _ => {
                        if req.method == "notifications/initialized" {
                            Ok(serde_json::json!({}))
                        } else {
                            Err(format!("Unknown method: {}", req.method))
                        }
                    }
                };

                if req.method.starts_with("notifications/") {
                    continue;
                }

                match res_result {
                    Ok(v) => {
                        if req.method == "tools/call" {
                            result_value = Some(serde_json::json!({
                                "content": [{
                                    "type": "text",
                                    "text": v.to_string()
                                }]
                            }));
                        } else {
                            result_value = Some(v);
                        }
                    },
                    Err(e) => {
                        error_val = Some(mcp_protocol::McpError {
                            code: -32603,
                            message: e,
                        });
                    }
                }

                let res = mcp_protocol::McpResponse {
                    jsonrpc: "2.0".to_string(),
                    id: req.id,
                    result: result_value,
                    error: error_val,
                };
                
                if let Ok(json_res) = serde_json::to_string(&res) {
                    println!("{}", json_res);
                }
            }
            Err(e) => error!("Parse error: {}", e),
        }
    }
}
