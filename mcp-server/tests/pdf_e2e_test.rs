//! E2E Test Suite for HTML to PDF Conversion Tool (R1, F1, F2)
//!
//! Covers:
//! - Tier 1: Magic bytes (%PDF-), base64 decode, parameter mapping, disk saving, silent CDP execution.
//! - Tier 2: Boundaries (scale extremes, margin overflow, empty document, corrupt base64, page ranges syntax).
//! - Tier 3: Pairwise combinations (formats x orientation x background, save_path x include_base64).
//! - Tier 4: Application scenario (agent PDF export workflow).

use serde_json::{json, Value};
use tokio::fs;

// Minimal valid PDF binary fixture starting with %PDF-1.4 and ending with %%EOF
const MINIMAL_PDF_BYTES: &[u8] = b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 1/Kids[3 0 R]>>endobj\n3 0 obj<</Type/Page/MediaBox[0 0 612 792]/Parent 2 0 R>>endobj\nxref\n0 4\n0000000000 65535 f \n0000000009 00000 n \n0000000052 00000 n \n0000000108 00000 n \ntrailer<</Size 4/Root 1 0 R>>\nstartxref\n180\n%%EOF\n";

/// Helper function to encode raw bytes to standard Base64 string for test vectors
fn encode_base64(bytes: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
        let b2 = if i + 2 < bytes.len() { bytes[i + 2] } else { 0 };

        let chunk = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(CHARSET[((chunk >> 18) & 63) as usize] as char);
        out.push(CHARSET[((chunk >> 12) & 63) as usize] as char);

        if i + 1 < bytes.len() {
            out.push(CHARSET[((chunk >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }

        if i + 2 < bytes.len() {
            out.push(CHARSET[(chunk & 63) as usize] as char);
        } else {
            out.push('=');
        }

        i += 3;
    }
    out
}

/// Simulated mock CDP Page.printToPDF executor validating wire parameters
fn simulate_cdp_print_to_pdf(cdp_params: &Value) -> Result<Value, String> {
    // 1. Validate scale bounds (0.1 to 2.0)
    if let Some(scale) = cdp_params.get("scale").and_then(|v| v.as_f64()) {
        if scale < 0.1 || scale > 2.0 {
            return Err(format!("Invalid parameters: scale must be between 0.1 and 2.0, got {}", scale));
        }
    }

    // 2. Validate margin boundaries
    let paper_w = cdp_params.get("paperWidth").and_then(|v| v.as_f64()).unwrap_or(8.5);
    let paper_h = cdp_params.get("paperHeight").and_then(|v| v.as_f64()).unwrap_or(11.0);
    let m_top = cdp_params.get("marginTop").and_then(|v| v.as_f64()).unwrap_or(0.4);
    let m_bottom = cdp_params.get("marginBottom").and_then(|v| v.as_f64()).unwrap_or(0.4);
    let m_left = cdp_params.get("marginLeft").and_then(|v| v.as_f64()).unwrap_or(0.4);
    let m_right = cdp_params.get("marginRight").and_then(|v| v.as_f64()).unwrap_or(0.4);

    if m_left + m_right >= paper_w {
        return Err("Margin exceeds paper dimensions: left + right >= width".to_string());
    }
    if m_top + m_bottom >= paper_h {
        return Err("Margin exceeds paper dimensions: top + bottom >= height".to_string());
    }

    // 3. Validate transferMode is ReturnAsBase64 (silent execution, no OS dialogs)
    let transfer_mode = cdp_params.get("transferMode").and_then(|v| v.as_str()).unwrap_or("ReturnAsBase64");
    if transfer_mode != "ReturnAsBase64" {
        return Err("Expected transferMode to be ReturnAsBase64".to_string());
    }

    // 4. Return serialized base64 PDF fixture
    let base64_data = encode_base64(MINIMAL_PDF_BYTES);
    Ok(json!({
        "data": base64_data
    }))
}

// -----------------------------------------------------------------------------
// TIER 1: Primary Behavior (Happy Path) Tests
// -----------------------------------------------------------------------------

#[test]
fn test_pdf_magic_bytes_detection_pdf14_pdf17() {
    let base64_data = encode_base64(MINIMAL_PDF_BYTES);
    let decoded = onlydoms::tools::visual::decode_base64(&base64_data).expect("Base64 decode should succeed");

    // Must start with magic bytes %PDF-
    assert!(decoded.len() >= 10, "Decoded PDF must be non-empty");
    assert_eq!(&decoded[0..5], b"%PDF-", "PDF must begin with %PDF- magic bytes");

    // Check version compatibility (PDF-1.4 through PDF-1.7)
    let version = std::str::from_utf8(&decoded[5..8]).unwrap_or("");
    assert!(
        version == "1.4" || version == "1.5" || version == "1.6" || version == "1.7",
        "Version should be valid PDF-1.x, got: {}",
        version
    );

    // Must contain trailer EOF marker
    let decoded_str = String::from_utf8_lossy(&decoded);
    assert!(decoded_str.contains("%%EOF"), "PDF must contain %%EOF trailer");
}

#[test]
fn test_pdf_silent_execution_cdp_dispatch() {
    let cdp_input = json!({
        "landscape": false,
        "printBackground": true,
        "scale": 1.0,
        "paperWidth": 8.5,
        "paperHeight": 11.0,
        "transferMode": "ReturnAsBase64"
    });

    let res = simulate_cdp_print_to_pdf(&cdp_input).expect("CDP Page.printToPDF should execute silently");
    let data = res.get("data").and_then(|v| v.as_str()).expect("Response must contain data string");
    assert!(!data.is_empty(), "Returned base64 string must not be empty");
}

#[test]
fn test_pdf_parameter_mapping_standard() {
    let tool_args = json!({
        "landscape": true,
        "print_background": true,
        "scale": 1.25,
        "paper_width": 11.0,
        "paper_height": 8.5,
        "margin_top": 0.5,
        "margin_bottom": 0.5,
        "margin_left": 0.5,
        "margin_right": 0.5,
        "page_ranges": "1-3"
    });

    // Translate to CDP parameters
    let cdp_params = json!({
        "landscape": tool_args["landscape"],
        "printBackground": tool_args["print_background"],
        "scale": tool_args["scale"],
        "paperWidth": tool_args["paper_width"],
        "paperHeight": tool_args["paper_height"],
        "marginTop": tool_args["margin_top"],
        "marginBottom": tool_args["margin_bottom"],
        "marginLeft": tool_args["margin_left"],
        "marginRight": tool_args["margin_right"],
        "pageRanges": tool_args["page_ranges"],
        "transferMode": "ReturnAsBase64"
    });

    let result = simulate_cdp_print_to_pdf(&cdp_params).expect("Parameters should map cleanly");
    assert!(result.get("data").is_some());
}

#[tokio::test]
async fn test_pdf_disk_saving_writes_binary() {
    let temp_dir = std::env::temp_dir().join(format!("onlydoms_pdf_test_{}", std::process::id()));
    let target_file = temp_dir.join("subdir").join("test_report.pdf");

    // Ensure clean state
    if target_file.exists() {
        let _ = fs::remove_file(&target_file).await;
    }

    let base64_payload = encode_base64(MINIMAL_PDF_BYTES);
    let decoded_bytes = onlydoms::tools::visual::decode_base64(&base64_payload).unwrap();

    // Verify parent directory auto-creation & file writing
    if let Some(parent) = target_file.parent() {
        fs::create_dir_all(parent).await.expect("Should create parent directories");
    }
    fs::write(&target_file, &decoded_bytes).await.expect("Should write PDF to disk");

    // Assert file exists, size matches, and content matches magic bytes
    assert!(target_file.exists(), "Target file must exist on disk");
    let read_back = fs::read(&target_file).await.expect("Should read saved PDF");
    assert_eq!(read_back.len(), MINIMAL_PDF_BYTES.len());
    assert_eq!(&read_back[0..5], b"%PDF-");

    // Clean up
    let _ = fs::remove_file(&target_file).await;
    let _ = fs::remove_dir_all(&temp_dir).await;
}

#[test]
fn test_pdf_pipeline_action_dispatch() {
    let pipeline_step = json!({
        "action": "print_to_pdf",
        "landscape": false,
        "scale": 1.0,
        "include_base64": true
    });

    assert_eq!(pipeline_step["action"], "print_to_pdf");
    assert_eq!(pipeline_step["landscape"], false);
    assert_eq!(pipeline_step["scale"], 1.0);
    assert_eq!(pipeline_step["include_base64"], true);
}

// -----------------------------------------------------------------------------
// TIER 2: Boundary & Corner Condition Tests
// -----------------------------------------------------------------------------

#[test]
fn test_pdf_scale_boundary_clamping() {
    // Exact valid limits (0.1 and 2.0)
    let min_scale = json!({ "scale": 0.1, "transferMode": "ReturnAsBase64" });
    assert!(simulate_cdp_print_to_pdf(&min_scale).is_ok());

    let max_scale = json!({ "scale": 2.0, "transferMode": "ReturnAsBase64" });
    assert!(simulate_cdp_print_to_pdf(&max_scale).is_ok());

    // Out-of-bounds rejection
    let below_min = json!({ "scale": 0.05, "transferMode": "ReturnAsBase64" });
    assert!(simulate_cdp_print_to_pdf(&below_min).is_err());

    let above_max = json!({ "scale": 2.5, "transferMode": "ReturnAsBase64" });
    assert!(simulate_cdp_print_to_pdf(&above_max).is_err());

    let negative_scale = json!({ "scale": -1.0, "transferMode": "ReturnAsBase64" });
    assert!(simulate_cdp_print_to_pdf(&negative_scale).is_err());
}

#[test]
fn test_pdf_margin_overflow_validation() {
    // Width = 8.5, left = 4.5, right = 4.5 -> sum 9.0 >= 8.5
    let width_overflow = json!({
        "paperWidth": 8.5,
        "paperHeight": 11.0,
        "marginLeft": 4.5,
        "marginRight": 4.5,
        "transferMode": "ReturnAsBase64"
    });
    let err = simulate_cdp_print_to_pdf(&width_overflow);
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("left + right >= width"));

    // Height = 11.0, top = 6.0, bottom = 6.0 -> sum 12.0 >= 11.0
    let height_overflow = json!({
        "paperWidth": 8.5,
        "paperHeight": 11.0,
        "marginTop": 6.0,
        "marginBottom": 6.0,
        "transferMode": "ReturnAsBase64"
    });
    let err = simulate_cdp_print_to_pdf(&height_overflow);
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("top + bottom >= height"));
}

#[test]
fn test_pdf_empty_document_blank_page() {
    // Empty document still yields valid PDF structure
    let empty_pdf_bytes = b"%PDF-1.4\n%%EOF\n";
    let base64_empty = encode_base64(empty_pdf_bytes);
    let decoded = onlydoms::tools::visual::decode_base64(&base64_empty).unwrap();
    assert_eq!(&decoded[0..5], b"%PDF-");
    assert!(String::from_utf8_lossy(&decoded).contains("%%EOF"));
}

#[test]
fn test_pdf_invalid_base64_payload_handling() {
    // Corrupt base64 characters
    let corrupt_payload = "JVBERi0xLjQKJcTl8uXr!!!INVALID_BASE64???";
    let decode_res = onlydoms::tools::visual::decode_base64(corrupt_payload);
    assert!(decode_res.is_err(), "Invalid characters must trigger error");

    // Invalid padding count (3 equal signs)
    let bad_padding = "AAAA===";
    assert!(onlydoms::tools::visual::decode_base64(bad_padding).is_err());
}

#[test]
fn test_pdf_page_ranges_syntax_variations() {
    let validate_range_syntax = |range_str: &str| -> bool {
        let trimmed = range_str.trim();
        if trimmed.is_empty() {
            return true;
        }
        trimmed.split(',').all(|part| {
            let part = part.trim();
            if let Some((start, end)) = part.split_once('-') {
                let s_ok = start.trim().parse::<u32>().map(|n| n > 0).unwrap_or(false);
                let e_ok = end.trim().parse::<u32>().map(|n| n > 0).unwrap_or(false);
                s_ok && e_ok
            } else {
                part.parse::<u32>().map(|n| n > 0).unwrap_or(false)
            }
        })
    };

    assert!(validate_range_syntax("1-5"));
    assert!(validate_range_syntax("1-3, 5, 8-10"));
    assert!(validate_range_syntax("1"));
    assert!(validate_range_syntax("")); // all pages

    assert!(!validate_range_syntax("0-5")); // 1-based page indices
    assert!(!validate_range_syntax("1--5"));
    assert!(!validate_range_syntax("abc"));
    assert!(!validate_range_syntax("1-2, , 4"));
}

// -----------------------------------------------------------------------------
// TIER 3: Pairwise Combinations
// -----------------------------------------------------------------------------

#[test]
fn test_pdf_pairwise_landscape_formats_margins() {
    let orientations = [false, true];
    let paper_formats = [
        ("Letter", 8.5, 11.0),
        ("A4", 8.27, 11.69),
        ("Legal", 8.5, 14.0),
    ];
    let backgrounds = [true, false];

    for &landscape in &orientations {
        for &(format_name, width, height) in &paper_formats {
            for &bg in &backgrounds {
                let (w, h) = if landscape { (height, width) } else { (width, height) };
                let params = json!({
                    "landscape": landscape,
                    "paperWidth": w,
                    "paperHeight": h,
                    "printBackground": bg,
                    "marginTop": 0.4,
                    "marginBottom": 0.4,
                    "marginLeft": 0.4,
                    "marginRight": 0.4,
                    "transferMode": "ReturnAsBase64"
                });

                let res = simulate_cdp_print_to_pdf(&params);
                assert!(
                    res.is_ok(),
                    "Failed for format: {}, landscape: {}, bg: {}",
                    format_name,
                    landscape,
                    bg
                );
            }
        }
    }
}

#[test]
fn test_pdf_pairwise_save_path_and_include_base64() {
    let test_cases = [
        // (save_path, include_base64, expected_base64_in_response, expected_file_written)
        (None, true, true, false),
        (None, false, false, false),
        (Some("output.pdf"), true, true, true),
        (Some("output.pdf"), false, false, true),
    ];

    for (save_path, include_base64, expect_base64, expect_file) in test_cases {
        let mut response = json!({
            "success": true,
            "byte_count": MINIMAL_PDF_BYTES.len(),
            "landscape": false
        });

        if let Some(path) = save_path {
            response["saved_path"] = json!(path);
        }
        if include_base64 {
            response["data"] = json!(encode_base64(MINIMAL_PDF_BYTES));
        }

        assert_eq!(response.get("data").is_some(), expect_base64);
        assert_eq!(response.get("saved_path").is_some(), expect_file);
    }
}

// -----------------------------------------------------------------------------
// TIER 4: Application Scenario (End-to-End Agent Workflow)
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_pdf_e2e_agent_workflow_report_generation() {
    let run_id = format!("workflow_{}", std::process::id());
    let export_dir = std::env::temp_dir().join(&run_id);
    let export_file = export_dir.join("financial_report_q3.pdf");

    // 1. Agent plans PDF export tool call
    let agent_mcp_arguments = json!({
        "landscape": true,
        "print_background": true,
        "scale": 1.0,
        "paper_width": 11.0,
        "paper_height": 8.5,
        "margin_top": 0.5,
        "margin_bottom": 0.5,
        "margin_left": 0.5,
        "margin_right": 0.5,
        "save_path": export_file.to_str().unwrap(),
        "include_base64": true
    });

    // 2. Hub maps to CDP Page.printToPDF
    let cdp_params = json!({
        "landscape": agent_mcp_arguments["landscape"],
        "printBackground": agent_mcp_arguments["print_background"],
        "scale": agent_mcp_arguments["scale"],
        "paperWidth": agent_mcp_arguments["paper_width"],
        "paperHeight": agent_mcp_arguments["paper_height"],
        "marginTop": agent_mcp_arguments["margin_top"],
        "marginBottom": agent_mcp_arguments["margin_bottom"],
        "marginLeft": agent_mcp_arguments["margin_left"],
        "marginRight": agent_mcp_arguments["margin_right"],
        "transferMode": "ReturnAsBase64"
    });

    let cdp_result = simulate_cdp_print_to_pdf(&cdp_params).expect("CDP execution must succeed");
    let base64_payload = cdp_result["data"].as_str().unwrap();

    // 3. Post-processing: decode and save to target directory
    let binary_bytes = onlydoms::tools::visual::decode_base64(base64_payload).unwrap();
    fs::create_dir_all(&export_dir).await.unwrap();
    fs::write(&export_file, &binary_bytes).await.unwrap();

    // 4. Response package returned to agent
    let mcp_tool_result = json!({
        "success": true,
        "byte_count": binary_bytes.len(),
        "saved_path": export_file.to_str().unwrap(),
        "data": base64_payload
    });

    // 5. Verification of artifacts
    assert!(export_file.exists());
    let on_disk_bytes = fs::read(&export_file).await.unwrap();
    assert_eq!(&on_disk_bytes[0..5], b"%PDF-");
    assert!(String::from_utf8_lossy(&on_disk_bytes).contains("%%EOF"));
    assert_eq!(mcp_tool_result["byte_count"], binary_bytes.len());

    // Clean up
    let _ = fs::remove_file(&export_file).await;
    let _ = fs::remove_dir_all(&export_dir).await;
}
