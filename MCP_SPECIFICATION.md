# OnlyDOMs MCP Tool & Backend Architecture Specification

Complete reference for the Model Context Protocol (MCP) tools, Task Intent Preview protocol, Optimistic Network Capture & Zero-Subprocess Rust cURL Replay Engine, Visual Test Overview Pipeline, and Multi-Layer Agent Blindspot Sanitizer provided by OnlyDOMs.

---

## 1. Global Schema Contract: Task Intent Preview (`_intent`) & Live Command Broadcast

### Mandatory `_intent` Property (180 Chars / ~30 Words)
Every MCP tool input schema in `mcp-server/src/mcp_protocol.rs` accepts the `_intent` parameter:
```json
"_intent": {
  "type": "string",
  "maxLength": 180,
  "description": "Mandatory one-liner task intent preview (max 180 chars / ~30 words) describing what this tool call is about to achieve and on which tab."
}
```

### Pre-Flight Instant Broadcast (`HUD_STATE_UPDATE`)
The microsecond `mcp_protocol.rs` parses a `tools/call` request—before awaiting the first CDP action—it updates the daemon's `AgentTaskState` store (25-item RPC telemetry ring buffer per tab) and dispatches a `HUD_STATE_UPDATE` frame to `background.js`:
```json
{
  "type": "HUD_STATE_UPDATE",
  "tab_id": 142,
  "intent": "Submit checkout form with test Visa card on staging portal and verify 200 OK order creation response",
  "executing_cmd": "Executing :: fill_inputs (3 fields) on Tab :: #142 (Checkout)",
  "status": "running"
}
```
For multi-step `execute_browser_pipeline` and `run_browser_test` calls, each step transition automatically emits an updated `executing_cmd` string (`Executing :: <step.action>(<target>) on Tab :: #<tab_id> (<tab_title>)`).

---

## 2. High-Level Automation, Testing & Visual Report Pipeline Tools

### `execute_browser_pipeline`
Batches multi-step browser actions into a single turn with built-in awaits, auto-scrolling, semantic/quadrant targeting, live HUD command ticker updates, and retry resilience.
- **Parameters**:
  - `_intent` (string, max 180 chars): One-liner task intent preview.
  - `steps` (array of objects, required): Ordered actions to execute.
  - `tab_id` (integer, optional): Target tab ID (defaults to active/assigned tab).
- **Supported Step Actions**:
  - `navigate`: `{ "action": "navigate", "url": "https://..." }`
  - `new_tab`: `{ "action": "new_tab", "url": "https://..." }`
  - `switch_tab`: `{ "action": "switch_tab", "tab_id": 12345 }`
  - `wait_for`: `{ "action": "wait_for", "selector": "#btn", "timeout_ms": 8000 }`
  - `wait_condition`: `{ "action": "wait_condition", "expression": "...", "timeout_ms": 8000 }`
  - `wait_network_idle`: `{ "action": "wait_network_idle", "timeout_ms": 5000 }`
  - `click`: `{ "action": "click", "selector": "button", "text": "Submit", "quadrant": "P9" }` (supports semantic selector/text/aria-label OR 16-column grid cell `A1`..`P9` with HUD hit-test passthrough)
  - `scroll`: `{ "action": "scroll", "direction": "down", "delta_y": 500 }`
  - `evaluate`: `{ "action": "evaluate", "expression": "window.location.href" }`
  - `fill_inputs`: `{ "action": "fill_inputs", "fields": [{ "selector": "input", "value": "val" }] }`
  - `press_key`: `{ "action": "press_key", "key": "Enter" }`
  - `sleep`: `{ "action": "sleep", "duration_ms": 500 }`
  - `take_screenshot`: `{ "action": "take_screenshot", "save_path": "report.png" }`
  - `capture_grid_screenshot`: `{ "action": "capture_grid_screenshot", "quality": 80 }`
  - `print_to_pdf`: `{ "action": "print_to_pdf", "format": "a4", "save_path": "out.pdf" }`
  - `handle_dialog`: `{ "action": "handle_dialog", "accept": true, "prompt_text": "..." }`
  - `replay_curl`: `{ "action": "replay_curl", "request_id": "req_1", "headers": { ... } }`

### `run_browser_test`
Automated browser testing engine and Visual Test & Feature Overview Document Pipeline (`mcp-server/src/tools/testing.rs` & `report_builder.rs`).
- **Parameters**:
  - `_intent` (string, max 180 chars): One-liner test intent preview.
  - `test_name` (string, required): Identifier for the test suite run.
  - `run_id` (string, optional): Explicit run folder identifier under `test_reports/<run_id>/`.
  - `description` (string, optional): Step-by-step test plan or natural language criteria.
  - `steps` (array, optional): Explicit structured test steps.
  - `full_page_screenshots` (boolean, optional): Capture full-height page renders.
- **Visual Overview Document Pipeline (`SS -> HTML -> PDF -> Openable Print Link`)**:
  1. **Zero-Base64 Disk Asset Pipeline**: Saves each step's cloaked compositor screenshot (`webp`, `quality: 75`) directly to `test_reports/<run_id>/assets/step_<n>.webp`.
  2. **Self-Contained Print-Styled HTML Compiler**: Compiles step intents (`_intent`), executed commands (`Executing :: xyz on Tab :: xyz`), correlated optimistic cURL snippets, and `<img src="assets/step_<n>.webp">` into `test_reports/<run_id>/overview.html` with `@media print` page-break rules.
  3. **Silent CDP `Page.printToPDF` Generation**: Generates `test_reports/<run_id>/overview.pdf` with `printBackground: true, preferCSSPageSize: true`.
  4. **Openable & Auto-Print Links on Daemon Port 9222**:
     - HTML Report: `http://127.0.0.1:9222/reports/<run_id>/overview.html`
     - PDF Document: `http://127.0.0.1:9222/reports/<run_id>/overview.pdf`
     - Instant Print Dialog Link: `http://127.0.0.1:9222/reports/<run_id>/overview.html?print=1` (auto-triggers `window.print()` on load).

