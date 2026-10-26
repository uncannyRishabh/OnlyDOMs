use crate::websocket_hub::{SharedHub, send_cdp_command};
use serde_json::Value;

/// Zero-dependency, memory-safe pure Rust Base64 decoder.
/// Supports standard Base64, URL-safe Base64, whitespace tolerance, and optional padding.
pub fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut clean = Vec::with_capacity(input.len());
    for b in input.bytes() {
        if !b.is_ascii_whitespace() {
            clean.push(b);
        }
    }

    let len = clean.len();
    if len == 0 {
        return Ok(Vec::new());
    }

    let decode_byte = |c: u8| -> Result<u8, String> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' | b'-' => Ok(62),
            b'/' | b'_' => Ok(63),
            _ => Err(format!("Invalid base64 character: '{}'", c as char)),
        }
    };

    // Strip trailing '=' padding
    let mut unpadded_len = len;
    while unpadded_len > 0 && clean[unpadded_len - 1] == b'=' {
        unpadded_len -= 1;
    }

    let padding_count = len - unpadded_len;
    if padding_count > 2 {
        return Err("Invalid base64: more than two padding characters".to_string());
    }
    if unpadded_len == 0 {
        return Err("Invalid base64: input consists only of padding characters".to_string());
    }

    let output_capacity = (unpadded_len * 3) / 4;
    let mut out = Vec::with_capacity(output_capacity);

    for chunk in clean[..unpadded_len].chunks(4) {
        match chunk.len() {
            4 => {
                let b0 = decode_byte(chunk[0])?;
                let b1 = decode_byte(chunk[1])?;
                let b2 = decode_byte(chunk[2])?;
                let b3 = decode_byte(chunk[3])?;
                out.push((b0 << 2) | (b1 >> 4));
                out.push(((b1 & 0x0f) << 4) | (b2 >> 2));
                out.push(((b2 & 0x03) << 6) | b3);
            }
            3 => {
                let b0 = decode_byte(chunk[0])?;
                let b1 = decode_byte(chunk[1])?;
                let b2 = decode_byte(chunk[2])?;
                out.push((b0 << 2) | (b1 >> 4));
                out.push(((b1 & 0x0f) << 4) | (b2 >> 2));
            }
            2 => {
                let b0 = decode_byte(chunk[0])?;
                let b1 = decode_byte(chunk[1])?;
                out.push((b0 << 2) | (b1 >> 4));
            }
            1 => {
                return Err("Invalid base64 length: trailing single character".to_string());
            }
            _ => unreachable!(),
        }
    }

    Ok(out)
}

/// Capture clean page screenshot (viewport or full scrollable page).
/// Uses compositor surface capture to strictly omit browser UI and DevTools banners.
pub async fn take_screenshot(state: SharedHub, params: Value) -> Result<Value, String> {
    let full_page = params.get("full_page").and_then(|v| v.as_bool()).unwrap_or(false);
    let raw_format = params.get("format").and_then(|v| v.as_str()).unwrap_or("png").to_lowercase();
    let format = match raw_format.as_str() {
        "jpg" | "jpeg" => "jpeg",
        "webp" => "webp",
        _ => "png",
    };
    let quality = params.get("quality").and_then(|v| v.as_u64()).map(|q| q.min(100) as u32);
    let save_path = params.get("save_path").and_then(|v| v.as_str()).map(|s| s.to_string());
    
    // By default, omit base64 string when save_path is provided to protect agent context window
    let include_base64 = params.get("include_base64")
        .and_then(|v| v.as_bool())
        .unwrap_or(save_path.is_none());

    // 1. Retrieve page dimensions using runtime evaluation fallback + getLayoutMetrics
    let mut width = 1280.0;
    let mut height = 800.0;

    let eval_res = send_cdp_command(state.clone(), "Runtime.evaluate", serde_json::json!({
        "expression": "(() => {
            const doc = document.documentElement;
            const body = document.body;
            const scrollW = Math.max(
                doc ? doc.scrollWidth : 0,
                doc ? doc.offsetWidth : 0,
                doc ? doc.clientWidth : 0,
                body ? body.scrollWidth : 0,
                body ? body.offsetWidth : 0,
                window.innerWidth || 0
            );
            const scrollH = Math.max(
                doc ? doc.scrollHeight : 0,
                doc ? doc.offsetHeight : 0,
                doc ? doc.clientHeight : 0,
                body ? body.scrollHeight : 0,
                body ? body.offsetHeight : 0,
                window.innerHeight || 0
            );
            return {
                scrollWidth: scrollW,
                scrollHeight: scrollH,
                viewportWidth: window.innerWidth || 1280,
                viewportHeight: window.innerHeight || 800
            };
        })()",
        "returnByValue": true
    })).await;

    let mut viewport_w = 1280.0;
    let mut viewport_h = 800.0;

    if let Ok(eval_val) = eval_res {
        if let Some(res_obj) = eval_val.get("result").and_then(|r| r.get("value")) {
            let sw = res_obj.get("scrollWidth").and_then(|v| v.as_f64()).unwrap_or(1280.0);
            let sh = res_obj.get("scrollHeight").and_then(|v| v.as_f64()).unwrap_or(800.0);
            let vw = res_obj.get("viewportWidth").and_then(|v| v.as_f64()).unwrap_or(1280.0);
            let vh = res_obj.get("viewportHeight").and_then(|v| v.as_f64()).unwrap_or(800.0);

            viewport_w = vw;
            viewport_h = vh;

            if full_page {
                width = sw.max(vw);
                height = sh.max(vh);
            } else {
                width = vw;
                height = vh;
            }
        }
    }

    // Attempt CDP getLayoutMetrics for additional precision (take maximum to avoid clipping)
    if let Ok(metrics) = send_cdp_command(state.clone(), "Page.getLayoutMetrics", serde_json::json!({})).await {
        if full_page {
            if let Some(cs) = metrics.get("cssContentSize").or_else(|| metrics.get("contentSize")) {
                let mw = cs.get("width").and_then(|v| v.as_f64()).unwrap_or(width);
                let mh = cs.get("height").and_then(|v| v.as_f64()).unwrap_or(height);
                if mw > 0.0 && mh > 0.0 {
                    width = width.max(mw);
                    height = height.max(mh);
                }
            }
        }
    }

    // 2. Prepare Page.captureScreenshot command parameters
    let mut screenshot_params = serde_json::json!({
        "format": format,
        "fromSurface": true,
    });

    if format == "jpeg" || format == "webp" {
        screenshot_params["quality"] = serde_json::json!(quality.unwrap_or(80));
    }

    if full_page {
        screenshot_params["captureBeyondViewport"] = serde_json::json!(true);
        screenshot_params["clip"] = serde_json::json!({
            "x": 0.0,
            "y": 0.0,
            "width": width,
            "height": height,
            "scale": 1.0
        });
    } else {
        screenshot_params["captureBeyondViewport"] = serde_json::json!(false);
    }

    // 3. Dispatch screenshot capture via CDP
    let screenshot_res = send_cdp_command(state.clone(), "Page.captureScreenshot", screenshot_params).await?;
    let data = screenshot_res.get("data").and_then(|d| d.as_str()).ok_or("Failed to obtain screenshot data from CDP")?;

    // 4. Decode base64 image data
    let decoded_bytes = decode_base64(data)?;
    let size_bytes = decoded_bytes.len();

    // 5. If save_path is requested, persist binary file to disk
    let mut saved_file_path = None;
    if let Some(ref path_str) = save_path {
        let path = std::path::Path::new(path_str);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| format!("Failed to create output directory {}: {}", parent.display(), e))?;
        }
        tokio::fs::write(path, &decoded_bytes).await
            .map_err(|e| format!("Failed to save screenshot to {}: {}", path.display(), e))?;
        saved_file_path = Some(path_str.clone());
    }

    // 6. Assemble response object
    let mut result = serde_json::json!({
        "success": true,
        "format": format,
        "full_page": full_page,
        "width": width,
        "height": height,
        "viewport_width": viewport_w,
        "viewport_height": viewport_h,
        "size_bytes": size_bytes,
    });

    if let Some(p) = saved_file_path {
        result["file_path"] = serde_json::json!(p);
    }

    if include_base64 {
        result["base64_image"] = serde_json::json!(data);
    }

    Ok(result)
}

