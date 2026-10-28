use crate::websocket_hub::{SharedHub, send_cdp_command};
use serde_json::Value;

pub const MAX_CHAIN_TEXT_LEN: usize = 100;

/// Strips inline CSS styling (e.g. style="..." or style='...') from HTML strings in the Rust layer.
pub fn strip_inline_styles(input: &str) -> String {
    // Fast path: if "style" does not occur anywhere, return original string without allocation
    let has_style = input.as_bytes().windows(5).any(|w| w.eq_ignore_ascii_case(b"style"));
    if !has_style {
        return input.to_string();
    }

    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut output = Vec::with_capacity(len);
    let mut i = 0;

    while i < len {
        // Check for "style" (case-insensitive) preceded by whitespace, '<', or '/'
        if i + 5 <= len && bytes[i..i + 5].eq_ignore_ascii_case(b"style") {
            let is_attr_start = if i == 0 {
                false
            } else {
                let prev = bytes[i - 1];
                prev.is_ascii_whitespace() || prev == b'<' || prev == b'/'
            };

            if is_attr_start {
                let mut cursor = i + 5;
                // Skip whitespace before '='
                while cursor < len && bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                if cursor < len && bytes[cursor] == b'=' {
                    cursor += 1;
                    // Skip whitespace after '='
                    while cursor < len && bytes[cursor].is_ascii_whitespace() {
                        cursor += 1;
                    }
                    if cursor < len && (bytes[cursor] == b'"' || bytes[cursor] == b'\'') {
                        let quote = bytes[cursor];
                        cursor += 1;
                        // Skip until matching quote, skipping escaped quotes
                        while cursor < len {
                            if bytes[cursor] == quote && (cursor == 0 || bytes[cursor - 1] != b'\\') {
                                cursor += 1;
                                break;
                            }
                            cursor += 1;
                        }

                        // Remove redundant preceding whitespace if followed by '>' or another attr
                        if !output.is_empty() && output.last().copied() == Some(b' ') {
                            if cursor < len && (bytes[cursor] == b'>' || bytes[cursor] == b'/' || bytes[cursor].is_ascii_whitespace()) {
                                output.pop();
                            }
                        }

                        i = cursor;
                        continue;
                    }
                }
            }
        }

        output.push(bytes[i]);
        i += 1;
    }

    String::from_utf8(output).unwrap_or_else(|_| input.to_string())
}

fn is_text_content_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("textContent")
        || key.eq_ignore_ascii_case("text")
        || key.eq_ignore_ascii_case("innerText")
}

/// Recursively optimizes evaluate responses:
/// 1. Drops any attribute with null value.
/// 2. Strips inline CSS styling from HTML strings.
/// 3. Truncates textContent/innerText only when inside a "chain" object if it exceeds 100 chars,
///    preserving full textContent when outside "chain".
pub fn optimize_evaluate_value(val: &mut Value, in_chain: bool) {
    match val {
        Value::Object(map) => {
            // 1. Remove attributes where value is null
            map.retain(|_, v| !v.is_null());

            // 2. Recursively optimize remaining attributes
            for (key, child) in map.iter_mut() {
                let is_chain_key = key.eq_ignore_ascii_case("chain");
                let child_in_chain = in_chain || is_chain_key;

                // Truncate textContent inside chain objects if large paragraph (>100 chars)
                if in_chain && is_text_content_key(key) {
                    if let Some(s) = child.as_str() {
                        if s.chars().count() > MAX_CHAIN_TEXT_LEN {
                            let truncated: String = s.chars().take(MAX_CHAIN_TEXT_LEN).collect();
                            *child = Value::String(format!("{}...", truncated));
                        }
                    }
                }

                optimize_evaluate_value(child, child_in_chain);
            }
        }
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                optimize_evaluate_value(child, in_chain);
            }
        }
        Value::String(s) => {
            // If string is stringified JSON, parse and optimize structure too
            if (s.starts_with('{') && s.ends_with('}')) || (s.starts_with('[') && s.ends_with(']')) {
                if let Ok(mut parsed) = serde_json::from_str::<Value>(s) {
                    optimize_evaluate_value(&mut parsed, in_chain);
                    *s = serde_json::to_string(&parsed).unwrap_or_else(|_| s.clone());
                    return;
                }
            }
            // Strip inline CSS styling from strings
            *s = strip_inline_styles(s);
        }
        _ => {}
    }
}

pub async fn evaluate_expression(state: SharedHub, tab_id: Option<u64>, params: Value) -> Result<Value, String> {
    let expression = params.get("expression").and_then(|v| v.as_str()).unwrap_or("");
    let return_by_value = params.get("return_by_value").and_then(|v| v.as_bool()).unwrap_or(true);
    let await_promise = params.get("await_promise").and_then(|v| v.as_bool()).unwrap_or(true);

    let mut res = send_cdp_command(state.clone(), tab_id, "Runtime.evaluate", serde_json::json!({
        "expression": expression,
        "returnByValue": return_by_value,
        "awaitPromise": await_promise
    })).await?;

    // Optimize evaluate response: strip null attributes, remove inline CSS styling,
    // and truncate textContent in chain objects
    optimize_evaluate_value(&mut res, false);

    // Safety guardrail: truncate massive string outputs (>8,000 chars) to prevent prompt token blowups
    if let Some(val_str) = res.get("result").and_then(|r| r.get("value")).and_then(|v| v.as_str()) {
        if val_str.len() > 8000 {
            let truncated = format!("{}... [TRUNCATED: Output exceeded 8,000 characters to prevent prompt bloat]", &val_str[..8000]);
            return Ok(serde_json::json!({
                "result": {
                    "type": "string",
                    "value": truncated,
                    "truncated": true,
                    "original_length": val_str.len()
                }
            }));
        }
    }

    Ok(res)
}

