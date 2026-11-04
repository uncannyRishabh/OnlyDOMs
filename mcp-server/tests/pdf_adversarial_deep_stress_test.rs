//! Deep Adversarial and Stress Tests for HTML-to-PDF Conversion and Pipeline Integration
//!
//! Evaluates:
//! 1. High concurrency: 50 simultaneous print_to_pdf requests against WebSocket hub
//! 2. Large PDF payload stress: 3MB multi-object synthetic PDF stream decode and validation
//! 3. Overwriting existing files and saving to complex directory paths with spaces
//! 4. MCP JSON-RPC 2.0 protocol contract completeness (data, byte_count, saved_path, tab_id)
//! 5. Error propagation over JSON-RPC 2.0 when CDP or validation fails

use onlydoms::mcp_protocol::{handle_mcp_request, McpRequest};
use onlydoms::tools::visual::{decode_and_validate_pdf, print_to_pdf, validate_pdf_bytes};
use onlydoms::websocket_hub::HubState;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

const VALID_PDF_BASE64: &str = "JVBERi0xLjQKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFszIDAgUl0gL0NvdW50IDEgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL1BhZ2UgL1BhcmVudCAyIDAgUiAvTWVkaWFCb3ggWzAgMCA2MTIgNzkyXSA+PgplbmRvYmoKeHJlZgowIDQKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDA5IDAwMDAwIG4gCjAwMDAwMDAwNTggMDAwMDAgbiAKMDAwMDAwMDExNSAwMDAwMCBuIAp0cmFpbGVyCjw8IC9TaXplIDQgL1Jvb3QgMSAwIFIgPj4Kc3RhcnR4cmVmCjE4NwolJUVPRgo=";

/// Set up mock CDP hub that handles concurrent requests
async fn setup_concurrent_mock_cdp(
    custom_pdf: Option<String>,
) -> (Arc<Mutex<HubState>>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(Mutex::new(HubState::new()));
    let (tx_hub, mut rx_hub) = tokio::sync::mpsc::channel::<Value>(256);

    {
        let mut lock = state.lock().await;
        lock.tx = Some(tx_hub);
    }

    let default_pdf = custom_pdf.unwrap_or_else(|| VALID_PDF_BASE64.to_string());
    let state_clone = state.clone();

    let handle = tokio::spawn(async move {
        while let Some(msg) = rx_hub.recv().await {
            let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

            let response = if method == "Runtime.evaluate" {
                serde_json::json!({
                    "id": id,
                    "result": { "result": { "value": "https://example.com/tested" } }
                })
            } else if method == "Page.printToPDF" {
                serde_json::json!({
                    "id": id,
                    "result": { "data": default_pdf }
                })
            } else {
                serde_json::json!({
                    "id": id,
                    "result": {}
                })
            };

            let mut lock = state_clone.lock().await;
            if let Some(sender) = lock.pending.remove(&id) {
                let _ = sender.send(response);
            }
        }
    });

    (state, handle)
}

#[tokio::test]
async fn stress_test_high_concurrency_print_to_pdf() {
    let (state, mock_handle) = setup_concurrent_mock_cdp(None).await;

    let mut handles = Vec::new();
    for i in 0..50 {
        let st = state.clone();
        handles.push(tokio::spawn(async move {
            let res = print_to_pdf(
                st,
                Some(1),
                serde_json::json!({
                    "scale": 1.0,
                    "landscape": i % 2 == 0,
                    "paper_width": 8.5,
                    "paper_height": 11.0,
                }),
            )
            .await;
            res.expect("Concurrent print_to_pdf call must succeed")
        }));
    }

    for (i, handle) in handles.into_iter().enumerate() {
        let val = handle.await.unwrap();
        assert_eq!(val["success"], true);
        assert_eq!(val["landscape"], i % 2 == 0);
        assert!(val["data"].as_str().unwrap().starts_with("JVBERi0"));
    }

    mock_handle.abort();
}

#[tokio::test]
async fn stress_test_large_pdf_payload_validation_and_saving() {
    // Generate a ~3MB synthetic PDF stream with valid %PDF- header and %%EOF trailer
    let header = b"%PDF-1.7\n";
    let trailer = b"\nstartxref\n3145728\n%%EOF\n";
    let body_len = 3 * 1024 * 1024;
    let mut large_pdf = Vec::with_capacity(header.len() + body_len + trailer.len());
    large_pdf.extend_from_slice(header);
    large_pdf.resize(header.len() + body_len, b'A');
    large_pdf.extend_from_slice(trailer);

    // Validate raw bytes directly
    assert!(validate_pdf_bytes(&large_pdf).is_ok());

    // Encode to base64
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut b64 = String::with_capacity((large_pdf.len() * 4) / 3 + 4);
    for chunk in large_pdf.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        b64.push(CHARSET[((n >> 18) & 63) as usize] as char);
        b64.push(CHARSET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            b64.push(CHARSET[((n >> 6) & 63) as usize] as char);
        } else {
            b64.push('=');
        }
        if chunk.len() > 2 {
            b64.push(CHARSET[(n & 63) as usize] as char);
        } else {
            b64.push('=');
        }
    }

    // Decode and validate large payload
    let decoded = decode_and_validate_pdf(&b64).expect("Large PDF decode must succeed");
    assert_eq!(decoded.len(), large_pdf.len());
    assert_eq!(&decoded[..8], b"%PDF-1.7");
    assert!(decoded.ends_with(b"%%EOF\n"));

    // Now test with print_to_pdf through the mock hub
    let (state, mock_handle) = setup_concurrent_mock_cdp(Some(b64)).await;
    let temp_file = std::env::temp_dir().join(format!("large_pdf_{}.pdf", uuid::Uuid::new_v4()));
    let temp_file_str = temp_file.to_string_lossy().to_string();

    let res = print_to_pdf(
        state.clone(),
        Some(1),
        serde_json::json!({
            "save_path": temp_file_str,
            "include_base64": false, // test context optimization
        }),
    )
    .await
    .unwrap();

    assert_eq!(res["success"], true);
    assert_eq!(res["byte_count"], large_pdf.len());
    assert_eq!(res["saved_path"], temp_file_str);
    assert!(res.get("data").is_none()); // correctly omitted when include_base64 = false

    assert!(temp_file.exists());
    let disk_bytes = tokio::fs::read(&temp_file).await.unwrap();
    assert_eq!(disk_bytes.len(), large_pdf.len());

    let _ = tokio::fs::remove_file(&temp_file).await;
    mock_handle.abort();
}

