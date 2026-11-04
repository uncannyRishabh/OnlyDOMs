//! Comprehensive Backend E2E & Verification Suite for OnlyDOMs Master Specification:
//! - Section 1 & 3: 180-Char `_intent` Schema, Pre-Flight `HUD_STATE_UPDATE` Broadcast & Multi-Tab State Store
//! - Section 4: Optimistic Network Capture (`NetworkHarvester`), Shell-Safe cURL Serializer & Zero-Subprocess Rust Replay (`replay_curl`)
//! - Section 5: Visual Test & Feature Overview Document Pipeline (`assets/step_<n>.webp` -> `overview.html` -> `overview.pdf` -> HTTP `/reports/<run_id>/`)
//! - Section 6: Multi-Layer Agent Blindspot Protection (`filter::hud_sanitizer`, `CLOAK_HUD`, `P9` Quadrant Hit-Test Passthrough)

use onlydoms::filter::hud_sanitizer::{sanitize_extension_payload, HUD_ATTR, HUD_ID, HUD_TAG_LOWER};
use onlydoms::mcp_protocol::{get_tools_list, handle_mcp_request, McpRequest};
use onlydoms::tools::curl_runner::{
    execute_http_over_stream, parse_curl_command, replay_curl, shlex_split,
};
use onlydoms::tools::hud_state::{format_executing_cmd, normalize_intent_preview, MAX_INTENT_CHARS};
use onlydoms::tools::input::resolve_quadrant_to_ratios;
use onlydoms::tools::network_harvester::serialize_to_curl;
use onlydoms::websocket_hub::{handle_http_report_stream, resolve_report_file_path, HubState};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

const VALID_PDF_BASE64: &str = "JVBERi0xLjQKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFszIDAgUl0gL0NvdW50IDEgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL1BhZ2UgL1BhcmVudCAyIDAgUiAvTWVkaWFCb3ggWzAgMCA2MTIgNzkyXSA+PgplbmRvYmoKeHJlZgowIDQKMDAwMDAwMDAwMCA2NTUzNSBmIAowMDAwMDAwMDA5IDAwMDAwIG4gCjAwMDAwMDAwNTggMDAwMDAgbiAKMDAwMDAwMDExNSAwMDAwMCBuIAp0cmFpbGVyCjw8IC9TaXplIDQgL1Jvb3QgMSAwIFIgPj4Kc3RhcnR4cmVmCjE4NwolJUVPRgo=";
const VALID_WEBP_BASE64: &str = "UklGRiQAAABXRUJQVlA4IBgAAAAwAQCdASoBAAEAAwA0JaQAA3AA/vuUAAA=";

