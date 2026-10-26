#![recursion_limit = "512"]

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
    
    info!("ChromaPilot MCP Server starting...");

    let state = Arc::new(Mutex::new(websocket_hub::HubState::new()));
    let state_for_hub = state.clone();

    // Start WebSocket Hub in background
    let _addr = format!("127.0.0.1:{}", args.port);
    tokio::spawn(async move {
        websocket_hub::start_hub(args.port, state_for_hub).await;
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
                                "name": "ChromaPilot",
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
                                    "description": "List open browser tabs across Chrome windows in a compact, token-efficient format",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "detailed": { "type": "boolean", "description": "Whether to return full Chromium tab properties (default false)" }
                                        }
                                    }
                                },
                                {
                                    "name": "create_tab",
                                    "description": "Open a new browser tab with optional URL and immediately attach to it",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "url": { "type": "string", "description": "URL to open in the new tab" }
                                        }
                                    }
                                },
                                {
                                    "name": "switch_tab",
                                    "description": "Switch active tab in Chrome",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "tab_id": { "type": "number", "description": "Numeric ID of the tab" }
                                        },
                                        "required": ["tab_id"]
                                    }
                                },
                                {
                                    "name": "close_tab",
                                    "description": "Close a specific browser tab",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "tab_id": { "type": "number", "description": "Numeric ID of the tab to close" }
                                        },
                                        "required": ["tab_id"]
                                    }
                                },
                                {
                                    "name": "select_session",
                                    "description": "Select or switch debugger focus to a specific tab by ID",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "session_id": { "type": "string", "description": "Numeric Chrome tab ID" }
                                        },
                                        "required": ["session_id"]
                                    }
                                },
                                {
                                    "name": "capture_viewport",
                                    "description": "Capture a clean screenshot of the current page viewport (excludes DevTools/browser infobars)",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "format": { "type": "string", "enum": ["png", "webp", "jpeg"], "description": "Image format (default: webp)" },
                                            "quality": { "type": "number", "description": "Image quality 0-100 (for webp/jpeg)" }
                                        }
                                    }
                                },
                                {
                                    "name": "take_screenshot",
                                    "description": "Capture a clean screenshot of the web page (viewport or full scrollable page). Saves to disk if save_path is provided to avoid context token consumption.",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "full_page": { "type": "boolean", "description": "Capture entire scrollable page height instead of just viewport (default: false)" },
                                            "format": { "type": "string", "enum": ["png", "webp", "jpeg"], "description": "Image format (default: png)" },
                                            "quality": { "type": "number", "description": "Image quality 0-100 for webp/jpeg" },
                                            "save_path": { "type": "string", "description": "Optional file path to save binary image directly on disk" },
                                            "include_base64": { "type": "boolean", "description": "Include base64 image data in response (default: true if save_path not set, false if save_path is set)" }
                                        }
                                    }
                                },
                                {
                                    "name": "run_browser_test",
                                    "description": "Automated browser testing engine. Decomposes high-level defect or feature descriptions into executable steps, executes them with visual capture at each step, validates assertions, and produces markdown summaries and JSON reports in dedicated test folders.",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "test_name": { "type": "string", "description": "Name or identifier for the test (e.g. 'checkout_coupon_error')" },
                                            "description": { "type": "string", "description": "Detailed defect or feature description to validate. If steps are not provided, this is automatically decomposed into executable steps." },
                                            "output_dir": { "type": "string", "description": "Custom directory to save test reports and step screenshots (default: test_reports/<test_name>_<timestamp>)" },
                                            "full_page_screenshots": { "type": "boolean", "description": "Capture full scrollable page for step screenshots (default: false)" },
                                            "continue_on_failure": { "type": "boolean", "description": "Continue executing remaining steps after a step fails (default: false)" },
                                            "steps": {
                                                "type": "array",
                                                "description": "Optional explicit list of test steps. If omitted, steps are parsed from description.",
                                                "items": {
                                                    "type": "object",
                                                    "properties": {
                                                        "name": { "type": "string", "description": "Human-readable step title" },
                                                        "action": {
                                                            "type": "string",
                                                            "enum": ["navigate", "click", "fill_inputs", "press_key", "scroll", "wait_for", "wait_condition", "wait_network_idle", "evaluate", "sleep", "assert"],
                                                            "description": "Action type"
                                                        },
                                                        "url": { "type": "string" },
                                                        "selector": { "type": "string" },
                                                        "text": { "type": "string" },
                                                        "aria_label": { "type": "string" },
                                                        "expression": { "type": "string" },
                                                        "key": { "type": "string" },
                                                        "screenshot": { "type": "boolean" },
                                                        "full_page": { "type": "boolean" },
                                                        "optional": { "type": "boolean" }
                                                    },
                                                    "required": ["action"]
                                                }
                                            }
                                        }
                                    }
                                },
                                {
                                    "name": "execute_browser_pipeline",
                                    "description": "Execute a batch sequence of resilient browser actions with built-in await, polling, and retry logic",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "steps": {
                                                "type": "array",
                                                "description": "Ordered actions: navigate, new_tab, switch_tab, wait_for, wait_condition, wait_network_idle, click, scroll, evaluate, fill_inputs, press_key, sleep, take_screenshot, capture_grid_screenshot",
                                                "items": {
                                                    "type": "object",
                                                    "properties": {
                                                        "action": {
                                                            "type": "string",
                                                            "enum": ["navigate", "new_tab", "switch_tab", "wait_for", "wait_condition", "wait_network_idle", "click", "scroll", "evaluate", "fill_inputs", "press_key", "sleep", "take_screenshot", "capture_grid_screenshot"],
                                                            "description": "Action type"
                                                        },
                                                        "url": { "type": "string", "description": "URL for navigate or new_tab" },
                                                        "tab_id": { "type": "number", "description": "Tab ID for switch_tab" },
                                                        "selector": { "type": "string", "description": "CSS selector to wait for, click, or scroll" },
                                                        "text": { "type": "string", "description": "Visible text to target for semantic click" },
                                                        "aria_label": { "type": "string", "description": "Aria-label to target for semantic click" },
                                                        "direction": { "type": "string", "enum": ["down", "up"], "description": "Direction for scroll (default: down)" },
                                                        "delta_y": { "type": "number", "description": "Vertical scroll delta in pixels" },
                                                        "delta_x": { "type": "number", "description": "Horizontal scroll delta in pixels" },
                                                        "expression": { "type": "string", "description": "JS boolean expression to await (wait_condition) or expression to execute (evaluate)" },
                                                        "timeout_ms": { "type": "number", "description": "Timeout in milliseconds" },
                                                        "wait_after_ms": { "type": "number", "description": "Delay in ms after clicking or scrolling" },
                                                        "duration_ms": { "type": "number", "description": "Duration for sleep action in ms" },
                                                        "key": { "type": "string", "description": "Key name for press_key (e.g. Enter, Tab, Escape, ArrowDown)" },
                                                        "optional": { "type": "boolean", "description": "Do not fail pipeline if this step times out" },
                                                        "fields": {
                                                            "type": "array",
                                                            "description": "Fields for fill_inputs",
                                                            "items": {
                                                                "type": "object",
                                                                "properties": {
                                                                    "selector": { "type": "string" },
                                                                    "value": { "type": "string" }
                                                                },
                                                                "required": ["selector", "value"]
                                                            }
                                                        }
                                                    },
                                                    "required": ["action"]
                                                }
                                            }
                                        },
                                        "required": ["steps"]
                                    }
                                },
                                {
                                    "name": "fill_inputs",
                                    "description": "Framework-safe batch form input filling (React, Vue, Angular)",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "fields": {
                                                "type": "array",
                                                "items": {
                                                    "type": "object",
                                                    "properties": {
                                                        "selector": { "type": "string", "description": "CSS selector for input" },
                                                        "value": { "type": "string", "description": "Text value to fill" }
                                                    },
                                                    "required": ["value"]
                                                }
                                            }
                                        },
                                        "required": ["fields"]
                                    }
                                },
                                {
                                    "name": "interact",
                                    "description": "Interact with the page using normalized coordinates (0.0 to 1.0)",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "action": { "type": "string", "enum": ["click", "hover", "scroll"], "description": "Interaction action" },
                                            "x_ratio": { "type": "number", "description": "X coordinate normalized (0.0 to 1.0)" },
                                            "y_ratio": { "type": "number", "description": "Y coordinate normalized (0.0 to 1.0)" },
                                            "deltaX": { "type": "number", "description": "Horizontal scroll delta" },
                                            "deltaY": { "type": "number", "description": "Vertical scroll delta" }
                                        },
                                        "required": ["action"]
                                    }
                                },
                                {
                                    "name": "evaluate_expression",
                                    "description": "Evaluate JavaScript in the active tab context via CDP Runtime.evaluate",
                                    "inputSchema": {
                                        "type": "object",
                                        "properties": {
                                            "expression": { "type": "string", "description": "JavaScript expression to evaluate" }
                                        },
                                        "required": ["expression"]
                                    }
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
                        
                        let parse_u64 = |val: Option<&Value>| -> Option<u64> {
                            val.and_then(|v| {
                                if let Some(n) = v.as_u64() {
                                    Some(n)
                                } else if let Some(s) = v.as_str() {
                                    s.trim().parse::<u64>().ok()
                                } else {
                                    v.as_f64().map(|f| f as u64)
                                }
                            })
                        };

                        match name {
                            "list_browser_sessions" => { tools::session::list_browser_sessions(state.clone(), args).await }
                            "create_tab" => {
                                let url = args.get("url").and_then(|u| u.as_str()).unwrap_or("about:blank");
                                tools::session::create_tab(state.clone(), url).await
                            }
                            "switch_tab" => {
                                match parse_u64(args.get("tab_id")) {
                                    Some(id) if id > 0 => tools::session::switch_tab(state.clone(), id).await,
                                    _ => Err("Missing or invalid tab_id.".to_string()),
                                }
                            }
                            "close_tab" => {
                                match parse_u64(args.get("tab_id")) {
                                    Some(id) if id > 0 => tools::session::close_tab(state.clone(), id).await,
                                    _ => Err("Missing or invalid tab_id.".to_string()),
                                }
                            }
                            "select_session" => {
                                match parse_u64(args.get("session_id")) {
                                    Some(id) if id > 0 => tools::session::select_session(state.clone(), id).await,
                                    _ => Err("Missing or invalid session_id. Please provide a valid tab ID.".to_string()),
                                }
                            }
                            "capture_viewport" => {
                                tools::visual::capture_viewport(state.clone(), args).await
                            }
                            "take_screenshot" => {
                                tools::visual::take_screenshot(state.clone(), args).await
                            }
                            "run_browser_test" => {
                                tools::testing::run_browser_test(state.clone(), args).await
                            }
                            "execute_browser_pipeline" => {
                                tools::navigation::execute_browser_pipeline(state.clone(), args).await
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
