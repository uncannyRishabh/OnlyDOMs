# OnlyDOMs MCP Tool Specification

Complete reference for all 15 Model Context Protocol (MCP) tools provided by OnlyDOMs.

---

## 1. High-Level Automation & Batch Tools

### `execute_browser_pipeline`
Batches multi-step browser actions into a single turn with built-in awaits, auto-scrolling, semantic targeting, and retry resilience.
- **Parameters**:
  - `steps` (array of objects, required): Ordered actions to execute.
  - `tab_id` (integer, optional): Target tab ID (defaults to active/assigned tab).
- **Supported Step Actions**:
  - `navigate`: `{ "action": "navigate", "url": "https://..." }`
  - `new_tab`: `{ "action": "new_tab", "url": "https://..." }`
  - `switch_tab`: `{ "action": "switch_tab", "tab_id": 12345 }`
  - `wait_for`: `{ "action": "wait_for", "selector": "#btn", "timeout_ms": 8000 }`
  - `wait_condition`: `{ "action": "wait_condition", "expression": "...", "timeout_ms": 8000 }`
  - `wait_network_idle`: `{ "action": "wait_network_idle", "timeout_ms": 5000 }`
  - `click`: `{ "action": "click", "selector": "button", "text": "Submit" }`
  - `scroll`: `{ "action": "scroll", "direction": "down", "delta_y": 500 }`
  - `evaluate`: `{ "action": "evaluate", "expression": "window.location.href" }`
  - `fill_inputs`: `{ "action": "fill_inputs", "fields": [{ "selector": "input", "value": "val" }] }`
  - `press_key`: `{ "action": "press_key", "key": "Enter" }`
  - `sleep`: `{ "action": "sleep", "duration_ms": 500 }`
  - `take_screenshot`: `{ "action": "take_screenshot", "save_path": "report.png" }`

### `run_browser_test`
Decomposes natural language or structured test instructions into verified browser execution with visual snapshots and markdown/JSON reports.
- **Parameters**:
  - `test_name` (string, required): Identifier for the test suite run.
  - `description` (string, required): Step-by-step test plan or natural language criteria.
  - `full_page_screenshots` (boolean, optional): Whether to capture full-height page renders.

---

## 2. Session & Tab Management Tools

### `list_browser_sessions`
Returns lightweight list of open Chrome tabs with protocol prefixes (`http://`, `https://`) and query parameters stripped to minimize context bloat.
- **Parameters**:
  - `detailed` (boolean, optional): If `true`, includes raw CDP debug metadata. Defaults to `false`.
- **Response Fields**:
  - `id` (integer): Tab ID.
  - `title` (string): Page title.
  - `url` (string): Clean domain and path (protocol and queries pruned).
  - `active` (boolean): Whether tab is active.
  - `is_locked` (boolean): Multi-agent lease status.
  - `locked_by_current_agent` (boolean): True if locked by caller.
  - `locked_by_other_agent` (boolean): True if locked by a concurrent agent.

### `create_tab`
Opens a new tab in Chrome, attaches the debugger, and grants exclusive lease to caller.
- **Parameters**:
  - `url` (string, optional): Initial URL (defaults to `about:blank`).

### `switch_tab`
Switches focus to an existing tab and acquires exclusive agent lease.
- **Parameters**:
  - `tab_id` (integer, required): Target tab ID.

### `close_tab`
Closes a tab and releases any associated leases.
- **Parameters**:
  - `tab_id` (integer, required): Target tab ID.

### `select_session`
Aliases active debugger focus to a specific tab session.
- **Parameters**:
  - `session_id` (string, required): Session ID.

---

## 3. DOM & Interaction Tools

### `fill_inputs`
Framework-safe input simulation that triggers synthetic events across React, Vue, Angular, and native HTML forms.
- **Parameters**:
  - `fields` (array of objects, required):
    - `selector` (string): CSS selector for target input/textarea.
    - `value` (string): Text value to insert.

### `interact`
Executes semantic mouse, keyboard, or scroll interactions with auto-scrolling and element targeting.
- **Parameters**:
  - `action` (string, required): `click`, `hover`, `scroll`, `focus`.
  - `selector` (string, optional): CSS selector.
  - `text` (string, optional): Visible text match.
  - `aria_label` (string, optional): ARIA label match.
  - `delta_x` / `delta_y` (integer, optional): Scroll distance.

### `evaluate_expression`
Evaluates JavaScript expressions in the target tab.
- **Parameters**:
  - `expression` (string, required): JavaScript snippet to evaluate.
  - `tab_id` (integer, optional): Target tab ID.
- **Rust Sanitization Pipeline**:
  - Inline CSS styles (`style="..."`) are stripped.
  - Attributes with `null` values are pruned.
  - Elements in `chain` arrays have `textContent` truncated to 100 characters. Standalone queries retain full text.

---

## 4. Visual Inspection Tools

### `capture_viewport`
Captures viewport snapshot using Chrome's compositor engine.
- **Parameters**:
  - `format` (string, optional): `png` (default) or `jpeg`.
  - `quality` (integer, optional): 0-100 (for jpeg).

### `take_screenshot`
High-resolution screenshot with optional full-page capture and direct file export.
- **Parameters**:
  - `full_page` (boolean, optional): Capture entire scrollable page height.
  - `format` (string, optional): `png` or `jpeg`.
  - `save_path` (string, optional): Absolute or relative filesystem destination.

---

## 5. Chrome DevTools Debugging Tools

### `add_breakpoint`
Sets a JavaScript breakpoint on script URL and line number.
- **Parameters**:
  - `url` (string, required): Script URL regex or exact path.
  - `line_number` (integer, required): 0-indexed line number.

### `pause_execution` / `resume_execution`
Pauses or resumes JavaScript thread execution.

### `step_over` / `step_into`
Steps over or steps into the next JavaScript function execution frame.
