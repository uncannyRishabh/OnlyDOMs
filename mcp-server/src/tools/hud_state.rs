use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use crate::websocket_hub::SharedHub;

/// Maximum character length for the `_intent` preview (2x limit: 180 chars / ~30 words).
pub const MAX_INTENT_CHARS: usize = 180;

/// Maximum number of RPC telemetry entries stored in the per-tab ring buffer.
pub const MAX_RPC_LOGS_PER_TAB: usize = 25;

/// Maximum number of HUD broadcast frames retained in daemon telemetry history.
pub const MAX_HUD_BROADCAST_HISTORY: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcTelemetryEntry {
    pub rpc_id: String,
    pub tool_name: String,
    pub intent: String,
    pub executing_cmd: String,
    pub status: String,
    pub latency_ms: Option<u64>,
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTaskState {
    pub tab_id: u64,
    pub tab_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon_url: Option<String>,
    pub intent: String,
    pub executing_cmd: String,
    pub status: String, // "intent_preview" | "running" | "waiting" | "completed" | "error"
    pub updated_at: u64,
    pub logs: VecDeque<RpcTelemetryEntry>,
}

impl AgentTaskState {
    pub fn new(tab_id: u64, tab_title: &str) -> Self {
        let clean_title = if tab_title.trim().is_empty() {
            "Active Tab"
        } else {
            tab_title.trim()
        };
        Self {
            tab_id,
            tab_title: clean_title.to_string(),
            favicon_url: None,
            intent: String::new(),
            executing_cmd: String::new(),
            status: "intent_preview".to_string(),
            updated_at: current_epoch_ms(),
            logs: VecDeque::with_capacity(MAX_RPC_LOGS_PER_TAB),
        }
    }

    pub fn push_log(&mut self, entry: RpcTelemetryEntry) {
        if self.logs.len() >= MAX_RPC_LOGS_PER_TAB {
            self.logs.pop_front();
        }
        self.logs.push_back(entry);
    }
}

pub fn current_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Validates and bounds a task intent string to the 180-character ceiling.
/// If `raw_intent` is omitted or blank, synthesizes a concise fallback from the tool and arguments.
pub fn normalize_intent_preview(raw_intent: Option<&str>, tool_name: &str, args: &Value) -> String {
    if let Some(raw) = raw_intent {
        let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
            return truncate_chars(&collapsed, MAX_INTENT_CHARS);
        }
    }

    let fallback = match tool_name {
        "execute_browser_pipeline" => {
            let count = args
                .get("steps")
                .or_else(|| args.get("actions"))
                .and_then(|s| s.as_array())
                .map(|a| a.len())
                .unwrap_or(1);
            format!("Execute {}-step browser automation pipeline", count)
        }
        "fill_inputs" => {
            let count = args
                .get("fields")
                .and_then(|f| f.as_array())
                .map(|a| a.len())
                .unwrap_or(1);
            format!("Fill {} form input field(s)", count)
        }
        "run_browser_test" => {
            let name = args
                .get("test_name")
                .and_then(|v| v.as_str())
                .unwrap_or("browser_test");
            format!("Run automated visual browser test '{}'", name)
        }
        "print_to_pdf" => "Export current page layout to PDF document".to_string(),
        "replay_curl" => "Replay captured or modified network request via native Rust engine".to_string(),
        "evaluate_expression" => "Evaluate JavaScript expression in active tab".to_string(),
        "interact" => "Perform coordinate interaction on active viewport".to_string(),
        other => format!("Execute {} on active browser tab", other),
    };

    truncate_chars(&fallback, MAX_INTENT_CHARS)
}

pub fn truncate_chars(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        input.to_string()
    } else {
        input.chars().take(max_chars).collect()
    }
}

/// Formats the live command ticker label according to the specification:
/// `"Executing :: <cmd> on Tab :: #<id> (<title>)"`
pub fn format_executing_cmd(cmd_descriptor: &str, tab_id: u64, tab_title: Option<&str>) -> String {
    let title = tab_title
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .unwrap_or("Active Tab");
    format!(
        "Executing :: {} on Tab :: #{} ({})",
        cmd_descriptor.trim(),
        tab_id,
        title
    )
}

