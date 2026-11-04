# TEST_INFRA: OnlyDOMs E2E Test Infrastructure & Specification

## 1. Test Philosophy: Opaque-Box, Requirement-Driven Testing

The OnlyDOMs automated testing infrastructure adheres strictly to an **opaque-box, requirement-driven philosophy**:
- **Black-Box Verification**: Tests validate observable system behavior (JSON-RPC inputs and outputs, CDP wire protocol commands, file artifacts, and error structures) rather than private internal implementation details.
- **Requirement-Driven**: Every test case directly derives from user requirements in `ORIGINAL_REQUEST.md` and architectural specifications in `PROJECT.md`.
- **Deterministic & Fast Execution**: All tests execute in-memory using `tokio::io::duplex` channels and mock protocol relays, eliminating external Google Chrome or network dependencies. The entire test suite completes in under 2 seconds.
- **Zero Silent Failure**: Errors produce explicit codes and structured error payloads (e.g. JSON-RPC `-32003`, `-32603`).

---

## 2. Feature Inventory Coverage Mapping

The test infrastructure covers all 5 core features across both Rust MCP daemon and Chrome Extension layers:

| Feature ID | Feature Name | Source Requirement | Primary Test Suite File | Covered Tiers |
|---|---|---|---|---|
| **F1 / F2** | HTML-to-PDF Conversion Tool & Pipeline Action | `ORIGINAL_REQUEST.md § R1`<br>`PROJECT.md § F1, F2` | `mcp-server/tests/pdf_e2e_test.rs` | Tiers 1, 2, 3, 4 |
| **F3 / F4** | Client Update Mechanism (Mtime & Git) | `ORIGINAL_REQUEST.md § R2`<br>`PROJECT.md § F3, F4` | `mcp-server/tests/client_update_e2e_test.rs` | Tiers 1, 2, 3, 4 |
| **F5 / F6** | Interacting Tab Warning Banner & Focus Shift | `ORIGINAL_REQUEST.md § R4`<br>`PROJECT.md § F5, F6` | `mcp-server/tests/tab_banner_test.rs` | Tiers 1, 2, 3, 4 |
| **F8 / F9** | Extension Popup Site Blocking & Enforcement | `ORIGINAL_REQUEST.md § R5`<br>`PROJECT.md § F8, F9` | `mcp-server/tests/site_blocking_e2e_test.rs` | Tiers 1, 2, 3, 4 |
| **F10-F13** | Dialog Detection & Programmatic Interaction | `ORIGINAL_REQUEST.md § R3`<br>`PROJECT.md § F10-F13` | `mcp-server/tests/dialog_e2e_test.rs` | Tiers 1, 2, 3, 4 |

---

## 3. Test Architecture & Runner Invocation

### 3.1 Test Architecture
```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Cargo Test Runner                                │
│                cargo test --manifest-path mcp-server/Cargo.toml             │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │
        ┌──────────────────────────────┼──────────────────────────────┐
        ▼                              ▼                              ▼
┌─────────────────────────┐  ┌─────────────────────────┐  ┌─────────────────────────┐
│ pdf_e2e_test.rs         │  │ dialog_e2e_test.rs      │  │ site_blocking_e2e_test  │
│ - %PDF- magic bytes     │  │ - alert/confirm/prompt   │  │ - Wildcard & exact pat  │
│ - Base64 decode         │  │ - Page.handleDialog      │  │ - Prefix match          │
│ - Disk saving & params  │  │ - Execution guard        │  │ - Explicit rejection    │
└─────────────────────────┘  └─────────────────────────┘  └─────────────────────────┘
        │                              │                              │
        └──────────────────────────────┼──────────────────────────────┘
                                       ▼
        ┌─────────────────────────────────────────────────────────────┐
        │ client_update_e2e_test.rs    │ tab_banner_test.rs           │
        │ - Mtime binary vs source     │ - Tab ID & Title tracking    │
        │ - Git commit comparisons     │ - Dynamic focus shift        │
        │ - Rebuild/reload status      │ - Shadow DOM banner protocol │
        └──────────────────────────────┴──────────────────────────────┘
```

### 3.2 Runner Invocation Commands

- **Run all E2E integration test suites**:
  ```bash
  cargo test --manifest-path mcp-server/Cargo.toml --test '*'
  ```
- **Run specific test suite**:
  ```bash
  cargo test --manifest-path mcp-server/Cargo.toml --test pdf_e2e_test
  cargo test --manifest-path mcp-server/Cargo.toml --test dialog_e2e_test
  cargo test --manifest-path mcp-server/Cargo.toml --test site_blocking_e2e_test
  cargo test --manifest-path mcp-server/Cargo.toml --test client_update_e2e_test
  cargo test --manifest-path mcp-server/Cargo.toml --test tab_banner_test
  ```
- **Run with full diagnostic output**:
  ```bash
  cargo test --manifest-path mcp-server/Cargo.toml -- --nocapture
  ```

---

## 4. Coverage Thresholds & Tier Taxonomy

To ensure industrial-grade test rigor, tests are stratified across five tiers:

### Tier 1: Primary Behavior (Happy Path)
- **Threshold**: $\ge 5$ distinct test cases per feature.
- **Scope**: Core functional contracts, valid inputs, expected output payloads, and success status codes.

### Tier 2: Boundary & Corner Conditions
- **Threshold**: $\ge 5$ boundary/corner test cases per feature.
- **Scope**: Min/max parameter extremes, empty buffers, invalid syntax, unicode/special characters, resource timeouts, and collision handling.

### Tier 3: Pairwise Combinations
- **Threshold**: Multi-factor orthogonal parameter matrices per feature.
- **Scope**: Cross-testing permutations of formats, flags, actions, and network/session states.

### Tier 4: Application Scenarios & Workflows
- **Threshold**: Full lifecycle end-to-end sequences.
- **Scope**: Realistic multi-step agent interaction sequences, state transitions across tabs, and failure recovery.

### Tier 5: Adversarial Stress & Hardening (Planned for M5 Phase 2)
- **Scope**: White-box stress testing, memory pressure, corrupted streams, concurrent race condition stress.

---

## 5. Coverage Matrix by Feature

| Feature | Tier 1 (Happy) | Tier 2 (Boundary) | Tier 3 (Pairwise) | Tier 4 (Scenario) | Total Tests |
|---|:---:|:---:|:---:|:---:|:---:|
| **PDF Conversion (F1, F2)** | 5 | 5 | 2 | 1 | **13** |
| **Dialog Interaction (F10, F11)** | 5 | 5 | 2 | 1 | **13** |
| **Site Blocking Engine (F8, F9)** | 5 | 5 | 2 | 1 | **13** |
| **Client Update Checker (F3, F4)** | 5 | 5 | 2 | 1 | **13** |
| **Warning Banner (F5, F6)** | 5 | 5 | 2 | 1 | **13** |
| **Total** | **25** | **25** | **10** | **5** | **65** |
