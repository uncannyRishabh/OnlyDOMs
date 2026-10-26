use crate::websocket_hub::{SharedHub, send_cdp_command};
use crate::tools::visual::take_screenshot;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Turn an arbitrary string into a filesystem-safe slug.
pub fn slugify(input: &str) -> String {
    let mut result = String::new();
    let mut last_was_dash = false;
    for c in input.chars() {
        if c == '\'' {
            continue;
        }
        if c.is_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash && !result.is_empty() {
            result.push('_');
            last_was_dash = true;
        }
    }
    while result.ends_with('_') {
        result.pop();
    }
    if result.is_empty() {
        "step".to_string()
    } else {
        result
    }
}

/// Helper to extract URL from text.
fn extract_url(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c| c == '(' || c == ')' || c == '"' || c == '\'' || c == '<' || c == '>' || c == ',' || c == ';');
        if clean.starts_with("http://") || clean.starts_with("https://") {
            return Some(clean.to_string());
        } else if clean.starts_with("localhost:") || clean.starts_with("127.0.0.1:") {
            return Some(format!("http://{}", clean));
        } else if (clean.ends_with(".com") || clean.ends_with(".org") || clean.ends_with(".net") || clean.ends_with(".io") || clean.ends_with(".dev") || clean.ends_with(".app")) && clean.contains('.') && !clean.contains('@') {
            return Some(format!("https://{}", clean));
        }
    }
    None
}

/// Strip step prefixes like "1.", "1)", "Step 1:", "-", "*", ">"
fn strip_step_prefix(line: &str) -> String {
    let mut s = line.trim();
    if s.starts_with('-') || s.starts_with('*') || s.starts_with('>') {
        s = s[1..].trim();
    }
    if let Some(colon_pos) = s.to_lowercase().find("step ") {
        if colon_pos == 0 {
            if let Some(idx) = s.find(':') {
                s = s[idx + 1..].trim();
            }
        }
    }
    // Strip leading number dots like "1." or "1)"
    let mut chars = s.chars().peekable();
    if chars.peek().map_or(false, |c| c.is_ascii_digit()) {
        while chars.peek().map_or(false, |c| c.is_ascii_digit()) {
            chars.next();
        }
        if chars.peek().map_or(false, |c| *c == '.' || *c == ')' || *c == ':') {
            chars.next();
            return chars.collect::<String>().trim().to_string();
        }
    }
    s.to_string()
}

/// Split input description into individual action clauses.
/// Handles multi-line input, semicolons, and case-insensitive conjunctions ("then", "and then", ", and").
fn split_into_clauses(input: &str) -> Vec<String> {
    let mut raw_lines = Vec::new();
    for line in input.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            raw_lines.push(trimmed.to_string());
        }
    }

    let mut result = Vec::new();
    for line in raw_lines {
        // First split by semicolon
        let semi_splits: Vec<&str> = line.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        for chunk in semi_splits {
            let mut remaining = chunk.to_string();
            let mut clauses_in_chunk = Vec::new();

            loop {
                let lower = remaining.to_lowercase();
                let split_indices = [
                    (lower.find(" then "), 6),
                    (lower.find(", then "), 7),
                    (lower.find(", and "), 6),
                    (lower.find(" and then "), 10),
                ];
                let earliest = split_indices.into_iter()
                    .filter_map(|(idx_opt, len)| idx_opt.map(|idx| (idx, len)))
                    .min_by_key(|(idx, _)| *idx);

                if let Some((idx, sep_len)) = earliest {
                    let first_part = remaining[..idx].trim();
                    if !first_part.is_empty() {
                        clauses_in_chunk.push(first_part.to_string());
                    }
                    remaining = remaining[idx + sep_len..].trim().to_string();
                } else {
                    if !remaining.trim().is_empty() {
                        clauses_in_chunk.push(remaining.trim().to_string());
                    }
                    break;
                }
            }

            result.extend(clauses_in_chunk);
        }
    }
    result
}