/// Generates a concise command descriptor for top-level MCP tool calls.
pub fn describe_tool_cmd(tool_name: &str, args: &Value) -> String {
    match tool_name {
        "fill_inputs" => {
            let count = args
                .get("fields")
                .and_then(|f| f.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            format!("fill_inputs ({} fields)", count)
        }
        "execute_browser_pipeline" => {
            let count = args
                .get("steps")
                .or_else(|| args.get("actions"))
                .and_then(|s| s.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            format!("execute_browser_pipeline ({} steps)", count)
        }
        "navigate" | "create_tab" => {
            let url = args.get("url").and_then(|u| u.as_str()).unwrap_or("about:blank");
            format!("{}({})", tool_name, truncate_chars(url, 36))
        }
        "interact" => {
            let act = args.get("action").and_then(|a| a.as_str()).unwrap_or("click");
            if let Some(q) = args.get("quadrant").or_else(|| args.get("cell")).and_then(|v| v.as_str()) {
                format!("{}({})", act, q)
            } else {
                format!("{}(viewport)", act)
            }
        }
        "replay_curl" => {
            if let Some(req_id) = args.get("request_id").and_then(|v| v.as_str()) {
                format!("replay_curl({})", req_id)
            } else {
                "replay_curl(raw)".to_string()
            }
        }
        _ => tool_name.to_string(),
    }
}

/// Generates a step-level command descriptor for `execute_browser_pipeline` or `run_browser_test`:
/// `<step.action>(<target>)`
pub fn describe_pipeline_step(step: &Value) -> String {
    let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("step");
    let target = match action {
        "navigate" | "new_tab" => step
            .get("url")
            .and_then(|v| v.as_str())
            .map(|u| truncate_chars(u, 40))
            .unwrap_or_else(|| "url".to_string()),
        "click" | "wait_for" | "wait_selector" | "scroll" => {
            if let Some(sel) = step.get("selector").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                truncate_chars(sel, 36)
            } else if let Some(txt) = step.get("text").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                truncate_chars(txt, 36)
            } else if let Some(aria) = step.get("aria_label").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                truncate_chars(aria, 36)
            } else if let Some(q) = step.get("quadrant").or_else(|| step.get("cell")).and_then(|v| v.as_str()) {
                q.to_string()
            } else {
                "target".to_string()
            }
        }
        "navigate_to_result" => step
            .get("url")
            .and_then(|v| v.as_str())
            .map(|u| truncate_chars(u, 40))
            .unwrap_or_else(|| "result_url".to_string()),
        "take_screenshot" | "screenshot" => step
            .get("save_path")
            .and_then(|v| v.as_str())
            .map(|p| truncate_chars(p, 36))
            .unwrap_or_else(|| "viewport".to_string()),
        "fill_inputs" => {
            let count = step
                .get("fields")
                .and_then(|f| f.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            format!("{} fields", count)
        }
        "evaluate" | "wait_condition" => step
            .get("expression")
            .and_then(|v| v.as_str())
            .map(|e| truncate_chars(e, 32))
            .unwrap_or_else(|| "expr".to_string()),
        "press_key" => step
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("Enter")
            .to_string(),
        "switch_tab" => {
            let tid = step.get("tab_id").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("#{}", tid)
        }
        "sleep" => {
            let ms = step.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(500);
            format!("{}ms", ms)
        }
        "replay_curl" => step
            .get("request_id")
            .and_then(|v| v.as_str())
            .unwrap_or("curl")
            .to_string(),
        _ => String::new(),
    };

    if target.is_empty() {
        action.to_string()
    } else {
        format!("{}({})", action, target)
    }
}

/// Records and broadcasts a `HUD_STATE_UPDATE` frame to `background.js`.
pub async fn broadcast_hud_state(
    state: SharedHub,
    tab_id: u64,
    tab_title: Option<&str>,
    intent: &str,
    cmd_descriptor: &str,
    status: &str,
    rpc_id: Option<&str>,
    tool_name: Option<&str>,
    latency_ms: Option<u64>,
    force_wire_send: bool,
) -> Value {
    broadcast_hud_state_full(
        state,
        tab_id,
        tab_title,
        intent,
        cmd_descriptor,
        status,
        rpc_id,
        tool_name,
        latency_ms,
        None,
        None,
        force_wire_send,
    )
    .await
}

