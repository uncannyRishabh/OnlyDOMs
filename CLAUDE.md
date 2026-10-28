# ChromaPilot — Claude & Agent Instructions

ChromaPilot is a high-performance browser automation system that bridges AI coding agents (Claude, Cursor, Windsurf, Antigravity) to an active Google Chrome browser session via a Rust MCP server and a Manifest V3 Chrome Extension.

## 🚀 Quick Initialization (When Asked to Initialize this Repo)

When a user pastes this repo link or asks you to initialize, set up, or run ChromaPilot:

1. **Run the automated setup script**:
   ```bash
   ./setup.sh --install
   ```
   *(Or `make install` / `./setup.sh`)*
   This automatically:
   - Validates Rust & Chrome environment
   - Compiles the release binary (`mcp-server/target/release/chromapilot`)
   - Creates project-level `.cursor/mcp.json`, `.vscode/mcp.json`, `.idea/mcp.json`, and `.windsurf/mcp.json`
   - Automatically registers ChromaPilot in Claude Desktop, Windsurf, JetBrains, Antigravity, and Claude Code CLI (if available)

2. **If running inside Claude Code CLI**:
   ```bash
   claude mcp add chromapilot -- $(pwd)/mcp-server/target/release/chromapilot --port 9222
   ```

3. **Preemptive Health Check Protocol**:
   Before executing browser commands:
   - Check daemon status: `$(pwd)/mcp-server/target/release/chromapilot status`
   - If not running, start it: `$(pwd)/mcp-server/target/release/chromapilot daemon --port 9222`
   - Confirm Chrome extension is 🟢 **Connected** (Port 9222).
   - If MCP tools are detached, use first-class CLI commands directly:
     - `chromapilot open <url>`
     - `chromapilot tabs`
     - `chromapilot eval "<expr>"`
     - `chromapilot pipeline '<steps_json>'`
     *(Never use raw python or ad-hoc scripts)*

4. **Inform the user to load the Chrome Extension**:
   - Directory: `<repo-root>/extension`
   - Steps: Open `chrome://extensions/` -> Enable **Developer mode** -> Click **Load unpacked** -> Select the `extension/` directory.
   - Or launch Chrome directly with the extension loaded:
     - macOS: `open -a "Google Chrome" --args --load-extension="$(pwd)/extension"`
     - Linux: `google-chrome --load-extension="$(pwd)/extension"`

5. **Verify Connectivity**:
   When the extension is attached, its toolbar badge shows 🟢 **Connected** (Port `9222`).

## Development Commands

- **Build release**: `cargo build --release --manifest-path mcp-server/Cargo.toml` (or `make build`)
- **Run tests**: `cargo test --manifest-path mcp-server/Cargo.toml` (or `make test`)
- **Run server directly**: `cargo run --release --manifest-path mcp-server/Cargo.toml -- --port 9222`
- **Check code**: `cargo check --manifest-path mcp-server/Cargo.toml`

## System Architecture

```
[AI Agent]
    │ stdio (MCP JSON-RPC 2.0)
    ▼
[Rust MCP Server (`chromapilot`)]
    │ WebSocket (`ws://127.0.0.1:9222/mcp-relay`)
    ▼
[Chrome Extension (Manifest V3)]
    ├── background.js: Service Worker, chrome.debugger API, multi-tab routing
    └── content/input_filler.js: Framework-safe DOM input simulation
```

## Agent Operational Guidelines

When interacting with the browser via ChromaPilot:

1. **Prefer `execute_browser_pipeline`**:
   Batch multi-step actions (navigate, wait, click, fill, scroll) into a single pipeline call instead of single-step round trips. This eliminates LLM latency and saves up to 80% token costs.

2. **Tab Preservation**:
   - If the active tab has user work (`github.com`, `docs.google.com`, etc.), open new tasks in a new tab with `create_tab` or the `new_tab` pipeline action.
   - If the active tab is `chrome://newtab/` or blank, use the current tab.
   - For in-session navigation (clicking internal links, going back, checkout), stay in the current tab.

3. **Extension Status**:
   If a tool returns `"No active browser session detected"`, advise the user to ensure the ChromaPilot extension icon in Chrome shows 🟢 Connected (Port `9222`).