pub async fn add_breakpoint(state: SharedHub, tab_id: Option<u64>, params: Value) -> Result<Value, String> {
    let url_regex = params.get("url_regex").and_then(|v| v.as_str()).unwrap_or(".*");
    let line_number = params.get("line_number").and_then(|v| v.as_u64()).unwrap_or(0);
    
    let mut payload = serde_json::json!({
        "urlRegex": url_regex,
        "lineNumber": line_number
    });

    if let Some(condition) = params.get("condition").and_then(|v| v.as_str()) {
        payload.as_object_mut().unwrap().insert("condition".to_string(), Value::String(condition.to_string()));
    }

    send_cdp_command(state.clone(), tab_id, "Debugger.setBreakpointByUrl", payload).await
}

pub async fn pause_execution(state: SharedHub, tab_id: Option<u64>) -> Result<Value, String> {
    send_cdp_command(state.clone(), tab_id, "Debugger.pause", serde_json::json!({})).await
}

pub async fn resume_execution(state: SharedHub, tab_id: Option<u64>) -> Result<Value, String> {
    send_cdp_command(state.clone(), tab_id, "Debugger.resume", serde_json::json!({})).await
}

pub async fn step_over(state: SharedHub, tab_id: Option<u64>) -> Result<Value, String> {
    send_cdp_command(state.clone(), tab_id, "Debugger.stepOver", serde_json::json!({})).await
}

pub async fn step_into(state: SharedHub, tab_id: Option<u64>) -> Result<Value, String> {
    send_cdp_command(state.clone(), tab_id, "Debugger.stepInto", serde_json::json!({})).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_inline_styles() {
        let html1 = r#"<div class="test" style="position: relative;">"#;
        assert_eq!(strip_inline_styles(html1), r#"<div class="test">"#);

        let html2 = r#"<span style="box-sizing: border-box; display: block; overflow: hidden; width: initial; height: initial; background: none; opacity: 1; border: 0px; margin: 0px; padding: 0px; position: absolute; inset: 0px;">"#;
        assert_eq!(strip_inline_styles(html2), "<span>");

        let html3 = r#"<div style='color: red;' id='btn'>"#;
        assert_eq!(strip_inline_styles(html3), "<div id='btn'>");

        let html4 = r#"<div class="a" style="color: red" id="b">"#;
        assert_eq!(strip_inline_styles(html4), r#"<div class="a" id="b">"#);

        // Tags named <style> should not be stripped
        let html5 = r#"<style>body { color: red; }</style>"#;
        assert_eq!(strip_inline_styles(html5), html5);

        // Plain string without style
        let plain = "Hello world!";
        assert_eq!(strip_inline_styles(plain), plain);
    }

    #[test]
    fn test_optimize_evaluate_value_null_pruning() {
        let mut val = serde_json::json!({
            "ariaLabel": null,
            "className": "btn-primary",
            "href": null,
            "role": null,
            "tagName": "BUTTON"
        });

        optimize_evaluate_value(&mut val, false);

        assert_eq!(
            val,
            serde_json::json!({
                "className": "btn-primary",
                "tagName": "BUTTON"
            })
        );
    }

    #[test]
    fn test_optimize_evaluate_value_chain_truncation() {
        let long_para = "A".repeat(150);
        let short_text = "Click here";

        let mut val = serde_json::json!({
            "chain": [
                {
                    "tagName": "DIV",
                    "textContent": long_para.clone(),
                    "ariaLabel": null
                },
                {
                    "tagName": "SPAN",
                    "textContent": short_text,
                    "href": null
                }
            ],
            "textContent": long_para.clone()
        });

        optimize_evaluate_value(&mut val, false);

        // In chain: long_para should be truncated to 100 chars + "..."
        let chain_arr = val.get("chain").unwrap().as_array().unwrap();
        let div_text = chain_arr[0].get("textContent").unwrap().as_str().unwrap();
        assert_eq!(div_text.len(), 103);
        assert!(div_text.ends_with("..."));
        assert!(chain_arr[0].get("ariaLabel").is_none());

        let span_text = chain_arr[1].get("textContent").unwrap().as_str().unwrap();
        assert_eq!(span_text, short_text);
        assert!(chain_arr[1].get("href").is_none());

        // Outside chain: textContent should NOT be truncated
        let root_text = val.get("textContent").unwrap().as_str().unwrap();
        assert_eq!(root_text.len(), 150);
        assert_eq!(root_text, long_para);
    }
}