/// Decompose a single line into a structured step.
fn parse_single_step(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_lowercase();

    // 1. Navigate / Open / Visit / Go to
    if lower.starts_with("navigate") || lower.starts_with("go to") || lower.starts_with("open") || lower.starts_with("visit") {
        if let Some(url) = extract_url(trimmed) {
            return Some(serde_json::json!({
                "name": format!("Navigate to {}", url),
                "action": "navigate",
                "url": url,
                "screenshot": true
            }));
        }
    }

    // 2. Fill inputs: "fill X with Y", "fill X as Y", "type Y into X", "type Y in X", "enter Y into X", "enter Y in X"
    if lower.starts_with("fill ") || lower.starts_with("type ") || lower.starts_with("input ") || lower.starts_with("enter ") {
        // e.g. "fill #username with admin" or "fill #username as admin"
        let with_split = lower.find(" with ").map(|p| (p, 6))
            .or_else(|| lower.find(" as ").map(|p| (p, 4)));

        if let Some((sep_pos, sep_len)) = with_split {
            let prefix_len = if lower.starts_with("fill ") { 5 } else if lower.starts_with("type ") { 5 } else if lower.starts_with("enter ") { 6 } else { 6 };
            let sel = trimmed[prefix_len..sep_pos].trim().trim_matches('\'').trim_matches('"');
            let val = trimmed[sep_pos + sep_len..].trim().trim_matches('\'').trim_matches('"');
            return Some(serde_json::json!({
                "name": format!("Fill {} with '{}'", sel, val),
                "action": "fill_inputs",
                "fields": [{ "selector": sel, "value": val }],
                "screenshot": true
            }));
        }

        // e.g. "type admin into #username" or "type admin in #username"
        let into_split = lower.find(" into ").map(|p| (p, 6))
            .or_else(|| lower.find(" in ").map(|p| (p, 4)));

        if let Some((sep_pos, sep_len)) = into_split {
            let prefix_len = if lower.starts_with("type ") { 5 } else if lower.starts_with("enter ") { 6 } else { 5 };
            let val = trimmed[prefix_len..sep_pos].trim().trim_matches('\'').trim_matches('"');
            let sel = trimmed[sep_pos + sep_len..].trim().trim_matches('\'').trim_matches('"');
            return Some(serde_json::json!({
                "name": format!("Fill {} with '{}'", sel, val),
                "action": "fill_inputs",
                "fields": [{ "selector": sel, "value": val }],
                "screenshot": true
            }));
        }
    }

    // 3. Click / Tap / Press button
    if lower.starts_with("click ") || lower.starts_with("tap ") || lower.starts_with("press button ") {
        let prefix_len = if lower.starts_with("click ") { 6 } else if lower.starts_with("tap ") { 4 } else { 13 };
        let mut target = trimmed[prefix_len..].trim();
        
        // Strip common natural prefixes like "on the ", "on ", "the "
        let target_lower = target.to_lowercase();
        if target_lower.starts_with("on the ") {
            target = target[7..].trim();
        } else if target_lower.starts_with("on ") {
            target = target[3..].trim();
        } else if target_lower.starts_with("the ") {
            target = target[4..].trim();
        }

        let mut unquoted = target.trim_matches('\'').trim_matches('"').trim();
        
        // If not in quotes, strip trailing "button", "link", "tab"
        let u_lower = unquoted.to_lowercase();
        if !target.starts_with('\'') && !target.starts_with('"') {
            if u_lower.ends_with(" button") {
                unquoted = unquoted[..unquoted.len() - 7].trim();
            } else if u_lower.ends_with(" link") {
                unquoted = unquoted[..unquoted.len() - 5].trim();
            }
        }

        if unquoted.starts_with('#') || unquoted.starts_with('.') || unquoted.starts_with('[') || unquoted.contains("button[") || unquoted.contains("input[") {
            return Some(serde_json::json!({
                "name": format!("Click selector {}", unquoted),
                "action": "click",
                "selector": unquoted,
                "screenshot": true
            }));
        } else {
            return Some(serde_json::json!({
                "name": format!("Click text '{}'", unquoted),
                "action": "click",
                "text": unquoted,
                "screenshot": true
            }));
        }
    }

    // 4. Assert / Verify / Check / Expect
    if lower.starts_with("assert ") || lower.starts_with("verify ") || lower.starts_with("check ") || lower.starts_with("expect ") {
        let prefix_len = if lower.starts_with("assert ") { 7 } else if lower.starts_with("verify ") { 7 } else if lower.starts_with("expect ") { 7 } else { 6 };
        let mut assertion_body = trimmed[prefix_len..].trim();
        if assertion_body.to_lowercase().starts_with("that ") {
            assertion_body = assertion_body[5..].trim();
        }
        let assert_lower = assertion_body.to_lowercase();

        // Negative selector assertion: "is not visible", "does not exist", "is hidden"
        if assert_lower.contains("is not visible") || assert_lower.contains("does not exist") || assert_lower.contains("is hidden") || assert_lower.contains("not present") {
            let sel = assertion_body
                .replace("is not visible", "")
                .replace("does not exist", "")
                .replace("is hidden", "")
                .replace("not present", "");
            let clean_sel = sel.trim().trim_matches('\'').trim_matches('"');
            return Some(serde_json::json!({
                "name": format!("Assert {} is NOT visible", clean_sel),
                "action": "assert",
                "selector": clean_sel,
                "negative": true,
                "screenshot": true
            }));
        }

        // Positive selector assertion: "is visible" or "exists"
        if assert_lower.contains("is visible") || assert_lower.contains("exists") {
            let sel = assertion_body.replace("is visible", "").replace("exists", "");
            let clean_sel = sel.trim().trim_matches('\'').trim_matches('"');
            return Some(serde_json::json!({
                "name": format!("Assert {} is visible", clean_sel),
                "action": "assert",
                "selector": clean_sel,
                "negative": false,
                "screenshot": true
            }));
        }

        // Negative text assertion: "does not contain", "doesn't contain"
        if assert_lower.contains("does not contain ") || assert_lower.contains("doesn't contain ") {
            let pos = assert_lower.find("contain ").unwrap_or(0);
            let target_text = assertion_body[pos + 8..].trim().trim_matches('\'').trim_matches('"');
            return Some(serde_json::json!({
                "name": format!("Assert page does not contain '{}'", target_text),
                "action": "assert",
                "text": target_text,
                "negative": true,
                "screenshot": true
            }));
        }

        // Positive text assertion: "contains "
        if assert_lower.contains("contains ") {
            let pos = assert_lower.find("contains ").unwrap_or(0);
            let target_text = assertion_body[pos + 9..].trim().trim_matches('\'').trim_matches('"');
            return Some(serde_json::json!({
                "name": format!("Assert page contains '{}'", target_text),
                "action": "assert",
                "text": target_text,
                "negative": false,
                "screenshot": true
            }));
        }

        // Explicit JS condition
        if assertion_body.contains("document.") || assertion_body.contains("window.") || assertion_body.contains("===") || assertion_body.contains("!==") {
            return Some(serde_json::json!({
                "name": format!("Assert JS condition: {}", assertion_body),
                "action": "assert",
                "expression": assertion_body,
                "screenshot": true
            }));
        }

        // Direct selector (starts with #, ., [)
        if assertion_body.starts_with('#') || assertion_body.starts_with('.') || assertion_body.starts_with('[') {
            let clean_sel = assertion_body.trim_matches('\'').trim_matches('"');
            return Some(serde_json::json!({
                "name": format!("Assert {} is visible", clean_sel),
                "action": "assert",
                "selector": clean_sel,
                "screenshot": true
            }));
        }

        // Quoted text or general text target
        let clean_text = assertion_body.trim_matches('\'').trim_matches('"');
        return Some(serde_json::json!({
            "name": format!("Assert page contains '{}'", clean_text),
            "action": "assert",
            "text": clean_text,
            "negative": false,
            "screenshot": true
        }));
    }

    // 5. Wait for / Await
    if lower.starts_with("wait for ") || lower.starts_with("await ") {
        let prefix_len = if lower.starts_with("wait for ") { 9 } else { 6 };
        let target = trimmed[prefix_len..].trim();
        if target.to_lowercase().contains("network") {
            return Some(serde_json::json!({
                "name": "Wait for network idle",
                "action": "wait_network_idle",
                "screenshot": true
            }));
        } else {
            return Some(serde_json::json!({
                "name": format!("Wait for selector {}", target),
                "action": "wait_for",
                "selector": target,
                "screenshot": true
            }));
        }
    }

    // 6. Scroll
    if lower.starts_with("scroll") {
        let dir = if lower.contains("up") { "up" } else { "down" };
        return Some(serde_json::json!({
            "name": format!("Scroll {}", dir),
            "action": "scroll",
            "direction": dir,
            "delta_y": if dir == "up" { -600 } else { 600 },
            "screenshot": true
        }));
    }

    // 7. Press Key
    if lower.starts_with("press key ") || lower.starts_with("press ") || lower.starts_with("hit ") {
        let key = trimmed.split_whitespace().last().unwrap_or("Enter");
        return Some(serde_json::json!({
            "name": format!("Press {}", key),
            "action": "press_key",
            "key": key,
            "screenshot": true
        }));
    }

    // 8. Sleep / Delay
    if lower.starts_with("sleep ") || lower.starts_with("delay ") {
        let num_str = trimmed.split_whitespace().nth(1).unwrap_or("1000");
        let ms = if num_str.ends_with("ms") {
            num_str.trim_end_matches("ms").parse::<u64>().unwrap_or(1000)
        } else if num_str.ends_with('s') {
            (num_str.trim_end_matches('s').parse::<f64>().unwrap_or(1.0) * 1000.0) as u64
        } else {
            num_str.parse::<u64>().unwrap_or(1000)
        };
        return Some(serde_json::json!({
            "name": format!("Sleep {}ms", ms),
            "action": "sleep",
            "duration_ms": ms,
            "screenshot": false
        }));
    }

    // If line is just a URL
    if let Some(url) = extract_url(trimmed) {
        return Some(serde_json::json!({
            "name": format!("Navigate to {}", url),
            "action": "navigate",
            "url": url,
            "screenshot": true
        }));
    }

    None
}

