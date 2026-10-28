use crate::websocket_hub::{SharedHub, send_hub_command};
use serde_json::Value;

/// Strips "http://" and "https://" prefixes and removes all query parameters from tab URLs
/// in the Rust layer for token efficiency in agent responses.
pub fn clean_tab_url(raw: &str) -> String {
    let s = raw.trim();
    if s.is_empty() {
        return String::new();
    }

    // 1. Strip http:// or https:// (case-insensitive)
    let without_proto = if s.len() >= 8 && s[..8].eq_ignore_ascii_case("https://") {
        &s[8..]
    } else if s.len() >= 7 && s[..7].eq_ignore_ascii_case("http://") {
        &s[7..]
    } else {
        s
    };

    // 2. Strip all query params: take everything before '?'
    let without_query = match without_proto.find('?') {
        Some(pos) => &without_proto[..pos],
        None => without_proto,
    };

    without_query.to_string()
}

pub async fn list_browser_sessions(
    state: SharedHub,
    client_id: &str,
    params: Value,
) -> Result<Value, String> {
    let detailed = params.get("detailed").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut res = send_hub_command(state.clone(), None, "LIST_TABS", "", serde_json::json!({ "detailed": detailed })).await?;

    // Annotate tabs with locking information and clean URL if not detailed
    let mut lock = state.lock().await;
    if let Some(tabs) = res.get_mut("tabs").and_then(|t| t.as_array_mut()) {
        for tab in tabs {
            if let Some(tid) = tab.get("id").and_then(|id| id.as_u64()) {
                let (is_locked, is_other) = lock.tab_registry.get_tab_lock_status(tid, client_id);
                if let Some(obj) = tab.as_object_mut() {
                    obj.insert("is_locked".to_string(), Value::Bool(is_locked));
                    obj.insert("locked_by_current_agent".to_string(), Value::Bool(is_locked && !is_other));
                    obj.insert("locked_by_other_agent".to_string(), Value::Bool(is_other));

                    // Clean URL in the Rust layer
                    if !detailed {
                        if let Some(url_val) = obj.get("url").and_then(|u| u.as_str()) {
                            let cleaned = clean_tab_url(url_val);
                            obj.insert("url".to_string(), Value::String(cleaned));
                        }
                    }
                }
            }
        }
    }

    Ok(res)
}

pub async fn select_session(
    state: SharedHub,
    client_id: &str,
    session_id: u64,
) -> Result<Value, String> {
    {
        let mut lock = state.lock().await;
        lock.tab_registry.acquire_tab(client_id, session_id)?;
    }
    send_hub_command(state.clone(), Some(session_id), "ATTACH_TAB", "", serde_json::json!(session_id)).await
}

pub async fn create_tab(
    state: SharedHub,
    client_id: &str,
    url: &str,
) -> Result<Value, String> {
    let res = send_hub_command(state.clone(), None, "CREATE_TAB", "", serde_json::json!({ "url": url })).await?;
    if let Some(tab_id) = res.get("tabId").and_then(|v| v.as_u64()) {
        let mut lock = state.lock().await;
        let _ = lock.tab_registry.acquire_tab(client_id, tab_id);
    }
    Ok(res)
}

pub async fn switch_tab(
    state: SharedHub,
    client_id: &str,
    tab_id: u64,
) -> Result<Value, String> {
    {
        let mut lock = state.lock().await;
        lock.tab_registry.acquire_tab(client_id, tab_id)?;
    }
    send_hub_command(state.clone(), Some(tab_id), "SWITCH_TAB", "", serde_json::json!({ "tabId": tab_id })).await
}

pub async fn close_tab(
    state: SharedHub,
    client_id: &str,
    tab_id: u64,
) -> Result<Value, String> {
    {
        let mut lock = state.lock().await;
        lock.tab_registry.release_tab(client_id, tab_id);
    }
    send_hub_command(state.clone(), Some(tab_id), "CLOSE_TAB", "", serde_json::json!({ "tabId": tab_id })).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_tab_url() {
        assert_eq!(clean_tab_url("https://www.youtube.com/"), "www.youtube.com/");
        assert_eq!(clean_tab_url("http://example.com/"), "example.com/");
        assert_eq!(
            clean_tab_url("https://login.microsoftonline.com/oauth2/authorize?client_id=123&response_type=code"),
            "login.microsoftonline.com/oauth2/authorize"
        );
        assert_eq!(clean_tab_url("http://localhost:4200/#/dashboard"), "localhost:4200/#/dashboard");
        assert_eq!(clean_tab_url("chrome://newtab/"), "chrome://newtab/");
        assert_eq!(clean_tab_url("about:blank"), "about:blank");
        assert_eq!(clean_tab_url("file:///path/to/file.html"), "file:///path/to/file.html");
        assert_eq!(
            clean_tab_url("https://docs.google.com/spreadsheets/d/123/edit?gid=0#gid=0"),
            "docs.google.com/spreadsheets/d/123/edit"
        );
        assert_eq!(clean_tab_url(""), "");
    }
}
