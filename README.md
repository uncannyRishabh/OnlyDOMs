# OnlyDOMs ⚡

>  **"Tip 500 prompt tokens to watch Chrome perform custom clicks in a private session."**

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Chrome MV3](https://img.shields.io/badge/Chrome-Manifest_V3-green.svg?style=flat-square&logo=googlechrome)](extension/)
[![Protocol](https://img.shields.io/badge/Protocol-Model_Context_Protocol-purple.svg?style=flat-square)](https://modelcontextprotocol.io/)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux%20%7C%20Windows-blue.svg?style=flat-square)](#quick-start)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg?style=flat-square)](LICENSE)

**Purpose-built for modern agentic SDLC and autonomous browser operations** — enabling AI coding agents to visually inspect, test, navigate, and debug authenticated web apps with radical token efficiency and zero bot friction.

Powered by a lightweight native **Rust MCP daemon** and a **Manifest V3 Google Chrome Extension**.

---

## ⚡ Why OnlyDOMs?

Most AI browser tools spin up a sterile, headless Chromium instance. This breaks logged-in sessions, triggers Cloudflare captchas, drops cookies, and burns tens of thousands of tokens per session.

**OnlyDOMs operates directly inside your live, authenticated Google Chrome browser.** Your GitHub sessions, Google Workspace, staging environments, internal VPN dashboards, and 2FA logins remain active and accessible to your coding agent.

### Core Highlights & Enhancements
- 🦀 **Radical Token Economy (Rust-Layer Sanitization)**:
  - Tab URLs automatically have `http://`, `https://`, and verbose query parameters pruned (~60% token reduction).
  - DOM tree inspections strip inline CSS noise (`style="..."`) and drop all `null` attributes.
  - Context-aware text truncation: DOM traversal `chain` arrays cap `textContent` at 100 characters to eliminate paragraph bloat, while preserving full text for targeted queries.
- 📦 **Compound Pipeline Protocol (`execute_browser_pipeline`)**: Batches navigation, element awaits, scrolling, typing, and semantic clicks into a **single compound turn**, eliminating multi-turn LLM latency loops and saving up to **80% token costs**.
- 🔀 **Decoupled Daemon + Multiplexed Client Model**: A shared background daemon maintains the live Chrome WebSocket connection. Multiple IDE chats (Cursor, Claude, Windsurf) run concurrently with **exclusive tab leases** and zero port collision conflicts.
- 🧪 **Automated Testing Engine (`run_browser_test`)**: Turns plain English test descriptions into deterministic browser validation runs with visual step-by-step screenshots, producing clean markdown and JSON reports in `test_reports/`.
- 🛡️ **Framework-Safe Input Simulation**: Synthetic event dispatch compatible with React, Vue, Angular, and complex custom design systems.
- 📸 **Compositor Screenshots**: Captures clean web content directly from the compositor, excluding browser chrome and DevTools banners. Saves directly to disk to prevent context window bloat.

---

## 📊 Token Economy & Speed Comparison

| Feature | Conventional Browser MCPs | OnlyDOMs 🔞⚡ | Advantage |
| :--- | :--- | :--- | :--- |
| **Execution Architecture** | Headless Node / Python subprocess | Native Rust Daemon + MV3 Extension | **< 10MB RAM**, sub-millisecond dispatch |
| **Session & Auth State** | Ephemeral, sterile sessions (captcha blocked) | Live everyday Chrome session | **Zero login friction**, 2FA preserved |
| **Workflow Roundtrips** | 5 – 10 turn-by-turn LLM roundtrips | 1 compound pipeline turn | **80% fewer inference turns** |
| **URL Context Size** | Full URLs with verbose tracking queries | Protocol & query params pruned in Rust | **60% smaller URL tokens** |
| **DOM Inspection Noise** | Megabytes of inline CSS & null values | Inline `style` stripped, nulls pruned | **70 – 85% token reduction** |
| **Multi-Agent Support** | Port conflicts, single-agent lockout | Multi-client IPC + Tab Lease Manager | **Full concurrent agent isolation** |
| **Cross-Platform** | Unix-biased wrappers | macOS, Linux, Windows native | **Native `.bat` and Unix `.sh`** |

---

## 🚀 1-Line Autonomous Setup

When dropping this repository link into any AI coding agent (Claude Code, Cursor, Windsurf, Antigravity, Devin, Copilot) or setting up locally:

### macOS / Linux
```bash
./setup.sh --install
```

### Windows (cmd / PowerShell)
```cmd
setup.bat --install
```

> **Autonomously performs**:
> 1. Prerequisites check & prompt-less non-interactive `rustup` install if Cargo is missing.
> 2. Compiles optimized release binary (`onlydoms` / `onlydoms.exe`).
> 3. Generates project workspace configs (`.cursor/mcp.json`, `.vscode/mcp.json`, `.idea/mcp.json`, `.windsurf/mcp.json`).
> 4. Auto-registers with installed agent clients (Claude Desktop, Windsurf, JetBrains, Antigravity, Claude Code CLI).

---

## 🧩 Load the Chrome Extension (One-Time)

1. Open Chrome and go to `chrome://extensions/`
2. Enable **Developer mode** (toggle in upper right corner).
3. Click **Load unpacked** and select the [`extension/`](extension/) directory from this repository.
4. The OnlyDOMs icon in your toolbar will show 🟢 **Connected** (Port `9222`).

*Terminal shortcut to launch Chrome with the extension loaded:*
```bash
# macOS
open -a "Google Chrome" --args --load-extension="$(pwd)/extension"

# Linux
google-chrome --load-extension="$(pwd)/extension"
```

---

## 🔌 AI Apps & IDE Integration Matrix

OnlyDOMs is pre-configured for every major agentic coding environment:

| Agent / IDE | Configuration Location | Auto-Configured? |
| :--- | :--- | :---: |
| **Cursor** | `.cursor/mcp.json` | ✔ Project Zero-Config |
| **VS Code / Cline / Roo** | `.vscode/mcp.json` | ✔ Project Zero-Config |
| **Windsurf** | `~/.codeium/windsurf/mcp_config.json` | ✔ Via `--install` |
| **Claude Desktop** | `~/Library/Application Support/Claude/claude_desktop_config.json` | ✔ Via `--install` |
| **Claude Code CLI** | `claude mcp add onlydoms` | ✔ Via `--install` |
| **JetBrains / IntelliJ** | `~/Library/Application Support/JetBrains/mcp.json` | ✔ Via `--install` |
| **Antigravity / Gemini** | `~/.gemini/config/mcp_config.json` | ✔ Via `--install` |

### Manual MCP Server Configuration
```json
{
  "mcpServers": {
    "onlydoms": {
      "command": "/absolute/path/to/onlydoms-browser-automation/mcp-server/target/release/onlydoms",
      "args": ["--port", "9222"]
    }
  }
}
```

---

## ⚡ First-Class Terminal CLI (Zero-Agent Mode)

When MCP tools are detached or when working directly from your shell, OnlyDOMs provides first-class native CLI subcommands:

```bash
# Check daemon health, port, extension status, and active tab leases
onlydoms status

# Open a URL in a new attached tab
onlydoms open "https://github.com"

# List active tabs (with sanitized URLs and lock statuses)
onlydoms tabs

# Evaluate JavaScript expression directly on the active tab
onlydoms eval "document.title"

# Evaluate JavaScript on a specific tab
onlydoms eval "window.location.pathname" --tab-id 12345

# Dispatch a compound pipeline directly from shell
onlydoms pipeline '[{"action":"navigate","url":"https://example.com"},{"action":"wait_for","selector":"h1"}]'

# Start daemon in background explicitly
onlydoms daemon --port 9222

# Stop running daemon
onlydoms stop
```

---

## 📚 Complete Wiki & Deep References

All deep architectural designs, workflows, test specs, and tool schemas have been organized into the **`wiki/`** documentation:

- 🏗️ **[Architecture & Multi-Agent Tab Lease Isolation](wiki/ARCHITECTURE.md)**: Deep dive into the decoupled daemon, IPC streaming, WebSocket relay, and collision-prevention tab leasing.
- ⚡ **[High-Leverage Agent Workflows & Prompt Catalog](wiki/AGENT_WORKFLOWS.md)**: Real-world prompts and compound workflows for SDLC QA, Datadog/AWS CloudWatch monitoring, social platforms, and SaaS administration.
- 🧪 **[Automated Browser Testing Engine (`run_browser_test`)](wiki/AUTOMATED_TESTING.md)**: How to convert plain English bug reports into visual regression suites with markdown/JSON reporting.
- 🛠️ **[15-Tool MCP Specification & Schemas](wiki/MCP_SPECIFICATION.md)**: Exhaustive schema documentation for all 15 MCP tools and Rust-layer sanitization rules.

---

## 🛠️ Build & Test

```bash
# Build release binary
cargo build --release --manifest-path mcp-server/Cargo.toml

# Run unit and integration tests
cargo test --manifest-path mcp-server/Cargo.toml
```

---

## 📄 License

Apache License 2.0. See [LICENSE](LICENSE) for details.
