use crate::websocket_hub::{SharedHub, send_cdp_command, send_hub_command};
use serde_json::Value;

/// JavaScript helper snippet ensuring any coordinate hit-test (`elementsFromPoint`)
/// ignores `<mcp-agent-hud>`, `#__mcp_agent_hud_host__`, and `[data-mcp-internal-ignore]`.
pub const HUD_HIT_TEST_PASSTHROUGH_JS: &str = r#"
(function(x, y) {
    if (typeof document.elementsFromPoint === 'function') {
        const els = document.elementsFromPoint(x, y);
        for (const el of els) {
            const tag = (el.nodeName || el.tagName || '').toLowerCase();
            if (tag === 'mcp-agent-hud' || el.id === '__mcp_agent_hud_host__' || (el.hasAttribute && el.hasAttribute('data-mcp-internal-ignore'))) {
                continue;
            }
            if (el.closest && el.closest('mcp-agent-hud, #__mcp_agent_hud_host__, [data-mcp-internal-ignore]')) {
                continue;
            }
            return el;
        }
    }
    return document.elementFromPoint(x, y);
})
"#;

/// Resolves a 16-column grid cell label (e.g., `"A1"` .. `"P9"` or `"P10"`) into normalized
/// `(x_ratio, y_ratio)` center coordinates in `[0.0, 1.0]`.
/// Columns `A`..`P` map to 16 columns (indices `0..16`); rows `1..=max_rows` map to vertical bands.
pub fn resolve_quadrant_to_ratios(cell: &str, default_rows: u32) -> Option<(f64, f64)> {
    let trimmed = cell.trim().to_ascii_uppercase();
    if trimmed.len() < 2 {
        return None;
    }
    let mut chars = trimmed.chars();
    let col_char = chars.next()?;
    if !('A'..='P').contains(&col_char) {
        return None;
    }
    let row_num: u32 = chars.as_str().parse().ok()?;
    let total_rows = default_rows.max(row_num).max(9);
    if row_num == 0 || row_num > 32 {
        return None;
    }

    let col_idx = (col_char as u32) - ('A' as u32);
    let row_idx = row_num - 1;

    let x_ratio = (col_idx as f64 + 0.5) / 16.0;
    let y_ratio = (row_idx as f64 + 0.5) / (total_rows as f64);
    Some((x_ratio.clamp(0.0, 1.0), y_ratio.clamp(0.0, 1.0)))
}

pub async fn interact(state: SharedHub, tab_id: Option<u64>, params: Value) -> Result<Value, String> {
    let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("click");

    let (x_ratio, y_ratio, resolved_cell) = if let Some(cell) = params
        .get("quadrant")
        .or_else(|| params.get("cell"))
        .and_then(|v| v.as_str())
    {
        let (xr, yr) = resolve_quadrant_to_ratios(cell, 9)
            .ok_or_else(|| format!("Invalid quadrant/cell coordinate '{}': expected A1..P9", cell))?;
        (xr, yr, Some(cell.to_ascii_uppercase()))
    } else {
        let xr = params.get("x_ratio").and_then(|v| v.as_f64()).unwrap_or(0.5);
        let yr = params.get("y_ratio").and_then(|v| v.as_f64()).unwrap_or(0.5);
        (xr, yr, None)
    };

    let dim_res = send_cdp_command(state.clone(), tab_id, "Runtime.evaluate", serde_json::json!({
        "expression": "JSON.stringify({width: window.innerWidth, height: window.innerHeight})",
        "returnByValue": true
    })).await?;
    let val_str = dim_res.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_str()).unwrap_or("{}");
    let dim_val: Value = serde_json::from_str(val_str).unwrap_or(serde_json::json!({}));
    let w = dim_val.get("width").and_then(|v| v.as_f64()).unwrap_or(1920.0);
    let h = dim_val.get("height").and_then(|v| v.as_f64()).unwrap_or(1080.0);

    let x = w * x_ratio;
    let y = h * y_ratio;

    let mut hit_target = Value::Null;

    if action == "click" {
        // Hit-test passthrough: ensure underlying element at (x, y) ignoring <mcp-agent-hud> receives focus/click
        let passthrough_expr = format!(
            "(() => {{
                const resolveEl = {passthrough};
                const el = resolveEl({x}, {y});
                if (!el) return null;
                return {{
                    tagName: (el.tagName || '').toLowerCase(),
                    id: el.id || null
                }};
            }})()",
            passthrough = HUD_HIT_TEST_PASSTHROUGH_JS,
            x = x,
            y = y
        );
        if let Ok(eval_val) = send_cdp_command(
            state.clone(),
            tab_id,
            "Runtime.evaluate",
            serde_json::json!({
                "expression": passthrough_expr,
                "returnByValue": true
            }),
        )
        .await
        {
            hit_target = eval_val
                .get("result")
                .and_then(|r| r.get("value"))
                .cloned()
                .unwrap_or(Value::Null);
        }

        send_cdp_command(state.clone(), tab_id, "Input.dispatchMouseEvent", serde_json::json!({
            "type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1
        })).await?;
        send_cdp_command(state.clone(), tab_id, "Input.dispatchMouseEvent", serde_json::json!({
            "type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1
        })).await?;
    } else if action == "hover" {
        send_cdp_command(state.clone(), tab_id, "Input.dispatchMouseEvent", serde_json::json!({
            "type": "mouseMoved", "x": x, "y": y
        })).await?;
    } else if action == "scroll" {
        let dx = params.get("deltaX").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let dy = params.get("deltaY").and_then(|v| v.as_f64()).unwrap_or(0.0);
        send_cdp_command(state.clone(), tab_id, "Input.dispatchMouseEvent", serde_json::json!({
            "type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy
        })).await?;
    }

    let mut out = serde_json::json!({
        "success": true,
        "x": x,
        "y": y
    });
    if let Some(cell) = resolved_cell {
        out["quadrant"] = serde_json::json!(cell);
    }
    if !hit_target.is_null() {
        out["hit_target"] = hit_target;
    }
    Ok(out)
}

pub async fn fill_inputs(state: SharedHub, tab_id: Option<u64>, params: Value) -> Result<Value, String> {
    send_hub_command(state.clone(), tab_id, "INJECT_SCRIPT", "", serde_json::json!("content/input_filler.js")).await?;

    let fields = params.get("fields").and_then(|f| f.as_array()).cloned().unwrap_or_default();
    let fields_json = serde_json::to_string(&fields).unwrap_or("[]".to_string());
    let expr = format!("(window.onlydomsInput || window.chromapilotInput) && (window.onlydomsInput || window.chromapilotInput).fill({});", fields_json);

    let res = send_cdp_command(state.clone(), tab_id, "Runtime.evaluate", serde_json::json!({
        "expression": expr,
        "awaitPromise": true,
        "returnByValue": true
    })).await?;

    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_quadrant_p9_bottom_right() {
        let (xr, yr) = resolve_quadrant_to_ratios("P9", 9).unwrap();
        assert!((xr - (15.5 / 16.0)).abs() < 1e-6);
        assert!((yr - (8.5 / 9.0)).abs() < 1e-6);

        let (xa, ya) = resolve_quadrant_to_ratios("A1", 9).unwrap();
        assert!((xa - (0.5 / 16.0)).abs() < 1e-6);
        assert!((ya - (0.5 / 9.0)).abs() < 1e-6);

        assert!(resolve_quadrant_to_ratios("Z9", 9).is_none());
        assert!(resolve_quadrant_to_ratios("P0", 9).is_none());
    }
}
