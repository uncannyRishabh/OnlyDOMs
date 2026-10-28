# OnlyDOMs Architecture Specification

## 1. Executive Summary

OnlyDOMs is an enterprise-grade, high-performance browser automation architecture that enables autonomous AI coding agents to control, test, navigate, and debug authenticated Google Chrome sessions.

Unlike conventional headless automation libraries (e.g. Puppeteer, Playwright, Selenium) that spawn isolated, unauthenticated browser instances vulnerable to bot detection and captcha walls, OnlyDOMs operates directly inside the developer's live, authenticated Google Chrome instance with zero browser restarting and zero lost cookies or session states.

---

## 2. Decoupled Daemon + Multiplexed Client Model

Traditional MCP servers run as standalone processes launched directly by an IDE or chat client via standard I/O (`stdio`). This naive approach creates fundamental limitations in multi-agent environments:
1. **Single-Client Lockout**: Only one agent or chat session can bind to Chrome's communication port at a time.
2. **Context Teardown**: When an agent finishes a turn or a user closes an IDE tab, the MCP process dies and terminates the browser connection.
3. **Session Collision**: Concurrent agents attempting to automate the browser fight over active tabs and race state changes.

OnlyDOMs resolves this with a **Decoupled Daemon + Multiplexed Client Architecture**:

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
│  ├── WebSocket Hub (ws://127.0.0.1:9222/mcp-relay)                  │
│  ├── CDP Command Multiplexer & Response Dispatcher                  │
│  └── Rust Layer Optimizers (Sanitization, CSS Strip, Text Truncate) │
└──────────────────────────────────┬──────────────────────────────────┘
                                   │ WebSocket Frame Transport
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│                 OnlyDOMs Chrome Extension (MV3)                     │
│  ├── background.js (chrome.debugger API multiplexing)               │
│  └── content/input_filler.js (Framework-safe DOM input simulation)  │
└──────────────────────────────────┬──────────────────────────────────┘
                                   │ Chrome DevTools Protocol (CDP)
                                   ▼
┌─────────────────────────────────────────────────────────────────────┐
│                       Live Google Chrome                            │
│           (Active tabs, user sessions, credentials intact)          │
└─────────────────────────────────────────────────────────────────────┘
```

### Components

1. **OnlyDOMs Daemon (`onlydoms daemon --port 9222`)**:
   - Long-lived background process holding the single WebSocket connection to the Chrome Extension.
   - Manages tab registries, leases, and command routing.
   - Auto-spawned on demand if an MCP proxy starts when the daemon is not running.
2. **OnlyDOMs MCP Proxy (`onlydoms`)**:
   - Lightweight binary launched by AI agent IDEs (VS Code, Cursor, Windsurf, Claude Desktop).
   - Reads JSON-RPC 2.0 requests over `stdin`, forwards them across the local IPC stream to the daemon, and writes responses back over `stdout`.
3. **Cross-Platform IPC Stream**:
   - **macOS / Linux**: High-throughput Unix Domain Sockets (`/tmp/onlydoms.sock`) with zero networking overhead.
   - **Windows**: Local TCP loopback (`127.0.0.1:9220`) with automatic port resolution and error recovery.

---

## 3. Multi-Agent Tab Lease Isolation

OnlyDOMs is purpose-built for multi-agent workflows where multiple AI agents run simultaneously across different tasks.

```
Agent A (Frontend QA)       ──► Leases Tab 101 (localhost:3000) ──► EXCLUSIVE LOCK
Agent B (API Monitoring)     ──► Leases Tab 102 (datadog.com)    ──► EXCLUSIVE LOCK
Agent C (New Agent)          ──► Attempts Tab 101               ──► REJECTED (Collision Protected)
                             ──► Spawns Tab 103 (github.com)    ──► LEASE GRANTED
```

### Isolation Guarantees:
- **Exclusive Tab Leases**: When an agent performs any action on a tab (navigation, click, evaluation, testing), that tab is assigned an exclusive agent lease.
- **Collision Prevention**: Other agents cannot switch to, inspect, or modify a leased tab. Requests to interact with another agent's tab return an explicit error and recommend targeting an unreserved tab or calling `create_tab`.
- **Automatic Lease Release**: When an agent session terminates or disconnects, all leases held by that agent are immediately released back to the general tab pool.

---

## 4. Rust-Layer Performance & Sanitization Pipeline

To eliminate context bloat and token waste, all incoming DevTools and DOM data passes through an aggressive Rust-layer sanitization engine before reaching the agent:

1. **URL Sanitization**:
   - Tab URLs automatically have protocol schemes (`http://`, `https://`) and redundant query parameters pruned, saving ~60% tokens on navigation payloads.
2. **Inline CSS Elimination**:
   - Element attributes strip all inline `style="..."` declarations, eliminating thousands of tokens of CSS clutter in DOM inspection calls.
3. **Null Attribute Pruning**:
   - DOM tree and debugger nodes with `null` values are omitted entirely from the serialized JSON output.
4. **Context-Aware Text Truncation**:
   - In DOM traversal chains (`chain` arrays), element `textContent` is capped at 100 characters to prevent multi-paragraph bloat.
   - Standalone, top-level text queries retain full text to ensure accuracy when reading critical page content.