/// Decompose a high-level test or defect description into small executable steps.
pub fn parse_description_into_steps(description: &str) -> Vec<Value> {
    let mut steps = Vec::new();
    let candidates = split_into_clauses(description);

    for candidate in candidates {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let clean = strip_step_prefix(trimmed);
        if clean.is_empty() {
            continue;
        }
        if let Some(step) = parse_single_step(&clean) {
            steps.push(step);
        }
    }

    // Fallback: If no steps were parsed but a URL is present
    if steps.is_empty() {
        if let Some(url) = extract_url(description) {
            steps.push(serde_json::json!({
                "name": format!("Navigate to {}", url),
                "action": "navigate",
                "url": url,
                "screenshot": true
            }));
            steps.push(serde_json::json!({
                "name": "Wait for page ready",
                "action": "wait_condition",
                "expression": "document.readyState === 'complete'",
                "screenshot": true
            }));
            steps.push(serde_json::json!({
                "name": "Verify page content loaded",
                "action": "assert",
                "expression": "document.body && document.body.children.length > 0",
                "screenshot": true
            }));
        }
    }

    steps
}

/// Step execution record.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StepExecutionRecord {
    pub step_index: usize,
    pub name: String,
    pub action: String,
    pub status: String, // "passed", "failed", "skipped"
    pub duration_ms: u64,
    pub error: Option<String>,
    pub screenshot_file: Option<String>,
}

