use crate::websocket_hub::{SharedHub, send_cdp_command};
use serde_json::Value;

/// Checks if tab execution is suspended by an active modal JavaScript dialog (alert, confirm, prompt, beforeunload).
pub async fn check_dialog_guard(state: SharedHub, tab_id: u64) -> Result<(), String> {
    let lock = state.lock().await;
    if let Some(dialog) = lock.active_dialogs.get(&tab_id) {
        Err(format!(
            "Tab {} execution is suspended by an active modal dialog: [{}] '{}'. Call handle_dialog to accept or dismiss.",
            tab_id, dialog.dialog_type, dialog.message
        ))
    } else {
        Ok(())
    }
}

/// Retrieves the active dialog state on a given tab, if any.
pub async fn get_active_dialog(state: SharedHub, tab_id: Option<u64>) -> Result<Value, String> {
    let lock = state.lock().await;
    let target_tab = match tab_id {
        Some(tid) if tid > 0 => tid,
        _ => {
            // If no specific tab requested, check if there is any active dialog
            if let Some((&tid, _)) = lock.active_dialogs.iter().next() {
                tid
            } else {
                0
            }
        }
    };

    if let Some(dialog) = lock.active_dialogs.get(&target_tab) {
        Ok(serde_json::json!({
            "has_dialog": true,
            "tab_id": target_tab,
            "dialog": dialog
        }))
    } else {
        Ok(serde_json::json!({
            "has_dialog": false,
            "tab_id": target_tab,
            "dialog": null
        }))
    }
}

/// Handles an active JavaScript dialog (accept, dismiss, supply prompt text) via CDP Page.handleJavaScriptDialog.
pub async fn handle_dialog(
    state: SharedHub,
    tab_id: Option<u64>,
    args: Value,
) -> Result<Value, String> {
    let action = args.get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required parameter 'action' ('accept' or 'dismiss')".to_string())?;

    let accept = match action {
        "accept" => true,
        "dismiss" => false,
        other => return Err(format!("Invalid action '{}', must be 'accept' or 'dismiss'", other)),
    };

    let prompt_text = args.get("prompt_text")
        .or_else(|| args.get("promptText"))
        .and_then(|v| v.as_str());

    // Resolve target tab
    let target_tab = match tab_id {
        Some(tid) if tid > 0 => tid,
        _ => {
            if let Some(tid) = args.get("tab_id").or_else(|| args.get("tabId")).and_then(|v| v.as_u64()) {
                tid
            } else {
                let lock = state.lock().await;
                if let Some((&tid, _)) = lock.active_dialogs.iter().next() {
                    tid
                } else {
                    return Err("No active dialog showing on any tab".to_string());
                }
            }
        }
    };

    let handled_type = {
        let mut lock = state.lock().await;
        let dialog = lock.active_dialogs.remove(&target_tab)
            .ok_or_else(|| format!("No active dialog showing on tab {}", target_tab))?;
        dialog.dialog_type
    };

    let mut cdp_params = serde_json::json!({ "accept": accept });
    if let Some(text) = prompt_text {
        cdp_params["promptText"] = serde_json::json!(text);
    }

    send_cdp_command(
        state.clone(),
        Some(target_tab),
        "Page.handleJavaScriptDialog",
        cdp_params,
    ).await?;

    Ok(serde_json::json!({
        "success": true,
        "tab_id": target_tab,
        "action": action,
        "handled_type": handled_type
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use crate::websocket_hub::{HubState, ActiveDialog};

    #[tokio::test]
    async fn test_check_dialog_guard_blocks_and_permits() {
        let hub_state = Arc::new(Mutex::new(HubState::new()));

        // Tab with no dialog -> passes guard
        assert!(check_dialog_guard(hub_state.clone(), 101).await.is_ok());

        // Add active dialog to HubState
        {
            let mut lock = hub_state.lock().await;
            lock.active_dialogs.insert(101, ActiveDialog {
                tab_id: 101,
                dialog_type: "confirm".to_string(),
                message: "Delete items?".to_string(),
                url: "https://app.test".to_string(),
                default_prompt: None,
                opened_at: 1000,
            });
        }

        // Tab 101 now fails guard
        let guard_res = check_dialog_guard(hub_state.clone(), 101).await;
        assert!(guard_res.is_err());
        let err = guard_res.unwrap_err();
        assert!(err.contains("Tab 101 execution is suspended by an active modal dialog"));
        assert!(err.contains("[confirm] 'Delete items?'"));

        // Tab 102 still passes guard
        assert!(check_dialog_guard(hub_state.clone(), 102).await.is_ok());
    }

    #[tokio::test]
    async fn test_get_active_dialog() {
        let hub_state = Arc::new(Mutex::new(HubState::new()));

        let res_empty = get_active_dialog(hub_state.clone(), Some(101)).await.unwrap();
        assert_eq!(res_empty["has_dialog"], false);

        {
            let mut lock = hub_state.lock().await;
            lock.active_dialogs.insert(101, ActiveDialog {
                tab_id: 101,
                dialog_type: "prompt".to_string(),
                message: "Enter discount code:".to_string(),
                url: "https://store.test".to_string(),
                default_prompt: Some("FALL2026".to_string()),
                opened_at: 1000,
            });
        }

        let res_with_dialog = get_active_dialog(hub_state.clone(), Some(101)).await.unwrap();
        assert_eq!(res_with_dialog["has_dialog"], true);
        assert_eq!(res_with_dialog["dialog"]["message"], "Enter discount code:");
        assert_eq!(res_with_dialog["dialog"]["default_prompt"], "FALL2026");
    }
}
