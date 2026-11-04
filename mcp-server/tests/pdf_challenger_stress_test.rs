//! Empirical Challenger Stress Test Suite for Milestone 1: HTML-to-PDF & Pipeline Action
//!
//! Tests:
//! 1. Parameter bounds (scale = 0.05, 0.1, 2.0, 2.5; negative margins; margins exceeding paper dimensions).
//! 2. Corrupt/truncated base64 inputs and invalid PDF magic headers (strict non-%PDF- rejection, %%EOF trailer enforcement).
//! 3. Page range formatting ("1-3, 5", "abc", "-1", "99999", and edge-case syntax).
//! 4. Pipeline action execution (print_to_pdf combined with navigation and screenshot steps, plus pipeline error propagation).

use onlydoms::tools::visual::{
    decode_and_validate_pdf, parse_and_validate_pdf_params, validate_pdf_bytes,
};
use onlydoms::websocket_hub::HubState;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Minimal valid synthetic PDF fixture
const VALID_PDF_BASE64: &str = "JVBERi0xLjQKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFszIDAgUl0gL0NvdW50IDEgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL1BhZ2UgL1BhcmVudCAyIDAgUiAvTWVkaWFCb3ggWzAgMCA2MTIgNzkyXSA+PgplbmRvYmoKeHJlZgowIDQKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDA5IDAwMDAwIG4gCjAwMDAwMDAwNTggMDAwMDAgbiAKMDAwMDAwMDExNSAwMDAwMCBuIAp0cmFpbGVyCjw8IC9TaXplIDQgL1Jvb3QgMSAwIFIgPj4Kc3RhcnR4cmVmCjE4NwolJUVPRgo=";

/// Minimal valid PNG screenshot fixture (1x1 transparent)
const VALID_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

/// Spawns a mock CDP server that handles Page.navigate, Runtime.evaluate, Page.captureScreenshot, and Page.printToPDF
async fn setup_comprehensive_mock_cdp(
    pdf_response_provider: Arc<Mutex<Box<dyn FnMut() -> Result<String, String> + Send>>>,
) -> (Arc<Mutex<HubState>>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(Mutex::new(HubState::new()));
    let (tx_hub, mut rx_hub) = tokio::sync::mpsc::channel::<Value>(32);

    {
        let mut lock = state.lock().await;
        lock.tx = Some(tx_hub);
    }

    let state_clone = state.clone();
    let handle = tokio::spawn(async move {
        while let Some(msg) = rx_hub.recv().await {
            let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

            let response = match method {
                "Page.navigate" => serde_json::json!({
                    "id": id,
                    "result": { "frameId": "mock-frame-123" }
                }),
                "Runtime.evaluate" => serde_json::json!({
                    "id": id,
                    "result": {
                        "result": {
                            "type": "string",
                            "value": "https://example.com/tested-page"
                        }
                    }
                }),
                "Page.captureScreenshot" => serde_json::json!({
                    "id": id,
                    "result": {
                        "data": VALID_PNG_BASE64
                    }
                }),
                "Page.printToPDF" => {
                    let mut provider = pdf_response_provider.lock().await;
                    match (*provider)() {
                        Ok(b64) => serde_json::json!({
                            "id": id,
                            "result": {
                                "data": b64
                            }
                        }),
                        Err(err) => serde_json::json!({
                            "id": id,
                            "error": err
                        }),
                    }
                }
                _ => serde_json::json!({
                    "id": id,
                    "result": {}
                }),
            };

            let mut lock = state_clone.lock().await;
            if let Some(sender) = lock.pending.remove(&id) {
                let _ = sender.send(response);
            }
        }
    });

    (state, handle)
}

// =============================================================================
// CHALLENGE 1: PARAMETER BOUNDS STRESS TESTS
// =============================================================================

