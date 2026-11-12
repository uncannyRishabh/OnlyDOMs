# OnlyDOMs ⚡

>  **"Tip 500 prompt tokens to watch Chrome perform custom clicks in a private session."**

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Chrome MV3](https://img.shields.io/badge/Chrome-Manifest_V3-green.svg?style=flat-square&logo=googlechrome)](extension/)
[![Protocol](https://img.shields.io/badge/Protocol-Model_Context_Protocol-purple.svg?style=flat-square)](https://modelcontextprotocol.io/)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Linux%20%7C%20Windows-blue.svg?style=flat-square)](#quick-start)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg?style=flat-square)](LICENSE)

**Purpose-built for modern agentic SDLC and autonomous browser operations** — enabling AI coding agents to visually inspect, test, navigate, replay network cURLs, and debug authenticated web apps with radical token efficiency and zero bot friction.

Powered by a lightweight native **Rust MCP daemon** and a **Manifest V3 Google Chrome Extension**.

---

## ⚡ Why OnlyDOMs?

Most AI browser tools spin up a sterile, headless Chromium instance. This breaks logged-in sessions, triggers Cloudflare captchas, drops cookies, and burns tens of thousands of tokens per session.

**OnlyDOMs operates directly inside your live, authenticated Google Chrome browser.** Your GitHub sessions, Google Workspace, staging environments, internal VPN dashboards, and 2FA logins remain active and accessible to your coding agent.

### Core Highlights & Enhancements
- 🎯 **Zero-Latency Task Intent Preview (`_intent`) & Modern Agent HUD**:
  - Every MCP tool accepts a 180-character (~30-word) `_intent` parameter.
  - The microsecond `mcp_protocol.rs` parses a `tools/call` request, it dispatches a pre-flight `HUD_STATE_UPDATE` frame (`Executing :: <cmd> on Tab :: #<id> (<title>)`) before awaiting the first CDP action, updating across every step of `execute_browser_pipeline` and `run_browser_test`.
  - **Modern Web Guidance Compliant Glassmorphic HUD**: Floating, non-invasive top-layer pill (`<mcp-agent-hud>`) with closed Shadow DOM isolation, live status breathing ring, click-to-expand telemetry drawer, RPC execution stream, optimistic cURL inspector, interactive pushpin toggle with distinct pinned (filled cyan glow) and unpinned (outlined stroke) states, smart unpin auto-hide lifecycle, pointer-capture draggable repositioning, and instant dismiss controls.
  - **Accordion Telemetry with Scroll Containment**: Smooth single-open accordion drawers with `flex-shrink: 0` preventing sibling card compression, and dedicated nested scrolling (`overflow-y: auto`, `overscroll-behavior: contain`) for long Intent, Agent Request, and Browser Generated Response payloads.
- 🛡️ **Multi-Layer Agent Blindspot Protection (`filter::hud_sanitizer`)**:
  - Every WebSocket response from the extension is scrubbed in Rust (`hud_sanitizer::sanitize_extension_payload`) to strip `<mcp-agent-hud>`, `#__mcp_agent_hud_host__`, and `data-mcp-internal-ignore="true"` from JSON DOM trees, flat CDP attribute arrays, and raw `outerHTML` strings.
  - Compositor screenshots and PDFs automatically cloak the HUD, and coordinate/quadrant clicks (including bottom-right cell `P9`) pass cleanly through to the underlying page.
- 🎛️ **Modern Extension Popup Control Center**:
  - Real-time connection badge with live daemon status and port indicator.
  - Active agent session lease monitor with one-click individual tab detach and emergency "Detach All" killswitch.
  - Site policy blacklist manager with interactive syntax templates (`*.wildcard`, `exact.com`, `prefix://`) and instant URL enforcement.
  - Expandable daemon port override drawer with automated candidate scanning (`9222`, `9922`, `9223`, `9923`, `9224`, `9924`).
- 🌐 **Optimistic Network Capture, Action Correlation & Zero-Subprocess Replay (`replay_curl` & `get_network_traffic`)**:
  - Captures XHR, Fetch, and Document calls (ignoring images, fonts, and stylesheets) within 3000ms of an agent step into a bounded 50-item ring buffer (`< 2MB` ceiling) per tab.
  - **Action-to-cURL Correlation Binding**: Directly associates every captured network request with the active agent tool call (`rpc_id`, `tool_name`, `intent`), displaying triggering action tags in the HUD Network tab and allowing single-open accordion inspection of full request headers/body and browser response status/headers/body.
  - **Agent Query Interface**: Agents can invoke `get_network_traffic` (or `replay_curl` with `action="list"` / `"get_network_traffic"`) filtering by `tab_id`, `action_id` / `rpc_id`, `request_id`, or `url_filter` to inspect complete request and response payloads.
  - Serializes shell-safe POSIX `curl` commands and replays them (or modified raw `curl` strings parsed via pure-Rust `shlex`) natively in Rust or via in-tab `fetch` without ever invoking `sh -c curl`.
- 📄 **Visual Test & Feature Overview Document Pipeline (`run_browser_test` & `report_builder`)**:
  - Saves cloaked step screenshots (`webp`, `quality: 75`) directly to `test_reports/<run_id>/assets/step_<n>.webp` (zero base64 context bloat).
  - Compiles step intents, executed commands, and correlated cURL snippets into `overview.html` (with `@media print` page-break rules) and silently renders `overview.pdf` via CDP `Page.printToPDF`.
  - Serves openable and instant-print links directly from the daemon on Port `9222`:
    - `http://127.0.0.1:9222/reports/<run_id>/overview.html`
    - `http://127.0.0.1:9222/reports/<run_id>/overview.pdf`
    - `http://127.0.0.1:9222/reports/<run_id>/overview.html?print=1` (auto-triggers `window.print()` on load).
- 🦀 **Radical Token Economy (Rust-Layer Sanitization)**:
  - Tab URLs automatically have `http://`, `https://`, and verbose query parameters pruned (~60% token reduction).
  - DOM tree inspections strip inline CSS noise (`style="..."`) and drop all `null` attributes.
  - Context-aware text truncation: DOM traversal `chain` arrays cap `textContent` at 100 characters to eliminate paragraph bloat, while preserving full text for targeted queries.
- 📦 **Compound Pipeline Protocol (`execute_browser_pipeline`)**: Batches navigation, element awaits, scrolling, typing, semantic/quadrant clicks, PDF generation, dialog handling, and cURL replay into a **single compound turn**, saving up to **80% token costs**.
- 🔀 **Decoupled Daemon + Multiplexed Client Model**: A shared background daemon maintains the live Chrome WebSocket connection. Multiple IDE chats (Cursor, Claude, Windsurf, Gemini) run concurrently with **exclusive tab leases** and zero port collision conflicts.

---

## 📊 Token Economy & Speed Comparison

| Feature | Conventional Browser MCPs | OnlyDOMs 🔞⚡ | Advantage |
| :--- | :--- | :--- | :--- |
| **Execution Architecture** | Headless Node / Python subprocess | Native Rust Daemon + MV3 Extension | **< 10MB RAM**, sub-millisecond dispatch |
| **Session & Auth State** | Ephemeral, sterile sessions (captcha blocked) | Live everyday Chrome session | **Zero login friction**, 2FA preserved |
| **Workflow Roundtrips** | 5 – 10 turn-by-turn LLM roundtrips | 1 compound pipeline turn + `_intent` | **80% fewer inference turns** |
| **Network Replay** | Unsafe `sh -c curl` or manual copy-paste | Native Rust `shlex` + `replay_curl` ring buffer | **Zero subprocess overhead**, shell-safe |
| **DOM & HUD Sanitization** | Megabytes of inline CSS & overlay pollution | `hud_sanitizer.rs` + CSS/null pruning | **70 – 85% token reduction**, zero HUD leaks |
| **Visual QA Reports** | Raw base64 screenshots in LLM context | Disk WebP + `overview.html` + `overview.pdf` | **Openable & auto-print links on :9222** |
| **Multi-Agent Support** | Port conflicts, single-agent lockout | Multi-client IPC + Tab Lease Manager | **Full concurrent agent isolation** |

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

---

## 🧩 Load the Chrome Extension (One-Time)

1. Open Chrome and go to `chrome://extensions/`
2. Enable **Developer mode** (toggle in upper right corner).
3. Click **Load unpacked** and select the [`extension/`](extension/) directory from this repository.
4. The OnlyDOMs icon in your toolbar will show 🟢 **Connected** (Port `9222`).

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

# Dispatch a compound pipeline directly from shell
onlydoms pipeline '[{"action":"navigate","url":"https://example.com"},{"action":"wait_for","selector":"h1"}]'

# Start daemon in background explicitly
onlydoms daemon --port 9222

# Stop or reload running daemon
onlydoms reload
onlydoms stop
```

---

## 📚 Complete Wiki & Deep References

All deep architectural designs, workflows, test specs, and tool schemas are organized in [`MCP_SPECIFICATION.md`](MCP_SPECIFICATION.md) and the **`wiki/`** directory:

- 🏗️ **[Architecture & Multi-Agent Tab Lease Isolation](wiki/ARCHITECTURE.md)**: Deep dive into the decoupled daemon, IPC streaming, WebSocket relay, and collision-prevention tab leasing.
- ⚡ **[High-Leverage Agent Workflows & Prompt Catalog](wiki/AGENT_WORKFLOWS.md)**: Real-world prompts and compound workflows for SDLC QA, Datadog/AWS CloudWatch monitoring, social platforms, and SaaS administration.
- 🧪 **[Automated Browser Testing & Visual Overview Pipeline (`run_browser_test`)](wiki/AUTOMATED_TESTING.md)**: How to convert plain English bug reports into visual regression suites with `overview.html`, `overview.pdf`, and auto-print links on port `9222`.
- 🛠️ **[MCP Tool & Backend Specification](wiki/MCP_SPECIFICATION.md)**: Complete schema documentation for `_intent`, `HUD_STATE_UPDATE`, `replay_curl`, `report_builder`, and `hud_sanitizer`.

---

## 🛠️ Build & Test

```bash
# Build release binary
cargo build --release --manifest-path mcp-server/Cargo.toml

# Run unit and integration tests (including hud_sanitizer, cURL replay, and visual report E2E suites)
cargo test --manifest-path mcp-server/Cargo.toml
cargo test --manifest-path mcp-server/Cargo.toml filter::hud_sanitizer
```

---

## 📄 License

Apache License 2.0. See [LICENSE](LICENSE) for details.
