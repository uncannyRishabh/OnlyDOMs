use crate::websocket_hub::{SharedHub, send_cdp_command, send_hub_command};
use serde_json::Value;

pub async fn capture_viewport(state: SharedHub, _cols: u32) -> Result<Value, String> {
    // inject overlay manager first
    send_hub_command(state.clone(), "INJECT_SCRIPT", "", serde_json::json!("content/overlay_manager.js")).await?;

    // 1. evaluate script to draw grid
    let draw_grid_expr = "window.magibotGrid && window.magibotGrid.drawGrid();";
    let draw_res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
        "expression": draw_grid_expr,
        "awaitPromise": true
    })).await?;

    let val_str = draw_res.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_str()).unwrap_or("{}");
    let dim_val: Value = serde_json::from_str(val_str).unwrap_or(serde_json::json!({}));
    
    let w = dim_val.get("viewport_css").and_then(|v| v.get("width")).and_then(|v| v.as_f64()).unwrap_or(1920.0);
    let h = dim_val.get("viewport_css").and_then(|v| v.get("height")).and_then(|v| v.as_f64()).unwrap_or(1080.0);
    let rows = dim_val.get("rows").and_then(|v| v.as_u64()).unwrap_or(9) as u32;

    // wait for drawing
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // capture screenshot
    let screenshot = send_cdp_command(state.clone(), "Page.captureScreenshot", serde_json::json!({
        "format": "webp",
        "quality": 80
    })).await?;
    let data = screenshot.get("data").and_then(|d| d.as_str()).unwrap_or("");

    // remove grid
    let remove_grid_expr = "window.magibotGrid && window.magibotGrid.removeGrid();";
    let _ = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
        "expression": remove_grid_expr,
        "awaitPromise": true
    })).await;

    Ok(serde_json::json!({
        "base64_image": data,
        "grid": {
            "cols": 16,
            "rows": rows,
            "viewport_css": {"width": w, "height": h},
            "cell_size_css": dim_val.get("cell_size_css").cloned().unwrap_or(Value::Null)
        }
    }))
}
