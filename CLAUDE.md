# OnlyDOMs — Claude & Agent Instructions

OnlyDOMs is a high-performance browser automation system purpose-built for agentic SDLC and autonomous web operations. It bridges AI coding agents (Claude, Cursor, Windsurf, Antigravity) to an active Google Chrome browser session via a Rust MCP server and a Manifest V3 Chrome Extension.

## 🚀 Quick Initialization (When Asked to Initialize this Repo)

1. **Run the automated setup script**:
   - macOS / Linux: `./setup.sh --install`
   - Windows: `setup.bat --install`

2. **Preemptive Health Check Protocol**:
   - Check daemon status: `$(pwd)/mcp-server/target/release/onlydoms status`
   - If not running, start it: `$(pwd)/mcp-server/target/release/onlydoms daemon --port 9222`
   - Confirm Chrome extension is 🟢 **Connected** (Port `9222`).

## Development Commands

- **Build release**: `cargo build --release --manifest-path mcp-server/Cargo.toml` (or `make build`)
- **Run all tests**: `cargo test --manifest-path mcp-server/Cargo.toml` (or `make test`)
- **Run HUD sanitizer tests**: `cargo test --manifest-path mcp-server/Cargo.toml filter::hud_sanitizer`
- **Check code**: `cargo check --manifest-path mcp-server/Cargo.toml`

## Agent Operational Guidelines

1. **Protocol Consistency (MCP-First Rule)**:
   Always use `call_mcp_tool` for all browser automation actions. Never run shell CLI commands (`onlydoms eval`, `onlydoms click`) on tabs owned by your MCP session, as OnlyDOMs enforces exclusive tab isolation leases.

2. **Zero Schema Micro-Reads**:
   Do NOT read `.json` tool schema files. All parameters are fully documented in `SKILL.md`.

3. **Task Intent Preview Rule (180-Char / 30-Word Ceiling)**:
   Before invoking any MCP tool, always output a single one-line preview in chat (`> 🎯 **Intent:** <up to 30 words / 180 chars>`) AND pass the exact same string in the `"_intent"` parameter of the tool call so the Chrome Floating Pill displays your intent in real time with zero extra tool turns.

4. **Prefer `execute_browser_pipeline`**:
   Batch multi-step actions (`navigate`, `wait_selector`, `click` via OmniClick, `evaluate`, `navigate_to_result`, `screenshot`, `fill_inputs`, `scroll`, `print_to_pdf`, `handle_dialog`, `replay_curl`) into a single pipeline call instead of single-step round trips.

5. **Async DOM Predicates (Banned: Shell Sleep)**:
   Never use `sleep N && onlydoms eval`. Use an `await Promise` with `MutationObserver` inside `evaluate_expression` for instant event-driven resolution.

6. **Deep Exploration Isolation (`invoke_subagent`)**:
   For broad exploratory crawls or multi-step discovery, delegate to a `research` subagent to isolate token accumulation.

7. **OmniClick Engine for React & Angular**:
   Clicks in `execute_browser_pipeline` automatically fire the 4-tier event cascade (Pointer/Mouse sequence + React Fiber fallback + native CDP mouse dispatch + router link fallback) for seamless interaction with React 18+ and Angular components.

8. **Visual Test & Feature Overview Pipeline (`run_browser_test`)**:
   Automatically captures cloaked WebP step assets (`test_reports/<run_id>/assets/step_<n>.webp`), compiles `overview.html` and `overview.pdf`, and serves them at:
   - `http://127.0.0.1:9222/reports/<run_id>/overview.html`
   - `http://127.0.0.1:9222/reports/<run_id>/overview.pdf`
   - `http://127.0.0.1:9222/reports/<run_id>/overview.html?print=1`

9. **Optimistic Network Capture & Rust cURL Replay (`replay_curl`)**:
   Use `replay_curl` (`action: "list"` or with `request_id` / `curl`) to inspect the 50-item optimistic XHR/Fetch/Document ring buffer and replay requests natively in Rust without spawning a shell.