/// Execute a single browser step.
async fn execute_step(state: SharedHub, step: &Value) -> Result<(), String> {
    let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("");
    match action {
        "navigate" => {
            let url = step.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() {
                return Err("Navigate step missing 'url'".to_string());
            }
            let nav_res = send_cdp_command(state.clone(), "Page.navigate", serde_json::json!({ "url": url })).await?;
            if let Some(err_text) = nav_res.get("errorText").and_then(|v| v.as_str()) {
                return Err(format!("Navigation to '{}' failed: {}", url, err_text));
            }
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            
            // Wait for document ready with timeout protection
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                    "expression": "new Promise(resolve => { if (document.readyState === 'complete') resolve(true); else { window.addEventListener('load', () => resolve(true), {once: true}); setTimeout(() => resolve(true), 3000); } })",
                    "awaitPromise": true
                }))
            ).await;
            Ok(())
        }
        "click" => {
            let selector = step.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let text = step.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let aria_label = step.get("aria_label").and_then(|v| v.as_str()).unwrap_or("");
            let timeout_ms = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(6000);
            let wait_after_ms = step.get("wait_after_ms").and_then(|v| v.as_u64()).unwrap_or(250);

            let sel_json = serde_json::to_string(selector).unwrap_or("\"\"".to_string());
            let text_json = serde_json::to_string(text).unwrap_or("\"\"".to_string());
            let aria_json = serde_json::to_string(aria_label).unwrap_or("\"\"".to_string());

            let click_expr = format!(
                "(() => {{
                    const sel = {sel};
                    const targetText = {target_text}.trim().toLowerCase();
                    const aria = {aria}.trim().toLowerCase();
                    let el = null;
                    if (aria) {{
                        el = document.querySelector(`[aria-label=\"${{aria}}\" i]`);
                        if (!el) {{
                            const svg = document.querySelector(`svg[aria-label=\"${{aria}}\" i]`);
                            if (svg) el = svg.closest('button, [role=\"button\"], a, div') || svg;
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
                sel = sel_json,
                target_text = text_json,
                aria = aria_json
            );

            let start = Instant::now();
            let mut clicked = false;
            while start.elapsed().as_millis() as u64 <= timeout_ms {
                if let Ok(val) = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                    "expression": click_expr,
                    "returnByValue": true
                })).await {
                    if val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false) {
                        clicked = true;
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }

            if !clicked {
                return Err(format!("Click failed: element not found (selector: '{}', text: '{}', aria_label: '{}')", selector, text, aria_label));
            }
            if wait_after_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(wait_after_ms)).await;
            }
            Ok(())
        }
        "fill_inputs" => {
            let fields = step.get("fields").cloned().unwrap_or(serde_json::json!([]));
            let res = crate::tools::input::fill_inputs(state.clone(), serde_json::json!({ "fields": fields })).await?;
            // Inspect fill results to verify all fields were located and populated
            if let Some(val_str) = res.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_str()) {
                if let Ok(items) = serde_json::from_str::<Vec<Value>>(val_str) {
                    for item in items {
                        if item.get("success").and_then(|s| s.as_bool()) == Some(false) {
                            let err = item.get("error").and_then(|e| e.as_str()).unwrap_or("Failed to fill input field");
                            return Err(err.to_string());
                        }
                    }
                }
            }
            Ok(())
        }
        "press_key" => {
            let key = step.get("key").and_then(|v| v.as_str()).unwrap_or("Enter");
            send_cdp_command(state.clone(), "Input.dispatchKeyEvent", serde_json::json!({
                "type": "rawKeyDown",
                "key": key
            })).await?;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            send_cdp_command(state.clone(), "Input.dispatchKeyEvent", serde_json::json!({
                "type": "keyUp",
                "key": key
            })).await?;
            Ok(())
        }
        "scroll" => {
            let delta_y = step.get("delta_y").and_then(|v| v.as_i64()).unwrap_or_else(|| {
                let dir = step.get("direction").and_then(|v| v.as_str()).unwrap_or("down");
                if dir == "up" { -600 } else { 600 }
            });
            let delta_x = step.get("delta_x").and_then(|v| v.as_i64()).unwrap_or(0);
            let selector = step.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let wait_after_ms = step.get("wait_after_ms").and_then(|v| v.as_u64()).unwrap_or(250);

            let sel_json = serde_json::to_string(selector).unwrap_or("\"\"".to_string());
            let scroll_expr = format!(
                "(() => {{
                    let target = null;
                    const sel = {sel};
                    if (sel) {{
                        target = document.querySelector(sel);
                    }}
                    if (target) {{
                        target.scrollBy({{ left: {dx}, top: {dy}, behavior: 'smooth' }});
                    }} else {{
                        window.scrollBy({{ left: {dx}, top: {dy}, behavior: 'smooth' }});
                    }}
                    return true;
                }})()",
                sel = sel_json, dx = delta_x, dy = delta_y
            );
            send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                "expression": scroll_expr,
                "returnByValue": true
            })).await?;
            if wait_after_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(wait_after_ms)).await;
            }
            Ok(())
        }
        "wait_for" => {
            let selector = step.get("selector").and_then(|v| v.as_str()).unwrap_or("");
            let timeout_ms = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(8000);
            let sel_json = serde_json::to_string(selector).unwrap_or("\"\"".to_string());
            let expr = format!(
                "(() => {{ const el = document.querySelector({sel}); return !!(el && (el.offsetWidth || el.offsetHeight || el.getClientRects().length)); }})()",
                sel = sel_json
            );
            let start = Instant::now();
            while start.elapsed().as_millis() as u64 <= timeout_ms {
                if let Ok(val) = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                    "expression": expr,
                    "returnByValue": true
                })).await {
                    if val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false) {
                        return Ok(());
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
            Err(format!("Timeout waiting for selector: '{}'", selector))
        }
        "wait_condition" => {
            let expr = step.get("expression").and_then(|v| v.as_str()).unwrap_or("true");
            let timeout_ms = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(8000);
            let start = Instant::now();
            while start.elapsed().as_millis() as u64 <= timeout_ms {
                if let Ok(val) = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                    "expression": expr,
                    "returnByValue": true,
                    "awaitPromise": true
                })).await {
                    if val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false) {
                        return Ok(());
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }
            Err(format!("Timeout waiting for condition: '{}'", expr))
        }
        "wait_network_idle" => {
            let timeout_ms = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(5000);
            let start = Instant::now();
            while start.elapsed().as_millis() as u64 <= timeout_ms {
                let res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                    "expression": "performance.getEntriesByType('resource').filter(e => !e.responseEnd).length === 0",
                    "returnByValue": true
                })).await;
                if let Ok(val) = res {
                    if val.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false) {
                        break;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }
            Ok(())
        }
        "evaluate" => {
            let expr = step.get("expression").and_then(|v| v.as_str()).unwrap_or("");
            send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                "expression": expr,
                "returnByValue": true,
                "awaitPromise": true
            })).await?;
            Ok(())
        }
        "sleep" => {
            let duration_ms = step.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(500);
            tokio::time::sleep(std::time::Duration::from_millis(duration_ms)).await;
            Ok(())
        }
        "assert" => {
            check_assertion(state, step).await
        }
        _ => Err(format!("Unknown action: '{}'", action)),
    }
}

