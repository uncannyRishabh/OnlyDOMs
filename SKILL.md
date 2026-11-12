---
name: onlydoms
description: High-performance Chrome browser automation, DOM inspection, optimistic cURL network replay, and visual PDF test reporting via OnlyDOMs MCP and CLI. Use whenever the user asks to automate Chrome, open or interact with websites, inspect live DOM, fill forms, run automated browser tests, generate HTML or PDF test overview reports, or capture and replay network cURL requests.
---

# Role & Operational Protocol: OnlyDOMs Chrome Automation

You have access to **OnlyDOMs**, a high-performance browser automation MCP server providing browser-wide, multi-tab control of Google Chrome via Chrome DevTools Protocol (CDP) on `127.0.0.1:9222`.

## 1. Pre-Flight Health Check & Execution Protocol

OnlyDOMs uses a **Decoupled Daemon + Multiplexed Client Architecture**:
- A single long-lived daemon owns the WebSocket connection to Google Chrome on `127.0.0.1:9222`.
- When an AI agent connects via MCP, it attaches directly to this shared daemon.
- Multiple chat windows and agents run concurrently without connection conflicts.

1. **Protocol Consistency (MCP-First Rule — CRITICAL)**:
   - When running in an environment with OnlyDOMs MCP tools available, **ALWAYS use `call_mcp_tool` for ALL browser operations**.
   - **NEVER** run shell commands (`run_command onlydoms ...`) on tabs managed by your MCP session. OnlyDOMs tab isolation assigns an exclusive lock to your agent session (e.g. `agent_5d0f724b`). Running CLI commands from a shell subshell causes immediate tab lock collisions (`Tab <id> is locked by another agent session`).

2. **Browser Extension Connection**:
   - If any tool returns `"No active browser session detected"`, stop and notify the user:
     > ⚠️ **OnlyDOMs Chrome extension is disconnected.**
     > Please ensure the OnlyDOMs extension icon in Chrome shows 🟢 **Connected** (Port `9222`).

3. **Zero Schema Micro-Reads**:
   - **DO NOT** read individual MCP tool schema JSON files (`take_screenshot.json`, `evaluate_expression.json`, etc.). All tool signatures and schemas are fully specified in Section 8 below.

## 2. Multi-Agent Tab Isolation & Lock Protection

1. **Exclusive Tab Leases**:
   - Every tab accessed by an agent is exclusively leased to that agent session. Other agents or subshells cannot intercept it.
2. **Tab Selection**:
   - `list_browser_sessions` shows `is_locked`, `locked_by_current_agent`, and `locked_by_other_agent`. Use unreserved tabs or provision a new one via `create_tab` (or the `new_tab` pipeline action).

## 3. Intelligent Tab Management (New Tab vs In-Session Navigation)

1. **New Tab Case (Preserve User Work)**:
   - If the active tab has an active user webpage (e.g. `docs.google.com`, `github.com`), do NOT overwrite the page. Use `create_tab` or `{"action": "new_tab", "url": "..."}`.
2. **Clean Slate Case**:
   - If the active tab is `chrome://newtab/` or `about:blank`, execute directly in the current tab.
3. **In-Session Navigation Case**:
   - If navigating within the same website or flow (e.g. clicking settings, viewing profile, checkout), stay in the current tab.

## 4. Resilient Batch Operations (`execute_browser_pipeline`)

**Always prefer `execute_browser_pipeline`** over individual atomic tool turns. Target a **1 to 2 compound turn ceiling** for complete workflows.

### Task Intent Preview Rule (180-Char / 30-Word Ceiling)
Before calling any MCP tool, always output a single one-line chat preview:
`> 🎯 **Intent:** <up to 30 words / 180 chars>`
AND pass the exact same string in the `"_intent"` parameter of the tool call so the Chrome Floating Pill displays your intent in real time with zero extra turns.

### Supported Pipeline Actions:
- `navigate`: `{"action": "navigate", "url": "https://..."}`
- `new_tab`: `{"action": "new_tab", "url": "https://..."}`
- `switch_tab`: `{"action": "switch_tab", "tab_id": 12345}`
- `wait_for` / `wait_selector`: `{"action": "wait_selector", "selector": "main a[href*='/p/']", "timeout_ms": 5000}` (instant resolution via in-page `MutationObserver` Promise)
- `wait_condition`: `{"action": "wait_condition", "expression": "document.readyState === 'complete'", "timeout_ms": 8000}`
- `wait_network_idle`: `{"action": "wait_network_idle", "timeout_ms": 5000}`
- `click`: Semantic targeting powered by OmniClick with automatic React/Angular event cascade:
  - By CSS selector: `{"action": "click", "selector": "button[type='submit']"}`
  - By text: `{"action": "click", "text": "Submit"}`
  - By aria-label: `{"action": "click", "aria_label": "Settings"}`
  - By 16-column grid quadrant: `{"action": "click", "quadrant": "P9"}`