/// Helper: spawns a mock extension hub responder that records all HUD_STATE_UPDATE frames
/// and responds to CDP commands (including injecting simulated HUD noise to verify blindspot stripping).
async fn setup_mock_extension_hub() -> (
    Arc<Mutex<HubState>>,
    Arc<Mutex<Vec<Value>>>,
    tokio::task::JoinHandle<()>,
) {
    let state = Arc::new(Mutex::new(HubState::new()));
    let (tx_hub, mut rx_hub) = tokio::sync::mpsc::channel::<Value>(128);
    let (tx_hud, mut rx_hud) = tokio::sync::mpsc::channel::<Value>(128);
    let hud_frames = Arc::new(Mutex::new(Vec::<Value>::new()));

    {
        let mut lock = state.lock().await;
        lock.tx = Some(tx_hub);
        lock.hud_tx = Some(tx_hud);
        lock.conn_token = 1;
    }

    let hud_frames_clone = hud_frames.clone();
    tokio::spawn(async move {
        while let Some(frame) = rx_hud.recv().await {
            hud_frames_clone.lock().await.push(frame);
        }
    });

    let state_clone = state.clone();
    let handle = tokio::spawn(async move {
        while let Some(msg) = rx_hub.recv().await {
            let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if msg_type == "HUD_STATE_UPDATE" {
                continue;
            }

            let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");

            let response = match (msg_type, method) {
                ("CREATE_TAB", _) => json!({
                    "id": id,
                    "result": { "tabId": 777 }
                }),
                ("CLOSE_TAB", _) | ("SWITCH_TAB", _) | ("INJECT_SCRIPT", _) | ("CLOAK_HUD", _) | ("UNCLOAK_HUD", _) => json!({
                    "id": id,
                    "result": { "success": true }
                }),
                ("CDP_CMD", "Page.navigate") => json!({
                    "id": id,
                    "result": { "frameId": "frame-1" }
                }),
                ("CDP_CMD", "Page.captureScreenshot") => json!({
                    "id": id,
                    "result": { "data": VALID_WEBP_BASE64 }
                }),
                ("CDP_CMD", "Page.printToPDF") => json!({
                    "id": id,
                    "result": { "data": VALID_PDF_BASE64 }
                }),
                ("CDP_CMD", "Runtime.evaluate") => {
                    let expr = msg
                        .get("params")
                        .and_then(|p| p.get("expression"))
                        .and_then(|e| e.as_str())
                        .unwrap_or("");

                    if expr.contains("await fetch(") {
                        let has_custom_hdr = expr.contains("X-Replay-Mode") && expr.contains("rust-native");
                        let has_modified_user = expr.contains("superuser");
                        let body_payload = json!({
                            "authenticated": true,
                            "header_verified": has_custom_hdr,
                            "body_verified": has_modified_user,
                            "leaked_hud": "<mcp-agent-hud id=\"__mcp_agent_hud_host__\">should_be_stripped</mcp-agent-hud>"
                        })
                        .to_string();
                        json!({
                            "id": id,
                            "result": {
                                "result": {
                                    "value": {
                                        "status": 200,
                                        "headers": {
                                            "content-type": "application/json",
                                            "x-auth-server": "OnlyDOMs-Test"
                                        },
                                        "body": body_payload
                                    }
                                }
                            }
                        })
                    } else if expr.contains("window.innerWidth") && expr.contains("JSON.stringify") {
                        json!({
                            "id": id,
                            "result": {
                                "result": {
                                    "type": "string",
                                    "value": "{\"width\":1920,\"height\":1080}"
                                }
                            }
                        })
                    } else if expr.contains("resolveEl") {
                        json!({
                            "id": id,
                            "result": {
                                "result": {
                                    "value": {
                                        "tagName": "button",
                                        "id": "underlying-p9-action",
                                        "node": {
                                            "tagName": "mcp-agent-hud",
                                            "id": "__mcp_agent_hud_host__"
                                        }
                                    }
                                }
                            }
                        })
                    } else if expr.contains("outerHTML") {
                        json!({
                            "id": id,
                            "result": {
                                "result": {
                                    "type": "string",
                                    "value": "<main><h1>Order Confirmed</h1><mcp-agent-hud id=\"__mcp_agent_hud_host__\" data-mcp-internal-ignore=\"true\">Executing :: click</mcp-agent-hud></main>"
                                }
                            }
                        })
                    } else {
                        json!({
                            "id": id,
                            "result": {
                                "result": {
                                    "type": "boolean",
                                    "value": true
                                }
                            }
                        })
                    }
                }
                _ => json!({
                    "id": id,
                    "result": { "success": true }
                }),
            };

            let mut lock = state_clone.lock().await;
            if let Some(sender) = lock.pending.remove(&id) {
                let _ = sender.send(response);
            }
        }
    });

    (state, hud_frames, handle)
}

// =============================================================================
// 1. RUST SANITIZER SUITE (Section 6.A & 7.1)
// =============================================================================

