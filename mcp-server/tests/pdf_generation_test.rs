use onlydoms::mcp_protocol::{handle_mcp_request, McpRequest};
use onlydoms::tools::visual::print_to_pdf;
use onlydoms::websocket_hub::HubState;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Valid synthetic minimal base64 PDF payload
const VALID_PDF_BASE64: &str = "JVBERi0xLjQKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFszIDAgUl0gL0NvdW50IDEgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL1BhZ2UgL1BhcmVudCAyIDAgUiAvTWVkaWFCb3ggWzAgMCA2MTIgNzkyXSA+PgplbmRvYmoKeHJlZgowIDQKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDA5IDAwMDAwIG4gCjAwMDAwMDAwNTggMDAwMDAgbiAKMDAwMDAwMDExNSAwMDAwMCBuIAp0cmFpbGVyCjw8IC9TaXplIDQgL1Jvb3QgMSAwIFIgPj4Kc3RhcnR4cmVmCjE4NwolJUVPRgo=";

/// Spawns a background mock CDP handler that receives commands and responds with custom results or errors
async fn setup_mock_cdp_hub() -> (
    Arc<Mutex<HubState>>,
    tokio::task::JoinHandle<()>,
    tokio::sync::mpsc::Sender<Result<String, String>>,
) {
    let state = Arc::new(Mutex::new(HubState::new()));
    let (tx_hub, mut rx_hub) = tokio::sync::mpsc::channel::<Value>(16);
    let (tx_control, mut rx_control) = tokio::sync::mpsc::channel::<Result<String, String>>(16);

    {
        let mut lock = state.lock().await;
        lock.tx = Some(tx_hub);
    }

    let state_clone = state.clone();
    let handle = tokio::spawn(async move {
        while let Some(msg) = rx_hub.recv().await {
            let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

            if method == "Runtime.evaluate" {
                let mut lock = state_clone.lock().await;
                if let Some(sender) = lock.pending.remove(&id) {
                    let _ = sender.send(serde_json::json!({
                        "id": id,
                        "result": {
                            "result": {
                                "type": "string",
                                "value": "https://example.com"
                            }
                        }
                    }));
                }
                continue;
            }

            let response_mode = rx_control.recv().await.unwrap_or(Ok(VALID_PDF_BASE64.to_string()));

            let mut lock = state_clone.lock().await;
            if let Some(sender) = lock.pending.remove(&id) {
                match response_mode {
                    Ok(b64) => {
                        let _ = sender.send(serde_json::json!({
                            "id": id,
                            "result": {
                                "data": b64
                            }
                        }));
                    }
                    Err(err_msg) => {
                        let _ = sender.send(serde_json::json!({
                            "id": id,
                            "error": err_msg
                        }));
                    }
                }
            }
        }
    });

    (state, handle, tx_control)
}

#[tokio::test]
async fn test_print_to_pdf_default_parameters() {
    let (state, mock_handle, control_tx) = setup_mock_cdp_hub().await;
    control_tx.send(Ok(VALID_PDF_BASE64.to_string())).await.unwrap();

    let res = print_to_pdf(state.clone(), Some(1), serde_json::json!({})).await.unwrap();

    assert_eq!(res["success"], true);
    assert_eq!(res["landscape"], false);
    assert_eq!(res["paper_width"], 8.5);
    assert_eq!(res["paper_height"], 11.0);
    assert!(res["size_bytes"].as_u64().unwrap() > 50);
    assert!(res["base64_pdf"].as_str().unwrap().starts_with("JVBERi0"));
    assert!(res["data"].as_str().unwrap().starts_with("JVBERi0"));

    mock_handle.abort();
}

#[tokio::test]
async fn test_print_to_pdf_custom_options() {
    let (state, mock_handle, control_tx) = setup_mock_cdp_hub().await;
    control_tx.send(Ok(VALID_PDF_BASE64.to_string())).await.unwrap();

    let res = print_to_pdf(
        state.clone(),
        Some(1),
        serde_json::json!({
            "landscape": true,
            "format": "a4",
            "scale": 1.25,
            "page_ranges": "1-2"
        }),
    )
    .await
    .unwrap();

    assert_eq!(res["success"], true);
    assert_eq!(res["landscape"], true);
    assert_eq!(res["paper_width"], 8.27);
    assert_eq!(res["paper_height"], 11.69);

    mock_handle.abort();
}

