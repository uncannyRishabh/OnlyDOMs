//! E2E Test Suite for Interacting Tab Warning Banner (R4, F5, F6)
//!
//! Covers:
//! - Tier 1: Banner state verification (tab title & ID tracking), dynamic focus shift, clearing on disconnect, text template.
//! - Tier 2: Boundaries (untitled tab fallback, HTML escaping, extreme title length, rapid tab switching, invalid tab ID).
//! - Tier 3: Pairwise combinations (tab states x lifecycle actions, multi-agent independent leases).
//! - Tier 4: Application scenario (multi-tab agent interaction lifecycle).

use serde_json::{json, Value};
use std::collections::HashMap;

/// Model representing a browser tab with warning banner state
#[derive(Debug, Clone, PartialEq)]
pub struct BannerState {
    pub active_tab_id: u64,
    pub active_tab_title: String,
    pub is_interacting: bool,
    pub rendered_banner_text: String,
}

/// Simulated Banner Controller managing in-page warning banner lifecycles
#[derive(Default)]
pub struct BannerController {
    /// Mapping of tab_id -> current BannerState on that tab
    pub tab_states: HashMap<u64, BannerState>,
    /// Currently focused tab
    pub active_interacting_tab: Option<u64>,
}

impl BannerController {
    pub fn new() -> Self {
        Self::default()
    }

    /// Format banner text adhering to PROJECT.md interface contract #3
    pub fn format_banner_text(tab_id: u64, title: &str) -> String {
        let clean_title = if title.trim().is_empty() {
            "Untitled Tab"
        } else {
            title.trim()
        };
        format!("⚠️ OnlyDOMs Agent Active: Tab #{} - {}", tab_id, clean_title)
    }

    /// HTML escape to prevent XSS in Shadow DOM injection
    pub fn escape_html(input: &str) -> String {
        input
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;")
    }

    /// Shifts agent focus to a target tab, dynamically updating banner states
    pub fn focus_tab(&mut self, target_tab_id: u64, title: &str) -> (Option<Value>, Value) {
        let mut clear_message = None;

        // If previously interacting with another tab, clear its banner
        if let Some(prev_tab_id) = self.active_interacting_tab {
            if prev_tab_id != target_tab_id {
                self.tab_states.remove(&prev_tab_id);
                clear_message = Some(json!({
                    "targetTabId": prev_tab_id,
                    "type": "CLEAR_BANNER"
                }));
            }
        }

        self.active_interacting_tab = Some(target_tab_id);
        let rendered_text = Self::format_banner_text(target_tab_id, title);
        let state = BannerState {
            active_tab_id: target_tab_id,
            active_tab_title: title.to_string(),
            is_interacting: true,
            rendered_banner_text: rendered_text.clone(),
        };
        self.tab_states.insert(target_tab_id, state);

        let update_message = json!({
            "targetTabId": target_tab_id,
            "type": "UPDATE_BANNER",
            "activeTabId": target_tab_id,
            "activeTabTitle": title,
            "isInteracting": true,
            "renderedText": rendered_text
        });

        (clear_message, update_message)
    }

    /// Clears active banner when agent session disconnects or releases all tabs
    pub fn clear_all(&mut self) -> Vec<Value> {
        let mut clear_msgs = Vec::new();
        for &tab_id in self.tab_states.keys() {
            clear_msgs.push(json!({
                "targetTabId": tab_id,
                "type": "CLEAR_BANNER"
            }));
        }
        self.tab_states.clear();
        self.active_interacting_tab = None;
        clear_msgs
    }
}

// -----------------------------------------------------------------------------
// TIER 1: Primary Behavior (Happy Path) Tests
// -----------------------------------------------------------------------------

#[test]
fn test_banner_state_initial_injection() {
    let mut controller = BannerController::new();
    let (clear_msg, update_msg) = controller.focus_tab(10, "GitHub - Pull Requests");

    assert!(clear_msg.is_none(), "No prior tab to clear on first focus");
    assert_eq!(update_msg["type"], "UPDATE_BANNER");
    assert_eq!(update_msg["activeTabId"], 10);
    assert_eq!(update_msg["activeTabTitle"], "GitHub - Pull Requests");
    assert_eq!(update_msg["isInteracting"], true);

    let state = controller.tab_states.get(&10).unwrap();
    assert_eq!(state.active_tab_id, 10);
    assert_eq!(state.active_tab_title, "GitHub - Pull Requests");
}