#[test]
fn test_hud_sanitizer_comprehensive_matrix() {
    let mut complex_payload = json!({
        "domTree": {
            "nodeName": "BODY",
            "children": [
                {
                    "nodeName": "DIV",
                    "id": "app-root",
                    "outerHTML": "<div id=\"app-root\"><span>Real Content</span><mcp-agent-hud id=\"__mcp_agent_hud_host__\" data-mcp-internal-ignore=\"true\">HUD Pill</mcp-agent-hud></div>"
                },
                {
                    "nodeName": "mcp-agent-hud",
                    "id": "__mcp_agent_hud_host__",
                    "attributes": ["id", "__mcp_agent_hud_host__", "data-mcp-internal-ignore", "true"]
                },
                {
                    "nodeName": "DIV",
                    "attributes": ["class", "floating", "data-mcp-internal-ignore", "true"]
                }
            ],
            "element": {
                "tag": "mcp-agent-hud"
            },
            "target": {
                "selector": "body > mcp-agent-hud#__mcp_agent_hud_host__"
            }
        }
    });

    sanitize_extension_payload(&mut complex_payload);

    let children = complex_payload["domTree"]["children"].as_array().unwrap();
    assert_eq!(children.len(), 1, "Both HUD nodes must be pruned from children array");
    assert_eq!(children[0]["id"], "app-root");

    let cleaned_html = children[0]["outerHTML"].as_str().unwrap();
    assert_eq!(cleaned_html, "<div id=\"app-root\"><span>Real Content</span></div>");
    assert!(!cleaned_html.contains(HUD_TAG_LOWER));
    assert!(!cleaned_html.contains(HUD_ID));
    assert!(!cleaned_html.contains(HUD_ATTR));

    assert!(complex_payload["domTree"].get("element").is_none());
    assert!(complex_payload["domTree"].get("target").is_none());
}

// =============================================================================
// 2. 180-CHAR INTENT & LIVE COMMAND TICKER VERIFICATION (Section 3 & 7.2)
// =============================================================================

#[test]
fn test_all_mcp_tools_expose_180_char_intent_schema() {
    let tools_list = get_tools_list();
    let tools = tools_list["tools"].as_array().expect("tools must be array");
    assert!(!tools.is_empty());

    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        let props = tool["inputSchema"]["properties"]
            .as_object()
            .unwrap_or_else(|| panic!("Tool {} missing properties", name));
        let intent_prop = props
            .get("_intent")
            .unwrap_or_else(|| panic!("Tool {} missing _intent schema property", name));
        assert_eq!(intent_prop["type"], "string");
        assert_eq!(intent_prop["maxLength"], MAX_INTENT_CHARS);
    }
}

#[tokio::test]
async fn test_four_step_pipeline_with_175_char_intent_and_multi_tab_concurrency() {
    let (state, hud_frames, mock_handle) = setup_mock_extension_hub().await;

    {
        let mut lock = state.lock().await;
        lock.tab_registry.acquire_tab("agent_1", 142).unwrap();
        lock.tab_registry.acquire_tab("agent_2", 205).unwrap();
    }

    let intent_175 = "Submit checkout form with test Visa card on staging portal, await payment gateway confirmation modal, click receipt download, and verify 200 OK order creation response on #142";
    assert_eq!(intent_175.chars().count(), 175);

    let pipeline_req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!("rpc-pipeline-142")),
        method: "tools/call".to_string(),
        params: Some(json!({
            "name": "execute_browser_pipeline",
            "arguments": {
                "tab_id": 142,
                "tab_title": "Checkout",
                "_intent": intent_175,
                "steps": [
                    { "action": "navigate", "url": "https://staging.example.com/checkout" },
                    { "action": "fill_inputs", "fields": [{ "selector": "#card", "value": "4242" }] },
                    { "action": "click", "selector": "#pay-btn" },
                    { "action": "wait_for", "selector": "#order-confirmed" }
                ]
            }
        })),
    };

    let tab2_req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!("rpc-eval-205")),
        method: "tools/call".to_string(),
        params: Some(json!({
            "name": "evaluate_expression",
            "arguments": {
                "tab_id": 205,
                "tab_title": "Admin Logs",
                "_intent": "Inspect live order webhook stream in admin dashboard on Tab #205",
                "expression": "document.body.outerHTML"
            }
        })),
    };

    let (resp1, resp2) = tokio::join!(
        handle_mcp_request("agent_1", pipeline_req, state.clone()),
        handle_mcp_request("agent_2", tab2_req, state.clone())
    );

    let r1 = resp1.unwrap();
    assert!(r1.error.is_none());
    let r2 = resp2.unwrap();
    assert!(r2.error.is_none());

    let r2_text = r2.result.unwrap()["content"][0]["text"].as_str().unwrap().to_string();
    assert!(!r2_text.contains("mcp-agent-hud"));
    assert!(!r2_text.contains("__mcp_agent_hud_host__"));

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let frames = hud_frames.lock().await;
    let tab142_frames: Vec<&Value> = frames
        .iter()
        .filter(|f| f["tab_id"] == 142)
        .collect();
    assert!(
        tab142_frames.len() >= 5,
        "Expected pre-flight + 4 step updates on Tab 142, got {}",
        tab142_frames.len()
    );

    assert_eq!(tab142_frames[0]["intent"].as_str().unwrap(), intent_175);

    let cmds: Vec<&str> = tab142_frames
        .iter()
        .filter_map(|f| f["executing_cmd"].as_str())
        .collect();
    assert!(cmds.iter().any(|c| *c == "Executing :: navigate(https://staging.example.com/checkout) on Tab :: #142 (Checkout)"));
    assert!(cmds.iter().any(|c| *c == "Executing :: fill_inputs(1 fields) on Tab :: #142 (Checkout)"));
    assert!(cmds.iter().any(|c| *c == "Executing :: click(#pay-btn) on Tab :: #142 (Checkout)"));
    assert!(cmds.iter().any(|c| *c == "Executing :: wait_for(#order-confirmed) on Tab :: #142 (Checkout)"));

    let lock = state.lock().await;
    assert!(lock.hud_tasks.contains_key(&142));
    assert!(lock.hud_tasks.contains_key(&205));
    assert_eq!(
        format_executing_cmd("fill_inputs (3 fields)", 142, Some("Checkout")),
        "Executing :: fill_inputs (3 fields) on Tab :: #142 (Checkout)"
    );
    assert_eq!(
        normalize_intent_preview(Some(intent_175), "execute_browser_pipeline", &json!({})).len(),
        175
    );

    mock_handle.abort();
}

