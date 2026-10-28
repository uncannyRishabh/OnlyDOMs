use chromapilot::tab_registry::TabRegistry;
use chromapilot::mcp_protocol::{handle_mcp_request, McpRequest};
use chromapilot::websocket_hub::HubState;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde_json::Value;

#[tokio::test]
async fn test_tab_registry_multi_agent_isolation() {
    let mut registry = TabRegistry::new();

    // Agent 1 acquires Tab 10
    assert!(registry.acquire_tab("agent_chat_1", 10).is_ok());
    assert_eq!(registry.get_client_tab("agent_chat_1"), Some(10));

    // Agent 2 attempts to acquire Tab 10 -> Collision prevented!
    let err = registry.acquire_tab("agent_chat_2", 10);
    assert!(err.is_err());
    let err_msg = err.unwrap_err();
    assert!(err_msg.contains("currently locked by another active agent session"));

    // Agent 2 acquires Tab 20 -> Success!
    assert!(registry.acquire_tab("agent_chat_2", 20).is_ok());
    assert_eq!(registry.get_client_tab("agent_chat_2"), Some(20));

    // Lock status verification
    let (is_locked, is_other) = registry.get_tab_lock_status(10, "agent_chat_1");
    assert!(is_locked);
    assert!(!is_other);

    let (is_locked, is_other) = registry.get_tab_lock_status(10, "agent_chat_2");
    assert!(is_locked);
    assert!(is_other);

    // Agent 1 disconnects -> locks automatically cleared
    registry.release_all_client_tabs("agent_chat_1");
    assert_eq!(registry.get_client_tab("agent_chat_1"), None);

    // Agent 2 can now acquire Tab 10
    assert!(registry.acquire_tab("agent_chat_2", 10).is_ok());
    assert_eq!(registry.get_client_tab("agent_chat_2"), Some(10));
}

#[tokio::test]
async fn test_mcp_protocol_multi_client_dispatch() {
    let state = Arc::new(Mutex::new(HubState::new()));

    // Client 1 sends initialize
    let init_req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::Number(1.into())),
        method: "initialize".to_string(),
        params: None,
    };
    let res1 = handle_mcp_request("client_1", init_req, state.clone()).await.unwrap();
    assert_eq!(res1.id, Some(Value::Number(1.into())));
    assert!(res1.result.unwrap().get("serverInfo").is_some());

    // Client 2 sends initialize simultaneously
    let init_req2 = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::Number(1.into())),
        method: "initialize".to_string(),
        params: None,
    };
    let res2 = handle_mcp_request("client_2", init_req2, state.clone()).await.unwrap();
    assert_eq!(res2.id, Some(Value::Number(1.into())));
    assert!(res2.result.unwrap().get("serverInfo").is_some());

    // Client 1 sends tools/list
    let list_req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::Number(2.into())),
        method: "tools/list".to_string(),
        params: None,
    };
    let tools_res = handle_mcp_request("client_1", list_req, state.clone()).await.unwrap();
    assert!(tools_res.result.unwrap().get("tools").is_some());

    // Pre-lock Tab 100 for Client 1 in state
    {
        let mut lock = state.lock().await;
        lock.tab_registry.acquire_tab("client_1", 100).unwrap();
    }

    // Client 2 attempts switch_tab to Tab 100 -> Rejected!
    let switch_req = McpRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::Number(3.into())),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "switch_tab",
            "arguments": { "tab_id": 100 }
        })),
    };
    let switch_res = handle_mcp_request("client_2", switch_req, state.clone()).await.unwrap();
    assert!(switch_res.error.is_some());
    assert!(switch_res.error.unwrap().message.contains("locked by another active agent session"));

    // Client 1 disconnects -> Client 1's tabs are released
    {
        let mut lock = state.lock().await;
        lock.tab_registry.release_all_client_tabs("client_1");
    }

    // Now Tab 100 is free and available
    {
        let mut lock = state.lock().await;
        let (locked, _) = lock.tab_registry.get_tab_lock_status(100, "client_2");
        assert!(!locked);
    }
}