#[tokio::test]
async fn test_print_to_pdf_save_to_disk() {
    let (state, mock_handle, control_tx) = setup_mock_cdp_hub().await;
    control_tx.send(Ok(VALID_PDF_BASE64.to_string())).await.unwrap();

    let temp_dir = std::env::temp_dir().join(format!("test_pdf_disk_{}", uuid::Uuid::new_v4()));
    let save_path = temp_dir.join("subfolder").join("output.pdf");
    let save_str = save_path.to_string_lossy().to_string();

    let res = print_to_pdf(
        state.clone(),
        Some(1),
        serde_json::json!({
            "save_path": save_str,
            "include_base64": false
        }),
    )
    .await
    .unwrap();

    assert_eq!(res["success"], true);
    assert_eq!(res["file_path"], serde_json::json!(save_str));
    assert!(res.get("base64_pdf").is_none());

    assert!(save_path.exists());
    let file_bytes = tokio::fs::read(&save_path).await.unwrap();
    assert!(file_bytes.starts_with(b"%PDF-"));
    assert_eq!(file_bytes.len(), res["size_bytes"].as_u64().unwrap() as usize);

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    mock_handle.abort();
}

#[tokio::test]
async fn test_print_to_pdf_cdp_error_handling() {
    let (state, mock_handle, control_tx) = setup_mock_cdp_hub().await;
    control_tx.send(Err("Target detached".to_string())).await.unwrap();

    let err = print_to_pdf(state.clone(), Some(1), serde_json::json!({})).await.unwrap_err();
    assert!(err.contains("Target detached"));

    mock_handle.abort();
}

#[tokio::test]
async fn test_print_to_pdf_invalid_magic_bytes_detection() {
    let (state, mock_handle, control_tx) = setup_mock_cdp_hub().await;
    let html_b64 = "PGh0bWw+PGJvZHk+Tm90IGEgUERGPC9ib2R5PjwvaHRtbD4=";
    control_tx.send(Ok(html_b64.to_string())).await.unwrap();

    let err = print_to_pdf(state.clone(), Some(1), serde_json::json!({})).await.unwrap_err();
    assert!(err.contains("missing '%PDF-' header magic bytes"));

    mock_handle.abort();
}

#[tokio::test]
async fn test_mcp_jsonrpc_print_to_pdf_dispatch() {
    let (state, mock_handle, control_tx) = setup_mock_cdp_hub().await;

    {
        let mut lock = state.lock().await;
        lock.tab_registry.acquire_tab("client_test", 42).unwrap();
    }

    control_tx.send(Ok(VALID_PDF_BASE64.to_string())).await.unwrap();

    let req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::Number(100.into())),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "print_to_pdf",
            "arguments": {
                "tab_id": 42,
                "landscape": true
            }
        })),
    };

    let resp = handle_mcp_request("client_test", req, state.clone()).await.unwrap();
    assert_eq!(resp.id, Some(Value::Number(100.into())));
    assert!(resp.error.is_none());

    let content = resp.result.unwrap()["content"][0]["text"].as_str().unwrap().to_string();
    let parsed: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["landscape"], true);

    mock_handle.abort();
}

#[tokio::test]
async fn test_browser_pipeline_print_to_pdf_action() {
    let (state, mock_handle, control_tx) = setup_mock_cdp_hub().await;
    control_tx.send(Ok(VALID_PDF_BASE64.to_string())).await.unwrap();

    let pipeline_res = onlydoms::tools::navigation::execute_browser_pipeline(
        state.clone(),
        Some(42),
        serde_json::json!({
            "steps": [
                {
                    "action": "print_to_pdf",
                    "landscape": false,
                    "format": "a4"
                }
            ]
        }),
    )
    .await
    .unwrap();

    assert_eq!(pipeline_res["success"], true);
    let steps = pipeline_res["steps_executed"].as_array().unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0]["action"], "print_to_pdf");
    assert_eq!(steps[0]["status"], "ok");

    mock_handle.abort();
}

#[tokio::test]
async fn test_print_to_pdf_parameter_validation_errors() {
    let state = Arc::new(Mutex::new(HubState::new()));

    // Scale out of bounds (< 0.1)
    let res = print_to_pdf(
        state.clone(),
        Some(1),
        serde_json::json!({
            "scale": 0.05
        }),
    )
    .await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("between 0.1 and 2.0"));

    // Margins exceed width
    let res = print_to_pdf(
        state.clone(),
        Some(1),
        serde_json::json!({
            "paper_width": 8.5,
            "margin_left": 5.0,
            "margin_right": 4.0
        }),
    )
    .await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Horizontal margins"));
}
