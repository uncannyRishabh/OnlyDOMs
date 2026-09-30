use crate::websocket_hub::{SharedHub, send_cdp_command, send_hub_command};
use crate::tools::coordinate_transformer::CoordinateTransformer;
use serde_json::Value;

pub async fn click_quadrant(state: SharedHub, params: Value) -> Result<Value, String> {
    let cell = params.get("cell").and_then(|v| v.as_str()).unwrap_or("A1");
    let offset_u = params.get("offset_x_ratio").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let offset_v = params.get("offset_y_ratio").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
    let click_count = params.get("click_count").and_then(|v| v.as_u64()).unwrap_or(1);

    let dim_res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
        "expression": "JSON.stringify({width: window.innerWidth, height: window.innerHeight})",
        "returnByValue": true
    })).await?;
    let val_str = dim_res.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_str()).unwrap_or("{}");
    let dim_val: Value = serde_json::from_str(val_str).unwrap_or(serde_json::json!({}));
    let w = dim_val.get("width").and_then(|v| v.as_f64()).unwrap_or(1920.0) as f32;
    let h = dim_val.get("height").and_then(|v| v.as_f64()).unwrap_or(1080.0) as f32;

    let (css_x, css_y) = CoordinateTransformer::cell_to_css(cell, w, h, offset_u, offset_v)?;

    for _ in 0..click_count {
        send_cdp_command(state.clone(), "Input.dispatchMouseEvent", serde_json::json!({
            "type": "mousePressed", "x": css_x, "y": css_y, "button": "left", "clickCount": 1
        })).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        send_cdp_command(state.clone(), "Input.dispatchMouseEvent", serde_json::json!({
            "type": "mouseReleased", "x": css_x, "y": css_y, "button": "left", "clickCount": 1
        })).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
    Ok(serde_json::json!({ "success": true, "x": css_x, "y": css_y }))
}

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
    
    let mut fields = params.get("fields").and_then(|f| f.as_array()).cloned().unwrap_or_default();
    
    let dim_res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
        "expression": "JSON.stringify({width: window.innerWidth, height: window.innerHeight})",
        "returnByValue": true
    })).await?;
    let val_str = dim_res.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_str()).unwrap_or("{}");
    let dim_val: Value = serde_json::from_str(val_str).unwrap_or(serde_json::json!({}));
    let w = dim_val.get("width").and_then(|v| v.as_f64()).unwrap_or(1920.0) as f32;
    let h = dim_val.get("height").and_then(|v| v.as_f64()).unwrap_or(1080.0) as f32;

    for field in fields.iter_mut() {
        if let Some(obj) = field.as_object_mut() {
            if let Some(cell) = obj.get("cell").and_then(|c| c.as_str()) {
                if let Ok((x, y)) = CoordinateTransformer::cell_to_css(cell, w, h, 0.0, 0.0) {
                    obj.insert("x".to_string(), serde_json::json!(x));
                    obj.insert("y".to_string(), serde_json::json!(y));
                }
            }
        }
    }

    let fields_json = serde_json::to_string(&fields).unwrap_or("[]".to_string());
    let expr = format!("window.magibotInput && window.magibotInput.fill({});", fields_json);
    
    let res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
        "expression": expr,
        "awaitPromise": true
    })).await?;

    Ok(res)
}