#[test]
fn stress_test_scale_bounds() {
    // 1. Min boundary exact (0.1) -> PASS
    let (opts, cdp) = parse_and_validate_pdf_params(&serde_json::json!({ "scale": 0.1 })).unwrap();
    assert!((opts.scale - 0.1).abs() < 1e-6);
    assert!((cdp["scale"].as_f64().unwrap() - 0.1).abs() < 1e-6);

    // 2. Max boundary exact (2.0) -> PASS
    let (opts, cdp) = parse_and_validate_pdf_params(&serde_json::json!({ "scale": 2.0 })).unwrap();
    assert!((opts.scale - 2.0).abs() < 1e-6);
    assert!((cdp["scale"].as_f64().unwrap() - 2.0).abs() < 1e-6);

    // 3. Below min (0.05) -> FAIL
    let err_005 = parse_and_validate_pdf_params(&serde_json::json!({ "scale": 0.05 })).unwrap_err();
    assert!(err_005.contains("between 0.1 and 2.0"), "Err was: {}", err_005);

    // 4. Slightly below min (0.0999) -> FAIL
    let err_0099 = parse_and_validate_pdf_params(&serde_json::json!({ "scale": 0.0999 })).unwrap_err();
    assert!(err_0099.contains("between 0.1 and 2.0"));

    // 5. Above max (2.5) -> FAIL
    let err_25 = parse_and_validate_pdf_params(&serde_json::json!({ "scale": 2.5 })).unwrap_err();
    assert!(err_25.contains("between 0.1 and 2.0"), "Err was: {}", err_25);

    // 6. Slightly above max (2.001) -> FAIL
    let err_2001 = parse_and_validate_pdf_params(&serde_json::json!({ "scale": 2.001 })).unwrap_err();
    assert!(err_2001.contains("between 0.1 and 2.0"));

    // 7. Negative scale (-1.0, -100.0) -> FAIL
    assert!(parse_and_validate_pdf_params(&serde_json::json!({ "scale": -1.0 })).is_err());
    assert!(parse_and_validate_pdf_params(&serde_json::json!({ "scale": -100.0 })).is_err());
}

#[test]
fn stress_test_negative_margins() {
    let negative_cases = [
        serde_json::json!({ "margin_top": -0.01 }),
        serde_json::json!({ "margin_bottom": -0.5 }),
        serde_json::json!({ "margin_left": -1.0 }),
        serde_json::json!({ "margin_right": -100.0 }),
        serde_json::json!({
            "margin_top": -1.0,
            "margin_bottom": -1.0,
            "margin_left": -1.0,
            "margin_right": -1.0
        }),
    ];

    for params in &negative_cases {
        let res = parse_and_validate_pdf_params(params);
        assert!(res.is_err(), "Expected negative margin failure for {:?}", params);
        assert!(res.unwrap_err().contains("Margins must be non-negative"));
    }
}

#[test]
fn stress_test_margins_exceeding_paper_dimensions() {
    // 1. Horizontal margin sum >= paper_width
    let err_h_exact = parse_and_validate_pdf_params(&serde_json::json!({
        "paper_width": 8.5,
        "margin_left": 4.25,
        "margin_right": 4.25
    })).unwrap_err();
    assert!(err_h_exact.contains("Horizontal margins"));

    let err_h_overflow = parse_and_validate_pdf_params(&serde_json::json!({
        "paper_width": 8.5,
        "margin_left": 5.0,
        "margin_right": 4.0
    })).unwrap_err();
    assert!(err_h_overflow.contains("Horizontal margins"));

    // Single margin exceeds paper_width
    let err_h_single = parse_and_validate_pdf_params(&serde_json::json!({
        "paper_width": 8.5,
        "margin_left": 10.0,
        "margin_right": 0.0
    })).unwrap_err();
    assert!(err_h_single.contains("Horizontal margins"));

    // 2. Vertical margin sum >= paper_height
    let err_v_exact = parse_and_validate_pdf_params(&serde_json::json!({
        "paper_height": 11.0,
        "margin_top": 5.5,
        "margin_bottom": 5.5
    })).unwrap_err();
    assert!(err_v_exact.contains("Vertical margins"));

    let err_v_overflow = parse_and_validate_pdf_params(&serde_json::json!({
        "paper_height": 11.0,
        "margin_top": 6.0,
        "margin_bottom": 6.0
    })).unwrap_err();
    assert!(err_v_overflow.contains("Vertical margins"));

    // Single margin exceeds paper_height
    let err_v_single = parse_and_validate_pdf_params(&serde_json::json!({
        "paper_height": 11.0,
        "margin_top": 15.0,
        "margin_bottom": 0.0
    })).unwrap_err();
    assert!(err_v_single.contains("Vertical margins"));

    // 3. Valid margins: zero margins (borderless printing)
    let ok_zero = parse_and_validate_pdf_params(&serde_json::json!({
        "paper_width": 8.5,
        "paper_height": 11.0,
        "margin_top": 0.0,
        "margin_bottom": 0.0,
        "margin_left": 0.0,
        "margin_right": 0.0
    }));
    assert!(ok_zero.is_ok(), "Zero margins should be supported for borderless printing");

    // 4. Margins strictly less than dimensions
    let ok_tight = parse_and_validate_pdf_params(&serde_json::json!({
        "paper_width": 8.5,
        "paper_height": 11.0,
        "margin_left": 4.24,
        "margin_right": 4.24,
        "margin_top": 5.49,
        "margin_bottom": 5.49
    }));
    assert!(ok_tight.is_ok());

    // 5. Invalid paper dimensions
    assert!(parse_and_validate_pdf_params(&serde_json::json!({ "paper_width": 0.0 })).is_err());
    assert!(parse_and_validate_pdf_params(&serde_json::json!({ "paper_width": -5.0 })).is_err());
    assert!(parse_and_validate_pdf_params(&serde_json::json!({ "paper_height": 0.0 })).is_err());
    assert!(parse_and_validate_pdf_params(&serde_json::json!({ "paper_height": -11.0 })).is_err());
}