- `evaluate`: `{"action": "evaluate", "expression": "document.querySelector('main a').href"}`
- `navigate_to_result`: `{"action": "navigate_to_result"}` (automatically navigates to the URL extracted by the preceding `evaluate` step)
- `scroll`: `{"action": "scroll", "direction": "down", "delta_y": 700}`
- `fill_inputs`: `{"action": "fill_inputs", "fields": [{"selector": "input[name='q']", "value": "search"}]}`
- `press_key`: `{"action": "press_key", "key": "Enter"}`
- `sleep`: `{"action": "sleep", "duration_ms": 500}`
- `take_screenshot` / `screenshot`: `{"action": "screenshot", "save_path": "profile.webp", "format": "webp"}`
- `print_to_pdf`: `{"action": "print_to_pdf", "save_path": "report.pdf", "format": "a4"}`
- `handle_dialog`: `{"action": "handle_dialog", "accept": true, "prompt_text": "text"}`
- `replay_curl`: `{"action": "replay_curl", "request_id": "req_1"}`

### Compound Batch Example:
```json
{
  "_intent": "Find non-private explore reel, navigate to profile, and capture screenshot",
  "steps": [
    { "action": "navigate", "url": "https://www.instagram.com/explore/" },
    { "action": "wait_selector", "selector": "main a[href*='/p/']", "timeout_ms": 5000 },
    { "action": "evaluate", "expression": "document.querySelector(\"main a[href*='/p/']\").href" },
    { "action": "navigate_to_result" },
    { "action": "screenshot", "save_path": "profile.webp" }
  ]
}
```

## 5. Async DOM Predicates (Banned: Shell Sleep)

**Never use `sleep N && onlydoms eval`**. Arbitrary sleep commands waste idle time and fail under network jitter.

When evaluating conditions in `evaluate_expression`, use an `await Promise` with `MutationObserver` for immediate resolution the moment hydration occurs:
```javascript
await new Promise((resolve) => {
    if (document.querySelector('header h1, header section')) return resolve(true);
    const obs = new MutationObserver(() => {
        if (document.querySelector('header h1, header section')) {
            obs.disconnect();
            resolve(true);
        }
    });
    obs.observe(document.body, { childList: true, subtree: true });
    setTimeout(() => { obs.disconnect(); resolve(false); }, 4000);
});
```

## 6. Deep Exploration Isolation via Subagents (`invoke_subagent`)

When performing open-ended exploratory crawling, trial-and-error discovery, or multi-page inspections, **launch a dedicated `research` subagent** to prevent token accumulation in the primary conversation:
```json
invoke_subagent({
  "TypeName": "research",
  "Role": "Instagram Explorer",
  "Prompt": "Open Instagram, find a random non-private profile, extract stats and recent themes, take a screenshot, and return the summary."
})
```
The parent context receives only the clean synthesized summary.

## 7. OmniClick Engine for React & Angular SPAs

When clicking dynamic elements on React, Angular, Vue, or Web Component applications, OnlyDOMs automatically triggers a 4-tier event cascade:
1. **Pointer & Mouse Cascade**: Dispatches `pointerover` → `pointerenter` → `pointerdown` (with buttons & pressure) → `mousedown` → `focus()` → `pointerup` → `mouseup` → `click` with `{ bubbles: true, cancelable: true, composed: true, view: window }`.
2. **React Fiber Direct Trigger**: If event bubbling is intercepted, directly triggers internal `__reactProps$*.onClick` / `onPointerDown`.
3. **Native CDP Mouse Dispatch**: Emits OS-level `isTrusted: true` CDP mouse events (`mouseMoved`, `mousePressed`, `mouseReleased`) at element center `(x, y)`.
4. **Router Link Fallback**: If an anchor tag `<a href>` was clicked and URL didn't update, resolves target URL for navigation.

## 8. Complete MCP Tool Reference Card (No Schema Reads Needed)

| Tool Name | Key Parameters | Description |
| :--- | :--- | :--- |
| `execute_browser_pipeline` | `steps` (array), `_intent` (string), `tab_id` (optional int) | Executes batch pipeline of actions in 1 turn. |
| `evaluate_expression` | `expression` (string), `await_promise` (bool, default true), `tab_id` (optional int) | Evaluates JS in page context. |
| `take_screenshot` | `save_path` (string), `format` ("webp"\|"png"), `full_page` (bool), `tab_id` (optional int) | Captures screenshot to disk. |
| `create_tab` | `url` (string) | Opens fresh tab without overwriting active page. |
| `close_tab` | `tab_id` (int) | Closes tab. |
| `switch_tab` | `tab_id` (int) | Focuses specified tab. |
| `interact` | `action` ("click"\|"hover"\|"scroll"), `quadrant` ("P1"-"P16"), `x`, `y` | Coordinates/grid interaction. |
| `run_browser_test` | `description` (string) or `steps` (array), `run_id` (optional) | Automated visual test run with HTML/PDF report. |
| `replay_curl` | `action` ("list"\|"replay"), `request_id` (string), `curl` (string) | Replays network request in Rust without shell. |
| `get_network_traffic` | `tab_id` (optional int), `action_id` (optional string), `request_id` (optional string), `url_filter` (optional string) | Queries captured HTTP/XHR/Fetch requests & responses correlated to agent actions. |
| `fill_inputs` | `fields` (`[{selector, value}]`) | Fills multiple inputs cleanly. |
| `list_browser_sessions` | none | Lists all Chrome tabs with lock states. |
| `print_to_pdf` | `save_path` (string), `format` ("a4"), `landscape` (bool) | Generates PDF of current tab. |
| `handle_dialog` | `action` ("accept"\|"dismiss"), `prompt_text` (string) | Handles alert/confirm/prompt dialogs. |