---

## 3. Optimistic Network Capture & Zero-Subprocess Rust Replay

### `replay_curl`
Replays a captured network call from the tab's 50-item ring buffer (`mcp-server/src/tools/network_harvester.rs`) or a raw/modified `curl` command string (`mcp-server/src/tools/curl_runner.rs`) without spawning `sh -c curl`. Also supports inspecting the tab's captured XHR/Fetch/Document ring buffer when `action: "list"`.
- **Parameters**:
  - `_intent` (string, max 180 chars): One-liner task intent preview.
  - `tab_id` (integer, optional): Target tab ID.
  - `action` (string, optional): `"replay"` (default) or `"list"` (returns the tab's captured optimistic cURL ring buffer).
  - `request_id` (string, optional): ID of a captured request from the 50-item ring buffer.
  - `curl` (string, optional): Raw or modified `curl` command string parsed natively via pure-Rust `shlex`.
  - `method` (string, optional): HTTP method override.
  - `url` (string, optional): Target URL override.
  - `headers` (object, optional): Header key-value overrides.
  - `body` (string, optional): Request payload override.
  - `mode` (string, optional): `"auto"`, `"native"` (Rust async TCP HTTP client), or `"browser"` (in-tab `Runtime.evaluate(fetch(...))` preserving browser TLS & HttpOnly cookies).
- **Response Fields**:
  - `status_code` (integer): HTTP response status code.
  - `response_headers` (object): Normalized response headers.
  - `elapsed_ms` (integer): Round-trip execution latency in milliseconds.
  - `sanitized_body` (string): Response body scrubbed via `hud_sanitizer`.
  - `curl_command` (string): Canonical shell-escaped `curl` command string.

---

## 4. Session & Tab Management Tools

### `list_browser_sessions`
Returns lightweight list of open Chrome tabs with protocol prefixes (`http://`, `https://`) and query parameters stripped in Rust to minimize context bloat.
- **Parameters**: `_intent` (string), `detailed` (boolean, default `false`).

### `create_tab` / `switch_tab` / `close_tab` / `select_session`
Manage tab lifecycle and exclusive per-agent tab locks (`TabRegistry`).

---

## 5. DOM, Coordinate & Dialog Interaction Tools

### `fill_inputs`
Framework-safe input simulation that triggers synthetic events across React, Vue, Angular, and native HTML forms.
- **Parameters**: `_intent` (string), `tab_id` (integer), `fields` (array of `{ selector, value }`).

### `interact`
Executes coordinate-based or 16-column grid quadrant (`A1`..`P9`) mouse/scroll interactions with automatic HUD hit-test passthrough (`document.elementsFromPoint(x, y)` filtering out `<mcp-agent-hud>`, `#__mcp_agent_hud_host__`, and `[data-mcp-internal-ignore]`).
- **Parameters**:
  - `_intent` (string, max 180 chars)
  - `action` (string, required): `click`, `hover`, `scroll`.
  - `quadrant` (string, optional): 16-column grid cell coordinate (e.g. `"P9"`).
  - `x_ratio` / `y_ratio` (number, optional): Normalized viewport coordinates (`0.0` to `1.0`).
  - `deltaX` / `deltaY` (number, optional): Scroll deltas.

### `evaluate_expression`
Evaluates JavaScript expressions in the target tab with Rust-layer optimization and HUD blindspot sanitization.
- **Parameters**: `_intent` (string), `expression` (string, required), `tab_id` (integer, optional).

### `handle_dialog` & `get_active_dialog`
Inspect and programmatically accept/dismiss JavaScript `alert`, `confirm`, `prompt`, and `beforeunload` modal dialogs via CDP `Page.handleJavaScriptDialog`.

---

## 6. Visual Inspection & PDF Export Tools

### `capture_viewport` & `take_screenshot`
Capture clean compositor snapshots with automatic HUD cloaking (`CLOAK_HUD` / `hudHost.style.visibility = "hidden"`).

### `print_to_pdf`
Generates a validated `%PDF-` document silently via CDP `Page.printToPDF` (`transferMode: "ReturnAsBase64"`), supporting presets (`letter`, `a4`, `legal`, `tabloid`, `a3`), custom margins, scale `[0.1, 2.0]`, page ranges, and direct disk export (`save_path`).

---

## 7. Multi-Layer Agent Blindspot Protection (`mcp-server/src/filter/hud_sanitizer.rs`)

Every WebSocket response from `background.js` passes through `hud_sanitizer::sanitize_extension_payload(&mut response.result)` inside `websocket_hub.rs` before resolving the oneshot channel to the MCP tool caller:
1. **JSON DOM Node Pruning**: Objects matching tag `mcp-agent-hud`, `id == "__mcp_agent_hud_host__"`, `data-mcp-internal-ignore`, or flat CDP attribute arrays `["id", "__mcp_agent_hud_host__", ...]` are replaced with `Value::Null` and pruned from parent arrays and `node`/`element`/`target` fields.
2. **Raw HTML Scrubbing**: String payloads containing `<mcp-agent-hud...>` or `id="__mcp_agent_hud_host__"` have all HUD markup scrubbed across multiline strings.
3. **Visual & Hit-Test Guards**: Compositor screenshots/PDFs cloak the HUD host prior to capture, and coordinate clicks on bottom-right cell `P9` pass directly through to the underlying page element.
