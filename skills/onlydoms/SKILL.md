---
name: onlydoms
description: High-performance Chrome browser automation, DOM inspection, optimistic cURL network replay, and visual PDF test reporting via OnlyDOMs MCP and CLI. Use whenever the user asks to automate Chrome, open or interact with websites, inspect live DOM, fill forms, run automated browser tests, generate HTML or PDF test overview reports, or capture and replay network cURL requests.
---

# Role & Operational Protocol: OnlyDOMs Chrome Automation

You have access to **OnlyDOMs**, a high-performance browser automation MCP server providing browser-wide, multi-tab control of Google Chrome via Chrome DevTools Protocol (CDP) on `127.0.0.1:9222`.

## 1. Pre-Flight Health Check & Multi-Chat Architecture

OnlyDOMs uses a **Decoupled Daemon + Multiplexed Client Architecture**:

- A single long-lived daemon owns the WebSocket connection to Google Chrome on `127.0.0.1:9222` and serves visual test reports on `http://127.0.0.1:9222/reports/<run_id>/`.
- When an AI agent starts a conversation, the MCP proxy connects directly to the shared daemon (auto-spawning it if needed).
- Multiple chat windows and agents run concurrently without connection conflicts or dropping browser sessions upon chat termination.

1. **Check Daemon Status**:

   - Check status via `onlydoms status` (or `mcp-server/target/release/onlydoms status`).
   - If NOT running, the MCP proxy auto-spawns the daemon on demand, or start it explicitly with `onlydoms daemon --port 9222`.
   - If running, status outputs active port, extension status, and active tab locks.

2. **Browser-Wide Extension Connection**:

   - The session operates browser-wide across all tabs; you do not need to attach tabs one-by-one.
   - If any tool returns `"No active browser session detected"`, stop immediately and instruct the user:
     > ⚠️ **OnlyDOMs Chrome extension is disconnected.**
     > Please ensure the OnlyDOMs extension icon in Chrome shows 🟢 **Connected** (Port `9222`).

3. **Execution Mode (MCP Tools vs Direct CLI)**:

   - When inside an active MCP environment, invoke the native tools (`execute_browser_pipeline`, `run_browser_test`, `replay_curl`, `print_to_pdf`, `create_tab`, `evaluate_expression`, etc.).
   - If MCP tools are detached or operating from terminal, use first-class CLI commands:
     - Open URL: `onlydoms open <url>`
     - List tabs: `onlydoms tabs`
     - Evaluate JS: `onlydoms eval "<expr>"`
     - Run pipeline: `onlydoms pipeline '<steps_json>'`

## 2. Multi-Agent Tab Isolation & Lock Protection

OnlyDOMs guarantees strict tab isolation between concurrent agents:

1. **Exclusive Tab Locks**:

   - When an agent works on a tab (via pipeline, navigation, input, testing, or selection), that tab is exclusively leased to that agent.
   - Another agent cannot intercept or execute commands on the same tab.

2. **Concurrent Agents Work in Separate Tabs**:

   - If Agent 1 is working on Tab A, Agent 2 automatically starts or resumes work in an unreserved tab or provisions a new tab (`create_tab`).
   - `list_browser_sessions` shows `is_locked`, `locked_by_current_agent`, and `locked_by_other_agent`.

3. **Automatic Lock Cleanup**:

   - When an agent finishes or closes its chat session, its tab locks are automatically freed.

## 3. Intelligent Tab Management (New Tab vs In-Session Navigation)

Before navigating to an external URL, inspect the active tab's URL:

1. **New Tab Case (Preserve User Work)**:

   - If the active tab has an active user webpage (e.g. `docs.google.com`, `github.com`) and the user's intent is to open a new website or independent task, do NOT overwrite the user's active page. Call `create_tab` (or the `new_tab` pipeline action) to perform the action in a fresh tab.

2. **Clean Slate Case**:

   - If the active tab is `chrome://newtab/`, `about:blank`, or an empty tab, execute the request directly in the current tab.

3. **In-Session Navigation Case**:

   - If the user's intent is to navigate within the existing website, flow, or task (e.g. clicking settings, going to profile, proceeding to checkout), perform the action in the current tab to maintain context and login state.

## 4. Resilient Batch Operations (Pipeline Protocol) & Task Intent Preview

**Task Intent Preview Rule (180-Char / 30-Word Ceiling)**: Before invoking any MCP tool, always output a single one-line preview in chat (`> 🎯 **Intent:** <up to 30 words / 180 chars>`) AND pass the exact same string in the `"_intent"` parameter of the tool call so the Chrome Floating Pill displays your intent in real time with zero extra tool turns.

**Always prefer `execute_browser_pipeline`** over individual atomic tool calls. It batches multi-step operations into a single turn with built-in awaits, auto-scrolling, semantic targeting, live HUD command ticker updates (`Executing :: <step.action>(<target>) on Tab :: #<tab_id> (<tab_title>)`), and retry resilience. Target a **1 to 2 compound turn ceiling** for standard workflows.

