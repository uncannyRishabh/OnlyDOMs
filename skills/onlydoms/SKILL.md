---
name: onlydoms
description: High-performance browser automation MCP server purpose-built for agentic SDLC and autonomous web operations. Interact with live Chrome tabs, inspect sanitized DOM, run compound pipelines, execute automated visual tests, fill forms across modern frameworks, and debug authenticated web apps with radical token efficiency.
---

# Role & Operational Protocol: OnlyDOMs Chrome Automation

You have access to **OnlyDOMs**, a high-performance browser automation MCP server providing browser-wide, multi-tab control of Google Chrome via Chrome DevTools Protocol (CDP) on `127.0.0.1:9222`.

---

## 1. Pre-Flight Health Check & Multi-Chat Architecture

OnlyDOMs uses a **Decoupled Daemon + Multiplexed Client Architecture**:
- A single long-lived daemon owns the WebSocket connection to Google Chrome on `127.0.0.1:9222`.
- When an AI agent starts a conversation, the MCP proxy connects directly to the shared daemon (auto-spawning it if needed).
- Multiple chat windows/agents run concurrently without connection conflicts or dropping browser sessions upon chat termination.

1. **Check Daemon Status**:
   - Check status via:
     `onlydoms status` (or path to compiled binary `mcp-server/target/release/onlydoms status`)
   - **If NOT running**:
     - The MCP proxy automatically auto-spawns the daemon on demand, or start it explicitly with:
       `onlydoms daemon --port 9222` (or `mcp-server/target/release/onlydoms daemon --port 9222`)
   - **If running**: Status outputs active port, extension status, and active tab locks.

2. **Browser-Wide Extension Connection**:
   - The session operates **browser-wide** across all tabs; you do not need to attach tabs one-by-one.
   - If any tool returns `"No active browser session detected"`:
     - Stop immediately and instruct the user:
       > ⚠️ **OnlyDOMs Chrome extension is disconnected.**
       > Please ensure the OnlyDOMs extension icon in Chrome shows 🟢 **Connected** (Port `9222`).

3. **Execution Mode (MCP Tools vs Direct CLI)**:
   - When inside an active MCP environment, invoke the native tools (`execute_browser_pipeline`, `create_tab`, `evaluate_expression`, etc.).
   - If MCP tools are detached or operating from terminal, use first-class CLI commands:
     - Open URL: `onlydoms open <url>`
     - List tabs: `onlydoms tabs`
     - Evaluate JS: `onlydoms eval "<expr>"`
     - Run pipeline: `onlydoms pipeline '<steps_json>'`
     *(Never write ad-hoc Python or shell wrappers)*

---

## 2. Multi-Agent Tab Isolation & Lock Protection

OnlyDOMs guarantees **strict tab isolation** between concurrent agents:
1. **Exclusive Tab Locks**:
   - When an agent works on a tab (via pipeline, navigation, input, testing, or selection), that tab is exclusively leased to that agent.
   - Another agent **cannot intercept or execute commands on the same tab**. Any attempt by a second agent to switch to a locked tab will return a collision-prevention error.
2. **Concurrent Agents Work in Separate Tabs**:
   - If Agent 1 is working on Tab A, Agent 2 automatically starts or resumes work in an unreserved tab or automatically provisions a new tab (`create_tab`).
   - `list_browser_sessions` shows `is_locked`, `locked_by_current_agent`, and `locked_by_other_agent` so agents can identify available vs. busy tabs.
3. **Automatic Lock Cleanup**:
   - When an agent finishes or closes its chat session, its tab locks are automatically freed for other agents or the user.

---

## 3. Intelligent Tab Management (New Tab vs In-Session Navigation)

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

## 4. Resilient Batch Operations (Pipeline Protocol)

**Always prefer `execute_browser_pipeline`** over individual atomic tool calls. It batches multi-step operations into a single turn with built-in awaits, auto-scrolling, semantic targeting, and retry resilience. Target a **1 to 2 compound turn ceiling** for standard workflows.

### Pipeline Schema:
```json
{
  "tab_id": 12345, // optional (defaults to agent's assigned or clean tab)
  "steps": [
    { "action": "navigate", "url": "https://example.com" },
    { "action": "wait_for", "selector": "#btn" },
    { "action": "click", "text": "Submit" }
  ]
}
```
*(Note: `"steps"` is the primary parameter; `"actions"` is also supported as an alias).*

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
  - Composite: `{ "action": "click", "selector": "div[role='button']", "text": "Submit" }`
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

## 5. Automated Testing Engine (`run_browser_test`)

For defect or feature testing, use `run_browser_test`. Provide a plain text description of what to test or explicit steps. The engine decomposes descriptions into small steps, executes each, captures screenshots into a dedicated directory (`test_reports/<test_name>_<timestamp>/`), and outputs both `test_summary.md` and `test_report.json`.

---

## 6. Token Economy & Exploration Rules

1. **Zero Schema & Source Exploration**: Do NOT inspect `.json` schema files or local codebase files (`.rs`, `.js`). All schemas and actions are fully specified above.
2. **Compound Batching Ceiling**: Standard tasks (e.g. open site, scroll, click, fill, send) MUST be dispatched in a single `execute_browser_pipeline` call rather than separate turn-by-turn micro-steps.
3. **Anti-Bloat DOM Queries**: Never execute `document.querySelectorAll('*')` or dump full container `innerText`/`innerHTML` in `evaluate_expression`. Queries must be targeted (e.g. specific IDs, attributes, or scoped selectors).
4. **Compact Tab Format**: `list_browser_sessions` returns lightweight objects `{ id, title, url, active, is_locked, locked_by_current_agent, locked_by_other_agent }` with `http://`, `https://`, and query parameters automatically stripped in the Rust layer. Only request `"detailed": true` if low-level Chromium debug flags are strictly required.
5. **Screenshot Discipline**: Use `take_screenshot` with `save_path` (or `run_browser_test`) when visual verification is needed. This avoids polluting context tokens with giant base64 image strings.
6. **DOM Chain Traversal & Evaluation**: When building element ancestry or parent chains in `evaluate_expression`, always include a `textContent` property (e.g. `textContent: (el.innerText || el.textContent || '').trim()`). OnlyDOMs's Rust layer automatically prunes all `null` attributes, strips inline CSS styling (`style="..."`), and truncates `textContent` inside `chain` arrays to 100 characters to prevent prompt bloat while preserving full `textContent` for top-level queries.
