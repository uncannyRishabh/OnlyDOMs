# Automated Testing Engine (`run_browser_test`)

The OnlyDOMs automated testing engine lets AI coding agents and developers convert plain English defect or feature descriptions into deterministic browser validation runs with visual proof.

---

## 1. Overview

Rather than forcing agents to write brittle Playwright or Cypress test scripts from scratch, agents can call `run_browser_test` with a human description or structured steps:

```json
{
  "name": "run_browser_test",
  "arguments": {
    "test_name": "checkout_discount_code_verification",
    "description": "1. Navigate to https://store.example.com/cart\n2. Fill input[name='coupon'] with 'SPRING25'\n3. Click button.apply-coupon\n4. Assert .discount-applied is visible\n5. Assert total matches expected discount",
    "full_page_screenshots": false
  }
}
```

---

## 2. Execution Pipeline

When `run_browser_test` is invoked:

1. **Step Decomposition**:
   - The engine parses the natural language description or step list into atomic test steps.
2. **Deterministic Step Execution**:
   - Navigates, fills inputs, and performs semantic clicks with auto-scroll and retry resilience.
3. **Compositor Visual Capture**:
   - Captures high-resolution, compositor-level screenshots at each intermediate step and assertion point.
4. **Artifact Generation**:
   - Writes all evidence directly to disk in `test_reports/<test_name>_<timestamp>/`:
     - `step_1_navigate.png`
     - `step_2_fill_coupon.png`
     - `step_3_apply_click.png`
     - `step_4_assertion_verified.png`
     - `test_summary.md` (Human-readable markdown report)
     - `test_report.json` (Structured machine-readable test log)

---

## 3. Sample Generated Report (`test_summary.md`)

```markdown
# Test Execution Summary: checkout_discount_code_verification

- **Status**: PASSED
- **Timestamp**: 2026-10-28T06:15:22Z
- **Duration**: 2,340 ms
- **Steps Executed**: 4 / 4

## Step Details
1. **Navigate**: `https://store.example.com/cart` — ✔ Succeeded (820ms)
2. **Fill Input**: `input[name='coupon']` = 'SPRING25' — ✔ Succeeded (140ms)
3. **Click**: `button.apply-coupon` — ✔ Succeeded (310ms)
4. **Assert Visible**: `.discount-applied` — ✔ Succeeded (210ms)

## Visual Evidence
- Step 1: `test_reports/checkout_.../step_1.png`
- Step 2: `test_reports/checkout_.../step_2.png`
- Step 3: `test_reports/checkout_.../step_3.png`
- Step 4: `test_reports/checkout_.../step_4.png`
```

---

## 4. Key Advantages for AI Coding Agents

- **Zero Prompt Image Bloat**: Screenshots are saved directly to disk rather than fed back to the LLM as megabytes of base64 text, conserving context window tokens.
- **Fail-Fast Error Reporting**: If an assertion or selector fails, the engine captures a snapshot of the exact failure state with the DOM stack and returns an actionable diagnostic message.
- **Continuous Integration (CI) Ready**: Run the test engine via CLI in headless or headed environments to validate PRs before merging.
