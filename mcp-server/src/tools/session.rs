use crate::websocket_hub::{SharedHub, send_hub_command};
use serde_json::Value;

pub async fn list_browser_sessions(state: SharedHub) -> Result<Value, String> {
    send_hub_command(state.clone(), "LIST_TABS", "", serde_json::json!({})).await
}

pub async fn select_session(state: SharedHub, session_id: u64) -> Result<Value, String> {
    send_hub_command(state.clone(), "ATTACH_TAB", "", serde_json::json!(session_id)).await
}
