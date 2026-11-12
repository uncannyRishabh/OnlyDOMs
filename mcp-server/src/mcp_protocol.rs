use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;
use crate::websocket_hub::SharedHub;
use crate::tools;
use crate::tools::hud_state::{
    broadcast_hud_state, broadcast_hud_state_full, describe_tool_cmd, normalize_intent_preview,
};

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

fn intent_schema_property() -> Value {
    serde_json::json!({
        "type": "string",
        "maxLength": 180,
        "description": "Mandatory one-liner task intent preview (max 180 chars / ~30 words) describing what this tool call is about to achieve and on which tab."
    })
}

pub fn get_tools_list() -> Value {
    let intent_prop = intent_schema_property();
    let mut list = serde_json::json!({
        "tools": [
            {
                "name": "list_browser_sessions",
                "description": "List open browser tabs across Chrome windows in a compact, token-efficient format with agent lock status",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
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
                        "_intent": intent_prop.clone(),
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
                        "_intent": intent_prop.clone(),
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
                        "_intent": intent_prop.clone(),
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
                        "_intent": intent_prop.clone(),
                        "session_id": { "type": "string", "description": "Numeric Chrome tab ID" }
                    },
                    "required": ["session_id"]
                }
            },
            {
                "name": "capture_viewport",
                "description": "Capture a clean screenshot of the current page viewport (automatically cloaks Agent HUD pill and excludes DevTools/browser infobars)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
                        "format": { "type": "string", "enum": ["png", "webp", "jpeg"], "description": "Image format (default: webp)" },
                        "quality": { "type": "number", "description": "Image quality 0-100 (for webp/jpeg)" },
                        "cloak_hud": { "type": "boolean", "description": "Explicitly dispatch synchronous CLOAK_HUD pre-filter before capture" }
                    }
                }
            },
            {
                "name": "take_screenshot",
                "description": "Capture a clean screenshot of the web page (viewport or full scrollable page) with HUD cloaking. Saves to disk if save_path is provided to avoid context token consumption.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
                        "full_page": { "type": "boolean", "description": "Capture entire scrollable page height instead of just viewport (default: false)" },
                        "format": { "type": "string", "enum": ["png", "webp", "jpeg"], "description": "Image format (default: png)" },
                        "quality": { "type": "number", "description": "Image quality 0-100 for webp/jpeg" },
                        "save_path": { "type": "string", "description": "Optional file path to save binary image directly on disk" },
                        "include_base64": { "type": "boolean", "description": "Include base64 image data in response (default: true if save_path not set, false if save_path is set)" },
                        "cloak_hud": { "type": "boolean", "description": "Explicitly dispatch synchronous CLOAK_HUD pre-filter before capture" }
                    }
                }
            },
            {
                "name": "print_to_pdf",
                "description": "Generate a PDF document from the current browser page using Chrome DevTools Protocol (CDP Page.printToPDF). Executes silently without native OS print dialogs and returns the PDF as a base64-encoded string.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
                        "tab_id": {
                            "type": "number",
                            "description": "Optional tab ID (defaults to agent's assigned tab)"
                        },
                        "landscape": {
                            "type": "boolean",
                            "description": "Paper orientation: false for portrait, true for landscape (default: false)"
                        },
                        "print_background": {
                            "type": "boolean",
                            "description": "Print background graphics and colors (default: true)"
                        },
                        "scale": {
                            "type": "number",
                            "description": "Page rendering scale factor from 0.1 to 2.0 (default: 1.0)"
                        },
                        "paper_width": {
                            "type": "number",
                            "description": "Paper width in inches (default: 8.5 for US Letter, 8.27 for A4)"
                        },
                        "paper_height": {
                            "type": "number",
                            "description": "Paper height in inches (default: 11.0 for US Letter, 11.69 for A4)"
                        },
                        "margin_top": {
                            "type": "number",
                            "description": "Top margin in inches (default: 0.4)"
                        },
                        "margin_bottom": {
                            "type": "number",
                            "description": "Bottom margin in inches (default: 0.4)"
                        },
                        "margin_left": {
                            "type": "number",
                            "description": "Left margin in inches (default: 0.4)"
                        },
                        "margin_right": {
                            "type": "number",
                            "description": "Right margin in inches (default: 0.4)"
                        },
                        "page_ranges": {
                            "type": "string",
                            "description": "Page ranges to print, e.g. '1-5, 8' (default: all pages)"
                        },
                        "prefer_css_page_size": {
                            "type": "boolean",
                            "description": "Whether to prefer page size defined by CSS @page (default: false)"
                        },
                        "format": {
                            "type": "string",
                            "description": "Standard paper format preset: 'letter', 'a4', 'legal', 'tabloid', 'a3'"
                        },
                        "paper_format": {
                            "type": "string",
                            "description": "Alias for format: 'letter', 'a4', 'legal', 'tabloid', 'a3'"
                        },
                        "save_path": {
                            "type": "string",
                            "description": "Optional local file path to save binary PDF directly to disk"
                        },
                        "include_base64": {
                            "type": "boolean",
                            "description": "Include base64 PDF data in response (default: true)"
                        },
                        "display_header_footer": {
                            "type": "boolean",
                            "description": "Display running header and footer (default: false)"
                        },
                        "header_template": {
                            "type": "string",
                            "description": "HTML template for print header"
                        },
                        "footer_template": {
                            "type": "string",
                            "description": "HTML template for print footer"
                        }
                    }
                }
            },
            {
                "name": "run_browser_test",
                "description": "Automated browser testing & visual overview report pipeline. Decomposes defect or feature descriptions into executable steps, saves cloaked WebP assets (test_reports/<run_id>/assets/step_<n>.webp), compiles self-contained print-styled overview.html and overview.pdf, and serves openable/auto-print links on http://127.0.0.1:9222/reports/<run_id>/.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
                        "run_id": { "type": "string", "description": "Optional explicit run identifier for test_reports/<run_id>/" },
                        "test_name": { "type": "string", "description": "Name or identifier for the test (e.g. 'checkout_coupon_error')" },
                        "description": { "type": "string", "description": "Detailed defect or feature description to validate. If steps are not provided, this is automatically decomposed into executable steps." },
                        "output_dir": { "type": "string", "description": "Custom directory to save test reports and step screenshots (default: test_reports/<run_id>)" },
                        "full_page_screenshots": { "type": "boolean", "description": "Capture full scrollable page for step screenshots (default: false)" },
                        "continue_on_failure": { "type": "boolean", "description": "Continue executing remaining steps after a step fails (default: false)" },
                        "steps": {
                            "type": "array",
                            "description": "Optional explicit list of test steps. If omitted, steps are parsed from description.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string", "description": "Human-readable step title" },
                                    "_intent": intent_prop.clone(),
                                    "action": {
                                        "type": "string",
                                        "enum": ["navigate", "click", "fill_inputs", "press_key", "scroll", "wait_for", "wait_condition", "wait_network_idle", "evaluate", "sleep", "assert", "replay_curl"],
                                        "description": "Action type"
                                    },
                                    "url": { "type": "string" },
                                    "selector": { "type": "string" },
                                    "text": { "type": "string" },
                                    "aria_label": { "type": "string" },
                                    "quadrant": { "type": "string", "description": "16-column grid cell coordinate (e.g. 'P9')" },
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
                "description": "Execute a batch sequence of resilient browser actions with built-in await, polling, live HUD command ticker updates, and retry logic on the agent's assigned tab",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
                        "steps": {
                            "type": "array",
                            "description": "Ordered actions: navigate, new_tab, switch_tab, wait_for, wait_condition, wait_network_idle, click, scroll, evaluate, fill_inputs, press_key, sleep, take_screenshot, capture_grid_screenshot, print_to_pdf, handle_dialog, replay_curl",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "action": {
                                        "type": "string",
                                        "enum": ["navigate", "new_tab", "switch_tab", "wait_for", "wait_condition", "wait_network_idle", "click", "scroll", "evaluate", "fill_inputs", "press_key", "sleep", "take_screenshot", "capture_grid_screenshot", "print_to_pdf", "handle_dialog", "replay_curl"],
                                        "description": "Action type"
                                    },
                                    "_intent": intent_prop.clone(),
                                    "url": { "type": "string", "description": "URL for navigate or new_tab" },
                                    "tab_id": { "type": "number", "description": "Tab ID for switch_tab" },
                                    "selector": { "type": "string", "description": "CSS selector to wait for, click, or scroll" },
                                    "text": { "type": "string", "description": "Visible text to target for semantic click" },
                                    "aria_label": { "type": "string", "description": "Aria-label to target for semantic click" },
                                    "quadrant": { "type": "string", "description": "Optional 16-column grid cell (e.g. 'P9') with HUD hit-test passthrough" },
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
                        "_intent": intent_prop.clone(),
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
                "description": "Interact with the page using normalized coordinates (0.0 to 1.0) or 16-column grid cell coordinates (e.g. 'P9') with automatic HUD hit-test passthrough",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
                        "action": { "type": "string", "enum": ["click", "hover", "scroll"], "description": "Interaction action" },
                        "quadrant": { "type": "string", "description": "Optional 16-column grid cell coordinate (e.g. 'A1'..'P9') with HUD hit-test passthrough" },
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
                "description": "Evaluate JavaScript in the assigned tab context via CDP Runtime.evaluate with automatic Rust-layer HUD & style sanitization",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
                        "expression": { "type": "string", "description": "JavaScript expression to evaluate" }
                    },
                    "required": ["expression"]
                }
            },
            {
                "name": "replay_curl",
                "description": "Zero-subprocess Rust network cURL replay & optimistic network ring-buffer inspector. Replays a captured request_id or a raw/modified curl command string without spawning a shell, or lists the tab's 50-item optimistic XHR/Fetch/Document ring buffer when action='list'.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
                        "action": { "type": "string", "enum": ["replay", "list"], "description": "Set to 'list' to inspect captured optimistic cURL requests in the ring buffer (default: 'replay')" },
                        "request_id": { "type": "string", "description": "ID of a captured request from the 50-item tab ring buffer" },
                        "curl": { "type": "string", "description": "Raw or modified curl command string parsed natively via shlex" },
                        "method": { "type": "string", "description": "Optional HTTP method override (e.g. POST, PUT, GET)" },
                        "url": { "type": "string", "description": "Optional URL override" },
                        "headers": { "type": "object", "description": "Optional header key-value overrides" },
                        "body": { "type": "string", "description": "Optional request body / postData override" },
                        "mode": { "type": "string", "enum": ["auto", "native", "browser"], "description": "Execution transport: 'native' (Rust TCP HTTP), 'browser' (in-tab fetch preserving HttpOnly cookies), or 'auto'" }
                    }
                }
            },
            {
                "name": "get_network_traffic",
                "description": "Query captured HTTP/XHR/Fetch network traffic correlated to agent actions and cURL commands. Returns full request and response details (method, URL, headers, post data, status code, response headers, response body, triggering action/intent, and cURL command).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" },
                        "action_id": { "type": "string", "description": "Optional filter by triggering agent step/RPC ID (e.g. 'pipeline_step_1')" },
                        "rpc_id": { "type": "string", "description": "Alias for action_id" },
                        "request_id": { "type": "string", "description": "Optional filter for a specific network request ID" },
                        "url_filter": { "type": "string", "description": "Optional substring filter on URL" }
                    }
                }
            },
            {
                "name": "add_breakpoint",
                "description": "Add a debugger breakpoint",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
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
                        "_intent": intent_prop.clone(),
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
                        "_intent": intent_prop.clone(),
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
                        "_intent": intent_prop.clone(),
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
                        "_intent": intent_prop.clone(),
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to agent's assigned tab)" }
                    }
                }
            },
            {
                "name": "handle_dialog",
                "description": "Programmatically respond to an active JavaScript modal dialog (alert, confirm, prompt, or beforeunload) in the browser",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop.clone(),
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to current agent's assigned tab)" },
                        "action": { "type": "string", "enum": ["accept", "dismiss"], "description": "Whether to accept (OK) or dismiss (Cancel) the dialog" },
                        "prompt_text": { "type": "string", "description": "Optional text to supply when responding to a prompt dialog" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "get_active_dialog",
                "description": "Check if an active modal dialog is currently blocking the tab, and retrieve its message and type",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "_intent": intent_prop,
                        "tab_id": { "type": "number", "description": "Optional tab ID (defaults to current agent's assigned tab)" }
                    }
                }
            }
        ]
    });

    if let Some(arr) = list.get_mut("tools").and_then(|t| t.as_array_mut()) {
        for tool in arr.iter_mut() {
            if let Some(props) = tool.get_mut("inputSchema").and_then(|s| s.get_mut("properties")).and_then(|p| p.as_object_mut()) {
                if !props.contains_key("_intent") {
                    props.insert("_intent".to_string(), intent_schema_property());
                }
            }
        }
    }
    list
}

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
    let rpc_id_str = req
        .id
        .as_ref()
        .map(|v| {
            if let Some(s) = v.as_str() {
                s.to_string()
            } else {
                v.to_string()
            }
        })
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

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
            "instructions": "⚠️ ONLYDOMS MCP AGENT DIRECTIVES:\n\
1. PROTOCOL CONSISTENCY (MCP-FIRST): Always use call_mcp_tool for all browser automation actions (execute_browser_pipeline, evaluate_expression, take_screenshot, create_tab, close_tab). NEVER mix shell CLI commands (onlydoms eval, onlydoms click) on active tabs; tab locks are exclusive per agent session and CLI will fail.\n\
2. PIPELINE COMPOUND BATCHING: Target 1-2 compound turns. Dispatch multi-step workflows in execute_browser_pipeline with steps: navigate, wait_for/wait_selector, click (OmniClick with React/Angular support), evaluate, navigate_to_result, take_screenshot/screenshot, print_to_pdf.\n\
3. ASYNC DOM PREDICATES: Never run shell sleep polling. In evaluate_expression, use `await new Promise(...)` with MutationObserver for instant resolution.\n\
4. SUBAGENT ISOLATION: For deep, open-ended trial-and-error exploratory crawls, delegate to a research subagent via invoke_subagent to keep parent context clean.\n\
5. ZERO SCHEMA MICRO-READS: Do NOT read individual tool .json schema files. Use the full parameter signatures documented in SKILL.md."
        })),
        "tools/list" => Ok(get_tools_list()),
        "tools/call" => {
            let name = req.params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()).unwrap_or("");
            let args = req.params.as_ref().and_then(|p| p.get("arguments")).cloned().unwrap_or(serde_json::json!({}));
            let args_snapshot = args.clone();

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

            let raw_intent = args.get("_intent").and_then(|v| v.as_str()).map(|s| s.to_string());
            let has_explicit_intent = raw_intent.is_some();
            let normalized_intent = normalize_intent_preview(raw_intent.as_deref(), name, &args);
            let cmd_desc = describe_tool_cmd(name, &args);
            let call_start = Instant::now();

            match name {
                "list_browser_sessions" => {
                    let res = tools::session::list_browser_sessions(state.clone(), client_id, args).await;
                    if let Ok(ref val) = res {
                        let active_tid = val
                            .get("tabs")
                            .and_then(|t| t.as_array())
                            .and_then(|arr| arr.iter().find(|t| t.get("active").and_then(|a| a.as_bool()) == Some(true)).or_else(|| arr.first()))
                            .and_then(|t| t.get("id"))
                            .and_then(|id| id.as_u64())
                            .unwrap_or(1);
                        let elapsed = call_start.elapsed().as_millis() as u64;
                        broadcast_hud_state(
                            state.clone(),
                            active_tid,
                            None,
                            &normalized_intent,
                            &cmd_desc,
                            "completed",
                            Some(&rpc_id_str),
                            Some(name),
                            Some(elapsed),
                            has_explicit_intent,
                        )
                        .await;
                    }
                    res
                }
                "create_tab" => {
                    let url = args.get("url").and_then(|u| u.as_str()).unwrap_or("about:blank");
                    let res = tools::session::create_tab(state.clone(), client_id, url).await;
                    if let Ok(ref val) = res {
                        let new_tid = val.get("tabId").and_then(|v| v.as_u64()).unwrap_or(1);
                        let elapsed = call_start.elapsed().as_millis() as u64;
                        broadcast_hud_state(
                            state.clone(),
                            new_tid,
                            None,
                            &normalized_intent,
                            &cmd_desc,
                            "completed",
                            Some(&rpc_id_str),
                            Some(name),
                            Some(elapsed),
                            has_explicit_intent,
                        )
                        .await;
                    }
                    res
                }
                "switch_tab" => {
                    match parse_u64(args.get("tab_id")) {
                        Some(id) if id > 0 => {
                            broadcast_hud_state(
                                state.clone(),
                                id,
                                None,
                                &normalized_intent,
                                &cmd_desc,
                                "running",
                                Some(&rpc_id_str),
                                Some(name),
                                None,
                                has_explicit_intent,
                            )
                            .await;
                            let res = tools::session::switch_tab(state.clone(), client_id, id).await;
                            let elapsed = call_start.elapsed().as_millis() as u64;
                            broadcast_hud_state(
                                state.clone(),
                                id,
                                None,
                                &normalized_intent,
                                &cmd_desc,
                                if res.is_ok() { "completed" } else { "error" },
                                None,
                                Some(name),
                                Some(elapsed),
                                false,
                            )
                            .await;
                            res
                        }
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
                        Some(id) if id > 0 => {
                            let res = tools::session::select_session(state.clone(), client_id, id).await;
                            let elapsed = call_start.elapsed().as_millis() as u64;
                            broadcast_hud_state(
                                state.clone(),
                                id,
                                None,
                                &normalized_intent,
                                &cmd_desc,
                                if res.is_ok() { "completed" } else { "error" },
                                Some(&rpc_id_str),
                                Some(name),
                                Some(elapsed),
                                has_explicit_intent,
                            )
                            .await;
                            res
                        }
                        _ => Err("Missing or invalid session_id. Please provide a valid tab ID.".to_string()),
                    }
                }
                "replay_curl" => {
                    let requested_tab = parse_u64(args.get("tab_id")).or_else(|| parse_u64(args.get("tabId")));
                    let target_tab = resolve_or_allocate_tab(state.clone(), client_id, requested_tab)
                        .await
                        .unwrap_or_else(|_| requested_tab.unwrap_or(1));
                    broadcast_hud_state_full(
                        state.clone(),
                        target_tab,
                        None,
                        &normalized_intent,
                        &cmd_desc,
                        "running",
                        Some(&rpc_id_str),
                        Some(name),
                        None,
                        Some(args_snapshot.clone()),
                        None,
                        has_explicit_intent,
                    )
                    .await;
                    let res = tools::curl_runner::replay_curl(state.clone(), Some(target_tab), args).await;
                    let elapsed = call_start.elapsed().as_millis() as u64;
                    let final_status = if res.is_ok() { "completed" } else { "error" };
                    broadcast_hud_state_full(
                        state.clone(),
                        target_tab,
                        None,
                        &normalized_intent,
                        &cmd_desc,
                        final_status,
                        Some(&rpc_id_str),
                        Some(name),
                        Some(elapsed),
                        Some(args_snapshot),
                        res.as_ref().ok().cloned(),
                        false,
                    )
                    .await;
                    res
                }
                "get_network_traffic" => {
                    let requested_tab = parse_u64(args.get("tab_id")).or_else(|| parse_u64(args.get("tabId")));
                    let target_tab = resolve_or_allocate_tab(state.clone(), client_id, requested_tab)
                        .await
                        .unwrap_or_else(|_| requested_tab.unwrap_or(1));
                    broadcast_hud_state_full(
                        state.clone(),
                        target_tab,
                        None,
                        &normalized_intent,
                        &cmd_desc,
                        "running",
                        Some(&rpc_id_str),
                        Some(name),
                        None,
                        Some(args_snapshot.clone()),
                        None,
                        has_explicit_intent,
                    )
                    .await;
                    let mut list_args = args.clone();
                    if let serde_json::Value::Object(ref mut map) = list_args {
                        map.insert("action".to_string(), serde_json::Value::String("list".to_string()));
                    }
                    let res = tools::curl_runner::replay_curl(state.clone(), Some(target_tab), list_args).await;
                    let elapsed = call_start.elapsed().as_millis() as u64;
                    let final_status = if res.is_ok() { "completed" } else { "error" };
                    broadcast_hud_state_full(
                        state.clone(),
                        target_tab,
                        None,
                        &normalized_intent,
                        &cmd_desc,
                        final_status,
                        Some(&rpc_id_str),
                        Some(name),
                        Some(elapsed),
                        Some(args_snapshot),
                        res.as_ref().ok().cloned(),
                        false,
                    )
                    .await;
                    res
                }
                _ => {
                    let requested_tab = parse_u64(args.get("tab_id")).or_else(|| parse_u64(args.get("tabId")));
                    match resolve_or_allocate_tab(state.clone(), client_id, requested_tab).await {
                        Ok(target_tab) => {
                            let tab_title = args.get("tab_title").and_then(|v| v.as_str()).map(|s| s.to_string());
                            broadcast_hud_state(
                                state.clone(),
                                target_tab,
                                tab_title.as_deref(),
                                &normalized_intent,
                                &cmd_desc,
                                "running",
                                Some(&rpc_id_str),
                                Some(name),
                                None,
                                has_explicit_intent,
                            )
                            .await;

                            let tool_res = match name {
                                "capture_viewport" => {
                                    tools::visual::capture_viewport(state.clone(), Some(target_tab), args).await
                                }
                                "take_screenshot" => {
                                    tools::visual::take_screenshot(state.clone(), Some(target_tab), args).await
                                }
                                "print_to_pdf" => {
                                    tools::visual::print_to_pdf(state.clone(), Some(target_tab), args).await
                                }
                                "run_browser_test" => {
                                    tools::testing::run_browser_test(state.clone(), Some(target_tab), args).await
                                }
                                "execute_browser_pipeline" => {
                                    tools::navigation::execute_browser_pipeline(state.clone(), Some(target_tab), args).await
                                }
                                "fill_inputs" => {
                                    if let Err(e) = tools::dialog::check_dialog_guard(state.clone(), target_tab).await {
                                        Err(e)
                                    } else {
                                        tools::input::fill_inputs(state.clone(), Some(target_tab), args).await
                                    }
                                }
                                "interact" => {
                                    if let Err(e) = tools::dialog::check_dialog_guard(state.clone(), target_tab).await {
                                        Err(e)
                                    } else {
                                        tools::input::interact(state.clone(), Some(target_tab), args).await
                                    }
                                }
                                "evaluate_expression" => {
                                    if let Err(e) = tools::dialog::check_dialog_guard(state.clone(), target_tab).await {
                                        Err(e)
                                    } else {
                                        tools::debugger::evaluate_expression(state.clone(), Some(target_tab), args).await
                                    }
                                }
                                "handle_dialog" => {
                                    tools::dialog::handle_dialog(state.clone(), Some(target_tab), args).await
                                }
                                "get_active_dialog" => {
                                    tools::dialog::get_active_dialog(state.clone(), Some(target_tab)).await
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
                            };

                            let elapsed = call_start.elapsed().as_millis() as u64;
                            let final_status = if tool_res.is_ok() { "completed" } else { "error" };
                            broadcast_hud_state_full(
                                state.clone(),
                                target_tab,
                                tab_title.as_deref(),
                                &normalized_intent,
                                &cmd_desc,
                                final_status,
                                Some(&rpc_id_str),
                                Some(name),
                                Some(elapsed),
                                Some(args_snapshot),
                                tool_res.as_ref().ok().cloned(),
                                false,
                            )
                            .await;

                            tool_res
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
            let code = if e.contains("BLOCKED_BY_SITE_POLICY") {
                -32003
            } else {
                -32603
            };
            error_val = Some(McpError {
                code,
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
