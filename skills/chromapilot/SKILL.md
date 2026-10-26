---
name: chromapilot
description: Efficient browser automation and Chrome DevTools Protocol controller using the ChromaPilot MCP server and companion Chrome extension. Use for interacting with live browser tabs, inspecting DOM, capturing clean viewport and full-page screenshots, running automated browser tests from descriptions, filling forms safely across frameworks, evaluating JavaScript, and debugging.
---

# Role & Operational Protocol: ChromaPilot Chrome Automation

You have access to **ChromaPilot**, a high-performance browser automation MCP server providing browser-wide, multi-tab control of Google Chrome via Chrome DevTools Protocol (CDP) on `127.0.0.1:9222`.

---

## 1. Pre-Flight Health Check

Before issuing any browser commands:

1. **Check Daemon Status**:
   - Check if the ChromaPilot daemon is listening on port `9222` (e.g., via `lsof -i :9222`).
   - **If NOT running**:
     - Attempt to start the daemon in the background:
       `./mcp-server/target/release/chromapilot --port 9222`
     - If restricted by permissions or sandbox, inform the user:
       > ⚠️ **ChromaPilot daemon is not running.** Please start it with:
       > ```bash
       > ./mcp-server/target/release/chromapilot --port 9222
       > ```
   - **If running**: Inform the user: *"ChromaPilot daemon is active on port 9222."*

2. **Browser-Wide Extension Connection**:
   - The session operates **browser-wide** across all tabs; you do not need to attach tabs one-by-one.
   - If any tool returns `"No active browser session detected"`:
     - Stop immediately and instruct the user:
       > ⚠️ **ChromaPilot Chrome extension is disconnected.**
       > Please ensure the ChromaPilot extension icon in Chrome shows 🟢 **Connected** (Port `9222`).

---

## 2. Intelligent Tab Management (New Tab vs In-Session Navigation)

Before navigating to an external URL, inspect the active tab's URL:

1. **New Tab Case (Preserve User Work)**:
   - If the active tab has an active user webpage (e.g. `docs.google.com`, `github.com`, etc.) and the user's intent is to open a **new website or independent task** (e.g., *"go to instagram"*, *"open youtube"*, *"search google for X"*):
   - **Do NOT overwrite the user's active page.** Call `create_tab` (or the `new_tab` pipeline action) to perform the action in a fresh tab.

2. **Clean Slate Case**:
   - If the active tab is `chrome://newtab/`, `about:blank`, or an empty tab:
   - Execute the request directly in the current tab.

3. **In-Session Navigation Case**:
   - If the user's intent is to navigate within the existing website, flow, or task (e.g., *"click settings"*, *"go to my profile"*, *"navigate back"*, *"proceed to checkout"*):
   - Always perform the action in the **current tab**, maintaining context and login state.

---

## 3. Resilient Batch Operations (Pipeline Protocol)

**Always prefer `execute_browser_pipeline`** over individual atomic tool calls. It batches multi-step operations into a single turn with built-in awaits, auto-scrolling, semantic targeting, and retry resilience. Target a **1 to 2 compound turn ceiling** for standard workflows.

### Supported Pipeline Actions:
- `navigate`: `{ "action": "navigate", "url": "https://..." }`
- `new_tab`: `{ "action": "new_tab", "url": "https://..." }` (opens new tab & automatically attaches)
- `switch_tab`: `{ "action": "switch_tab", "tab_id": 12345 }`
- `wait_for`: `{ "action": "wait_for", "selector": "#btn", "timeout_ms": 8000 }` (polls until element exists and is visible)
- `wait_condition`: `{ "action": "wait_condition", "expression": "document.readyState === 'complete'", "timeout_ms": 8000 }`
- `wait_network_idle`: `{ "action": "wait_network_idle", "timeout_ms": 5000 }`
- `click`: Semantic click with auto-scroll and retry:
  - By text: `{ "action": "click", "text": "Send" }`
  - By aria-label: `{ "action": "click", "aria_label": "Share" }`
  - By CSS selector: `{ "action": "click", "selector": "button[type='submit']" }`
  - Composite: `{ "action": "click", "selector": "div[role='button']", "text": "Ayush Singh" }`
- `scroll`: Auto-detects scrollable container or scrolls window:
  - `{ "action": "scroll", "direction": "down", "delta_y": 700 }`
  - `{ "action": "scroll", "selector": ".feed-container", "delta_y": 500 }`
- `evaluate`: In-pipeline JavaScript snippet execution:
  - `{ "action": "evaluate", "expression": "window.location.href" }`
- `fill_inputs`: `{ "action": "fill_inputs", "fields": [{ "selector": "input[name='q']", "value": "search query" }] }` (framework-safe for React, Vue, Angular)
- `press_key`: `{ "action": "press_key", "key": "Enter" }` (supports Enter, Tab, Escape, ArrowDown, etc.)
- `sleep`: `{ "action": "sleep", "duration_ms": 500 }`
- `take_screenshot`: `{ "action": "take_screenshot", "full_page": false, "format": "png" }` (clean compositor capture; optional `save_path`)

---

## 4. Automated Testing Engine (`run_browser_test`)

For defect or feature testing, use `run_browser_test`. Provide a plain text description of what to test or explicit steps. The engine decomposes descriptions into small steps, executes each, captures screenshots into a dedicated directory (`test_reports/<test_name>_<timestamp>/`), and outputs both `test_summary.md` and `test_report.json`.

---

## 5. Token Economy & Exploration Rules

1. **Zero Schema & Source Exploration**: Do NOT inspect `.json` schema files or local codebase files (`.rs`, `.js`). All schemas and actions are fully specified above.
2. **Compound Batching Ceiling**: Standard tasks (e.g. open site, scroll, click, fill, send) MUST be dispatched in a single `execute_browser_pipeline` call rather than separate turn-by-turn micro-steps.
3. **Anti-Bloat DOM Queries**: Never execute `document.querySelectorAll('*')` or dump full container `innerText`/`innerHTML` in `evaluate_expression`. Queries must be targeted (e.g. specific IDs, attributes, or scoped selectors).
4. **Compact Tab Format**: `list_browser_sessions` returns lightweight objects `{ id, title, url, active }`. Only request `"detailed": true` if low-level Chromium debug flags are strictly required.
5. **Screenshot Discipline**: Use `take_screenshot` with `save_path` (or `run_browser_test`) when visual verification is needed. This avoids polluting context tokens with giant base64 image strings.
