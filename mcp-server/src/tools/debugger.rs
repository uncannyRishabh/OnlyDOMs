use crate::websocket_hub::{SharedHub, send_cdp_command};
use serde_json::Value;

pub async fn evaluate_expression(state: SharedHub, params: Value) -> Result<Value, String> {
    let expression = params.get("expression").and_then(|v| v.as_str()).unwrap_or("");
    let return_by_value = params.get("return_by_value").and_then(|v| v.as_bool()).unwrap_or(true);

    let res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
        "expression": expression,
        "returnByValue": return_by_value
    })).await?;

    Ok(res)
}

pub async fn add_breakpoint(state: SharedHub, params: Value) -> Result<Value, String> {
    let url_regex = params.get("url_regex").and_then(|v| v.as_str()).unwrap_or(".*");
    let line_number = params.get("line_number").and_then(|v| v.as_u64()).unwrap_or(0);
    
    let mut payload = serde_json::json!({
        "urlRegex": url_regex,
        "lineNumber": line_number
    });

    if let Some(condition) = params.get("condition").and_then(|v| v.as_str()) {
        payload.as_object_mut().unwrap().insert("condition".to_string(), Value::String(condition.to_string()));
    }

    send_cdp_command(state.clone(), "Debugger.setBreakpointByUrl", payload).await
}

pub async fn pause_execution(state: SharedHub) -> Result<Value, String> {
    send_cdp_command(state.clone(), "Debugger.pause", serde_json::json!({})).await
}

pub async fn resume_execution(state: SharedHub) -> Result<Value, String> {
    send_cdp_command(state.clone(), "Debugger.resume", serde_json::json!({})).await
}

pub async fn step_over(state: SharedHub) -> Result<Value, String> {
    send_cdp_command(state.clone(), "Debugger.stepOver", serde_json::json!({})).await
}

pub async fn step_into(state: SharedHub) -> Result<Value, String> {
    send_cdp_command(state.clone(), "Debugger.stepInto", serde_json::json!({})).await
}
