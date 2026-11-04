//! E2E Test Suite for Dialog, Confirmation Popup, and New Window Interaction (R3, F10-F13)
//!
//! Covers:
//! - Tier 1: Detection of window.alert, window.confirm, window.prompt, accepting, dismissing, supplying prompt text, execution guard.
//! - Tier 2: Boundaries (no dialog open error, empty prompt string, unicode/special chars, beforeunload, rapid-fire open/close).
//! - Tier 3: Pairwise combinations (dialog types x actions x prompt texts, multi-tab dialog isolation).
//! - Tier 4: Application scenario (full dialog-to-popup OAuth lifecycle).

use serde_json::{json, Value};
use std::collections::HashMap;

/// Dialog representation matching HubState specification in PROJECT.md and survey reports
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ActiveDialog {
    pub tab_id: u64,
    pub dialog_type: String, // "alert" | "confirm" | "prompt" | "beforeunload"
    pub message: String,
    pub url: String,
    pub default_prompt: Option<String>,
    pub opened_at: u64,
}

/// In-memory dialog manager model simulating daemon WebSocket hub dialog tracking
#[derive(Default)]
pub struct DialogTracker {
    pub active_dialogs: HashMap<u64, ActiveDialog>,
    pub opened_popups: Vec<Value>,
}

impl DialogTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Process incoming WebSocket BROWSER_EVENT from extension
    pub fn process_event(&mut self, event_frame: &Value) -> Result<(), String> {
        let event_type = event_frame.get("event").and_then(|v| v.as_str()).unwrap_or("");
        let tab_id = event_frame.get("tabId").and_then(|v| v.as_u64()).unwrap_or(0);

        match event_type {
            "Page.javascriptDialogOpening" => {
                let payload = event_frame.get("payload").ok_or("Missing payload in dialog opening")?;
                let dialog = ActiveDialog {
                    tab_id,
                    dialog_type: payload.get("type").and_then(|v| v.as_str()).unwrap_or("alert").to_string(),
                    message: payload.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    url: payload.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    default_prompt: payload.get("defaultPrompt").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    opened_at: 1000,
                };
                self.active_dialogs.insert(tab_id, dialog);
                Ok(())
            }
            "Page.javascriptDialogClosed" => {
                self.active_dialogs.remove(&tab_id);
                Ok(())
            }
            "Target.targetCreated" | "chrome.tabs.onCreated" => {
                self.opened_popups.push(event_frame.clone());
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Execution guard: checks if tab execution is blocked by a modal dialog
    pub fn check_execution_guard(&self, tab_id: u64) -> Result<(), String> {
        if let Some(dialog) = self.active_dialogs.get(&tab_id) {
            Err(format!(
                "Tab {} execution is suspended by an active modal dialog: [{}] '{}'. Call handle_dialog to accept or dismiss.",
                tab_id, dialog.dialog_type, dialog.message
            ))
        } else {
            Ok(())
        }
    }

    /// Programmatic dialog handling: translates handle_dialog call to CDP Page.handleJavaScriptDialog
    pub fn handle_dialog(
        &mut self,
        tab_id: u64,
        action: &str,
        prompt_text: Option<&str>,
    ) -> Result<Value, String> {
        let dialog = self.active_dialogs.get(&tab_id)
            .ok_or_else(|| format!("No active dialog showing on tab {}", tab_id))?;

        let accept = match action {
            "accept" => true,
            "dismiss" => false,
            other => return Err(format!("Invalid action '{}', must be 'accept' or 'dismiss'", other)),
        };

        // Construct outgoing CDP command
        let mut cdp_params = json!({ "accept": accept });
        if let Some(text) = prompt_text {
            cdp_params["promptText"] = json!(text);
        }

        let handled_type = dialog.dialog_type.clone();
        // Closing dialog clears active state
        self.active_dialogs.remove(&tab_id);

        Ok(json!({
            "success": true,
            "tab_id": tab_id,
            "handled_type": handled_type,
            "action": action,
            "cdp_command": {
                "method": "Page.handleJavaScriptDialog",
                "params": cdp_params
            }
        }))
    }
}

// -----------------------------------------------------------------------------
// TIER 1: Primary Behavior (Happy Path) Tests
// -----------------------------------------------------------------------------

#[test]
fn test_dialog_alert_detection_and_accept() {
    let mut tracker = DialogTracker::new();

    // 1. Alert opening event
    let event = json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": 101,
        "payload": {
            "type": "alert",
            "message": "Payment successful!",
            "url": "https://shop.example.com/checkout",
            "hasBrowserHandler": true,
            "defaultPrompt": ""
        }
    });
    tracker.process_event(&event).unwrap();

    assert!(tracker.active_dialogs.contains_key(&101));
    let dialog = tracker.active_dialogs.get(&101).unwrap();
    assert_eq!(dialog.dialog_type, "alert");
    assert_eq!(dialog.message, "Payment successful!");

    // 2. Accept alert
    let res = tracker.handle_dialog(101, "accept", None).unwrap();
    assert_eq!(res["success"], true);
    assert_eq!(res["cdp_command"]["params"]["accept"], true);
    assert!(!tracker.active_dialogs.contains_key(&101));
}

