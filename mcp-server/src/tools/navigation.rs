use crate::websocket_hub::{SharedHub, send_cdp_command};
use serde_json::Value;

pub async fn execute_browser_pipeline(state: SharedHub, params: Value) -> Result<Value, String> {
    let steps = params.get("steps").and_then(|s| s.as_array()).cloned().unwrap_or_default();
    let mut final_url = String::new();
    let mut screenshot = None;
    
    for step in steps {
        let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("");
        match action {
            "navigate" => {
                let url = step.get("url").and_then(|v| v.as_str()).unwrap_or("");
                send_cdp_command(state.clone(), "Page.navigate", serde_json::json!({
                    "url": url
                })).await?;
            }
            "wait_for" => {
                let selector = step.get("selector").and_then(|v| v.as_str()).unwrap_or("");
                let timeout = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(5000);
                
                let start = std::time::Instant::now();
                loop {
                    if start.elapsed().as_millis() as u64 > timeout {
                        break;
                    }
                    let expr = format!("document.querySelector('{}') !== null", selector);
                    let res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                        "expression": expr,
                        "returnByValue": true
                    })).await;
                    if let Ok(val) = res {
                        if val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false) {
                            break;
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
            "wait_network_idle" => {
                let timeout = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(5000);
                let start = std::time::Instant::now();
                loop {
                    if start.elapsed().as_millis() as u64 > timeout {
                        break;
                    }
                    let res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                        "expression": "performance.getEntriesByType('resource').filter(e => !e.responseEnd).length === 0",
                        "returnByValue": true
                    })).await;
                    if let Ok(val) = res {
                        if val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false) {
                            break;
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }
            }
            "fill_inputs" => {
                let fields = step.get("fields").cloned().unwrap_or(serde_json::json!([]));
                crate::tools::input::fill_inputs(state.clone(), serde_json::json!({"fields": fields})).await?;
            }
            "capture_grid_screenshot" => {
                let quality = step.get("quality").and_then(|v| v.as_u64()).unwrap_or(80) as u32;
                if let Ok(res) = crate::tools::visual::capture_viewport(state.clone(), quality).await {
                    screenshot = res.get("base64_image").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
            }
            _ => {}
        }
    }
    
    let res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
        "expression": "window.location.href",
        "returnByValue": true
    })).await;
    if let Ok(val) = res {
        final_url = val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    }
    
    let mut result = serde_json::json!({ "success": true, "final_url": final_url });
    if let Some(s) = screenshot {
        result.as_object_mut().unwrap().insert("screenshot".to_string(), serde_json::json!(s));
    }
    Ok(result)
}
