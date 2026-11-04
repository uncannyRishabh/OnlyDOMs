# OnlyDOMs Architecture Specification

## 1. Executive Summary

OnlyDOMs is an enterprise-grade, high-performance browser automation architecture that enables autonomous AI coding agents to control, test, navigate, replay network cURLs, and debug authenticated Google Chrome sessions.

Unlike conventional headless automation libraries (e.g. Puppeteer, Playwright, Selenium) that spawn isolated, unauthenticated browser instances vulnerable to bot detection and captcha walls, OnlyDOMs operates directly inside the developer's live, authenticated Google Chrome instance with zero browser restarting and zero lost cookies or session states.

---

## 2. Decoupled Daemon + Multiplexed Client Model

```
┌─────────────────┐       ┌─────────────────┐       ┌─────────────────┐
│   Cursor IDE    │       │ Claude Desktop  │       │ Antigravity CLI │
│  (MCP Client 1) │       │  (MCP Client 2) │       │  (MCP Client 3) │
└────────┬────────┘       └────────┬────────┘       └────────┬────────┘
         │ stdio                   │ stdio                   │ stdio
         ▼                         ▼                         ▼
┌─────────────────────────────────────────────────────────────────────┐
│                       OnlyDOMs MCP Proxies                          │
│               (Lightweight, transient stdio bridges)                │
└──────────────────────────────────┬──────────────────────────────────┘
                                   │ Unix Domain Socket (/tmp/onlydoms.sock)
                                   │ or Windows TCP IPC (127.0.0.1:9220)
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│                         OnlyDOMs Daemon                             │
│  ├── Tab Registry & Lease Manager (Multi-Agent Tab Isolation)       │
│  ├── WebSocket & Report Hub (127.0.0.1:9222 /reports/<run_id>/)     │
│  ├── Pre-Flight HUD Broadcaster (hud_state.rs, 180-char _intent)    │
│  ├── Network Harvester & Rust cURL Replay (network_harvester.rs)    │
│  ├── Visual Report Compiler (report_builder.rs: WebP -> HTML -> PDF)│
│  └── Multi-Layer Blindspot Filter (filter/hud_sanitizer.rs)         │
└──────────────────────────────────┬──────────────────────────────────┘
                                   │ WebSocket Frame Transport
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│                 OnlyDOMs Chrome Extension (MV3)                     │
│  ├── background.js (chrome.debugger API, Network capture, HUD sync) │
│  └── content/ (Top-layer <mcp-agent-hud> pill & input_filler.js)    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 3. Core Backend Modules (`mcp-server/src/`)

1. **`filter/hud_sanitizer.rs`**:
   - Scrubs `<mcp-agent-hud>`, `id="__mcp_agent_hud_host__"`, and `data-mcp-internal-ignore="true"` from JSON DOM trees, flat CDP attribute arrays, and multiline `outerHTML` strings before resolving WebSocket responses.
2. **`tools/hud_state.rs`**:
   - Enforces the 180-character `_intent` ceiling, maintains the per-tab `AgentTaskState` and 25-item RPC telemetry ring buffer, and broadcasts instant pre-flight and step-by-step `HUD_STATE_UPDATE` frames (`Executing :: <cmd> on Tab :: #<id> (<title>)`).
3. **`tools/network_harvester.rs` & `tools/curl_runner.rs`**:
   - Stores up to 50 XHR/Fetch/Document requests per tab (`< 2MB` memory ceiling), correlates requests occurring within `3000ms` of an agent step (`rpc_id`), serializes shell-safe POSIX `curl` commands, and replays them via pure-Rust `shlex` + native HTTP or in-tab `fetch` (`replay_curl`).
4. **`tools/report_builder.rs` & `websocket_hub.rs`**:
   - Saves cloaked `webp` (`quality: 75`) screenshots to `test_reports/<run_id>/assets/step_<n>.webp`, compiles `overview.html` and `overview.pdf`, and serves openable and `?print=1` auto-print links on `http://127.0.0.1:9222/reports/<run_id>/`.
