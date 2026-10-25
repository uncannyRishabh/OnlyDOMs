# ChromaPilot 🌐⚡

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Chrome MV3](https://img.shields.io/badge/Chrome-Manifest_V3-green.svg?style=flat-square&logo=googlechrome)](extension/)
[![Protocol](https://img.shields.io/badge/Protocol-Model_Context_Protocol-purple.svg?style=flat-square)](https://modelcontextprotocol.io/)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg?style=flat-square)](LICENSE)
[![GitHub Stars](https://img.shields.io/github/stars/uncannyRishabh/browser-mcp?style=flat-square)](https://github.com/uncannyRishabh/browser-mcp/stargazers)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg?style=flat-square)](https://github.com/uncannyRishabh/browser-mcp/pulls)

> **High-performance, token-efficient Chromium browser automation for AI coding agents.**  
> Powered by a lightweight native Rust MCP Server and a Manifest V3 Google Chrome Extension.

---

> ⚠️ **RESPONSIBLE USE & SAFETY WARNING**  
> ChromaPilot grants AI agents real-time inspection, navigation, DOM manipulation, and arbitrary JavaScript execution inside your active Google Chrome browser sessions via the Chrome DevTools Protocol (`chrome.debugger`). It has direct visibility into loaded tabs, cookies, and authenticated states. Run strictly in trusted environments and operate responsibly.

---

> 🤖 **Pasting this repository link to an AI Agent?**  
> If you are an AI coding assistant (Claude Code, Cursor, Windsurf, Antigravity, Devin, Copilot) given this repository link and asked to initialize it, jump straight to [🤖 Quick Setup for AI Agents (Auto-Initialization)](#-quick-setup-for-ai-agents-auto-initialization) or execute:
> ```bash
> ./setup.sh --install
> ```

---

## Table of Contents

- [Overview](#overview)
- [Why ChromaPilot?](#why-chromapilot)
- [🤖 Quick Setup for AI Agents (Auto-Initialization)](#-quick-setup-for-ai-agents-auto-initialization)
- [Quick Start for Developers](#quick-start-for-developers)
- [🚀 Real-World Usage & Example Prompts](#-real-world-usage--example-prompts)
- [MCP Client Configurations](#mcp-client-configurations)
  - [Cursor (Zero-Config Workspace)](#cursor)
  - [VS Code / Cline / Roo Code (Zero-Config Workspace)](#vs-code-cline--roo-code)
  - [Claude Desktop](#claude-desktop)
  - [Claude Code CLI](#claude-code-cli)
  - [Windsurf](#windsurf)
  - [Antigravity / Gemini CLI](#antigravity--gemini-cli)
  - [Zed Editor](#zed-editor)
- [Architecture](#architecture)
- [MCP Tools Reference (14 Tools)](#mcp-tools-reference)
- [Resilient Browser Pipeline (`execute_browser_pipeline`)](#resilient-browser-pipeline-execute_browser_pipeline)
- [Adaptive 16×N Grid System](#adaptive-16n-grid-system)
- [Agent Operational Directives](#agent-operational-directives)
- [Troubleshooting & FAQ](#troubleshooting--faq)
- [Development & Testing](#development--testing)
- [License](#license)

---

## Overview

Most AI browser automation tools spin up a sterile, headless Chromium instance. This breaks logged-in sessions, triggers bot-detection captchas, and forces developers to inject fragile credentials or cookies.

**ChromaPilot operates differently:** it connects directly to your **actual everyday Google Chrome browser**. Your existing logins, GitHub sessions, Google Workspace, staging cookies, and 2FA sessions remain completely accessible to your AI assistant.

Built with a high-performance **Rust MCP server** and a **Manifest V3 Chrome Extension**, ChromaPilot delivers sub-millisecond execution, compound batching to slash LLM token waste, and resolution-independent visual interaction.

---

## Why ChromaPilot?

- ⚡ **Native Rust Performance**: Consumes `< 10 MB` of RAM with instant WebSocket dispatch.
- 🍪 **Zero Login Friction**: Runs directly inside your authenticated Chrome tabs—no credential injection needed.
- 📦 **Compound Batch Pipeline (`execute_browser_pipeline`)**: Dispatches complex multi-step workflows (navigate, wait for selector, scroll, semantic click, fill inputs) in **a single model turn**, eliminating LLM latency loops and saving up to **80% token costs**.
- 🗂️ **Browser-Wide Multi-Tab Control**: Query, open, switch, and close any tab across all Chrome windows without manual re-attachment.
- 🎯 **Adaptive 16×N Visual Grid**: Resolution-independent coordinate mapping for pixel-perfect clicking on canvas, icons, and dynamic UI elements.
- 🛡️ **Framework-Safe Form Filling**: Emulates synthetic events for React, Angular, and Vue so modern reactive stores never drop typed input.
- 🔍 **CDP Runtime & Debugging**: Set breakpoints, pause/resume execution, step over/into, and safely evaluate JS with payload-size protection.

---

## 🤖 Quick Setup for AI Agents (Auto-Initialization)

**When an AI assistant (Claude Code, Cursor, Windsurf, Antigravity, Devin, Copilot) is asked to initialize or set up this repository:**

### Step 1: Execute the Auto-Initialization Script

Run the following command in the repository root:

```bash
./setup.sh --install
```
*(Or run `make install`)*

**What this automatically does:**
1. **Toolchain check**: Validates Rust (`cargo`) and automatically sources `~/.cargo/env` if needed.
2. **Release build**: Compiles the optimized binary at `mcp-server/target/release/chromapilot`.
3. **Workspace configs**: Generates project-level `.cursor/mcp.json` and `.vscode/mcp.json` so Cursor and VS Code immediately detect ChromaPilot.
4. **Agent registration**: Automatically merges the MCP server configuration into Claude Desktop (`claude_desktop_config.json`), Windsurf (`mcp_config.json`), Antigravity, and runs `claude mcp add` if Claude Code CLI is present.
5. **Extension validation**: Verifies `extension/manifest.json`.

---

### Step 2: Instruct the User to Load the Extension in Chrome

Provide these exact instructions to the user:

**Option A (Chrome UI — Recommended):**
1. Open Google Chrome and go to `chrome://extensions/`.
2. Toggle **Developer mode** ON (top right switch).
3. Click **Load unpacked** and select the `<repo-path>/extension` folder.
4. The ChromaPilot extension icon will appear in your Chrome toolbar.

**Option B (Terminal One-Liner):**
- **macOS**:
  ```bash
  open -a "Google Chrome" --args --load-extension="$(pwd)/extension"
  ```
- **Linux**:
  ```bash
  google-chrome --load-extension="$(pwd)/extension"
  ```

---

### Step 3: Verify Connectivity

1. Start the server (or launch your MCP client which spawns it automatically):
   ```bash
   ./mcp-server/target/release/chromapilot --port 9222
   ```
2. Click the ChromaPilot icon in the Chrome toolbar.
3. The popup badge will display 🟢 **Connected** (Port `9222`).
4. Done! The agent now has full control of Chrome.

---

## Quick Start for Developers

### Prerequisites
- **Rust & Cargo** (>= 1.70): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Google Chrome** or Chromium browser

### 1. Build the MCP Server
```bash
# Clone the repository
git clone https://github.com/uncannyRishabh/browser-mcp.git
cd browser-mcp

# Run setup
./setup.sh
```
Or build manually:
```bash
cd mcp-server
cargo build --release
```
The optimized release binary is located at `mcp-server/target/release/chromapilot`.

### 2. Install the Chrome Extension
1. Open Chrome and navigate to `chrome://extensions/`.
2. Enable **Developer mode** (toggle in the upper right corner).
3. Click **Load unpacked** and choose the `extension/` directory.
4. The ChromaPilot extension icon will appear in your Chrome toolbar.

### 3. Start the Server
```bash
# Default port 9222
./mcp-server/target/release/chromapilot

# Custom port
./mcp-server/target/release/chromapilot --port 9222
```

### 4. Connect Extension
1. Click the ChromaPilot icon in the Chrome toolbar.
2. Ensure the port matches your server (default `9222`).
3. Click **Attach Session** on any tab.
4. The indicator will show 🟢 **Connected**.

---

## 🚀 Real-World Usage & Example Prompts

Once ChromaPilot is connected, your AI assistant (Cursor, Claude Desktop, Claude Code, Windsurf, Antigravity) has browser-wide access to your **active, authenticated Google Chrome instance**. 

You do **not** need to write JSON payloads or low-level CDP calls. Simply talk to your agent in natural language, or prefix prompts with `chromapilot:` / `/chromapilot`.

Here are practical, high-value prompt examples across real-world workflows:

### 📱 1. Social Media & Messaging
Because ChromaPilot operates inside your existing Chrome tabs, your logins, cookies, and 2FA sessions remain intact:
* **Instagram Reels & Direct Messages**:
  > *"Navigate to Instagram in a new tab, search for `@mkbhd`, open their newest reel, click the share button, and send it via DM to `@alex_dev`."*
* **YouTube Video & Research**:
  > *"Go to YouTube, search for 'Rust async tokio tutorial 2026', open the top result, like the video, and extract the video transcript from the description."*
* **X / Twitter Trend Digest**:
  > *"Open X (Twitter), search for `#ModelContextProtocol`, scroll through the top 5 tweets, and summarize the community discussions and top repository links."*
* **LinkedIn Networking & Research**:
  > *"Navigate to LinkedIn, search for 'Staff Systems Engineer in San Francisco', browse 3 profiles, and summarize their experience highlights in markdown."*

### 🛠️ 2. Developer & DevOps Workflows
* **GitHub Actions & CI Log Inspection**:
  > *"Open pull request #42 on my repository in GitHub, click the failing GitHub Actions check, scroll down to the test failure, and copy the compiler error log here."*
* **Staging & Cloud Dashboard Verification**:
  > *"Navigate to our staging dashboard at `staging.internal.net`, verify the new analytics toggle renders properly, take a screenshot of the view, and confirm there are no console errors."*
* **Pull Request Code Review Helper**:
  > *"Navigate to my open GitHub pull request, expand the changed files tab, scroll to `src/api.rs`, and verify that all exported functions have docstrings."*
* **Vercel / Cloudflare Deployment Verification**:
  > *"Check my latest Vercel deployment dashboard, verify the production build succeeded, and open the live preview URL in a fresh tab."*

### 🔍 3. Deep Research & Multi-Tab Synthesis
* **Multi-Tab Tech & Benchmark Comparison**:
  > *"Search Google for 'best vector databases for self-hosted LLMs 2026', open the top 3 benchmark comparisons in separate tabs, inspect their benchmark charts, and build a comparison table comparing latency, memory usage, and license."*
* **Hacker News Daily Digest**:
  > *"Open Hacker News (`news.ycombinator.com`), find the top 5 trending AI or systems engineering posts, visit each link, and draft a 3-bullet executive summary for each."*
* **API Documentation Deep-Dive**:
  > *"Go to the official Tokio documentation, navigate to the `tokio::sync::broadcast` module, and extract code examples demonstrating multi-producer channels."*

### 💼 4. Workspace & Productivity SaaS
* **Google Docs Note Append**:
  > *"Open my Google Doc titled 'Sprint Architecture Notes' in a new tab, scroll to the bottom of the page, and append today's meeting action items."*
* **Google Calendar Schedule Brief**:
  > *"Navigate to Google Calendar, check my schedule for today, and list all upcoming meetings with their start times and Google Meet links."*
* **Linear / Jira Issue Triage**:
  > *"Open our Linear board, filter for issues assigned to me in the current cycle with priority 'High', and list their ticket IDs and summaries."*

### 🧪 5. Form Testing & Web App QA (Framework-Safe)
ChromaPilot uses synthetic event dispatching safe for React, Vue, and Angular:
* **Interactive Signup & Auth Flow**:
  > *"Navigate to `http://localhost:3000/signup`, fill the registration form with email `test+agent@example.com`, full name `Agent QA`, and password `SecurePass123!`, submit the form, and verify that the onboarding wizard appears."*
* **Cart & Checkout Smoke Test**:
  > *"Go to our e-commerce demo site, search for 'mechanical keyboard', add the first item to cart, proceed to checkout, fill test address details, and verify order review totals."*

---

### 💡 Pro Tips for Prompting Your Agent

| Goal | Recommended Prompt Pattern |
|------|----------------------------|
| **Preserve current tab** | *"In a new tab, go to..."* or *"Open ... in the background"* |
| **Inspect visually** | *"Take a screenshot of the modal / pricing table..."* |
| **Batch multi-step tasks fast** | ChromaPilot's `execute_browser_pipeline` automatically batches navigation, typing, and clicking into a single turn! |
| **Framework-safe typing** | Just say *"Fill the form with..."* — synthetic DOM events ensure React/Vue state updates immediately. |

---

## MCP Client Configurations

### Cursor
*Pre-configured out of the box via project-level `.cursor/mcp.json`:*
```json
{
  "mcpServers": {
    "chromapilot": {
      "command": "${workspaceFolder}/mcp-server/target/release/chromapilot",
      "args": ["--port", "9222"]
    }
  }
}
```

### VS Code (Cline / Roo Code)
*Pre-configured out of the box via project-level `.vscode/mcp.json`:*
```json
{
  "mcpServers": {
    "chromapilot": {
      "command": "${workspaceFolder}/mcp-server/target/release/chromapilot",
      "args": ["--port", "9222"]
    }
  }
}
```

### Claude Desktop

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`  
**Linux**: `~/.config/Claude/claude_desktop_config.json`  
**Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

*(Auto-configured with `./setup.sh --install`)*
```json
{
  "mcpServers": {
    "chromapilot": {
      "command": "<ABSOLUTE_REPO_PATH>/mcp-server/target/release/chromapilot",
      "args": ["--port", "9222"]
    }
  }
}
```

### Claude Code CLI

Add with one command:
```bash
claude mcp add chromapilot -- <ABSOLUTE_REPO_PATH>/mcp-server/target/release/chromapilot --port 9222
```

### Windsurf

File: `~/.codeium/windsurf/mcp_config.json` *(Auto-configured with `./setup.sh --install`)*
```json
{
  "mcpServers": {
    "chromapilot": {
      "command": "<ABSOLUTE_REPO_PATH>/mcp-server/target/release/chromapilot",
      "args": ["--port", "9222"]
    }
  }
}
```

### Antigravity / Gemini CLI

File: `~/.gemini/antigravity/mcp_config.json` *(Auto-configured with `./setup.sh --install`)*
```json
{
  "mcpServers": {
    "chromapilot": {
      "command": "<ABSOLUTE_REPO_PATH>/mcp-server/target/release/chromapilot",
      "args": ["--port", "9222"]
    }
  }
}
```

### Zed Editor

In Zed's `settings.json`:
```json
{
  "context_servers": {
    "chromapilot": {
      "command": {
        "path": "<ABSOLUTE_REPO_PATH>/mcp-server/target/release/chromapilot",
        "args": ["--port", "9222"]
      }
    }
  }
}
```

---

## Architecture

```
┌────────────────────────────────────────────────────────┐
│             AI Agent (Claude / Cursor / Antigravity)    │
└───────────────────────────┬────────────────────────────┘
                            │ JSON-RPC 2.0 over stdio
                            ▼
┌────────────────────────────────────────────────────────┐
│             ChromaPilot Rust MCP Server                 │
│  ├── tokio async event loop                            │
│  ├── stdio reader & JSON-RPC parser                    │
│  └── WebSocket hub relay (ws://127.0.0.1:9222)         │
└───────────────────────────┬────────────────────────────┘
                            │ WebSocket bidirectional frames
                            ▼
┌────────────────────────────────────────────────────────┐
│             Google Chrome Extension (MV3)              │
│  ├── background.js (Service Worker)                    │
│  │     ├── chrome.debugger protocol bridge             │
│  │     ├── Browser-wide tab registry                   │
│  │     └── Automatic heartbeat keepalive               │
│  ├── content/overlay_manager.js (Adaptive 16×N grid)   │
│  └── content/input_filler.js (Framework-safe forms)    │
└───────────────────────────┬────────────────────────────┘
                            │ CDP Commands & DOM injection
                            ▼
┌────────────────────────────────────────────────────────┐
│             Active Google Chrome Browser Tabs          │
│    (GitHub, Google Docs, Staging, Internal Portals)    │
└────────────────────────────────────────────────────────┘
```

---

## MCP Tools Reference

ChromaPilot exposes 14 tools designed for AI agent speed and token efficiency:

| Tool | Parameters | Description |
|------|------------|-------------|
| `execute_browser_pipeline` | `steps` (array) | **Primary compound tool.** Batches navigate, wait, click, fill, scroll, evaluate, and keys into one model turn. |
| `list_browser_sessions` | `detailed` (bool, optional) | Lists all open Chrome tabs across all windows in compact format (`id`, `title`, `url`, `active`). |
| `create_tab` | `url` (string, optional) | Opens a new tab with optional URL and immediately attaches the debugger. |
| `switch_tab` | `tab_id` (number) | Switches focus to a specific Chrome tab. |
| `close_tab` | `tab_id` (number) | Closes a specific tab by ID. |
| `select_session` | `session_id` (string/number) | Directs debugger focus to a target tab. |
| `capture_viewport` | `cols` (number, default: 16) | Captures screenshot with the adaptive 16×N overlay grid. |
| `click_quadrant` | `cell` (string), `offset_x_ratio`, `offset_y_ratio`, `click_count` | Clicks a grid cell (e.g. `D4`) with optional sub-cell offsets. |
| `fill_inputs` | `fields` (array of `{selector, value}`) | Framework-safe form input simulation (React, Angular, Vue). |
| `interact` | `action` (`click`\|`hover`\|`scroll`), `x_ratio`, `y_ratio`, `deltaX`, `deltaY` | Coordinate-based interaction using normalized ratios (0.0 to 1.0). |
| `evaluate_expression` | `expression` (string) | Evaluates arbitrary JavaScript via CDP `Runtime.evaluate` (with safety length guards). |
| `add_breakpoint` | `url_regex` (string), `line_number` (number) | Sets a debugger breakpoint in Chrome. |
| `pause_execution` | *none* | Pauses JavaScript execution. |
| `resume_execution` | *none* | Resumes paused JavaScript execution. |
| `step_over` / `step_into` | *none* | Debugger stepping controls. |

---

## Resilient Browser Pipeline (`execute_browser_pipeline`)

Instead of dispatching single-turn requests that waste model context, use `execute_browser_pipeline` to batch multi-step workflows into a single LLM invocation.

### Supported Actions:
- `navigate`: `{ "action": "navigate", "url": "https://..." }`
- `new_tab`: `{ "action": "new_tab", "url": "https://..." }` (opens and attaches to fresh tab)
- `switch_tab`: `{ "action": "switch_tab", "tab_id": 12345 }`
- `wait_for`: `{ "action": "wait_for", "selector": "#submit-btn", "timeout_ms": 8000 }`
- `wait_condition`: `{ "action": "wait_condition", "expression": "document.readyState === 'complete'", "timeout_ms": 8000 }`
- `wait_network_idle`: `{ "action": "wait_network_idle", "timeout_ms": 5000 }`
- `click`: Semantic click with auto-scroll and automatic retries:
  - By text: `{ "action": "click", "text": "Confirm Purchase" }`
  - By aria-label: `{ "action": "click", "aria_label": "Close dialog" }`
  - By CSS selector: `{ "action": "click", "selector": "button.primary" }`
  - Composite: `{ "action": "click", "selector": "div[role='button']", "text": "Save" }`
- `scroll`: Auto-detects scrollable container or scrolls window:
  - Window scroll: `{ "action": "scroll", "direction": "down", "delta_y": 700 }`
  - Container scroll: `{ "action": "scroll", "selector": ".feed-list", "delta_y": 500 }`
- `evaluate`: In-pipeline JavaScript expression execution:
  - `{ "action": "evaluate", "expression": "document.title" }`
- `fill_inputs`: `{ "action": "fill_inputs", "fields": [{ "selector": "input[name='q']", "value": "ChromaPilot" }] }`
- `press_key`: `{ "action": "press_key", "key": "Enter" }`
- `sleep`: `{ "action": "sleep", "duration_ms": 1000 }`
- `capture_grid_screenshot`: `{ "action": "capture_grid_screenshot", "quality": 80 }`

### Example: End-to-End Search & Form Fill in One Turn

```json
{
  "name": "execute_browser_pipeline",
  "arguments": {
    "steps": [
      { "action": "navigate", "url": "https://github.com/search" },
      { "action": "wait_for", "selector": "input[name='q']", "timeout_ms": 5000 },
      { "action": "fill_inputs", "fields": [{ "selector": "input[name='q']", "value": "chromapilot" }] },
      { "action": "press_key", "key": "Enter" },
      { "action": "wait_network_idle", "timeout_ms": 5000 },
      { "action": "scroll", "direction": "down", "delta_y": 400 },
      { "action": "evaluate", "expression": "document.querySelector('.repo-list-item h3')?.innerText" }
    ]
  }
}
```

---

## Adaptive 16×N Grid System

The viewport is divided into an adaptive **16×N grid**:
- **16 Columns**: Labeled `A` through `P` (left to right).
- **N Rows**: Dynamically computed from viewport aspect ratio:  
  $$\text{rows} = \text{round}\left(16 \times \frac{\text{height}}{\text{width}}\right)$$
- Standard 16:9 displays yield cells from `A1` to `P9`.
- Coordinates are resolution-independent and map to true CSS pixels regardless of device pixel ratio (DPR) or zoom.

```json
{
  "name": "click_quadrant",
  "arguments": {
    "cell": "H5",
    "offset_x_ratio": 0.0,
    "offset_y_ratio": 0.0,
    "click_count": 1
  }
}
```

---

## Agent Operational Directives

AI agents using ChromaPilot should adhere to these production directives:

1. **Tab Preservation**:
   - If the user has active work on the current tab (`docs.google.com`, `github.com`), **do not overwrite it**. Open external requests in a new tab using `create_tab` or `new_tab`.
   - If on `chrome://newtab/` or `about:blank`, navigate in the current tab.
   - For internal navigation (clicking links, checkout, paging), stay in the current tab.
2. **Compound Batching Ceiling**:
   Target standard tasks within a **1 to 2 compound turn ceiling** using `execute_browser_pipeline`.
3. **Anti-Bloat DOM Queries**:
   Never dump full `document.body.innerHTML` or `document.querySelectorAll('*')`. Use targeted selectors or scoped evaluations.
4. **Token-Efficient Session Queries**:
   `list_browser_sessions` returns compact objects by default (`id`, `title`, `url`, `active`). Only pass `"detailed": true` if low-level CDP flags are required.

*(See [`AGENT_INSTRUCTIONS.md`](AGENT_INSTRUCTIONS.md) for the copy-pasteable system prompt block).*

---

## Troubleshooting & FAQ

### "No active browser session detected"
- Make sure Google Chrome is open and the ChromaPilot extension is loaded.
- Click the extension icon in Chrome and check the status indicator.
- Ensure the port in the extension matches the server port (default `9222`).
- Click **Attach Session** on any open tab.

### Port Conflicts or Custom Ports
- By default, the server listens on `9222`.
- You can specify any available port:
  ```bash
  ./mcp-server/target/release/chromapilot --port 9922
  ```
- The Chrome extension automatically probes candidate ports (`9222`, `9922`, `9223`, `9923`, `9224`, `9924`).

### Chrome Debugger Warning Bar
- Chrome displays a top banner: `"ChromaPilot started debugging this browser"`.
- This is a standard Google Chrome security measure for any tool utilizing the `chrome.debugger` API and cannot be suppressed by design.

### Service Worker Keepalive
- Manifest V3 extensions can suspend inactive service workers. ChromaPilot includes an automated 15-second heartbeat ping loop to ensure continuous, uninterrupted availability.

---

## Development & Testing

```bash
# Check compilation across targets
make test
# Or: cd mcp-server && cargo check && cargo test

# Build optimized release binary
make build

# Initialize and auto-register with all detected MCP clients
make install
# Or: ./setup.sh --install

# Run MCP server locally with trace logs
RUST_LOG=info cargo run --manifest-path mcp-server/Cargo.toml -- --port 9222

# Clean build artifacts
make clean
```

---

## License

ChromaPilot is open source software licensed under the **[Apache License, Version 2.0](LICENSE)**.