// =============================================================================
// CHALLENGE 2: CORRUPT/TRUNCATED BASE64 & INVALID PDF MAGIC HEADERS
// =============================================================================

#[test]
fn stress_test_corrupt_and_truncated_base64_inputs() {
    // 1. Truncated base64 producing fewer than MIN_VALID_PDF_SIZE (32) bytes
    // "JVBERi0=" decodes to "%PDF-" (5 bytes)
    let err_trunc = decode_and_validate_pdf("JVBERi0=").unwrap_err();
    assert!(err_trunc.contains("too small"), "Err was: {}", err_trunc);

    // 2. Completely empty string
    let err_empty = decode_and_validate_pdf("").unwrap_err();
    assert!(err_empty.contains("too small"));

    // 3. Invalid base64 characters
    let err_chars = decode_and_validate_pdf("JVBERi0xLjQKJcTl8uXr!!!INVALID_CHARS???").unwrap_err();
    assert!(err_chars.contains("Failed to decode PDF base64"));

    // 4. Invalid base64 padding
    let err_padding = decode_and_validate_pdf("AAAA===").unwrap_err();
    assert!(err_padding.contains("Failed to decode PDF base64"));

    // 5. Non-base64 binary junk
    let err_junk = decode_and_validate_pdf(" \t \r\n ").unwrap_err();
    assert!(err_junk.contains("too small"));
}

#[test]
fn stress_test_invalid_pdf_magic_headers_and_trailers() {
    // 1. Strictly reject HTML returned as PDF
    let html_bytes = b"<!DOCTYPE html><html><body><h1>Error 500</h1></body></html>";
    let err_html = validate_pdf_bytes(html_bytes).unwrap_err();
    assert!(err_html.contains("missing '%PDF-' header magic bytes"));

    // 2. Strictly reject PNG / JPEG magic bytes
    let png_bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89";
    let err_png = validate_pdf_bytes(png_bytes).unwrap_err();
    assert!(err_png.contains("missing '%PDF-' header magic bytes"));

    // 3. Header magic byte in middle or prefixed with space
    let space_prefixed = b" %PDF-1.4\nsome content here 1234567890\n%%EOF\n";
    let err_prefix = validate_pdf_bytes(space_prefixed).unwrap_err();
    assert!(err_prefix.contains("missing '%PDF-' header magic bytes"));

    // 4. Valid header %PDF-1.4 but missing %%EOF trailer
    let missing_trailer = b"%PDF-1.4\nvalid header but missing trailer marker in this 45-byte buffer";
    assert!(missing_trailer.len() >= 32);
    let err_eof = validate_pdf_bytes(missing_trailer).unwrap_err();
    assert!(err_eof.contains("missing '%%EOF' trailer marker"));

    // 5. Valid PDF with %%EOF at exact tail
    let valid_exact = b"%PDF-1.4\ncontent stream 1234567890\n%%EOF";
    assert!(validate_pdf_bytes(valid_exact).is_ok());

    // 6. Valid PDF with trailing newlines / comments after %%EOF
    let valid_trailing = b"%PDF-1.4\ncontent stream 1234567890\n%%EOF\r\n%comment\n";
    assert!(validate_pdf_bytes(valid_trailing).is_ok());

    // 7. Base64 roundtrip with valid PDF fixture
    let decoded = decode_and_validate_pdf(VALID_PDF_BASE64).unwrap();
    assert!(decoded.starts_with(b"%PDF-"));
    assert!(decoded.len() >= 32);
}