/// Check assertion parameters on a step with polling resilience and exception diagnostics.
async fn check_assertion(state: SharedHub, step: &Value) -> Result<(), String> {
    let timeout_ms = step.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(3000);
    let negative = step.get("negative").and_then(|v| v.as_bool()).unwrap_or(false);

    // Resolve target expression, selector, or text from either explicit keys or generic "assertion" parameter
    let (expression, selector, text) = if step.get("expression").is_some() || step.get("selector").is_some() || step.get("text").is_some() {
        (
            step.get("expression").and_then(|v| v.as_str()),
            step.get("selector").and_then(|v| v.as_str()),
            step.get("text").and_then(|v| v.as_str()),
        )
    } else if let Some(raw_assert) = step.get("assertion").and_then(|v| v.as_str()) {
        let raw_trimmed = raw_assert.trim();
        if raw_trimmed.starts_with('#') || raw_trimmed.starts_with('.') || raw_trimmed.starts_with('[') {
            (None, Some(raw_trimmed), None)
        } else if raw_trimmed.contains("document.") || raw_trimmed.contains("window.") || raw_trimmed.contains("===") || raw_trimmed.contains("!==") {
            (Some(raw_trimmed), None, None)
        } else {
            let clean_text = raw_trimmed.trim_matches('\'').trim_matches('"');
            (None, None, Some(clean_text))
        }
    } else {
        (None, None, None)
    };

    let start = Instant::now();
    let mut last_error = String::new();

    while start.elapsed().as_millis() as u64 <= timeout_ms {
        let mut pass = false;

        if let Some(expr) = expression {
            let res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                "expression": expr,
                "returnByValue": true,
                "awaitPromise": true
            })).await;

            match res {
                Ok(val) => {
                    if let Some(exc) = val.get("exceptionDetails").and_then(|e| e.get("text")).and_then(|t| t.as_str()) {
                        last_error = format!("Assertion JS threw exception: {}", exc);
                    } else {
                        let eval_res = val.get("result").and_then(|r| r.get("value"));
                        let is_truthy = match eval_res {
                            Some(Value::Bool(b)) => *b,
                            Some(Value::Null) => false,
                            None => false,
                            _ => true,
                        };
                        pass = if negative { !is_truthy } else { is_truthy };
                        if !pass {
                            last_error = format!("Assertion condition '{}' evaluated to {:?}", expr, eval_res);
                        }
                    }
                }
                Err(e) => {
                    last_error = format!("Assertion execution error: {}", e);
                }
            }
        } else if let Some(sel) = selector {
            let sel_json = serde_json::to_string(sel).unwrap_or("\"\"".to_string());
            let dom_expr = format!(
                "(() => {{ const el = document.querySelector({sel}); return !!(el && (el.offsetWidth || el.offsetHeight || el.getClientRects().length)); }})()",
                sel = sel_json
            );
            if let Ok(res) = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                "expression": dom_expr,
                "returnByValue": true
            })).await {
                let is_visible = res.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false);
                pass = if negative { !is_visible } else { is_visible };
                if !pass {
                    last_error = if negative {
                        format!("Assertion failed: element matching '{}' is unexpectedly visible", sel)
                    } else {
                        format!("Assertion failed: element matching '{}' is not visible or not found", sel)
                    };
                }
            }
        } else if let Some(txt) = text {
            let txt_json = serde_json::to_string(txt).unwrap_or("\"\"".to_string());
            let txt_expr = format!(
                "(() => {{ return !!(document.body && (document.body.innerText || '').includes({txt})); }})()",
                txt = txt_json
            );
            if let Ok(res) = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
                "expression": txt_expr,
                "returnByValue": true
            })).await {
                let has_text = res.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_bool()).unwrap_or(false);
                pass = if negative { !has_text } else { has_text };
                if !pass {
                    last_error = if negative {
                        format!("Assertion failed: page unexpectedly contains text '{}'", txt)
                    } else {
                        format!("Assertion failed: page does not contain text '{}'", txt)
                    };
                }
            }
        } else {
            return Ok(());
        }

        if pass {
            return Ok(());
        }

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }

    Err(if last_error.is_empty() { "Assertion timed out".to_string() } else { last_error })
}