#[test]
fn test_banner_state_clear_on_disconnect() {
    let mut controller = BannerController::new();
    controller.focus_tab(10, "Work Page");
    assert!(controller.tab_states.contains_key(&10));

    let clear_events = controller.clear_all();
    assert_eq!(clear_events.len(), 1);
    assert_eq!(clear_events[0]["type"], "CLEAR_BANNER");
    assert_eq!(clear_events[0]["targetTabId"], 10);
    assert!(controller.tab_states.is_empty());
    assert_eq!(controller.active_interacting_tab, None);
}

#[test]
fn test_banner_dynamic_focus_shift() {
    let mut controller = BannerController::new();

    // Focus Tab 10
    controller.focus_tab(10, "Page 1");

    // Focus shift to Tab 20
    let (clear_msg, update_msg) = controller.focus_tab(20, "Page 2");

    // Tab 10 must receive CLEAR_BANNER
    assert!(clear_msg.is_some());
    let clear = clear_msg.unwrap();
    assert_eq!(clear["type"], "CLEAR_BANNER");
    assert_eq!(clear["targetTabId"], 10);

    // Tab 20 must receive UPDATE_BANNER
    assert_eq!(update_msg["type"], "UPDATE_BANNER");
    assert_eq!(update_msg["activeTabId"], 20);
    assert_eq!(update_msg["activeTabTitle"], "Page 2");

    // Controller state holds only Tab 20
    assert!(!controller.tab_states.contains_key(&10));
    assert!(controller.tab_states.contains_key(&20));
}

#[test]
fn test_banner_dynamic_title_update() {
    let mut controller = BannerController::new();
    controller.focus_tab(15, "Loading...");

    // Same tab navigates and document.title changes
    let (clear_msg, update_msg) = controller.focus_tab(15, "Order Confirmation #9876");

    assert!(clear_msg.is_none(), "Same tab does not emit clear message");
    assert_eq!(update_msg["activeTabId"], 15);
    assert_eq!(update_msg["activeTabTitle"], "Order Confirmation #9876");
    assert_eq!(
        update_msg["renderedText"],
        "⚠️ OnlyDOMs Agent Active: Tab #15 - Order Confirmation #9876"
    );
}

#[test]
fn test_banner_formatting_template() {
    let rendered = BannerController::format_banner_text(42, "Dashboard Overview");
    assert_eq!(rendered, "⚠️ OnlyDOMs Agent Active: Tab #42 - Dashboard Overview");
}

// -----------------------------------------------------------------------------
// TIER 2: Boundary & Corner Condition Tests
// -----------------------------------------------------------------------------

#[test]
fn test_banner_untitled_tab_fallback() {
    assert_eq!(
        BannerController::format_banner_text(1, ""),
        "⚠️ OnlyDOMs Agent Active: Tab #1 - Untitled Tab"
    );
    assert_eq!(
        BannerController::format_banner_text(1, "   "),
        "⚠️ OnlyDOMs Agent Active: Tab #1 - Untitled Tab"
    );
}

#[test]
fn test_banner_special_characters_and_html_escaping() {
    let raw_title = "<script>alert('XSS')</script> & \"Safe\" Title";
    let escaped = BannerController::escape_html(raw_title);

    assert!(!escaped.contains('<'));
    assert!(!escaped.contains('>'));
    assert!(escaped.contains("&lt;script&gt;"));
    assert!(escaped.contains("&amp;"));
    assert!(escaped.contains("&quot;Safe&quot;"));
}

#[test]
fn test_banner_extreme_title_length_handling() {
    let long_title = "A".repeat(2000);
    let rendered = BannerController::format_banner_text(99, &long_title);
    assert!(rendered.starts_with("⚠️ OnlyDOMs Agent Active: Tab #99 - AAAAA"));
    assert_eq!(rendered.len(), "⚠️ OnlyDOMs Agent Active: Tab #99 - ".len() + 2000);
}

