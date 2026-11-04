# Automated Testing & Visual Overview Pipeline (`run_browser_test`)

The OnlyDOMs automated testing engine (`mcp-server/src/tools/testing.rs` & `report_builder.rs`) lets AI coding agents and developers convert plain English defect or feature descriptions into deterministic browser validation runs with a full **SS -> HTML -> PDF -> Openable Print Link** pipeline.

---

## 1. Overview

Agents call `run_browser_test` with a 180-char `_intent` preview and a natural language description or structured steps:

```json
{
  "name": "run_browser_test",
  "arguments": {
    "_intent": "Validate SPRING25 promo code discount calculation on staging cart and generate PDF overview",
    "run_id": "checkout_discount_verification",
    "test_name": "checkout_discount_code_verification",
    "description": "1. Navigate to https://store.example.com/cart\n2. Fill input[name='coupon'] with 'SPRING25'\n3. Click button.apply-coupon\n4. Assert .discount-applied is visible",
    "full_page_screenshots": false
  }
}
```

---

## 2. Visual Overview Document Pipeline (`SS -> HTML -> PDF -> Print Link`)

When `run_browser_test` is invoked:

1. **Pre-Flight & Step-by-Step HUD Telemetry (`HUD_STATE_UPDATE`)**:
   - Broadcasts `Executing :: <step.action>(<target>) on Tab :: #<id> (<title>)` before each step and correlates any XHR/Fetch/Document network requests within `3000ms` via `step_rpc_id`.
2. **Zero-Base64 Disk Asset Pipeline (`report_builder.rs`)**:
   - Saves each step's cloaked compositor screenshot (`webp`, `quality: 75`) directly to `test_reports/<run_id>/assets/step_<n>.webp`.
3. **Self-Contained Print-Styled HTML Compiler (`overview.html`)**:
   - Compiles step intents (`_intent`), executed commands, correlated optimistic cURL snippets, and `<img src="assets/step_<n>.webp">` into `test_reports/<run_id>/overview.html` with `@media print` page-break rules and zero HUD traces.
4. **Silent CDP `Page.printToPDF` Generation (`overview.pdf`)**:
   - Renders `test_reports/<run_id>/overview.pdf` silently with `printBackground: true, preferCSSPageSize: true`.
5. **Openable & Auto-Print Links on Daemon Port `9222` (`websocket_hub.rs`)**:
   - **HTML Report**: `http://127.0.0.1:9222/reports/<run_id>/overview.html`
   - **PDF Document**: `http://127.0.0.1:9222/reports/<run_id>/overview.pdf`
   - **Instant Print Dialog Link**: `http://127.0.0.1:9222/reports/<run_id>/overview.html?print=1` (auto-triggers `window.print()` on load).
   - Also writes `test_summary.md` and `test_report.json` in `test_reports/<run_id>/`.