/// Backwards-compatible capture_viewport function: captures a clean viewport screenshot without grid overlay.
pub async fn capture_viewport(state: SharedHub, params: Value) -> Result<Value, String> {
    let format = params.get("format").and_then(|v| v.as_str()).unwrap_or("webp");
    let quality = params.get("quality").and_then(|v| v.as_u64()).unwrap_or(80);

    let res = take_screenshot(state, serde_json::json!({
        "full_page": false,
        "format": format,
        "quality": quality,
        "include_base64": true
    })).await?;

    let b64 = res.get("base64_image").and_then(|v| v.as_str()).unwrap_or("");
    let w = res.get("viewport_width").and_then(|v| v.as_f64()).unwrap_or(1920.0);
    let h = res.get("viewport_height").and_then(|v| v.as_f64()).unwrap_or(1080.0);

    Ok(serde_json::json!({
        "base64_image": b64,
        "viewport_css": { "width": w, "height": h }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_base64_standard_strings() {
        assert_eq!(decode_base64("").unwrap(), Vec::<u8>::new());
        assert_eq!(decode_base64("Zg==").unwrap(), b"f");
        assert_eq!(decode_base64("Zm8=").unwrap(), b"fo");
        assert_eq!(decode_base64("Zm9v").unwrap(), b"foo");
        assert_eq!(decode_base64("Zm9vYg==").unwrap(), b"foob");
        assert_eq!(decode_base64("Zm9vYmE=").unwrap(), b"fooba");
        assert_eq!(decode_base64("Zm9vYmFy").unwrap(), b"foobar");
    }

    #[test]
    fn test_decode_base64_url_safe_and_whitespace() {
        // b"\xfb\xff\xbe" in standard is "+/++" and in url-safe is "-_--"
        let std_res = decode_base64("+/++").unwrap();
        let url_res = decode_base64("-_--").unwrap();
        assert_eq!(std_res, vec![0xfb, 0xff, 0xbe]);
        assert_eq!(url_res, vec![0xfb, 0xff, 0xbe]);
        assert_eq!(std_res, url_res);

        // With newlines/spaces
        let whitespace_b64 = "  Zm9v\n  \r\n YmFy ";
        assert_eq!(decode_base64(whitespace_b64).unwrap(), b"foobar");
    }

    #[test]
    fn test_decode_base64_invalid_chars() {
        assert!(decode_base64("Zm9v!").is_err());
        assert!(decode_base64("A").is_err());
        assert!(decode_base64("Zm9vY").is_err());
        assert!(decode_base64("====").is_err());
        assert!(decode_base64("===").is_err());
        assert!(decode_base64("A===").is_err());
    }

    #[test]
    fn test_decode_base64_binary_data() {
        let sample = vec![0x00, 0xFF, 0x7F, 0x80];
        let encoded = "AP9/gA==";
        assert_eq!(decode_base64(encoded).unwrap(), sample);
    }
}

