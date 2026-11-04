use serde_json::{Map, Value};

pub const HUD_TAG_LOWER: &str = "mcp-agent-hud";
pub const HUD_ID: &str = "__mcp_agent_hud_host__";
pub const HUD_ATTR: &str = "data-mcp-internal-ignore";

/// Deterministically sanitizes any JSON payload arriving from the Chrome Extension
/// before it is returned to an MCP tool caller.
///
/// Strips:
/// 1. JSON objects representing `<mcp-agent-hud>`, `#__mcp_agent_hud_host__`, or `[data-mcp-internal-ignore]`.
/// 2. Null-valued `"node"`, `"element"`, or `"target"` parent fields left behind after HUD pruning.
/// 3. Null entries inside JSON arrays after HUD objects are pruned.
/// 4. Raw HTML substrings (`outerHTML`, `innerHTML`, serialized DOM dumps) containing
///    `<mcp-agent-hud ...>...</mcp-agent-hud>`, `<mcp-agent-hud .../>`, or any element carrying
///    `id="__mcp_agent_hud_host__"` or `data-mcp-internal-ignore`.
pub fn sanitize_extension_payload(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if is_hud_json_object(map) {
                *value = Value::Null;
                return;
            }
            let keys: Vec<String> = map.keys().cloned().collect();
            for k in keys {
                if let Some(child) = map.get_mut(&k) {
                    sanitize_extension_payload(child);
                    if child.is_null() && should_prune_null_field(&k) {
                        map.remove(&k);
                    }
                }
            }
        }
        Value::Array(arr) => {
            if is_flat_hud_attribute_array(arr) {
                arr.clear();
                return;
            }
            for item in arr.iter_mut() {
                sanitize_extension_payload(item);
            }
            arr.retain(|item| !item.is_null());
        }
        Value::String(s) => {
            if contains_hud_marker_ci(s) {
                // If the string itself is serialized JSON (e.g., from Runtime.evaluate JSON.stringify),
                // parse, sanitize the JSON structure, and re-serialize.
                let trimmed = s.trim();
                if (trimmed.starts_with('{') && trimmed.ends_with('}'))
                    || (trimmed.starts_with('[') && trimmed.ends_with(']'))
                {
                    if let Ok(mut parsed) = serde_json::from_str::<Value>(trimmed) {
                        sanitize_extension_payload(&mut parsed);
                        if parsed.is_null() {
                            s.clear();
                        } else if let Ok(serialized) = serde_json::to_string(&parsed) {
                            *s = scrub_hud_html(&serialized);
                        }
                        return;
                    }
                }
                *s = scrub_hud_html(s);
            }
        }
        _ => {}
    }
}

fn contains_hud_marker_ci(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.contains(HUD_TAG_LOWER) || s.contains(HUD_ID) || lower.contains(HUD_ATTR)
}