#[tokio::test]
async fn stress_test_save_path_file_overwrite_and_spaces() {
    let (state, mock_handle) = setup_concurrent_mock_cdp(None).await;

    let temp_dir = std::env::temp_dir().join(format!("path with spaces_{}", uuid::Uuid::new_v4()));
    let target_file = temp_dir.join("report document.pdf");
    let target_file_str = target_file.to_string_lossy().to_string();

    // First write
    let res1 = print_to_pdf(
        state.clone(),
        Some(1),
        serde_json::json!({
            "save_path": target_file_str
        }),
    )
    .await
    .unwrap();
    assert_eq!(res1["success"], true);
    assert!(target_file.exists());

    // Second write (overwrite existing file)
    let res2 = print_to_pdf(
        state.clone(),
        Some(1),
        serde_json::json!({
            "save_path": target_file_str
        }),
    )
    .await
    .unwrap();
    assert_eq!(res2["success"], true);
    assert!(target_file.exists());

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    mock_handle.abort();
}

#[tokio::test]
async fn stress_test_mcp_jsonrpc_contract_adherence() {
    let (state, mock_handle) = setup_concurrent_mock_cdp(None).await;

    {
        let mut lock = state.lock().await;
        lock.tab_registry.acquire_tab("client-1", 42).unwrap();
    }

    let req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!("test-mcp-pdf-contract")),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "print_to_pdf",
            "arguments": {
                "tab_id": 42,
                "landscape": true,
                "scale": 1.5,
                "paper_format": "letter"
            }
        })),
    };

    let resp = handle_mcp_request("client-1", req, state.clone()).await.expect("Expected McpResponse");
    assert_eq!(resp.jsonrpc, "2.0");
    assert_eq!(resp.id, Some(serde_json::json!("test-mcp-pdf-contract")));
    assert!(resp.error.is_none(), "Unexpected error: {:?}", resp.error);

    let result = resp.result.expect("Expected result payload");
    let content = result["content"].as_array().expect("Expected content array");
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["type"], "text");

    let text_str = content[0]["text"].as_str().expect("Expected text field");
    let payload: Value = serde_json::from_str(text_str).expect("Text must parse as JSON");

    // Strictly verify PROJECT.md Interface Contract #1 fields:
    // data: String (base64 PDF starting with %PDF-)
    // byte_count: usize
    // saved_path: Option<String>
    // tab_id: u64
    assert!(payload.get("data").is_some(), "data field missing");
    let data_str = payload["data"].as_str().expect("data must be a string");
    assert!(data_str.starts_with("JVBERi0"), "data must be base64 %PDF-");

    assert!(payload.get("byte_count").is_some(), "byte_count field missing");
    assert!(payload["byte_count"].as_u64().unwrap() > 0);

    assert_eq!(payload.get("tab_id").and_then(|v| v.as_u64()), Some(42));

    // saved_path is null or None since no save_path was provided
    assert!(payload.get("saved_path").is_some(), "saved_path field must be present");

    mock_handle.abort();
}

#[tokio::test]
async fn stress_test_mcp_jsonrpc_error_handling() {
    let (state, mock_handle) = setup_concurrent_mock_cdp(None).await;

    {
        let mut lock = state.lock().await;
        lock.tab_registry.acquire_tab("client-err", 42).unwrap();
    }

    // Call print_to_pdf with invalid scale parameter
    let req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(serde_json::json!("test-err-id")),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "print_to_pdf",
            "arguments": {
                "tab_id": 42,
                "scale": 99.0 // invalid: > 2.0
            }
        })),
    };

    let resp = handle_mcp_request("client-err", req, state.clone()).await.expect("Expected response");
    assert_eq!(resp.jsonrpc, "2.0");
    assert_eq!(resp.id, Some(serde_json::json!("test-err-id")));

    // When a tool errors, either error object or isError in content is returned
    let is_err = resp.error.is_some()
        || resp.result.as_ref().map_or(false, |r| {
            r.get("isError").and_then(|v| v.as_bool()).unwrap_or(false)
        });
    assert!(is_err, "Invalid scale parameter must produce an error response");

    mock_handle.abort();
}
