# MagiBot — Lightweight Rust MCP Server & Chrome Extension

> **⚠️ USER DISCRETIONARY WARNING:**
> This agent possesses capabilities to actively inspect, navigate, execute arbitrary JavaScript, manipulate the DOM, and control your active browser session. It can view sensitive information on open pages. Use strictly in trusted environments and operate responsibly.

## Overview

MagiBot is a high-performance browser automation system consisting of:

1. **Rust MCP Server** — Communicates with AI agents via MCP (JSON-RPC over stdio) and bridges commands to Chrome through a WebSocket hub.
2. **Chrome Extension (Manifest V3)** — Connects to the WebSocket hub, manages `chrome.debugger` sessions, and injects content scripts for grid overlays and framework-safe input filling.

### Architecture

```
[AI Agent (e.g., Antigravity)]
│ (JSON-RPC over stdio)
▼
[Rust MCP Server] ← tokio, tokio-tungstenite, serde_json
│ (WebSocket: ws://127.0.0.1:9222/mcp-relay)
▼
[Chrome Extension (MV3)]
├── background.js — Service Worker: chrome.debugger, tab registry, WebSocket
├── content/overlay_manager.js — Adaptive 16×N grid overlay
└── content/input_filler.js — Framework-safe DOM input filling
```

## Setup

### 1. Build the Rust MCP Server

```bash
cd mcp-server
cargo build --release
```

The binary will be at `target/release/magibot-mcp`.

### 2. Install the Chrome Extension

1. Open `chrome://extensions/` in Chrome.
2. Enable **Developer mode** (toggle in the top right).
3. Click **Load unpacked** and select the `extension/` directory.
4. The MagiBot icon will appear in the toolbar.

### 3. Run the Server

```bash
# Default port 9222
./mcp-server/target/release/magibot-mcp

# Custom port
./mcp-server/target/release/magibot-mcp --port 8080
```

### 4. Connect the Extension

1. Click the MagiBot extension icon in Chrome.
2. Set the port if not using the default (9222).
3. Click **Attach Session** to attach the debugger to the active tab.
4. The status indicator will show 🟢 Connected when the WebSocket is live.

## MCP Tools

| Tool | Description |
|------|-------------|
| `list_browser_sessions` | Lists connected browser tabs |
| `select_session` | Attaches debugger to a specific tab by ID |
| `capture_viewport` | Screenshots the viewport with an adaptive 16×N grid overlay |
| `click_quadrant` | Clicks at a grid cell (e.g., `D3`) with optional sub-cell offset |
| `fill_inputs` | Framework-safe batch form filling (React/Angular/Vue compatible) |
| `interact` | Click, hover, or scroll by normalized coordinates (0.0–1.0) |
| `execute_browser_pipeline` | Batch multi-step pipeline (navigate, wait, fill, screenshot) |
| `evaluate_expression` | Execute JavaScript via CDP `Runtime.evaluate` |
| `add_breakpoint` | Set a debugger breakpoint by URL regex and line number |
| `pause_execution` | Pause JavaScript execution |
| `resume_execution` | Resume JavaScript execution |
| `step_over` / `step_into` | Debugger stepping controls |

## Grid Coordinate System

The viewport is divided into a **16×N adaptive grid**:
- **16 columns** labeled `A` through `P`
- **N rows** dynamically calculated: `rows = round(16 × height / width)`
- A standard 16:9 viewport yields a 16×9 grid (cells `A1` to `P9`)

Cell coordinates are resolution-independent — they map to true CSS pixels regardless of DPR or screenshot compression.

### Example: Click a Button

```json
{"method": "tools/call", "params": {"name": "click_quadrant", "arguments": {"cell": "D3"}}}
```

### Example: Fill a Form

```json
{"method": "tools/call", "params": {"name": "fill_inputs", "arguments": {
  "fields": [
    {"selector": "#email", "value": "user@example.com"},
    {"selector": "#password", "value": "secret", "press_enter": true}
  ]
}}}
```

### Example: Multi-step Pipeline

```json
{"method": "tools/call", "params": {"name": "execute_browser_pipeline", "arguments": {
  "steps": [
    {"action": "navigate", "url": "https://example.com/login"},
    {"action": "wait_for", "selector": "#email", "timeout_ms": 5000},
    {"action": "fill_inputs", "fields": [{"selector": "#email", "value": "user@example.com"}]},
    {"action": "capture_grid_screenshot", "quality": 80}
  ]
}}}
```

## Development

```bash
# Check compilation
cd mcp-server && cargo check

# Run with logging
RUST_LOG=info cargo run -- --port 9222

# Release build (optimized for size)
cargo build --release
```

## Security Notes

- The `chrome.debugger` API grants near-total control over a browser tab. Chrome displays a warning bar ("MagiBot started debugging this browser") which cannot be suppressed.
- This tool is designed for **local development and testing environments only**.
- Do not use on tabs containing sensitive information (banking, email, etc.) in untrusted environments.
- The WebSocket server binds to `127.0.0.1` only — not accessible from the network.
