use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::websocket_hub::SharedHub;
use crate::tools;

#[derive(Debug, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
}

pub fn get_tools_list() -> Value {
    serde_json::json!({
        "tools": [
            {
                "name": "list_browser_sessions",
                "description": "List open browser tabs across Chrome windows in a compact, token-efficient format with agent lock status",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "detailed": { "type": "boolean", "description": "Whether to return full Chromium tab properties (default false)" }
                    }
                }
            },
            {
                "name": "create_tab",
                "description": "Open a new browser tab with optional URL, immediately attach to it, and acquire an exclusive agent lock",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "URL to open in the new tab" }
                    }
                }
            },
            {
                "name": "switch_tab",
                "description": "Switch active tab in Chrome and lock it for the current agent (fails if tab is locked by another agent)",
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
                "description": "Close a specific browser tab and release its lock",
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
                "description": "Select or switch debugger focus to a specific tab by ID and acquire an exclusive lock",
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
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
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
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
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
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
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
                "description": "Execute a batch sequence of resilient browser actions with built-in await, polling, and retry logic on the agent's assigned tab",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
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
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
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
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
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
                "description": "Evaluate JavaScript in the assigned tab context via CDP Runtime.evaluate",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
                        "expression": { "type": "string", "description": "JavaScript expression to evaluate" }
                    },
                    "required": ["expression"]
                }
            },
            {
                "name": "add_breakpoint",
                "description": "Add a debugger breakpoint",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
                        "url_regex": { "type": "string" },
                        "line_number": { "type": "number" }
                    }
                }
            },
            {
                "name": "pause_execution",
                "description": "Pause JS execution",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" }
                    }
                }
            },
            {
                "name": "resume_execution",
                "description": "Resume JS execution",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" }
                    }
                }
            },
            {
                "name": "step_over",
                "description": "Step over in debugger",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" }
                    }
                }
            },
            {
                "name": "step_into",
                "description": "Step into in debugger",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" }
                    }
                }
            }
        ]
    })
}

/// Resolves the tab ID for the current agent:
/// 1. If explicit `requested_tab` provided, verifies lock. If locked by another agent, returns Err.
/// 2. If agent already holds a locked tab, uses it.
/// 3. Looks for an unreserved clean-slate tab (e.g. `chrome://newtab/` or `about:blank`).
/// 4. If no clean-slate tab is available, automatically provisions a fresh tab to avoid hijacking user work.
pub async fn resolve_or_allocate_tab(
    state: SharedHub,
    client_id: &str,
    requested_tab: Option<u64>,
) -> Result<u64, String> {
    if let Some(tid) = requested_tab {
        if tid > 0 {
            let mut lock = state.lock().await;
            lock.tab_registry.touch_or_acquire_tab(client_id, tid)?;
            return Ok(tid);
        }
    }

    {
        let mut lock = state.lock().await;
        if let Some(existing_tab) = lock.tab_registry.get_client_tab(client_id) {
            return Ok(existing_tab);
        }
    }

    // Check for an unreserved clean-slate tab (about:blank or chrome://newtab/)
    let list_res = crate::websocket_hub::send_hub_command(
        state.clone(),
        None,
        "LIST_TABS",
        "",
        serde_json::json!({ "detailed": false }),
    )
    .await;

    if let Ok(tabs_val) = list_res {
        if let Some(tabs) = tabs_val.get("tabs").and_then(|t| t.as_array()) {
            let mut lock = state.lock().await;
            for tab in tabs {
                let url = tab.get("url").and_then(|u| u.as_str()).unwrap_or("");
                let is_clean_slate = url.is_empty() 
                    || url == "about:blank" 
                    || url.starts_with("chrome://newtab") 
                    || url == "newtab";
                
                if is_clean_slate {
                    if let Some(tid) = tab.get("id").and_then(|id| id.as_u64()) {
                        let (is_locked, is_other) = lock.tab_registry.get_tab_lock_status(tid, client_id);
                        if !is_locked || !is_other {
                            if lock.tab_registry.acquire_tab(client_id, tid).is_ok() {
                                return Ok(tid);
                            }
                        }
                    }
                }
            }
        }
    }

    // If no clean-slate tab is available, provision a fresh tab to preserve user tabs
    let new_tab_res = tools::session::create_tab(state.clone(), client_id, "about:blank").await?;
    if let Some(new_tid) = new_tab_res.get("tabId").and_then(|v| v.as_u64()) {
        Ok(new_tid)
    } else {
        Err("No browser tab available and failed to automatically create a new tab.".to_string())
    }
}