/// Generate Markdown summary report.
pub fn generate_markdown_summary(
    test_name: &str,
    description: &str,
    passed: bool,
    total_steps: usize,
    passed_steps: usize,
    failed_steps: usize,
    skipped_steps: usize,
    duration_ms: u64,
    steps: &[StepExecutionRecord],
) -> String {
    let status_badge = if passed { "✅ PASSED" } else { "❌ FAILED" };
    let mut md = String::new();

    md.push_str(&format!("# Browser Test Summary: {}\n\n", test_name));
    md.push_str(&format!("**Verdict**: {}\n\n", status_badge));
    md.push_str(&format!("**Description**: {}\n\n", description));

    md.push_str("## Execution Metrics\n\n");
    md.push_str(&format!("- **Total Steps**: {}\n", total_steps));
    md.push_str(&format!("- **Passed**: {}\n", passed_steps));
    md.push_str(&format!("- **Failed**: {}\n", failed_steps));
    md.push_str(&format!("- **Skipped**: {}\n", skipped_steps));
    md.push_str(&format!("- **Duration**: {:.2}s\n\n", duration_ms as f64 / 1000.0));

    md.push_str("## Step Execution Results\n\n");
    md.push_str("| # | Step Name | Action | Status | Duration | Screenshot |\n");
    md.push_str("|---|-----------|--------|--------|----------|------------|\n");

    for s in steps {
        let status_emoji = match s.status.as_str() {
            "passed" => "✅ Passed",
            "failed" => "❌ Failed",
            _ => "⏭️ Skipped",
        };
        let img_link = if let Some(ref img) = s.screenshot_file {
            format!("[`{}`]({})", img, img)
        } else {
            "-".to_string()
        };
        md.push_str(&format!(
            "| {} | {} | `{}` | {} | {}ms | {} |\n",
            s.step_index, s.name, s.action, status_emoji, s.duration_ms, img_link
        ));
    }

    md.push_str("\n## Visual Evidence & Step Details\n\n");
    for s in steps {
        let status_emoji = match s.status.as_str() {
            "passed" => "✅",
            "failed" => "❌",
            _ => "⏭️",
        };
        md.push_str(&format!(
            "### Step {:02}: {} {} ({}ms)\n\n",
            s.step_index, s.name, status_emoji, s.duration_ms
        ));

        if let Some(ref err) = s.error {
            md.push_str(&format!("> [!CAUTION]\n> **Error**: {}\n\n", err));
        }

        if let Some(ref img) = s.screenshot_file {
            md.push_str(&format!("![Step {:02}]({})\n\n", s.step_index, img));
        }
    }

    md
}