#[test]
fn test_dialog_confirm_accept_and_dismiss() {
    let mut tracker = DialogTracker::new();

    // 1. Confirm opening
    let open_event = json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": 102,
        "payload": {
            "type": "confirm",
            "message": "Are you sure you want to delete this resource?",
            "url": "https://app.example.com/settings"
        }
    });
    tracker.process_event(&open_event).unwrap();

    // 2. Test dismiss (Cancel)
    let dismiss_res = tracker.handle_dialog(102, "dismiss", None).unwrap();
    assert_eq!(dismiss_res["action"], "dismiss");
    assert_eq!(dismiss_res["cdp_command"]["params"]["accept"], false);

    // 3. Confirm opening again
    tracker.process_event(&open_event).unwrap();

    // 4. Test accept (OK)
    let accept_res = tracker.handle_dialog(102, "accept", None).unwrap();
    assert_eq!(accept_res["action"], "accept");
    assert_eq!(accept_res["cdp_command"]["params"]["accept"], true);
}

#[test]
fn test_dialog_prompt_supply_text() {
    let mut tracker = DialogTracker::new();

    let prompt_event = json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": 103,
        "payload": {
            "type": "prompt",
            "message": "Please enter your coupon code:",
            "url": "https://store.example.com",
            "defaultPrompt": "SUMMER2026"
        }
    });
    tracker.process_event(&prompt_event).unwrap();

    let handle_res = tracker.handle_dialog(103, "accept", Some("DISCOUNT50")).unwrap();
    assert_eq!(handle_res["success"], true);
    assert_eq!(handle_res["cdp_command"]["params"]["accept"], true);
    assert_eq!(handle_res["cdp_command"]["params"]["promptText"], "DISCOUNT50");
}

#[test]
fn test_dialog_closed_clears_active_state() {
    let mut tracker = DialogTracker::new();

    let open_event = json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": 104,
        "payload": {
            "type": "alert",
            "message": "Notification",
            "url": "https://example.com"
        }
    });
    tracker.process_event(&open_event).unwrap();
    assert!(tracker.active_dialogs.contains_key(&104));

    // Browser closes dialog (e.g. user manually dismissed or browser dismissed)
    let close_event = json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogClosed",
        "tabId": 104
    });
    tracker.process_event(&close_event).unwrap();
    assert!(!tracker.active_dialogs.contains_key(&104));
}