fn is_hud_json_object(map: &Map<String, Value>) -> bool {
    for tag_key in ["tag", "tagName", "nodeName", "localName", "name"] {
        if let Some(Value::String(val)) = map.get(tag_key) {
            if val.eq_ignore_ascii_case(HUD_TAG_LOWER) {
                return true;
            }
        }
    }
    if map.get("id").and_then(|v| v.as_str()) == Some(HUD_ID) {
        return true;
    }
    if map
        .get(HUD_ATTR)
        .map(|v| v.as_bool() == Some(true) || v.as_str() == Some("true"))
        .unwrap_or(false)
    {
        return true;
    }
    for sel_key in ["selector", "cssSelector", "xpath", "path"] {
        if let Some(Value::String(sel)) = map.get(sel_key) {
            let sel_lower = sel.to_ascii_lowercase();
            if sel_lower.contains(HUD_TAG_LOWER)
                || sel.contains(HUD_ID)
                || sel_lower.contains(HUD_ATTR)
            {
                return true;
            }
        }
    }
    if let Some(attrs) = map.get("attributes") {
        match attrs {
            Value::Object(attr_map) => {
                if attr_map.contains_key(HUD_ATTR)
                    || attr_map.get("id").and_then(|v| v.as_str()) == Some(HUD_ID)
                {
                    return true;
                }
            }
            Value::Array(attr_arr) => {
                if is_flat_hud_attribute_array(attr_arr) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn is_flat_hud_attribute_array(attr_arr: &[Value]) -> bool {
    for window in attr_arr.chunks(2) {
        if let [Value::String(k), Value::String(v)] = window {
            if k.eq_ignore_ascii_case(HUD_ATTR) || (k.eq_ignore_ascii_case("id") && v == HUD_ID) {
                return true;
            }
        }
    }
    false
}

fn should_prune_null_field(key: &str) -> bool {
    matches!(key, "node" | "element" | "target")
}

/// Scrubs `<mcp-agent-hud ...>...</mcp-agent-hud>`, `<mcp-agent-hud .../>`, and any HTML tag
/// carrying `id="__mcp_agent_hud_host__"` or `data-mcp-internal-ignore` across multiline strings.
pub fn scrub_hud_html(input: &str) -> String {
    let mut current = input.to_string();
    let mut passes = 0;

    while passes < 8 && contains_hud_marker_ci(&current) {
        passes += 1;
        let next = scrub_hud_html_single_pass(&current);
        if next == current {
            break;
        }
        current = next;
    }

    current
}

fn scrub_hud_html_single_pass(input: &str) -> String {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        if bytes[i] == b'<' {
            if let Some(skip_to) = try_match_hud_element(input, i) {
                i = skip_to;
                continue;
            }
        }
        if let Some(ch) = input[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    out
}

/// Checks if `input[start..]` begins an opening or self-closing HTML tag that belongs to the
/// agent HUD (`<mcp-agent-hud...>` or `<tag ... id="__mcp_agent_hud_host__" ...>` or `data-mcp-internal-ignore`).
/// If matched, returns the byte index immediately after the closing `</tag>` (or after `>` if self-closing/unclosed).
fn try_match_hud_element(input: &str, start: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let len = bytes.len();
    if start + 2 >= len || bytes[start] != b'<' {
        return None;
    }

    // Ignore closing tags here unless orphaned </mcp-agent-hud>
    if bytes[start + 1] == b'/' {
        let mut cursor = start + 2;
        while cursor < len && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'-') {
            cursor += 1;
        }
        let tag_name = &input[start + 2..cursor];
        if tag_name.eq_ignore_ascii_case(HUD_TAG_LOWER) {
            while cursor < len && bytes[cursor] != b'>' {
                cursor += 1;
            }
            if cursor < len && bytes[cursor] == b'>' {
                return Some(cursor + 1);
            }
        }
        return None;
    }

    // Parse tag name
    let mut name_end = start + 1;
    while name_end < len && (bytes[name_end].is_ascii_alphanumeric() || bytes[name_end] == b'-') {
        name_end += 1;
    }
    if name_end == start + 1 {
        return None;
    }
    let tag_name = &input[start + 1..name_end];

    // Find end of opening tag `>`, respecting quoted attribute values
    let mut tag_close = name_end;
    let mut in_quote: Option<u8> = None;
    while tag_close < len {
        let b = bytes[tag_close];
        if let Some(q) = in_quote {
            if b == q && (tag_close == 0 || bytes[tag_close - 1] != b'\\') {
                in_quote = None;
            }
        } else if b == b'"' || b == b'\'' {
            in_quote = Some(b);
        } else if b == b'>' {
            break;
        }
        tag_close += 1;
    }

    if tag_close >= len || bytes[tag_close] != b'>' {
        return None;
    }

    let open_tag_slice = &input[start..=tag_close];
    let open_tag_lower = open_tag_slice.to_ascii_lowercase();

    let is_hud_tag = tag_name.eq_ignore_ascii_case(HUD_TAG_LOWER);
    let has_hud_id = open_tag_slice.contains(&format!("id=\"{}\"", HUD_ID))
        || open_tag_slice.contains(&format!("id='{}'", HUD_ID))
        || open_tag_slice.contains(&format!("id={}", HUD_ID));
    let has_hud_attr = open_tag_lower.contains(HUD_ATTR);

    if !is_hud_tag && !has_hud_id && !has_hud_attr {
        return None;
    }

    // Check if self-closing `<.../>`
    let before_gt = open_tag_slice[..open_tag_slice.len() - 1].trim_end();
    if before_gt.ends_with('/') {
        return Some(tag_close + 1);
    }

    // Search for matching closing tag `</tag_name>` (case-insensitive)
    let close_pattern = format!("</{}", tag_name.to_ascii_lowercase());
    let rest_lower = input[tag_close + 1..].to_ascii_lowercase();
    if let Some(rel_pos) = rest_lower.find(&close_pattern) {
        let close_start = tag_close + 1 + rel_pos;
        let after_name = close_start + close_pattern.len();
        if after_name < len {
            let next_b = bytes[after_name];
            if next_b == b'>' || next_b.is_ascii_whitespace() {
                let mut close_end = after_name;
                while close_end < len && bytes[close_end] != b'>' {
                    close_end += 1;
                }
                if close_end < len && bytes[close_end] == b'>' {
                    return Some(close_end + 1);
                }
            }
        }
    }

    // If unclosed `<mcp-agent-hud ...>`, strip the opening tag itself
    Some(tag_close + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_sanitize_json_tree_pruning_by_tag_and_id() {
        let mut payload = json!({
            "root": {
                "tagName": "DIV",
                "children": [
                    {
                        "tagName": "H1",
                        "textContent": "Checkout"
                    },
                    {
                        "tagName": "MCP-AGENT-HUD",
                        "id": "__mcp_agent_hud_host__",
                        "textContent": "Executing :: fill_inputs on Tab :: #142"
                    },
                    {
                        "tagName": "DIV",
                        "id": "__mcp_agent_hud_host__",
                        "textContent": "Fallback HUD Host"
                    },
                    {
                        "tagName": "BUTTON",
                        "id": "pay-now",
                        "textContent": "Pay Now"
                    }
                ],
                "target": {
                    "nodeName": "mcp-agent-hud",
                    "id": "__mcp_agent_hud_host__"
                },
                "node": {
                    "localName": "div",
                    "attributes": {
                        "data-mcp-internal-ignore": "true"
                    }
                }
            }
        });

        sanitize_extension_payload(&mut payload);

        let children = payload["root"]["children"].as_array().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0]["tagName"], "H1");
        assert_eq!(children[1]["id"], "pay-now");

        // Null fields "target" and "node" must be pruned from the parent map
        assert!(payload["root"].get("target").is_none());
        assert!(payload["root"].get("node").is_none());
    }

    #[test]
    fn test_sanitize_cdp_flat_attribute_array_stripping() {
        let mut cdp_dom_payload = json!({
            "nodes": [
                {
                    "nodeId": 1,
                    "nodeName": "DIV",
                    "attributes": ["class", "app-container", "id", "main"]
                },
                {
                    "nodeId": 2,
                    "nodeName": "DIV",
                    "attributes": ["class", "hud-wrapper", "id", "__mcp_agent_hud_host__", "popover", "manual"]
                },
                {
                    "nodeId": 3,
                    "nodeName": "SPAN",
                    "attributes": ["data-mcp-internal-ignore", "true", "role", "status"]
                }
            ]
        });

        sanitize_extension_payload(&mut cdp_dom_payload);

        let nodes = cdp_dom_payload["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["nodeId"], 1);
    }

    #[test]
    fn test_sanitize_raw_outer_html_regex_scrubbing() {
        let mut html_payload = json!({
            "outerHTML": "<div class=\"page\"><h1>Welcome</h1><mcp-agent-hud id=\"__mcp_agent_hud_host__\" data-mcp-internal-ignore=\"true\" popover=\"manual\">\n  <div class=\"pill\">🎯 Intent :: Testing checkout</div>\n</mcp-agent-hud><button id=\"submit\">Submit</button><mcp-agent-hud data-mcp-internal-ignore=\"true\"/><div id='__mcp_agent_hud_host__'>Hidden HUD</div></div>"
        });

        sanitize_extension_payload(&mut html_payload);

        let cleaned = html_payload["outerHTML"].as_str().unwrap();
        assert_eq!(
            cleaned,
            "<div class=\"page\"><h1>Welcome</h1><button id=\"submit\">Submit</button></div>"
        );
        assert!(!cleaned.contains("mcp-agent-hud"));
        assert!(!cleaned.contains("__mcp_agent_hud_host__"));
        assert!(!cleaned.contains("data-mcp-internal-ignore"));
    }

    #[test]
    fn test_sanitize_selector_and_xpath_references() {
        let mut selector_payload = json!({
            "elements": [
                {
                    "selector": "body > div.main > button#submit",
                    "visible": true
                },
                {
                    "selector": "body > mcp-agent-hud#__mcp_agent_hud_host__",
                    "visible": true
                },
                {
                    "xpath": "//*[@id=\"__mcp_agent_hud_host__\"]",
                    "visible": true
                }
            ]
        });

        sanitize_extension_payload(&mut selector_payload);

        let elements = selector_payload["elements"].as_array().unwrap();
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0]["selector"], "body > div.main > button#submit");
    }
}