pub async fn handle_mcp_request(
    client_id: &str,
    req: McpRequest,
    state: SharedHub,
) -> Option<McpResponse> {
    if req.method.starts_with("notifications/") {
        return None;
    }

    let mut result_value = None;
    let mut error_val = None;

    let res_result: Result<Value, String> = match req.method.as_str() {
        "initialize" => Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "OnlyDOMs",
                "version": "1.0.0"
            },
            "instructions": "⚠️ USER DISCRETIONARY WARNING: This agent possesses capabilities to actively inspect, navigate, execute arbitrary JavaScript, manipulate the DOM, and control your active browser session. OnlyDOMs automatically isolates agent tabs so concurrent agents work safely in separate tabs without collisions."
        })),
        "tools/list" => Ok(get_tools_list()),
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
                "list_browser_sessions" => {
                    tools::session::list_browser_sessions(state.clone(), client_id, args).await
                }
                "create_tab" => {
                    let url = args.get("url").and_then(|u| u.as_str()).unwrap_or("about:blank");
                    tools::session::create_tab(state.clone(), client_id, url).await
                }
                "switch_tab" => {
                    match parse_u64(args.get("tab_id")) {
                        Some(id) if id > 0 => tools::session::switch_tab(state.clone(), client_id, id).await,
                        _ => Err("Missing or invalid tab_id.".to_string()),
                    }
                }
                "close_tab" => {
                    match parse_u64(args.get("tab_id")) {
                        Some(id) if id > 0 => tools::session::close_tab(state.clone(), client_id, id).await,
                        _ => Err("Missing or invalid tab_id.".to_string()),
                    }
                }
                "select_session" => {
                    match parse_u64(args.get("session_id")) {
                        Some(id) if id > 0 => tools::session::select_session(state.clone(), client_id, id).await,
                        _ => Err("Missing or invalid session_id. Please provide a valid tab ID.".to_string()),
                    }
                }
                _ => {
                    let requested_tab = parse_u64(args.get("tab_id")).or_else(|| parse_u64(args.get("tabId")));
                    match resolve_or_allocate_tab(state.clone(), client_id, requested_tab).await {
                        Ok(target_tab) => {
                            match name {
                                "capture_viewport" => {
                                    tools::visual::capture_viewport(state.clone(), Some(target_tab), args).await
                                }
                                "take_screenshot" => {
                                    tools::visual::take_screenshot(state.clone(), Some(target_tab), args).await
                                }
                                "run_browser_test" => {
                                    tools::testing::run_browser_test(state.clone(), Some(target_tab), args).await
                                }
                                "execute_browser_pipeline" => {
                                    tools::navigation::execute_browser_pipeline(state.clone(), Some(target_tab), args).await
                                }
                                "fill_inputs" => {
                                    tools::input::fill_inputs(state.clone(), Some(target_tab), args).await
                                }
                                "interact" => {
                                    tools::input::interact(state.clone(), Some(target_tab), args).await
                                }
                                "evaluate_expression" => {
                                    tools::debugger::evaluate_expression(state.clone(), Some(target_tab), args).await
                                }
                                "add_breakpoint" => {
                                    tools::debugger::add_breakpoint(state.clone(), Some(target_tab), args).await
                                }
                                "pause_execution" => {
                                    tools::debugger::pause_execution(state.clone(), Some(target_tab)).await
                                }
                                "resume_execution" => {
                                    tools::debugger::resume_execution(state.clone(), Some(target_tab)).await
                                }
                                "step_over" => {
                                    tools::debugger::step_over(state.clone(), Some(target_tab)).await
                                }
                                "step_into" => {
                                    tools::debugger::step_into(state.clone(), Some(target_tab)).await
                                }
                                _ => Err(format!("Unknown tool: {}", name)),
                            }
                        }
                        Err(e) => Err(e),
                    }
                }
            }
        }
        _ => Err(format!("Unknown method: {}", req.method)),
    };

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
        }
        Err(e) => {
            error_val = Some(McpError {
                code: -32603,
                message: e,
            });
        }
    }

    Some(McpResponse {
        jsonrpc: "2.0".to_string(),
        id: req.id,
        result: result_value,
        error: error_val,
    })
}
