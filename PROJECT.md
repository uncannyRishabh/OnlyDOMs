# Project: OnlyDOMs Master Specification — Top-Layer Agent HUD, Blindspot Sanitizer, cURL Replay & Visual PDF Pipeline

## Architecture
OnlyDOMs consists of a modular Rust MCP server/daemon (`mcp-server/`) and a Manifest V3 Chrome Extension (`extension/`).
- **Rust MCP Server / Daemon (`mcp-server/`)**:
  - `src/mcp_protocol.rs`: Exposes JSON-RPC 2.0 MCP tools with mandatory 180-char `_intent` schema and instant pre-flight `HUD_STATE_UPDATE` broadcast.
  - `src/websocket_hub.rs`: Hosts WebSocket Hub & HTTP `/reports/<run_id>/` server on `127.0.0.1:9222`, routes extension events, and enforces `hud_sanitizer::sanitize_extension_payload` on all incoming responses.
  - `src/filter/hud_sanitizer.rs`: Multi-layer application filter stripping `<mcp-agent-hud>`, `#__mcp_agent_hud_host__`, and `data-mcp-internal-ignore="true"` from JSON DOM trees, flat CDP attribute arrays, and raw `outerHTML` strings.
  - `src/tools/hud_state.rs`: Per-tab `AgentTaskState` store, 25-item RPC telemetry ring buffer, 180-char intent normalization, and `Executing :: <cmd> on Tab :: #<id> (<title>)` ticker formatting.
  - `src/tools/network_harvester.rs`: Per-tab 50-item `VecDeque<CapturedRequest>` ring buffer (`< 2MB` memory ceiling), XHR/Fetch/Document filtering, 3000ms `rpc_id` step correlation, and shell-safe POSIX `curl` serializer.
  - `src/tools/curl_runner.rs`: Zero-subprocess Rust `shlex` parser and HTTP/browser-fetch `replay_curl` engine.
  - `src/tools/report_builder.rs`: Zero-base64 WebP asset pipeline (`test_reports/<run_id>/assets/step_<n>.webp`), self-contained `@media print` HTML compiler (`overview.html`), silent CDP `Page.printToPDF` generator (`overview.pdf`), and port `9222` auto-print URL builder.
  - `src/tools/visual.rs`, `input.rs`, `navigation.rs`, `testing.rs`, `dialog.rs`, `debugger.rs`, `session.rs`: Core browser automation, HUD cloaking, and `P9` quadrant hit-test passthrough.

## Feature Inventory
| # | Feature | Module | Status |
|---|---------|--------|--------|
| F1-F15 | Core PDF, Update Checker, Warning Banner, Site Blocking, Dialogs & Popups | `visual.rs`, `update_checker.rs`, `dialog.rs`, `websocket_hub.rs` | DONE |
| F16 | 180-Char `_intent` Schema & Pre-Flight `HUD_STATE_UPDATE` Broadcast | `mcp_protocol.rs`, `tools/hud_state.rs` | DONE |
| F17 | Optimistic Network Ring Buffer (50 items, `< 2MB`, 3000ms correlation) | `tools/network_harvester.rs`, `websocket_hub.rs` | DONE |
| F18 | Zero-Subprocess Rust cURL Lexer (`shlex`) & Replay Engine (`replay_curl`) | `tools/curl_runner.rs`, `mcp_protocol.rs` | DONE |
| F19 | Visual Test & Feature Overview Pipeline (`webp` -> `overview.html` -> `overview.pdf` -> HTTP `:9222/reports/`) | `tools/report_builder.rs`, `tools/testing.rs`, `websocket_hub.rs` | DONE |
| F20 | Multi-Layer Agent Blindspot Protection (`hud_sanitizer`, `CLOAK_HUD`, `P9` Passthrough) | `filter/hud_sanitizer.rs`, `tools/input.rs`, `tools/visual.rs` | DONE |