/// Run an automated browser test from description or structured steps.
pub async fn run_browser_test(state: SharedHub, params: Value) -> Result<Value, String> {
    let raw_name = params.get("test_name").and_then(|v| v.as_str()).unwrap_or("");
    let description = params.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let full_page_screenshots = params.get("full_page_screenshots").and_then(|v| v.as_bool()).unwrap_or(false);
    let continue_on_failure = params.get("continue_on_failure").and_then(|v| v.as_bool()).unwrap_or(false);

    let test_name = if !raw_name.trim().is_empty() {
        slugify(raw_name)
    } else if !description.trim().is_empty() {
        let first_line = description.lines().next().unwrap_or("browser_test");
        let truncated: String = first_line.chars().take(40).collect();
        slugify(&truncated)
    } else {
        "browser_test".to_string()
    };

    // Determine output directory
    let epoch_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let default_dir_name = format!("{}_{}", test_name, epoch_secs);

    let output_dir = if let Some(custom_dir) = params.get("output_dir").and_then(|v| v.as_str()) {
        PathBuf::from(custom_dir)
    } else {
        Path::new("test_reports").join(&default_dir_name)
    };

    tokio::fs::create_dir_all(&output_dir).await
        .map_err(|e| format!("Failed to create test directory {}: {}", output_dir.display(), e))?;

    // Determine steps: either from params["steps"] or parse from description
    let steps: Vec<Value> = if let Some(arr) = params.get("steps").and_then(|s| s.as_array()) {
        if !arr.is_empty() {
            arr.clone()
        } else {
            parse_description_into_steps(description)
        }
    } else {
        parse_description_into_steps(description)
    };

    if steps.is_empty() {
        return Err("No steps provided and unable to derive steps from description. Please supply a 'steps' array or a description with actions/URL.".to_string());
    }

    let mut execution_records: Vec<StepExecutionRecord> = Vec::new();
    let mut overall_passed = true;
    let mut passed_steps = 0;
    let mut failed_steps = 0;
    let suite_start = Instant::now();

    for (idx, step) in steps.iter().enumerate() {
        let step_num = idx + 1;
        let step_name = step.get("name").and_then(|v| v.as_str()).unwrap_or_else(|| {
            step.get("action").and_then(|v| v.as_str()).unwrap_or("step")
        });
        let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("unknown");
        let should_screenshot = step.get("screenshot").and_then(|v| v.as_bool()).unwrap_or(true);
        let step_full_page = step.get("full_page").and_then(|v| v.as_bool()).unwrap_or(full_page_screenshots);
        let optional = step.get("optional").and_then(|v| v.as_bool()).unwrap_or(false);

        let step_start = Instant::now();
        let step_res = execute_step(state.clone(), step).await;
        let step_duration = step_start.elapsed().as_millis() as u64;

        let (status, err_msg) = match step_res {
            Ok(_) => {
                passed_steps += 1;
                ("passed", None)
            }
            Err(e) => {
                failed_steps += 1;
                if !optional {
                    overall_passed = false;
                }
                ("failed", Some(e))
            }
        };

        // Capture visual evidence
        let mut screenshot_file = None;
        if should_screenshot || status == "failed" {
            let filename = if status == "passed" {
                format!("step_{:02}_{}.png", step_num, slugify(step_name))
            } else {
                format!("step_{:02}_{}_FAILED.png", step_num, slugify(step_name))
            };
            let full_path = output_dir.join(&filename);
            let screenshot_params = serde_json::json!({
                "full_page": step_full_page,
                "save_path": full_path.to_str().unwrap_or(""),
                "format": "png",
                "include_base64": false
            });

            if let Ok(_) = take_screenshot(state.clone(), screenshot_params).await {
                screenshot_file = Some(filename);
            }
        }

        execution_records.push(StepExecutionRecord {
            step_index: step_num,
            name: step_name.to_string(),
            action: action.to_string(),
            status: status.to_string(),
            duration_ms: step_duration,
            error: err_msg.clone(),
            screenshot_file,
        });

        // Abort on failure unless continue_on_failure is enabled
        if status == "failed" && !optional && !continue_on_failure {
            // Record remaining steps as skipped
            for remaining_idx in (idx + 1)..steps.len() {
                let rem_step = &steps[remaining_idx];
                let rem_name = rem_step.get("name").and_then(|v| v.as_str()).unwrap_or_else(|| {
                    rem_step.get("action").and_then(|v| v.as_str()).unwrap_or("step")
                });
                let rem_action = rem_step.get("action").and_then(|v| v.as_str()).unwrap_or("unknown");
                execution_records.push(StepExecutionRecord {
                    step_index: remaining_idx + 1,
                    name: rem_name.to_string(),
                    action: rem_action.to_string(),
                    status: "skipped".to_string(),
                    duration_ms: 0,
                    error: None,
                    screenshot_file: None,
                });
            }
            break;
        }
    }

    let total_duration_ms = suite_start.elapsed().as_millis() as u64;
    let skipped_steps = execution_records.iter().filter(|r| r.status == "skipped").count();

    // Generate Markdown summary
    let summary_md = generate_markdown_summary(
        &test_name,
        description,
        overall_passed,
        steps.len(),
        passed_steps,
        failed_steps,
        skipped_steps,
        total_duration_ms,
        &execution_records,
    );
    let summary_path = output_dir.join("test_summary.md");
    tokio::fs::write(&summary_path, &summary_md).await
        .map_err(|e| format!("Failed to write test_summary.md: {}", e))?;

    // Generate JSON report
    let report_json = serde_json::json!({
        "test_name": test_name,
        "description": description,
        "passed": overall_passed,
        "total_steps": steps.len(),
        "passed_steps": passed_steps,
        "failed_steps": failed_steps,
        "skipped_steps": skipped_steps,
        "duration_ms": total_duration_ms,
        "output_dir": output_dir.to_str().unwrap_or(""),
        "steps": execution_records
    });
    let report_content = serde_json::to_string_pretty(&report_json).unwrap_or("{}".to_string());
    let report_path = output_dir.join("test_report.json");
    tokio::fs::write(&report_path, &report_content).await
        .map_err(|e| format!("Failed to write test_report.json: {}", e))?;

    let screenshots: Vec<String> = execution_records.iter()
        .filter_map(|r| r.screenshot_file.as_ref().map(|f| output_dir.join(f).to_str().unwrap_or("").to_string()))
        .collect();

    Ok(serde_json::json!({
        "success": true,
        "test_name": test_name,
        "passed": overall_passed,
        "total_steps": steps.len(),
        "passed_steps": passed_steps,
        "failed_steps": failed_steps,
        "skipped_steps": skipped_steps,
        "duration_ms": total_duration_ms,
        "output_dir": output_dir.to_str().unwrap_or(""),
        "summary_markdown_path": summary_path.to_str().unwrap_or(""),
        "report_json_path": report_path.to_str().unwrap_or(""),
        "step_screenshots": screenshots
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Login Test"), "login_test");
        assert_eq!(slugify("Check Defect #123: Can't Login!"), "check_defect_123_cant_login");
        assert_eq!(slugify("   ___Hello   World___  "), "hello_world");
        assert_eq!(slugify(""), "step");
    }

    #[test]
    fn test_parse_numbered_steps() {
        let desc = r#"
1. Navigate to https://example.com/login
2. Fill input[name="username"] with testuser
3. Fill input[name="password"] with secretpass
4. Click button[type="submit"]
5. Assert #dashboard is visible
"#;
        let steps = parse_description_into_steps(desc);
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0]["action"], "navigate");
        assert_eq!(steps[0]["url"], "https://example.com/login");
        assert_eq!(steps[1]["action"], "fill_inputs");
        assert_eq!(steps[2]["action"], "fill_inputs");
        assert_eq!(steps[3]["action"], "click");
        assert_eq!(steps[3]["selector"], "button[type=\"submit\"]");
        assert_eq!(steps[4]["action"], "assert");
        assert_eq!(steps[4]["selector"], "#dashboard");
    }

    #[test]
    fn test_parse_natural_text_with_then_case_insensitive() {
        let desc = "Navigate to https://example.com/cart Then fill #coupon with PROMO50 THEN click #apply and then assert .discount is visible";
        let steps = parse_description_into_steps(desc);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0]["action"], "navigate");
        assert_eq!(steps[1]["action"], "fill_inputs");
        assert_eq!(steps[2]["action"], "click");
        assert_eq!(steps[3]["action"], "assert");
    }

    #[test]
    fn test_parse_multi_line_with_internal_then() {
        let desc = "1. Go to https://example.com then click #login\n2. Fill username with admin then click #submit";
        let steps = parse_description_into_steps(desc);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0]["action"], "navigate");
        assert_eq!(steps[1]["action"], "click");
        assert_eq!(steps[2]["action"], "fill_inputs");
        assert_eq!(steps[2]["fields"][0]["value"], "admin");
        assert_eq!(steps[3]["action"], "click");
    }

    #[test]
    fn test_parse_fill_variations() {
        let d1 = "fill #email as test@example.com";
        let s1 = parse_description_into_steps(d1);
        assert_eq!(s1[0]["fields"][0]["selector"], "#email");
        assert_eq!(s1[0]["fields"][0]["value"], "test@example.com");

        let d2 = "type 'password123' in #pass";
        let s2 = parse_description_into_steps(d2);
        assert_eq!(s2[0]["fields"][0]["selector"], "#pass");
        assert_eq!(s2[0]["fields"][0]["value"], "password123");
    }

    #[test]
    fn test_parse_negative_and_text_assertions() {
        let desc = "Assert #error-banner is not visible then verify page does not contain 'Server Error' then assert page contains 'Success' then verify 'Welcome User'";
        let steps = parse_description_into_steps(desc);
        assert_eq!(steps.len(), 4);

        // 1. Negative selector
        assert_eq!(steps[0]["action"], "assert");
        assert_eq!(steps[0]["selector"], "#error-banner");
        assert_eq!(steps[0]["negative"], true);

        // 2. Negative text
        assert_eq!(steps[1]["action"], "assert");
        assert_eq!(steps[1]["text"], "Server Error");
        assert_eq!(steps[1]["negative"], true);

        // 3. Positive text with 'contains'
        assert_eq!(steps[2]["action"], "assert");
        assert_eq!(steps[2]["text"], "Success");
        assert_eq!(steps[2]["negative"], false);

        // 4. Quoted text assertion
        assert_eq!(steps[3]["action"], "assert");
        assert_eq!(steps[3]["text"], "Welcome User");
        assert_eq!(steps[3]["negative"], false);
    }

    #[test]
    fn test_parse_url_fallback() {
        let desc = "Test the main landing page at https://example.org";
        let steps = parse_description_into_steps(desc);
        assert!(!steps.is_empty());
        assert_eq!(steps[0]["action"], "navigate");
        assert_eq!(steps[0]["url"], "https://example.org");
    }

    #[test]
    fn test_markdown_summary_generation_with_skipped() {
        let records = vec![
            StepExecutionRecord {
                step_index: 1,
                name: "Navigate to site".to_string(),
                action: "navigate".to_string(),
                status: "passed".to_string(),
                duration_ms: 350,
                error: None,
                screenshot_file: Some("step_01_navigate.png".to_string()),
            },
            StepExecutionRecord {
                step_index: 2,
                name: "Click missing button".to_string(),
                action: "click".to_string(),
                status: "failed".to_string(),
                duration_ms: 120,
                error: Some("Element not found".to_string()),
                screenshot_file: Some("step_02_click_FAILED.png".to_string()),
            },
            StepExecutionRecord {
                step_index: 3,
                name: "Verify dashboard".to_string(),
                action: "assert".to_string(),
                status: "skipped".to_string(),
                duration_ms: 0,
                error: None,
                screenshot_file: None,
            },
        ];

        let md = generate_markdown_summary(
            "sample_test",
            "Verify checkout flow",
            false,
            3,
            1,
            1,
            1,
            470,
            &records,
        );

        assert!(md.contains("❌ FAILED"));
        assert!(md.contains("sample_test"));
        assert!(md.contains("Verify checkout flow"));
        assert!(md.contains("- **Skipped**: 1"));
        assert!(md.contains("⏭️ Skipped"));
        assert!(md.contains("step_01_navigate.png"));
        assert!(md.contains("Element not found"));
    }

    #[test]
    fn test_parse_bullet_points() {
        let desc = "- Go to https://news.ycombinator.com\n* Click 'newest'\n- Wait for .athing\n* Assert page contains 'Hacker News'";
        let steps = parse_description_into_steps(desc);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0]["action"], "navigate");
        assert_eq!(steps[1]["action"], "click");
        assert_eq!(steps[2]["action"], "wait_for");
        assert_eq!(steps[3]["action"], "assert");
    }

    #[test]
    fn test_strip_step_prefix() {
        assert_eq!(strip_step_prefix("1. Navigate to https://test.com"), "Navigate to https://test.com");
        assert_eq!(strip_step_prefix("1) Click button"), "Click button");
        assert_eq!(strip_step_prefix("Step 4: Assert title"), "Assert title");
        assert_eq!(strip_step_prefix("- Fill input with val"), "Fill input with val");
        assert_eq!(strip_step_prefix("* Scroll down"), "Scroll down");
    }

    #[test]
    fn test_parse_empty_description() {
        let steps = parse_description_into_steps("");
        assert!(steps.is_empty());
        let steps2 = parse_description_into_steps("   \n\n  ");
        assert!(steps2.is_empty());
    }
}
