use crate::websocket_hub::{SharedHub, send_cdp_command, send_hub_command};
use serde_json::Value;
use std::time::{Duration, Instant};

pub async fn execute_browser_pipeline(
    state: SharedHub,
    mut current_tab: Option<u64>,
    params: Value,
) -> Result<Value, String> {
    let steps_opt = params.get("steps").or_else(|| params.get("actions")).and_then(|s| s.as_array());
    let steps = match steps_opt {
        Some(arr) if !arr.is_empty() => arr.clone(),
        Some(_) => return Err("The 'steps' array is empty. Please provide at least one action.".to_string()),
        None => return Err("Missing required property 'steps' (or 'actions') array in execute_browser_pipeline.".to_string()),
    };
    let mut final_url = String::new();
    let mut screenshot = None;
    let mut step_results = Vec::new();
    
    for (idx, step) in steps.iter().enumerate() {
        let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("");
        match action {
            "navigate" => {
                let url = step.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let cdp_res = send_cdp_command(state.clone(), current_tab, "Page.navigate", serde_json::json!({
                    "url": url
                })).await;
                if cdp_res.is_err() {
                    if let Some(tid) = current_tab {
                        send_hub_command(state.clone(), Some(tid), "NAVIGATE_TAB", "", serde_json::json!({ "tabId": tid, "url": url })).await?;
                    } else {
                        cdp_res?;
                    }
                }
                
                // Resilient wait for frame/load
                tokio::time::sleep(Duration::from_millis(500)).await;
                step_results.push(serde_json::json!({ "step": idx, "action": "navigate", "status": "ok" }));
            }
            "new_tab" => {
                let url = step.get("url").and_then(|v| v.as_str()).unwrap_or("about:blank");
                let res = send_hub_command(state.clone(), None, "CREATE_TAB", "", serde_json::json!({ "url": url })).await?;
                if let Some(new_tid) = res.get("tabId").and_then(|v| v.as_u64()) {
                    current_tab = Some(new_tid);
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
                step_results.push(serde_json::json!({ "step": idx, "action": "new_tab", "status": "ok", "result": res }));
            }
            "switch_tab" => {
                let tab_id = step.get("tab_id").and_then(|v| v.as_u64()).unwrap_or(0);
                let res = send_hub_command(state.clone(), Some(tab_id), "SWITCH_TAB", "", serde_json::json!({ "tabId": tab_id })).await?;
                current_tab = Some(tab_id);
                step_results.push(serde_json::json!({ "step": idx, "action": "switch_tab", "status": "ok", "result": res }));
            }
            "wait_for" => {
                let selector = step.get("selector").and_then(|v| v.as_str()).unwrap_or("");
                let timeout_ms = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(8000);
                let start = Instant::now();
                let mut found = false;

                while start.elapsed().as_millis() as u64 <= timeout_ms {
                    let expr = format!(
                        "(() => {{ const el = document.querySelector('{}'); return !!(el && (el.offsetWidth || el.offsetHeight || el.getClientRects().length)); }})()",
                        selector.replace('\'', "\\'")
                    );
                    if let Ok(val) = send_cdp_command(state.clone(), current_tab, "Runtime.evaluate", serde_json::json!({
                        "expression": expr,
                        "returnByValue": true
                    })).await {
                        if val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false) {
                            found = true;
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
                if !found && !step.get("optional").and_then(|v| v.as_bool()).unwrap_or(false) {
                    return Err(format!("Timeout waiting for selector: '{}'", selector));
                }
                step_results.push(serde_json::json!({ "step": idx, "action": "wait_for", "found": found }));
            }
            "wait_condition" => {
                let expr = step.get("expression").and_then(|v| v.as_str()).unwrap_or("true");
                let timeout_ms = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(8000);
                let start = Instant::now();
                let mut satisfied = false;

                while start.elapsed().as_millis() as u64 <= timeout_ms {
                    if let Ok(val) = send_cdp_command(state.clone(), current_tab, "Runtime.evaluate", serde_json::json!({
                        "expression": expr,
                        "returnByValue": true,
                        "awaitPromise": true
                    })).await {
                        if val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false) {
                            satisfied = true;
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
                if !satisfied && !step.get("optional").and_then(|v| v.as_bool()).unwrap_or(false) {
                    return Err(format!("Timeout waiting for condition: '{}'", expr));
                }
                step_results.push(serde_json::json!({ "step": idx, "action": "wait_condition", "satisfied": satisfied }));
            }
            "wait_network_idle" => {
                let timeout_ms = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(5000);
                let start = Instant::now();
                while start.elapsed().as_millis() as u64 <= timeout_ms {
                    let res = send_cdp_command(state.clone(), current_tab, "Runtime.evaluate", serde_json::json!({
                        "expression": "performance.getEntriesByType('resource').filter(e => !e.responseEnd).length === 0",
                        "returnByValue": true
                    })).await;
                    if let Ok(val) = res {
                        if val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false) {
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
                step_results.push(serde_json::json!({ "step": idx, "action": "wait_network_idle" }));
            }
            "click" => {
                let selector = step.get("selector").and_then(|v| v.as_str()).unwrap_or("");
                let text = step.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let aria_label = step.get("aria_label").and_then(|v| v.as_str()).unwrap_or("");
                let timeout_ms = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(6000);
                let wait_after_ms = step.get("wait_after_ms").and_then(|v| v.as_u64()).unwrap_or(250);
                let start = Instant::now();
                let mut clicked = false;

                // Resilient semantic click with wait & retry
                while start.elapsed().as_millis() as u64 <= timeout_ms {
                    let click_expr = format!(
                        "(() => {{
                            const sel = '{}';
                            const targetText = '{}'.trim().toLowerCase();
                            const aria = '{}'.trim().toLowerCase();
                            
                            let el = null;
                            if (aria) {{
                                el = document.querySelector(`[aria-label=\"${{aria}}\" i]`);
                                if (!el) {{
                                    const svg = document.querySelector(`svg[aria-label=\"${{aria}}\" i]`);
                                    if (svg) {{
                                        el = svg.closest('button, [role=\"button\"], a, div') || svg;
                                    }}
                                }}
                            }}
                            if (!el && sel && targetText) {{
                                const list = Array.from(document.querySelectorAll(sel));
                                el = list.find(e => (e.innerText || '').trim().toLowerCase() === targetText)
                                  || list.find(e => (e.innerText || '').toLowerCase().includes(targetText));
                            }}
                            if (!el && sel) {{
                                el = document.querySelector(sel);
                            }}
                            if (!el && targetText) {{
                                const candidates = Array.from(document.querySelectorAll('button, [role=\"button\"], a, input[type=\"button\"], input[type=\"submit\"], span, div'));
                                el = candidates.find(e => (e.innerText || '').trim().toLowerCase() === targetText)
                                  || candidates.find(e => (e.innerText || '').toLowerCase().includes(targetText));
                                if (el) {{
                                    const clickable = el.closest('button, [role=\"button\"], a');
                                    if (clickable) el = clickable;
                                }}
                            }}
                            if (!el) return false;
                            el.scrollIntoView({{ block: 'center', inline: 'center' }});
                            el.click();
                            return true;
                        }})()",
                        selector.replace('\'', "\\'"),
                        text.replace('\'', "\\'"),
                        aria_label.replace('\'', "\\'")
                    );
                    if let Ok(val) = send_cdp_command(state.clone(), current_tab, "Runtime.evaluate", serde_json::json!({
                        "expression": click_expr,
                        "returnByValue": true
                    })).await {
                        if val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false) {
                            clicked = true;
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
                if !clicked && !step.get("optional").and_then(|v| v.as_bool()).unwrap_or(false) {
                    return Err(format!("Failed to click element (selector: '{}', text: '{}', aria_label: '{}')", selector, text, aria_label));
                }
                if wait_after_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(wait_after_ms)).await;
                }
                step_results.push(serde_json::json!({ "step": idx, "action": "click", "clicked": clicked }));
            }
            "scroll" => {
                let delta_y = step.get("delta_y").and_then(|v| v.as_i64()).unwrap_or_else(|| {
                    let dir = step.get("direction").and_then(|v| v.as_str()).unwrap_or("down");
                    if dir == "up" { -600 } else { 600 }
                });
                let delta_x = step.get("delta_x").and_then(|v| v.as_i64()).unwrap_or(0);
                let selector = step.get("selector").and_then(|v| v.as_str()).unwrap_or("");
                let wait_after_ms = step.get("wait_after_ms").and_then(|v| v.as_u64()).unwrap_or(250);

                let scroll_expr = format!(
                    "(() => {{
                        let target = null;
                        const sel = '{}';
                        if (sel) {{
                            target = document.querySelector(sel);
                        }} else {{
                            const containers = Array.from(document.querySelectorAll('*')).filter(el => {{
                                const style = window.getComputedStyle(el);
                                const overflow = style.overflowY || style.overflow;
                                return (overflow === 'auto' || overflow === 'scroll') && el.scrollHeight > el.clientHeight;
                            }});
                            if (containers.length > 0) {{
                                target = containers.find(c => {{
                                    const r = c.getBoundingClientRect();
                                    return r.width > 0 && r.height > 0 && r.top < window.innerHeight && r.bottom > 0;
                                }}) || containers[0];
                            }}
                        }}
                        if (target) {{
                            target.scrollBy({{ left: {}, top: {}, behavior: 'smooth' }});
                            return {{ scrolled: true, mode: 'container', scrollTop: target.scrollTop }};
                        }} else {{
                            window.scrollBy({{ left: {}, top: {}, behavior: 'smooth' }});
                            return {{ scrolled: true, mode: 'window', scrollY: window.scrollY }};
                        }}
                    }})()",
                    selector.replace('\'', "\\'"),
                    delta_x, delta_y,
                    delta_x, delta_y
                );

                let res = send_cdp_command(state.clone(), current_tab, "Runtime.evaluate", serde_json::json!({
                    "expression": scroll_expr,
                    "returnByValue": true
                })).await;
                if wait_after_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(wait_after_ms)).await;
                }
                let details = res.ok().and_then(|v| v.get("result").and_then(|r| r.get("value")).cloned());
                step_results.push(serde_json::json!({
                    "step": idx,
                    "action": "scroll",
                    "delta_y": delta_y,
                    "status": "ok",
                    "details": details
                }));
            }
            "evaluate" => {
                let expr = step.get("expression").and_then(|v| v.as_str()).unwrap_or("");
                let res = send_cdp_command(state.clone(), current_tab, "Runtime.evaluate", serde_json::json!({
                    "expression": expr,
                    "returnByValue": true,
                    "awaitPromise": true
                })).await?;
                let mut val = res.get("result").and_then(|r| r.get("value")).cloned().unwrap_or(Value::Null);
                crate::tools::debugger::optimize_evaluate_value(&mut val, false);
                step_results.push(serde_json::json!({
                    "step": idx,
                    "action": "evaluate",
                    "result": val
                }));
            }
            "press_key" => {
                let key = step.get("key").and_then(|v| v.as_str()).unwrap_or("Enter");
                let code = key;
                let windows_virtual_key_code = match key {
                    "Enter" => 13,
                    "Tab" => 9,
                    "Escape" => 27,
                    "Backspace" => 8,
                    "ArrowUp" => 38,
                    "ArrowDown" => 40,
                    _ => 0,
                };
                send_cdp_command(state.clone(), current_tab, "Input.dispatchKeyEvent", serde_json::json!({
                    "type": "rawKeyDown",
                    "key": key,
                    "code": code,
                    "windowsVirtualKeyCode": windows_virtual_key_code
                })).await?;
                send_cdp_command(state.clone(), current_tab, "Input.dispatchKeyEvent", serde_json::json!({
                    "type": "keyUp",
                    "key": key,
                    "code": code,
                    "windowsVirtualKeyCode": windows_virtual_key_code
                })).await?;
                step_results.push(serde_json::json!({ "step": idx, "action": "press_key", "key": key }));
            }
            "sleep" => {
                let duration_ms = step.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(500);
                tokio::time::sleep(Duration::from_millis(duration_ms)).await;
                step_results.push(serde_json::json!({ "step": idx, "action": "sleep", "duration_ms": duration_ms }));
            }
            "fill_inputs" => {
                let fields = step.get("fields").cloned().unwrap_or(serde_json::json!([]));
                crate::tools::input::fill_inputs(state.clone(), current_tab, serde_json::json!({ "fields": fields })).await?;
                step_results.push(serde_json::json!({ "step": idx, "action": "fill_inputs", "status": "ok" }));
            }
            "take_screenshot" => {
                let full_page = step.get("full_page").and_then(|v| v.as_bool()).unwrap_or(false);
                let format = step.get("format").and_then(|v| v.as_str()).unwrap_or("png");
                let quality = step.get("quality").and_then(|v| v.as_u64());
                let save_path = step.get("save_path").and_then(|v| v.as_str());
                let mut params = serde_json::json!({
                    "full_page": full_page,
                    "format": format,
                    "include_base64": true
                });
                if let Some(q) = quality {
                    params["quality"] = serde_json::json!(q);
                }
                if let Some(sp) = save_path {
                    params["save_path"] = serde_json::json!(sp);
                }
                if let Ok(res) = crate::tools::visual::take_screenshot(state.clone(), current_tab, params).await {
                    if let Some(b64) = res.get("base64_image").and_then(|v| v.as_str()) {
                        screenshot = Some(b64.to_string());
                    }
                    step_results.push(serde_json::json!({ "step": idx, "action": "take_screenshot", "result": res }));
                }
            }
            "capture_grid_screenshot" => {
                let quality = step.get("quality").and_then(|v| v.as_u64()).unwrap_or(80) as u32;
                if let Ok(res) = crate::tools::visual::take_screenshot(state.clone(), current_tab, serde_json::json!({
                    "full_page": false,
                    "format": "webp",
                    "quality": quality,
                    "include_base64": true
                })).await {
                    screenshot = res.get("base64_image").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
                step_results.push(serde_json::json!({ "step": idx, "action": "capture_grid_screenshot" }));
            }
            _ => {}
        }
    }
    
    let res = send_cdp_command(state.clone(), current_tab, "Runtime.evaluate", serde_json::json!({
        "expression": "window.location.href",
        "returnByValue": true
    })).await;
    if let Ok(val) = res {
        final_url = val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    }
    
    let mut result = serde_json::json!({
        "success": true,
        "final_url": final_url,
        "steps_executed": step_results
    });
    if let Some(s) = screenshot {
        result.as_object_mut().unwrap().insert("screenshot".to_string(), serde_json::json!(s));
    }
    Ok(result)
}