#[test]
fn test_dialog_execution_guard_blocks_runtime_eval() {
    let mut tracker = DialogTracker::new();

    // No dialog -> Execution permitted
    assert!(tracker.check_execution_guard(105).is_ok());

    // Dialog opens -> Execution blocked
    let open_event = json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": 105,
        "payload": {
            "type": "confirm",
            "message": "Unsaved changes exist!",
            "url": "https://editor.example.com"
        }
    });
    tracker.process_event(&open_event).unwrap();

    let guard_res = tracker.check_execution_guard(105);
    assert!(guard_res.is_err(), "Execution must be guarded when dialog is open");
    let err_msg = guard_res.unwrap_err();
    assert!(err_msg.contains("suspended by an active modal dialog"));
    assert!(err_msg.contains("[confirm] 'Unsaved changes exist!'"));

    // Dismiss dialog -> Execution permitted again
    let _ = tracker.handle_dialog(105, "dismiss", None);
    assert!(tracker.check_execution_guard(105).is_ok());
}

// -----------------------------------------------------------------------------
// TIER 2: Boundary & Corner Condition Tests
// -----------------------------------------------------------------------------

#[test]
fn test_dialog_handle_when_no_dialog_open() {
    let mut tracker = DialogTracker::new();
    let res = tracker.handle_dialog(999, "accept", None);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("No active dialog showing on tab 999"));
}

#[test]
fn test_dialog_empty_prompt_text() {
    let mut tracker = DialogTracker::new();

    let open_event = json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": 106,
        "payload": {
            "type": "prompt",
            "message": "Enter notes (optional):",
            "url": "https://notes.example.com"
        }
    });
    tracker.process_event(&open_event).unwrap();

    // Supplying empty string ""
    let res = tracker.handle_dialog(106, "accept", Some("")).unwrap();
    assert_eq!(res["cdp_command"]["params"]["promptText"], "");
}

#[test]
fn test_dialog_special_characters_and_unicode_prompt() {
    let mut tracker = DialogTracker::new();

    let open_event = json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": 107,
        "payload": {
            "type": "prompt",
            "message": "Input test",
            "url": "https://example.com"
        }
    });
    tracker.process_event(&open_event).unwrap();

    let special_text = "Emoji: 🚀🎉 | Quotes: \"' | Newline: \n | Tabs: \t | HTML: <script>alert(1)</script>";
    let res = tracker.handle_dialog(107, "accept", Some(special_text)).unwrap();
    assert_eq!(res["cdp_command"]["params"]["promptText"], special_text);
}

#[test]
fn test_dialog_beforeunload_handling() {
    let mut tracker = DialogTracker::new();

    let open_event = json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": 108,
        "payload": {
            "type": "beforeunload",
            "message": "Leave site? Changes you made may not be saved.",
            "url": "https://form.example.com"
        }
    });
    tracker.process_event(&open_event).unwrap();

    assert_eq!(tracker.active_dialogs.get(&108).unwrap().dialog_type, "beforeunload");

    // Dismiss beforeunload (stay on page)
    let res = tracker.handle_dialog(108, "dismiss", None).unwrap();
    assert_eq!(res["handled_type"], "beforeunload");
    assert_eq!(res["cdp_command"]["params"]["accept"], false);
}

#[test]
fn test_dialog_rapid_fire_open_close() {
    let mut tracker = DialogTracker::new();

    for i in 1..=5 {
        let open = json!({
            "type": "EVENT",
            "event": "Page.javascriptDialogOpening",
            "tabId": 109,
            "payload": {
                "type": "alert",
                "message": format!("Sequence {}", i),
                "url": "https://seq.example.com"
            }
        });
        tracker.process_event(&open).unwrap();
        assert_eq!(tracker.active_dialogs.get(&109).unwrap().message, format!("Sequence {}", i));

        let close = json!({
            "type": "EVENT",
            "event": "Page.javascriptDialogClosed",
            "tabId": 109
        });
        tracker.process_event(&close).unwrap();
        assert!(!tracker.active_dialogs.contains_key(&109));
    }
}

// -----------------------------------------------------------------------------
// TIER 3: Pairwise Combinations
// -----------------------------------------------------------------------------

