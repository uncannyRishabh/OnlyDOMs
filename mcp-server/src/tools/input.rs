use crate::websocket_hub::{SharedHub, send_cdp_command, send_hub_command};
use serde_json::Value;

pub async fn interact(state: SharedHub, params: Value) -> Result<Value, String> {
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("click");
    let x_ratio = params.get("x_ratio").and_then(|v| v.as_f64()).unwrap_or(0.5);
    let y_ratio = params.get("y_ratio").and_then(|v| v.as_f64()).unwrap_or(0.5);

    let dim_res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
        "expression": "JSON.stringify({width: window.innerWidth, height: window.innerHeight})",
        "returnByValue": true
    })).await?;
    let val_str = dim_res.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_str()).unwrap_or("{}");
    let dim_val: Value = serde_json::from_str(val_str).unwrap_or(serde_json::json!({}));
    let w = dim_val.get("width").and_then(|v| v.as_f64()).unwrap_or(1920.0);
    let h = dim_val.get("height").and_then(|v| v.as_f64()).unwrap_or(1080.0);

    let x = w * x_ratio;
    let y = h * y_ratio;

    if action == "click" {
        send_cdp_command(state.clone(), "Input.dispatchMouseEvent", serde_json::json!({
            "type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1
        })).await?;
        send_cdp_command(state.clone(), "Input.dispatchMouseEvent", serde_json::json!({
            "type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1
        })).await?;
    } else if action == "hover" {
        send_cdp_command(state.clone(), "Input.dispatchMouseEvent", serde_json::json!({
            "type": "mouseMoved", "x": x, "y": y
        })).await?;
    } else if action == "scroll" {
        let dx = params.get("deltaX").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let dy = params.get("deltaY").and_then(|v| v.as_f64()).unwrap_or(0.0);
        send_cdp_command(state.clone(), "Input.dispatchMouseEvent", serde_json::json!({
            "type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy
        })).await?;
    }
    Ok(serde_json::json!({ "success": true }))
}

pub async fn fill_inputs(state: SharedHub, params: Value) -> Result<Value, String> {
    send_hub_command(state.clone(), "INJECT_SCRIPT", "", serde_json::json!("content/input_filler.js")).await?;
    
    let fields = params.get("fields").and_then(|f| f.as_array()).cloned().unwrap_or_default();
    let fields_json = serde_json::to_string(&fields).unwrap_or("[]".to_string());
    let expr = format!("window.chromapilotInput && window.chromapilotInput.fill({});", fields_json);
    
    let res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
        "expression": expr,
        "awaitPromise": true,
        "returnByValue": true
    })).await?;

    Ok(res)
}