// =============================================================================
// 3. OPTIMISTIC NETWORK CAPTURE & ZERO-SUBPROCESS RUST cURL REPLAY (Section 4 & 7.3)
// =============================================================================

#[tokio::test]
async fn test_curl_capture_and_native_rust_replay_e2e() {
    let (state, _hud_frames, mock_handle) = setup_mock_extension_hub().await;

    // 1. Verify native Rust HTTP/1.1 wire protocol executor over an in-memory duplex stream
    let (client_stream, mut server_stream) = tokio::io::duplex(16384);
    let server_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        let n = server_stream.read(&mut buf).await.unwrap_or(0);
        let req_text = String::from_utf8_lossy(&buf[..n]).to_string();
        assert!(req_text.starts_with("POST /api/login HTTP/1.1"));
        assert!(req_text.to_ascii_lowercase().contains("x-replay-mode: rust-native"));
        assert!(req_text.contains("\"role\":\"superuser\""));

        let body = "{\"status\":\"ok\",\"mode\":\"stream\"}";
        let resp = format!(
            "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = server_stream.write_all(resp.as_bytes()).await;
    });

    let raw_curl = "curl -X POST 'http://staging.example.com/api/login' -H 'Content-Type: application/json' -H 'X-Replay-Mode: rust-native' --data-raw '{\"username\":\"alice\",\"role\":\"superuser\"}'";
    let parsed_curl = parse_curl_command(raw_curl).unwrap();
    let (wire_status, wire_headers, wire_body) = execute_http_over_stream(
        client_stream,
        &parsed_curl,
        "staging.example.com",
        80,
        "/api/login",
        5000,
    )
    .await
    .unwrap();
    assert_eq!(wire_status, 201);
    assert_eq!(wire_headers.get("Content-Type").map(|s| s.as_str()), Some("application/json"));
    assert!(wire_body.contains("\"mode\":\"stream\""));
    let _ = server_task.await;

    // 2. Record agent step on Tab 88 and ingest optimistic POST /api/login within 3000ms
    {
        let mut lock = state.lock().await;
        lock.network_harvester.record_agent_step(88, "rpc_login_submit", 50_000);

        // Ignored image request (must be filtered out)
        let img_ev = json!({
            "type": "EVENT",
            "event": "NETWORK_CAPTURE",
            "tabId": 88,
            "payload": {
                "requestId": "req_img_1",
                "resourceType": "Image",
                "url": "https://staging.example.com/logo.png",
                "method": "GET",
                "timestamp_ms": 50_100
            }
        });
        assert!(lock.network_harvester.ingest_extension_event(&img_ev).is_none());

        // Captured POST /api/login Fetch request within 3000ms window
        let login_ev = json!({
            "type": "EVENT",
            "event": "NETWORK_CAPTURE",
            "tabId": 88,
            "payload": {
                "requestId": "req_login_post_1",
                "resourceType": "Fetch",
                "url": "https://staging.example.com/api/login",
                "method": "POST",
                "headers": {
                    "Content-Type": "application/json",
                    "X-Session": "initial"
                },
                "postData": "{\"username\":\"alice\",\"role\":\"user\"}",
                "timestamp_ms": 50_450
            }
        });
        let captured = lock.network_harvester.ingest_extension_event(&login_ev).unwrap();
        assert_eq!(captured.rpc_id.as_deref(), Some("rpc_login_submit"));
        assert!(captured.curl_command.starts_with("curl -X POST"));
    }

    // 3. Replay captured request via replay_curl with modified header and body (zero subprocesses)
    let replay_res = replay_curl(
        state.clone(),
        Some(88),
        json!({
            "_intent": "Replay captured POST /api/login with elevated role header and verify 200 OK",
            "request_id": "req_login_post_1",
            "headers": {
                "X-Replay-Mode": "rust-native"
            },
            "body": "{\"username\":\"alice\",\"role\":\"superuser\"}",
            "mode": "browser"
        }),
    )
    .await
    .unwrap();

    assert_eq!(replay_res["success"], true);
    assert_eq!(replay_res["status_code"], 200);
    assert_eq!(replay_res["body_json"]["authenticated"], true);
    assert_eq!(replay_res["body_json"]["header_verified"], true);
    assert_eq!(replay_res["body_json"]["body_verified"], true);
    // Verify HUD sanitizer scrubbed the leaked <mcp-agent-hud> inside the response body
    assert_eq!(replay_res["body_json"]["leaked_hud"], "");
    assert!(!replay_res["sanitized_body"].as_str().unwrap().contains("mcp-agent-hud"));

    // Verify shlex & curl serializer roundtrip
    let tokens = shlex_split(replay_res["curl_command"].as_str().unwrap()).unwrap();
    assert_eq!(tokens[0], "curl");
    let reparsed = parse_curl_command(replay_res["curl_command"].as_str().unwrap()).unwrap();
    assert_eq!(reparsed.method, "POST");
    let reserialized = serialize_to_curl(&reparsed.method, &reparsed.url, &reparsed.headers, reparsed.body.as_deref());
    assert!(reserialized.contains("--data-raw"));

    mock_handle.abort();
}