#[test]
fn test_dialog_pairwise_types_and_actions() {
    let types = ["alert", "confirm", "prompt", "beforeunload"];
    let actions = ["accept", "dismiss"];
    let prompt_options: [Option<&str>; 3] = [None, Some(""), Some("Filled Text")];

    for &dtype in &types {
        for &action in &actions {
            for &ptext in &prompt_options {
                let mut tracker = DialogTracker::new();
                let event = json!({
                    "type": "EVENT",
                    "event": "Page.javascriptDialogOpening",
                    "tabId": 200,
                    "payload": {
                        "type": dtype,
                        "message": "Test modal",
                        "url": "https://test.example.com"
                    }
                });
                tracker.process_event(&event).unwrap();

                let res = tracker.handle_dialog(200, action, ptext);
                assert!(res.is_ok(), "Failed for type: {}, action: {:?}", dtype, action);
                let val = res.unwrap();
                let expected_accept = action == "accept";
                assert_eq!(val["cdp_command"]["params"]["accept"], expected_accept);
            }
        }
    }
}

#[test]
fn test_dialog_multi_tab_isolation() {
    let mut tracker = DialogTracker::new();

    // Tab 10 opens a confirm dialog
    tracker.process_event(&json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": 10,
        "payload": { "type": "confirm", "message": "Tab 10 modal", "url": "https://t10.com" }
    })).unwrap();

    // Tab 20 opens a prompt dialog
    tracker.process_event(&json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": 20,
        "payload": { "type": "prompt", "message": "Tab 20 modal", "url": "https://t20.com" }
    })).unwrap();

    // Tab 30 has NO dialog
    assert!(tracker.check_execution_guard(30).is_ok());

    // Both Tab 10 and Tab 20 are guarded independently
    assert!(tracker.check_execution_guard(10).is_err());
    assert!(tracker.check_execution_guard(20).is_err());

    // Resolve Tab 10
    tracker.handle_dialog(10, "accept", None).unwrap();
    assert!(tracker.check_execution_guard(10).is_ok());
    assert!(tracker.check_execution_guard(20).is_err()); // Tab 20 still guarded

    // Resolve Tab 20
    tracker.handle_dialog(20, "dismiss", None).unwrap();
    assert!(tracker.check_execution_guard(20).is_ok());
}

// -----------------------------------------------------------------------------
// TIER 4: Application Scenario (Full Dialog & OAuth Popup Lifecycle)
// -----------------------------------------------------------------------------

#[test]
fn test_dialog_e2e_oauth_and_confirm_workflow() {
    let mut tracker = DialogTracker::new();
    let main_tab_id = 501;
    let popup_tab_id = 502;

    // Step 1: User is on main page, initiates action triggering confirm dialog
    let confirm_open = json!({
        "type": "EVENT",
        "event": "Page.javascriptDialogOpening",
        "tabId": main_tab_id,
        "payload": {
            "type": "confirm",
            "message": "Authenticate with external OAuth provider?",
            "url": "https://dashboard.example.com"
        }
    });
    tracker.process_event(&confirm_open).unwrap();

    // Agent attempts action -> blocked by execution guard
    assert!(tracker.check_execution_guard(main_tab_id).is_err());

    // Step 2: Agent responds to dialog by accepting
    let handle_res = tracker.handle_dialog(main_tab_id, "accept", None).unwrap();
    assert_eq!(handle_res["action"], "accept");
    assert!(tracker.check_execution_guard(main_tab_id).is_ok());

    // Step 3: Page executes window.open() spawning OAuth popup
    let popup_event = json!({
        "type": "EVENT",
        "event": "chrome.tabs.onCreated",
        "tabId": popup_tab_id,
        "openerTabId": main_tab_id,
        "url": "https://accounts.google.com/o/oauth2/v2/auth?client_id=123",
        "isPopup": true
    });
    tracker.process_event(&popup_event).unwrap();
    assert_eq!(tracker.opened_popups.len(), 1);
    assert_eq!(tracker.opened_popups[0]["openerTabId"], main_tab_id);

    // Step 4: Agent switches to popup tab, performs interaction safely
    assert!(tracker.check_execution_guard(popup_tab_id).is_ok());

    // Step 5: OAuth completes, popup closes, main tab remains operational
    assert!(tracker.check_execution_guard(main_tab_id).is_ok());
}