// =============================================================================
// CHALLENGE 3: PAGE RANGE FORMATTING STRESS TESTS
// =============================================================================

#[test]
fn stress_test_page_range_formatting() {
    // Valid: "1-3, 5"
    let (opts1, cdp1) = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "1-3, 5" })).unwrap();
    assert_eq!(opts1.page_ranges, Some("1-3, 5".to_string()));
    assert_eq!(cdp1["pageRanges"], "1-3, 5");

    // Valid: "99999"
    let (opts2, cdp2) = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "99999" })).unwrap();
    assert_eq!(opts2.page_ranges, Some("99999".to_string()));
    assert_eq!(cdp2["pageRanges"], "99999");

    // Valid: "1-1" (single-page interval)
    let (opts3, _) = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "1-1" })).unwrap();
    assert_eq!(opts3.page_ranges, Some("1-1".to_string()));

    // Valid: multiple ranges with varying whitespace
    let (opts4, _) = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "1 - 3, 5 , 8 - 10" })).unwrap();
    assert_eq!(opts4.page_ranges, Some("1 - 3, 5 , 8 - 10".to_string()));

    // Invalid: "abc"
    let err_abc = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "abc" })).unwrap_err();
    assert!(err_abc.contains("Invalid page number 'abc'"));

    // Invalid: "-1" (leading dash with no start)
    let err_neg1 = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "-1" })).unwrap_err();
    assert!(err_neg1.contains("Invalid start page number in range '-1'"));

    // Invalid: "1-" (trailing dash with no end)
    let err_dash_end = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "1-" })).unwrap_err();
    assert!(err_dash_end.contains("Invalid end page number in range '1-'"));

    // Invalid: "0" (0-indexed rejected)
    let err_zero = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "0" })).unwrap_err();
    assert!(err_zero.contains("1-indexed"));

    // Invalid: "0-5" (0-indexed start rejected)
    let err_zero_start = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "0-5" })).unwrap_err();
    assert!(err_zero_start.contains("1-indexed"));

    // Invalid: "5-2" (start exceeds end)
    let err_inverted = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "5-2" })).unwrap_err();
    assert!(err_inverted.contains("Start page 5 cannot exceed end page 2"));

    // Invalid: empty entries in comma-separated list
    let err_empty_entry = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "1-3, , 5" })).unwrap_err();
    assert!(err_empty_entry.contains("empty entry in '1-3, , 5'"));

    let err_trailing_comma = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "1-3, 5," })).unwrap_err();
    assert!(err_trailing_comma.contains("empty entry in '1-3, 5,'"));

    // None or empty string -> allowed and maps to None
    let (opts_empty, cdp_empty) = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "   " })).unwrap();
    assert!(opts_empty.page_ranges.is_none());
    assert!(cdp_empty.get("pageRanges").is_none());
}

// =============================================================================
// CHALLENGE 4: PIPELINE ACTION COMBINED EXECUTION & ERROR PROPAGATION
// =============================================================================