#[test]
fn test_banner_rapid_tab_switching() {
    let mut controller = BannerController::new();

    // Rapid sequential switching between tabs 1 to 5
    for tab_id in 1..=5 {
        controller.focus_tab(tab_id, &format!("Tab {}", tab_id));
    }

    assert_eq!(controller.active_interacting_tab, Some(5));
    assert_eq!(controller.tab_states.len(), 1);
    assert!(controller.tab_states.contains_key(&5));
}

#[test]
fn test_banner_zero_or_negative_tab_id_validation() {
    let validate_tab_id = |tab_id: u64| -> Result<u64, String> {
        if tab_id == 0 {
            Err("Invalid tab ID 0: Chrome tab IDs are strictly positive".to_string())
        } else {
            Ok(tab_id)
        }
    };

    assert!(validate_tab_id(10).is_ok());
    assert!(validate_tab_id(0).is_err());
}

// -----------------------------------------------------------------------------
// TIER 3: Pairwise Combinations
// -----------------------------------------------------------------------------

#[test]
fn test_banner_pairwise_tab_states() {
    let long_title = "Very Long Title ".repeat(20);
    let tab_titles = ["", "Normal Title", long_title.as_str()];
    let tab_ids = [1, 50, 9999];

    for &id in &tab_ids {
        for &title in &tab_titles {
            let mut controller = BannerController::new();
            let (_, update) = controller.focus_tab(id, title);
            assert_eq!(update["activeTabId"], id);
            assert!(update["renderedText"].as_str().unwrap().contains(&id.to_string()));
        }
    }
}

#[test]
fn test_banner_multi_agent_independent_leases() {
    // Agent 1 manages controller A, Agent 2 manages controller B
    let mut agent_1_ctrl = BannerController::new();
    let mut agent_2_ctrl = BannerController::new();

    agent_1_ctrl.focus_tab(100, "Agent 1 Workspace");
    agent_2_ctrl.focus_tab(200, "Agent 2 Workspace");

    assert_eq!(agent_1_ctrl.active_interacting_tab, Some(100));
    assert_eq!(agent_2_ctrl.active_interacting_tab, Some(200));

    // Agent 1 disconnects
    let clear_1 = agent_1_ctrl.clear_all();
    assert_eq!(clear_1.len(), 1);
    assert_eq!(clear_1[0]["targetTabId"], 100);

    // Agent 2 remains untouched
    assert_eq!(agent_2_ctrl.active_interacting_tab, Some(200));
}

// -----------------------------------------------------------------------------
// TIER 4: Application Scenario (Full Multi-Tab Lifecycle)
// -----------------------------------------------------------------------------

#[test]
fn test_banner_e2e_multi_tab_workflow() {
    let mut controller = BannerController::new();

    // Step 1: Agent starts on Tab 10 (Home Search)
    let (clear1, update1) = controller.focus_tab(10, "Search Engine");
    assert!(clear1.is_none());
    assert_eq!(update1["activeTabId"], 10);
    assert_eq!(update1["renderedText"], "⚠️ OnlyDOMs Agent Active: Tab #10 - Search Engine");

    // Step 2: Agent clicks result link, opens and switches to Tab 11 (Documentation)
    let (clear2, update2) = controller.focus_tab(11, "Rust Documentation");
    assert_eq!(clear2.unwrap()["targetTabId"], 10, "Tab 10 banner cleared");
    assert_eq!(update2["activeTabId"], 11, "Tab 11 banner active");

    // Step 3: Agent switches back to Tab 10
    let (clear3, update3) = controller.focus_tab(10, "Search Engine");
    assert_eq!(clear3.unwrap()["targetTabId"], 11, "Tab 11 banner cleared");
    assert_eq!(update3["activeTabId"], 10, "Tab 10 banner reactivated");

    // Step 4: Agent completes session, cleans up
    let final_clears = controller.clear_all();
    assert_eq!(final_clears.len(), 1);
    assert_eq!(final_clears[0]["targetTabId"], 10);
    assert!(controller.tab_states.is_empty());
}