// =============================================================================
// 4. BLINDSPOT P9 PASSTHROUGH & VISUAL REPORT PIPELINE E2E (Section 5, 6 & 7.4)
// =============================================================================

#[tokio::test]
async fn test_blindspot_p9_click_and_visual_report_pipeline_e2e() {
    let (state, _hud_frames, mock_handle) = setup_mock_extension_hub().await;

    let temp_reports_root = std::env::temp_dir().join(format!("onlydoms_reports_e2e_{}", uuid::Uuid::new_v4()));
    std::env::set_var("ONLYDOMS_REPORTS_DIR", &temp_reports_root);

    {
        let mut lock = state.lock().await;
        lock.active_port = 9222;
        lock.tab_registry.acquire_tab("agent_qa", 101).unwrap();
        lock.network_harvester.record_agent_step(101, "checkout_p9_run_step_2", 100_000);
        lock.network_harvester.ingest_extension_event(&json!({
            "type": "EVENT",
            "event": "NETWORK_CAPTURE",
            "tabId": 101,
            "payload": {
                "requestId": "req_p9_checkout",
                "resourceType": "XHR",
                "url": "https://staging.example.com/api/checkout/confirm",
                "method": "POST",
                "headers": { "Content-Type": "application/json" },
                "postData": "{\"confirm\":true}",
                "timestamp_ms": 100_200
            }
        }));
    }

    let (p9_x, p9_y) = resolve_quadrant_to_ratios("P9", 9).unwrap();
    assert!(p9_x > 0.90 && p9_y > 0.90, "P9 must resolve to bottom-right corner");

    let test_req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!("rpc-test-p9")),
        method: "tools/call".to_string(),
        params: Some(json!({
            "name": "run_browser_test",
            "arguments": {
                "tab_id": 101,
                "run_id": "checkout_p9_run",
                "test_name": "Checkout P9 Blindspot Verification",
                "_intent": "Verify bottom-right P9 click passes through 3-tab HUD carousel and generates clean HTML/PDF overview",
                "steps": [
                    {
                        "name": "Open checkout page",
                        "action": "navigate",
                        "url": "https://staging.example.com/checkout"
                    },
                    {
                        "name": "Click bottom-right action button at P9 under HUD pill",
                        "action": "click",
                        "quadrant": "P9"
                    },
                    {
                        "name": "Assert order confirmation visible",
                        "action": "assert",
                        "selector": "#order-confirmed"
                    }
                ]
            }
        })),
    };

    let resp = handle_mcp_request("agent_qa", test_req, state.clone()).await.unwrap();
    assert!(resp.error.is_none(), "Unexpected error: {:?}", resp.error);

    let result_text = resp.result.unwrap()["content"][0]["text"].as_str().unwrap().to_string();
    let result_json: Value = serde_json::from_str(&result_text).unwrap();

    assert_eq!(result_json["success"], true);
    assert_eq!(result_json["passed"], true);
    assert_eq!(result_json["run_id"], "checkout_p9_run");

    assert_eq!(
        result_json["report_links"]["html_report"],
        "http://127.0.0.1:9222/reports/checkout_p9_run/overview.html"
    );
    assert_eq!(
        result_json["report_links"]["pdf_document"],
        "http://127.0.0.1:9222/reports/checkout_p9_run/overview.pdf"
    );
    assert_eq!(
        result_json["report_links"]["instant_print"],
        "http://127.0.0.1:9222/reports/checkout_p9_run/overview.html?print=1"
    );

    let run_dir = temp_reports_root.join("checkout_p9_run");
    assert!(run_dir.join("assets").join("step_1.webp").exists());
    assert!(run_dir.join("assets").join("step_2.webp").exists());
    assert!(run_dir.join("assets").join("step_3.webp").exists());
    assert!(run_dir.join("overview.html").exists());
    assert!(run_dir.join("overview.pdf").exists());

    let html_on_disk = tokio::fs::read_to_string(run_dir.join("overview.html")).await.unwrap();
    assert!(html_on_disk.contains("assets/step_1.webp"));
    assert!(html_on_disk.contains("assets/step_2.webp"));
    assert!(html_on_disk.contains("@media print"));
    assert!(html_on_disk.contains("window.print()"));
    assert!(!html_on_disk.contains("mcp-agent-hud"));
    assert!(!html_on_disk.contains("__mcp_agent_hud_host__"));

    let pdf_on_disk = tokio::fs::read(run_dir.join("overview.pdf")).await.unwrap();
    assert!(pdf_on_disk.starts_with(b"%PDF-"));

    assert!(resolve_report_file_path("/reports/../etc/passwd", &temp_reports_root).is_err());
    let (_, content_type, is_print) =
        resolve_report_file_path("/reports/checkout_p9_run/overview.html?print=1", &temp_reports_root).unwrap();
    assert_eq!(content_type, "text/html; charset=utf-8");
    assert!(is_print);

    let (mut client_stream, server_stream) = tokio::io::duplex(65536);
    let reports_root_clone = temp_reports_root.clone();
    tokio::spawn(async move {
        handle_http_report_stream(server_stream, &reports_root_clone).await;
    });

    client_stream
        .write_all(b"GET /reports/checkout_p9_run/overview.html?print=1 HTTP/1.1\r\nHost: 127.0.0.1:9222\r\n\r\n")
        .await
        .unwrap();
    let mut http_resp_bytes = Vec::new();
    client_stream.read_to_end(&mut http_resp_bytes).await.unwrap();
    let http_resp_str = String::from_utf8_lossy(&http_resp_bytes);
    assert!(http_resp_str.starts_with("HTTP/1.1 200 OK"));
    assert!(http_resp_str.contains("Content-Type: text/html; charset=utf-8"));
    assert!(http_resp_str.contains("window.print()"));

    let _ = tokio::fs::remove_dir_all(&temp_reports_root).await;
    std::env::remove_var("ONLYDOMS_REPORTS_DIR");
    mock_handle.abort();
}