#[tokio::test]
async fn stress_test_pipeline_execution_combined_steps() {
    let pdf_provider: Arc<Mutex<Box<dyn FnMut() -> Result<String, String> + Send>>> =
        Arc::new(Mutex::new(Box::new(|| Ok(VALID_PDF_BASE64.to_string()))));

    let (state, mock_handle) = setup_comprehensive_mock_cdp(pdf_provider).await;

    let temp_dir = std::env::temp_dir().join(format!("pipeline_stress_{}", uuid::Uuid::new_v4()));
    let output_pdf = temp_dir.join("sub").join("report.pdf");
    let output_pdf_str = output_pdf.to_string_lossy().to_string();

    let pipeline_params = serde_json::json!({
        "steps": [
            {
                "action": "navigate",
                "url": "https://example.com"
            },
            {
                "action": "evaluate",
                "expression": "document.title"
            },
            {
                "action": "capture_grid_screenshot",
                "quality": 85
            },
            {
                "action": "print_to_pdf",
                "landscape": true,
                "format": "a4",
                "scale": 1.25,
                "save_path": output_pdf_str
            }
        ]
    });

    let result = onlydoms::tools::navigation::execute_browser_pipeline(
        state.clone(),
        Some(10),
        pipeline_params,
    )
    .await
    .unwrap();

    assert_eq!(result["success"], true);
    assert_eq!(result["final_url"], "https://example.com/tested-page");

    // Screenshot from step 2 should be captured
    assert!(result.get("screenshot").is_some());

    let steps = result["steps_executed"].as_array().unwrap();
    assert_eq!(steps.len(), 4);

    assert_eq!(steps[0]["action"], "navigate");
    assert_eq!(steps[1]["action"], "evaluate");
    assert_eq!(steps[2]["action"], "capture_grid_screenshot");
    assert_eq!(steps[3]["action"], "print_to_pdf");
    assert_eq!(steps[3]["status"], "ok");

    let pdf_step_res = &steps[3]["result"];
    assert_eq!(pdf_step_res["success"], true);
    assert_eq!(pdf_step_res["landscape"], true);
    assert_eq!(pdf_step_res["paper_width"], 8.27);
    assert_eq!(pdf_step_res["paper_height"], 11.69);
    assert_eq!(pdf_step_res["file_path"], output_pdf_str);

    // Verify file written to disk
    assert!(output_pdf.exists());
    let written_bytes = tokio::fs::read(&output_pdf).await.unwrap();
    assert!(written_bytes.starts_with(b"%PDF-"));
    assert_eq!(written_bytes.len(), pdf_step_res["size_bytes"].as_u64().unwrap() as usize);

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    mock_handle.abort();
}

#[tokio::test]
async fn stress_test_pipeline_fails_fast_on_invalid_pdf_scale() {
    let pdf_provider: Arc<Mutex<Box<dyn FnMut() -> Result<String, String> + Send>>> =
        Arc::new(Mutex::new(Box::new(|| Ok(VALID_PDF_BASE64.to_string()))));

    let (state, mock_handle) = setup_comprehensive_mock_cdp(pdf_provider).await;

    let pipeline_params = serde_json::json!({
        "steps": [
            {
                "action": "navigate",
                "url": "https://example.com"
            },
            {
                "action": "print_to_pdf",
                "scale": 0.05
            }
        ]
    });

    let res = onlydoms::tools::navigation::execute_browser_pipeline(
        state.clone(),
        Some(10),
        pipeline_params,
    )
    .await;

    assert!(res.is_err(), "Pipeline must fail fast when print_to_pdf has invalid parameters");
    let err_msg = res.unwrap_err();
    assert!(err_msg.contains("between 0.1 and 2.0"), "Err was: {}", err_msg);

    mock_handle.abort();
}

#[tokio::test]
async fn stress_test_pipeline_fails_fast_on_invalid_pdf_magic_bytes_from_cdp() {
    // CDP returns HTML error payload instead of valid PDF
    let html_b64 = "PGh0bWw+PGJvZHk+RXJyb3IgNTAwPC9ib2R5PjwvaHRtbD4=";
    let pdf_provider: Arc<Mutex<Box<dyn FnMut() -> Result<String, String> + Send>>> =
        Arc::new(Mutex::new(Box::new(move || Ok(html_b64.to_string()))));

    let (state, mock_handle) = setup_comprehensive_mock_cdp(pdf_provider).await;

    let pipeline_params = serde_json::json!({
        "steps": [
            {
                "action": "print_to_pdf",
                "scale": 1.0
            }
        ]
    });

    let res = onlydoms::tools::navigation::execute_browser_pipeline(
        state.clone(),
        Some(10),
        pipeline_params,
    )
    .await;

    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(err_msg.contains("missing '%PDF-' header magic bytes"), "Err was: {}", err_msg);

    mock_handle.abort();
}