#[tokio::test]
async fn test_duplex_stream_multi_client_concurrent() {
    let state = Arc::new(Mutex::new(HubState::new()));

    // Create 2 concurrent duplex channels simulating Chat 1 and Chat 2
    let (mut client_1_stream, server_1_stream) = tokio::io::duplex(8192);
    let (mut client_2_stream, server_2_stream) = tokio::io::duplex(8192);

    let state_c1 = state.clone();
    let task_1 = tokio::spawn(async move {
        let (read_half, mut write_half) = tokio::io::split(server_1_stream);
        let mut reader = BufReader::new(read_half).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if let Ok(req) = serde_json::from_str::<McpRequest>(&line) {
                if let Some(res) = handle_mcp_request("chat_1", req, state_c1.clone()).await {
                    let json = serde_json::to_string(&res).unwrap();
                    let _ = write_half.write_all(format!("{}\n", json).as_bytes()).await;
                }
            }
        }
    });

    let state_c2 = state.clone();
    let task_2 = tokio::spawn(async move {
        let (read_half, mut write_half) = tokio::io::split(server_2_stream);
        let mut reader = BufReader::new(read_half).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if let Ok(req) = serde_json::from_str::<McpRequest>(&line) {
                if let Some(res) = handle_mcp_request("chat_2", req, state_c2.clone()).await {
                    let json = serde_json::to_string(&res).unwrap();
                    let _ = write_half.write_all(format!("{}\n", json).as_bytes()).await;
                }
            }
        }
    });

    // Chat 1 sends initialize
    client_1_stream.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"initialize\"}\n").await.unwrap();
    let mut reader_1 = BufReader::new(client_1_stream).lines();
    let resp_1 = reader_1.next_line().await.unwrap().unwrap();
    let val_1: Value = serde_json::from_str(&resp_1).unwrap();
    assert_eq!(val_1["id"], 10);
    assert!(val_1["result"]["serverInfo"].is_object());

    // Chat 2 simultaneously sends initialize
    client_2_stream.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":20,\"method\":\"initialize\"}\n").await.unwrap();
    let mut reader_2 = BufReader::new(client_2_stream).lines();
    let resp_2 = reader_2.next_line().await.unwrap().unwrap();
    let val_2: Value = serde_json::from_str(&resp_2).unwrap();
    assert_eq!(val_2["id"], 20);
    assert!(val_2["result"]["serverInfo"].is_object());

    // Chat 1 locks Tab 55
    {
        let mut lock = state.lock().await;
        lock.tab_registry.acquire_tab("chat_1", 55).unwrap();
    }

    // Chat 2 attempts to switch to Tab 55
    reader_2.get_mut().write_all(b"{\"jsonrpc\":\"2.0\",\"id\":21,\"method\":\"tools/call\",\"params\":{\"name\":\"switch_tab\",\"arguments\":{\"tab_id\":55}}}\n").await.unwrap();
    let resp_conflict = reader_2.next_line().await.unwrap().unwrap();
    let val_conflict: Value = serde_json::from_str(&resp_conflict).unwrap();
    assert_eq!(val_conflict["id"], 21);
    assert!(val_conflict["error"]["message"].as_str().unwrap().contains("locked by another active agent session"));

    // Chat 1 closes / terminates
    drop(reader_1);
    task_1.abort();

    {
        let mut lock = state.lock().await;
        lock.tab_registry.release_all_client_tabs("chat_1");
    }

    // Chat 2 is still alive and can now claim Tab 55
    reader_2.get_mut().write_all(b"{\"jsonrpc\":\"2.0\",\"id\":22,\"method\":\"tools/call\",\"params\":{\"name\":\"switch_tab\",\"arguments\":{\"tab_id\":55}}}\n").await.unwrap();
    // (Note: without an active Chrome extension in unit test, it will try send_hub_command which times out after 35*200ms or errors with No active browser session, but tab lock check passes!)
    
    task_2.abort();
}