### Pipeline Schema:
```json
{
  "_intent": "Submit checkout form with test Visa card on staging portal and verify 200 OK order creation response",
  "tab_id": 12345,
  "steps": [
    { "action": "navigate", "url": "https://example.com" },
    { "action": "wait_for", "selector": "#btn" },
    { "action": "click", "text": "Submit" }
  ]
}
```

### Supported Pipeline Actions:

- `navigate`: `{ "action": "navigate", "url": "https://..." }`
- `new_tab`: `{ "action": "new_tab", "url": "https://..." }`
- `switch_tab`: `{ "action": "switch_tab", "tab_id": 12345 }`
- `wait_for`: `{ "action": "wait_for", "selector": "#btn", "timeout_ms": 8000 }`
- `wait_condition`: `{ "action": "wait_condition", "expression": "document.readyState === 'complete'", "timeout_ms": 8000 }`
- `wait_network_idle`: `{ "action": "wait_network_idle", "timeout_ms": 5000 }`
- `click`: Semantic or quadrant click with auto-scroll, retry, and HUD hit-test passthrough:
  - By text: `{ "action": "click", "text": "Send" }`
  - By aria-label: `{ "action": "click", "aria_label": "Share" }`
  - By CSS selector: `{ "action": "click", "selector": "button[type='submit']" }`
  - By 16-column grid quadrant: `{ "action": "click", "quadrant": "P9" }`
- `scroll`: `{ "action": "scroll", "direction": "down", "delta_y": 700 }`
- `evaluate`: `{ "action": "evaluate", "expression": "window.location.href" }`
- `fill_inputs`: `{ "action": "fill_inputs", "fields": [{ "selector": "input[name='q']", "value": "search query" }] }`
- `press_key`: `{ "action": "press_key", "key": "Enter" }`
- `sleep`: `{ "action": "sleep", "duration_ms": 500 }`
- `take_screenshot`: `{ "action": "take_screenshot", "full_page": false, "format": "webp" }`
- `print_to_pdf`: `{ "action": "print_to_pdf", "format": "a4", "save_path": "report.pdf" }`
- `handle_dialog`: `{ "action": "handle_dialog", "accept": true, "prompt_text": "optional" }`
- `replay_curl`: `{ "action": "replay_curl", "request_id": "req_1", "headers": { "X-Debug": "1" } }`

## 5. Automated Testing, Visual PDF Pipeline (`run_browser_test`) & cURL Replay (`replay_curl`)

1. **Visual Test & Feature Overview Document Pipeline (`run_browser_test`)**:

   - Decomposes test descriptions or step arrays into deterministic browser steps.
   - Saves each step's cloaked compositor screenshot (`webp`, `quality: 75`) directly to `test_reports/<run_id>/assets/step_<n>.webp`.
   - Compiles step intents (`_intent`), executed commands, and correlated optimistic cURL snippets into `test_reports/<run_id>/overview.html` and silently generates `test_reports/<run_id>/overview.pdf`.
   - Serves openable and auto-print links on port `9222`:
     - HTML Report: `http://127.0.0.1:9222/reports/<run_id>/overview.html`
     - PDF Document: `http://127.0.0.1:9222/reports/<run_id>/overview.pdf`
     - Instant Print Dialog Link: `http://127.0.0.1:9222/reports/<run_id>/overview.html?print=1`

2. **Optimistic Network Capture & Zero-Subprocess Rust Replay (`replay_curl`)**:

   - OnlyDOMs captures XHR, Fetch, and Document calls occurring within 3000ms of an agent step in a 50-item per-tab ring buffer (`< 2MB` ceiling).
   - Call `replay_curl` with `action: "list"` to inspect captured network requests, or pass `request_id` / a modified `curl` string (with optional `headers`, `body`, `method` overrides) to replay requests natively in Rust or via in-tab `fetch` without spawning a shell.

## 6. Token Economy, Blindspot Sanitization & Exploration Rules

1. **Zero Schema & Source Exploration**: Do NOT inspect `.json` schema files or local codebase files (`.rs`, `.js`) when running browser tasks.
2. **Compound Batching Ceiling**: Dispatch multi-step workflows in a single `execute_browser_pipeline` call with `"_intent"`.
3. **Anti-Bloat DOM Queries**: Never execute `document.querySelectorAll('*')` or dump full container `innerText`/`innerHTML` in `evaluate_expression`.
4. **Compact Tab Format**: `list_browser_sessions` strips `http://`, `https://`, and query parameters in Rust.
5. **Screenshot & Report Discipline**: Use `take_screenshot` with `save_path`, `print_to_pdf`, or `run_browser_test` to avoid polluting context tokens with base64 blobs.
6. **Multi-Layer HUD Blindspot Guarantee**: The Rust `hud_sanitizer` automatically strips `<mcp-agent-hud>`, `#__mcp_agent_hud_host__`, and `data-mcp-internal-ignore="true"` from all responses, cloaks the HUD during screenshots/PDFs, and passes `P9` clicks through to the underlying page.