/// Enriched broadcast including structured agent request arguments and browser response payload.
pub async fn broadcast_hud_state_full(
    state: SharedHub,
    tab_id: u64,
    tab_title: Option<&str>,
    intent: &str,
    cmd_descriptor: &str,
    status: &str,
    rpc_id: Option<&str>,
    tool_name: Option<&str>,
    latency_ms: Option<u64>,
    request_data: Option<serde_json::Value>,
    response_data: Option<serde_json::Value>,
    force_wire_send: bool,
) -> Value {
    let bounded_intent = truncate_chars(intent.trim(), MAX_INTENT_CHARS);
    let mut lock = state.lock().await;

    let resolved_title = tab_title
        .map(|s| s.to_string())
        .or_else(|| lock.hud_tasks.get(&tab_id).map(|t| t.tab_title.clone()))
        .unwrap_or_else(|| "Active Tab".to_string());

    let executing_cmd = if cmd_descriptor.starts_with("Executing :: ") {
        cmd_descriptor.to_string()
    } else {
        format_executing_cmd(cmd_descriptor, tab_id, Some(&resolved_title))
    };

    let now_ms = current_epoch_ms();
    let task_entry = lock
        .hud_tasks
        .entry(tab_id)
        .or_insert_with(|| AgentTaskState::new(tab_id, &resolved_title));

    task_entry.tab_title = resolved_title;
    task_entry.intent = bounded_intent.clone();
    task_entry.executing_cmd = executing_cmd.clone();
    task_entry.status = status.to_string();
    task_entry.updated_at = now_ms;

    if let Some(rid) = rpc_id {
        task_entry.push_log(RpcTelemetryEntry {
            rpc_id: rid.to_string(),
            tool_name: tool_name.unwrap_or("mcp_tool").to_string(),
            intent: bounded_intent.clone(),
            executing_cmd: executing_cmd.clone(),
            status: status.to_string(),
            latency_ms,
            timestamp: now_ms,
            request_data: request_data.clone(),
            response_data: response_data.clone(),
        });
        lock.network_harvester.record_agent_step_with_meta(
            tab_id,
            rid,
            tool_name,
            Some(&bounded_intent),
            now_ms,
        );
    }

    let mut frame = serde_json::json!({
        "type": "HUD_STATE_UPDATE",
        "tab_id": tab_id,
        "intent": bounded_intent,
        "executing_cmd": executing_cmd,
        "status": status,
        "updated_at": now_ms,
        "rpc_id": rpc_id,
        "tool_name": tool_name.unwrap_or("mcp_tool"),
        "latency_ms": latency_ms
    });

    if let Some(ref req) = request_data {
        frame["request_data"] = req.clone();
    }
    if let Some(ref resp) = response_data {
        frame["response_data"] = resp.clone();
    }

    if lock.hud_broadcast_log.len() >= MAX_HUD_BROADCAST_HISTORY {
        lock.hud_broadcast_log.pop_front();
    }
    lock.hud_broadcast_log.push_back(frame.clone());

    let should_send_tx = lock.conn_token > 0 || force_wire_send;
    let tx_opt = if should_send_tx { lock.tx.clone() } else { None };
    let hud_tx_opt = lock.hud_tx.clone();
    drop(lock);

    if let Some(hud_tx) = hud_tx_opt {
        let _ = hud_tx.try_send(frame.clone());
    }
    if let Some(tx) = tx_opt {
        let _ = tx.try_send(frame.clone());
    }

    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_intent_180_char_limit_enforcement() {
        let exact_175 = "A".repeat(175);
        let norm_175 = normalize_intent_preview(Some(&exact_175), "execute_browser_pipeline", &json!({}));
        assert_eq!(norm_175.chars().count(), 175);

        let over_220 = "B".repeat(220);
        let norm_180 = normalize_intent_preview(Some(&over_220), "execute_browser_pipeline", &json!({}));
        assert_eq!(norm_180.chars().count(), MAX_INTENT_CHARS);
    }

    #[test]
    fn test_format_executing_cmd_matches_spec() {
        let cmd = format_executing_cmd("fill_inputs (3 fields)", 142, Some("Checkout"));
        assert_eq!(cmd, "Executing :: fill_inputs (3 fields) on Tab :: #142 (Checkout)");
    }

    #[test]
    fn test_describe_pipeline_step_formatting() {
        let step = json!({ "action": "click", "selector": "#submit-order" });
        assert_eq!(describe_pipeline_step(&step), "click(#submit-order)");

        let nav_step = json!({ "action": "navigate", "url": "https://staging.example.com/checkout" });
        assert_eq!(describe_pipeline_step(&nav_step), "navigate(https://staging.example.com/checkout)");
    }

    #[test]
    fn test_rpc_telemetry_ring_buffer_max_25() {
        let mut state = AgentTaskState::new(10, "Staging");
        for i in 0..35 {
            state.push_log(RpcTelemetryEntry {
                rpc_id: format!("rpc_{}", i),
                tool_name: "evaluate_expression".to_string(),
                intent: "Test ring buffer".to_string(),
                executing_cmd: "Executing :: evaluate_expression on Tab :: #10 (Staging)".to_string(),
                status: "running".to_string(),
                latency_ms: Some(12),
                timestamp: 1000 + i,
                request_data: None,
                response_data: None,
            });
        }
        assert_eq!(state.logs.len(), MAX_RPC_LOGS_PER_TAB);
        assert_eq!(state.logs.front().unwrap().rpc_id, "rpc_10");
        assert_eq!(state.logs.back().unwrap().rpc_id, "rpc_34");
    }
}
