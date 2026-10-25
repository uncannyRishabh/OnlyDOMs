use crate::websocket_hub::{SharedHub, send_hub_command};
use serde_json::Value;

pub async fn list_browser_sessions(state: SharedHub, params: Value) -> Result<Value, String> {
    let detailed = params.get("detailed").and_then(|v| v.as_bool()).unwrap_or(false);
    send_hub_command(state.clone(), "LIST_TABS", "", serde_json::json!({ "detailed": detailed })).await
}

pub async fn select_session(state: SharedHub, session_id: u64) -> Result<Value, String> {
    send_hub_command(state.clone(), "ATTACH_TAB", "", serde_json::json!(session_id)).await
}

pub async fn create_tab(state: SharedHub, url: &str) -> Result<Value, String> {
    send_hub_command(state.clone(), "CREATE_TAB", "", serde_json::json!({ "url": url })).await
}

pub async fn switch_tab(state: SharedHub, tab_id: u64) -> Result<Value, String> {
    send_hub_command(state.clone(), "SWITCH_TAB", "", serde_json::json!({ "tabId": tab_id })).await
}

pub async fn close_tab(state: SharedHub, tab_id: u64) -> Result<Value, String> {
    send_hub_command(state.clone(), "CLOSE_TAB", "", serde_json::json!({ "tabId": tab_id })).await
}
