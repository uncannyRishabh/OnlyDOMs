use crate::websocket_hub::{SharedHub, send_cdp_command, send_hub_command};
use serde_json::Value;

/// Sends a synchronous `CLOAK_HUD` command to `background.js` before/after visual capture
/// when explicitly enabled (`cloak_hud: true`) so `hudHost.style.visibility = "hidden"` is applied
/// for one `requestAnimationFrame` paint tick and immediately restored after capture.
pub async fn set_hud_cloak_state(state: SharedHub, tab_id: Option<u64>, cloak: bool) {
    let cmd_type = if cloak { "CLOAK_HUD" } else { "UNCLOAK_HUD" };
    let _ = send_hub_command(
        state,
        tab_id,
        cmd_type,
        "",
        serde_json::json!({
            "cloak": cloak,
            "visibility": if cloak { "hidden" } else { "visible" }
        }),
    )
    .await;
}

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
/// Uses compositor surface capture to strictly omit browser UI, DevTools banners, and the Agent HUD pill.
pub async fn take_screenshot(state: SharedHub, tab_id: Option<u64>, params: Value) -> Result<Value, String> {
    let full_page = params.get("full_page").and_then(|v| v.as_bool()).unwrap_or(false);
    let raw_format = params.get("format").and_then(|v| v.as_str()).unwrap_or("png").to_lowercase();
    let format = match raw_format.as_str() {
        "jpg" | "jpeg" => "jpeg",
        "webp" => "webp",
        _ => "png",
    };
    let quality = params.get("quality").and_then(|v| v.as_u64()).map(|q| q.min(100) as u32);
    let save_path = params.get("save_path").and_then(|v| v.as_str()).map(|s| s.to_string());
    let explicit_cloak = params.get("cloak_hud").and_then(|v| v.as_bool()).unwrap_or(false);

    // By default, omit base64 string when save_path is provided to protect agent context window
    let include_base64 = params.get("include_base64")
        .and_then(|v| v.as_bool())
        .unwrap_or(save_path.is_none());

    if explicit_cloak {
        set_hud_cloak_state(state.clone(), tab_id, true).await;
    }

    // 1. Retrieve page dimensions using runtime evaluation fallback + getLayoutMetrics
    let mut width = 1280.0;
    let mut height = 800.0;

    let eval_res = send_cdp_command(state.clone(), tab_id, "Runtime.evaluate", serde_json::json!({
        "expression": "(() => {
            const hud = document.getElementById('__mcp_agent_hud_host__') || document.querySelector('mcp-agent-hud');
            if (hud) hud.style.visibility = 'hidden';
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
    if let Ok(metrics) = send_cdp_command(state.clone(), tab_id, "Page.getLayoutMetrics", serde_json::json!({})).await {
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
    let screenshot_res = send_cdp_command(state.clone(), tab_id, "Page.captureScreenshot", screenshot_params).await;

    if explicit_cloak {
        set_hud_cloak_state(state.clone(), tab_id, false).await;
    }

    let screenshot_res = screenshot_res?;
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
        "hud_cloaked": true
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
pub async fn capture_viewport(state: SharedHub, tab_id: Option<u64>, params: Value) -> Result<Value, String> {
    let format = params.get("format").and_then(|v| v.as_str()).unwrap_or("webp");
    let quality = params.get("quality").and_then(|v| v.as_u64()).unwrap_or(80);
    let cloak_hud = params.get("cloak_hud").and_then(|v| v.as_bool()).unwrap_or(false);

    let res = take_screenshot(state, tab_id, serde_json::json!({
        "full_page": false,
        "format": format,
        "quality": quality,
        "include_base64": true,
        "cloak_hud": cloak_hud
    })).await?;

    let b64 = res.get("base64_image").and_then(|v| v.as_str()).unwrap_or("");
    let w = res.get("viewport_width").and_then(|v| v.as_f64()).unwrap_or(1920.0);
    let h = res.get("viewport_height").and_then(|v| v.as_f64()).unwrap_or(1080.0);

    Ok(serde_json::json!({
        "base64_image": b64,
        "viewport_css": { "width": w, "height": h },
        "hud_cloaked": true
    }))
}

pub const PDF_HEADER_MAGIC: &[u8] = b"%PDF-";
pub const PDF_TRAILER_MAGIC: &[u8] = b"%%EOF";
pub const MIN_VALID_PDF_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct PrintToPdfOptions {
    pub tab_id: Option<u64>,
    pub landscape: bool,
    pub print_background: bool,
    pub scale: f64,
    pub paper_width: f64,
    pub paper_height: f64,
    pub margin_top: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
    pub margin_right: f64,
    pub page_ranges: Option<String>,
    pub prefer_css_page_size: bool,
    pub display_header_footer: bool,
    pub header_template: Option<String>,
    pub footer_template: Option<String>,
    pub save_path: Option<String>,
    pub include_base64: bool,
}

pub fn parse_and_validate_pdf_params(params: &serde_json::Value) -> Result<(PrintToPdfOptions, serde_json::Value), String> {
    let mut paper_width = 8.5;
    let mut paper_height = 11.0;

    if let Some(format) = params.get("paper_format").or_else(|| params.get("format")).and_then(|v| v.as_str()) {
        match format.to_lowercase().as_str() {
            "a4" => { paper_width = 8.27; paper_height = 11.69; }
            "letter" => { paper_width = 8.5; paper_height = 11.0; }
            "legal" => { paper_width = 8.5; paper_height = 14.0; }
            "tabloid" => { paper_width = 11.0; paper_height = 17.0; }
            "a3" => { paper_width = 11.69; paper_height = 16.54; }
            other => return Err(format!("Unsupported paper format: '{}'. Supported formats: letter, a4, legal, tabloid, a3", other)),
        }
    }

    if let Some(pw) = params.get("paper_width").and_then(|v| v.as_f64()) {
        if pw <= 0.0 {
            return Err("paper_width must be greater than 0.0 inches".to_string());
        }
        paper_width = pw;
    }

    if let Some(ph) = params.get("paper_height").and_then(|v| v.as_f64()) {
        if ph <= 0.0 {
            return Err("paper_height must be greater than 0.0 inches".to_string());
        }
        paper_height = ph;
    }

    let scale = params.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
    if !(0.1..=2.0).contains(&scale) {
        return Err(format!("Invalid scale factor {}: scale must be between 0.1 and 2.0", scale));
    }

    let margin_top = params.get("margin_top").and_then(|v| v.as_f64()).unwrap_or(0.4);
    let margin_bottom = params.get("margin_bottom").and_then(|v| v.as_f64()).unwrap_or(0.4);
    let margin_left = params.get("margin_left").and_then(|v| v.as_f64()).unwrap_or(0.4);
    let margin_right = params.get("margin_right").and_then(|v| v.as_f64()).unwrap_or(0.4);

    if margin_top < 0.0 || margin_bottom < 0.0 || margin_left < 0.0 || margin_right < 0.0 {
        return Err("Margins must be non-negative (>= 0.0 inches)".to_string());
    }

    if (margin_left + margin_right) >= paper_width {
        return Err(format!(
            "Horizontal margins (left: {} + right: {} = {}) exceed paper width ({})",
            margin_left, margin_right, margin_left + margin_right, paper_width
        ));
    }

    if (margin_top + margin_bottom) >= paper_height {
        return Err(format!(
            "Vertical margins (top: {} + bottom: {} = {}) exceed paper height ({})",
            margin_top, margin_bottom, margin_top + margin_bottom, paper_height
        ));
    }

    let landscape = params.get("landscape").and_then(|v| v.as_bool()).unwrap_or(false);
    let print_background = params.get("print_background").and_then(|v| v.as_bool()).unwrap_or(true);
    let prefer_css_page_size = params.get("prefer_css_page_size").and_then(|v| v.as_bool()).unwrap_or(false);
    let display_header_footer = params.get("display_header_footer").and_then(|v| v.as_bool()).unwrap_or(false);
    let header_template = params.get("header_template").and_then(|v| v.as_str()).map(|s| s.to_string());
    let footer_template = params.get("footer_template").and_then(|v| v.as_str()).map(|s| s.to_string());

    let page_ranges = params.get("page_ranges")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(ref ranges) = page_ranges {
        for part in ranges.split(',') {
            let part = part.trim();
            if part.is_empty() {
                return Err(format!("Invalid page_ranges syntax: empty entry in '{}'", ranges));
            }
            if let Some((start_str, end_str)) = part.split_once('-') {
                let start: u32 = start_str.trim().parse()
                    .map_err(|_| format!("Invalid start page number in range '{}'", part))?;
                let end: u32 = end_str.trim().parse()
                    .map_err(|_| format!("Invalid end page number in range '{}'", part))?;
                if start == 0 || end == 0 {
                    return Err("Page numbers must be 1-indexed (>= 1)".to_string());
                }
                if start > end {
                    return Err(format!("Start page {} cannot exceed end page {} in range '{}'", start, end, part));
                }
            } else {
                let single: u32 = part.parse()
                    .map_err(|_| format!("Invalid page number '{}' in page_ranges", part))?;
                if single == 0 {
                    return Err("Page numbers must be 1-indexed (>= 1)".to_string());
                }
            }
        }
    }

    let save_path = params.get("save_path").and_then(|v| v.as_str()).map(|s| s.to_string());
    let include_base64 = params.get("include_base64")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let tab_id = params.get("tab_id").or_else(|| params.get("tabId")).and_then(|v| v.as_u64());

    let mut cdp_params = serde_json::json!({
        "landscape": landscape,
        "displayHeaderFooter": display_header_footer,
        "printBackground": print_background,
        "scale": scale,
        "paperWidth": paper_width,
        "paperHeight": paper_height,
        "marginTop": margin_top,
        "marginBottom": margin_bottom,
        "marginLeft": margin_left,
        "marginRight": margin_right,
        "preferCSSPageSize": prefer_css_page_size,
        "transferMode": "ReturnAsBase64"
    });

    if let Some(ref ranges) = page_ranges {
        cdp_params["pageRanges"] = serde_json::json!(ranges);
    }
    if let Some(ref ht) = header_template {
        cdp_params["headerTemplate"] = serde_json::json!(ht);
    }
    if let Some(ref ft) = footer_template {
        cdp_params["footerTemplate"] = serde_json::json!(ft);
    }

    let opts = PrintToPdfOptions {
        tab_id,
        landscape,
        print_background,
        scale,
        paper_width,
        paper_height,
        margin_top,
        margin_bottom,
        margin_left,
        margin_right,
        page_ranges,
        prefer_css_page_size,
        display_header_footer,
        header_template,
        footer_template,
        save_path,
        include_base64,
    };

    Ok((opts, cdp_params))
}

pub fn validate_pdf_bytes(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < MIN_VALID_PDF_SIZE {
        return Err(format!(
            "PDF payload is too small ({} bytes, expected >= {} bytes)",
            bytes.len(),
            MIN_VALID_PDF_SIZE
        ));
    }

    if !bytes.starts_with(PDF_HEADER_MAGIC) {
        let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(8)]);
        return Err(format!(
            "Invalid PDF payload: missing '%PDF-' header magic bytes. Found prefix: '{}'",
            prefix
        ));
    }

    let scan_start = bytes.len().saturating_sub(1024);
    let tail = &bytes[scan_start..];
    let has_eof = tail.windows(PDF_TRAILER_MAGIC.len()).any(|w| w == PDF_TRAILER_MAGIC);
    if !has_eof {
        return Err("Invalid PDF payload: missing '%%EOF' trailer marker in document tail".to_string());
    }

    Ok(())
}

pub fn decode_and_validate_pdf(base64_str: &str) -> Result<Vec<u8>, String> {
    let bytes = decode_base64(base64_str)
        .map_err(|e| format!("Failed to decode PDF base64: {}", e))?;
    validate_pdf_bytes(&bytes)?;
    Ok(bytes)
}

/// Generate a PDF document from the current browser page using CDP Page.printToPDF.
/// Executes completely silently without OS or browser print dialogs.
pub async fn print_to_pdf(
    state: SharedHub,
    tab_id: Option<u64>,
    params: Value,
) -> Result<Value, String> {
    let (opts, cdp_params) = parse_and_validate_pdf_params(&params)?;
    let target_tab = opts.tab_id.or(tab_id);
    let explicit_cloak = params.get("cloak_hud").and_then(|v| v.as_bool()).unwrap_or(false);

    if explicit_cloak {
        set_hud_cloak_state(state.clone(), target_tab, true).await;
    }

    let cdp_res = send_cdp_command(state.clone(), target_tab, "Page.printToPDF", cdp_params).await;

    if explicit_cloak {
        set_hud_cloak_state(state.clone(), target_tab, false).await;
    }

    let cdp_res = cdp_res?;
    let data = cdp_res.get("data").and_then(|d| d.as_str()).ok_or("Failed to obtain PDF data from CDP")?;

    let decoded_bytes = decode_and_validate_pdf(data)?;
    let size_bytes = decoded_bytes.len();

    let mut saved_file_path = None;
    if let Some(ref path_str) = opts.save_path {
        let path = std::path::Path::new(path_str);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await
                    .map_err(|e| format!("Failed to create output directory {}: {}", parent.display(), e))?;
            }
        }
        tokio::fs::write(path, &decoded_bytes).await
            .map_err(|e| format!("Failed to save PDF to {}: {}", path.display(), e))?;
        saved_file_path = Some(path_str.clone());
    }

    let target_tid = target_tab.unwrap_or(0);
    let mut result = serde_json::json!({
        "success": true,
        "byte_count": size_bytes,
        "size_bytes": size_bytes,
        "saved_path": saved_file_path.clone(),
        "file_path": saved_file_path,
        "tab_id": target_tid,
        "paper_width": opts.paper_width,
        "paper_height": opts.paper_height,
        "landscape": opts.landscape
    });

    if opts.include_base64 {
        result["data"] = serde_json::json!(data);
        result["base64_pdf"] = serde_json::json!(data);
    }

    Ok(result)
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
        let std_res = decode_base64("+/++").unwrap();
        let url_res = decode_base64("-_--").unwrap();
        assert_eq!(std_res, vec![0xfb, 0xff, 0xbe]);
        assert_eq!(url_res, vec![0xfb, 0xff, 0xbe]);
        assert_eq!(std_res, url_res);

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

    #[test]
    fn test_pdf_magic_bytes_detection() {
        let valid_pdf_b64 = "JVBERi0xLjQKJcTl8uXrCg==";
        let decoded = decode_base64(valid_pdf_b64).unwrap();
        assert!(decoded.len() >= 5);
        assert_eq!(&decoded[0..5], b"%PDF-");

        let invalid_b64 = "SGVsbG8gV29ybGQ=";
        let decoded_invalid = decode_base64(invalid_b64).unwrap();
        assert_ne!(&decoded_invalid[0..5], b"%PDF-");
    }

    #[test]
    fn test_validate_pdf_bytes_valid_minimal() {
        let valid_pdf = b"%PDF-1.4\nsome stream content\n%%EOF\n";
        assert!(validate_pdf_bytes(valid_pdf).is_ok());
    }

    #[test]
    fn test_validate_pdf_bytes_invalid_header_magic() {
        let bad_header = b"NOT_A_PDF_STREAM_DATA_HERE_1234567890%%EOF\n";
        let res = validate_pdf_bytes(bad_header);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("missing '%PDF-' header magic bytes"));
    }

    #[test]
    fn test_validate_pdf_bytes_invalid_missing_eof() {
        let missing_eof = b"%PDF-1.4\nsome incomplete stream content without trailer delimiter";
        let res = validate_pdf_bytes(missing_eof);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("missing '%%EOF' trailer marker"));
    }

    #[test]
    fn test_validate_pdf_bytes_too_short() {
        let short = b"%PDF-";
        let res = validate_pdf_bytes(short);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("too small"));
    }

    #[test]
    fn test_decode_and_validate_pdf_roundtrip() {
        let test_b64 = "JVBERi0xLjQKTWluaW1hbCB2YWxpZCBib2R5IGNvbnRlbnQKJSVFT0YK";
        let bytes = decode_and_validate_pdf(test_b64).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.windows(5).any(|w| w == b"%%EOF"));
    }

    #[test]
    fn test_pdf_params_defaults() {
        let (opts, cdp) = parse_and_validate_pdf_params(&serde_json::json!({})).unwrap();
        assert_eq!(opts.landscape, false);
        assert_eq!(opts.print_background, true);
        assert_eq!(opts.scale, 1.0);
        assert_eq!(opts.paper_width, 8.5);
        assert_eq!(opts.paper_height, 11.0);
        assert_eq!(opts.margin_top, 0.4);
        assert_eq!(opts.margin_bottom, 0.4);
        assert_eq!(opts.margin_left, 0.4);
        assert_eq!(opts.margin_right, 0.4);
        assert_eq!(opts.prefer_css_page_size, false);
        assert!(opts.page_ranges.is_none());

        assert_eq!(cdp["landscape"], false);
        assert_eq!(cdp["printBackground"], true);
        assert_eq!(cdp["scale"], 1.0);
        assert_eq!(cdp["paperWidth"], 8.5);
        assert_eq!(cdp["paperHeight"], 11.0);
        assert_eq!(cdp["transferMode"], "ReturnAsBase64");
    }

    #[test]
    fn test_pdf_params_format_presets() {
        let (_, a4_cdp) = parse_and_validate_pdf_params(&serde_json::json!({ "format": "a4" })).unwrap();
        assert_eq!(a4_cdp["paperWidth"], 8.27);
        assert_eq!(a4_cdp["paperHeight"], 11.69);

        let (_, legal_cdp) = parse_and_validate_pdf_params(&serde_json::json!({ "paper_format": "legal" })).unwrap();
        assert_eq!(legal_cdp["paperWidth"], 8.5);
        assert_eq!(legal_cdp["paperHeight"], 14.0);

        let err = parse_and_validate_pdf_params(&serde_json::json!({ "format": "unknown_paper" }));
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("Unsupported paper format"));
    }

    #[test]
    fn test_pdf_params_scale_bounds() {
        assert!(parse_and_validate_pdf_params(&serde_json::json!({ "scale": 0.1 })).is_ok());
        assert!(parse_and_validate_pdf_params(&serde_json::json!({ "scale": 2.0 })).is_ok());
        assert!(parse_and_validate_pdf_params(&serde_json::json!({ "scale": 1.5 })).is_ok());

        let err_low = parse_and_validate_pdf_params(&serde_json::json!({ "scale": 0.05 }));
        assert!(err_low.is_err());
        assert!(err_low.unwrap_err().contains("between 0.1 and 2.0"));

        let err_high = parse_and_validate_pdf_params(&serde_json::json!({ "scale": 2.5 }));
        assert!(err_high.is_err());
        assert!(err_high.unwrap_err().contains("between 0.1 and 2.0"));

        let err_neg = parse_and_validate_pdf_params(&serde_json::json!({ "scale": -1.0 }));
        assert!(err_neg.is_err());
    }

    #[test]
    fn test_pdf_params_margin_bounds() {
        let err_neg = parse_and_validate_pdf_params(&serde_json::json!({ "margin_top": -0.1 }));
        assert!(err_neg.is_err());
        assert!(err_neg.unwrap_err().contains("non-negative"));

        let err_h = parse_and_validate_pdf_params(&serde_json::json!({
            "paper_width": 8.5,
            "margin_left": 4.5,
            "margin_right": 4.5
        }));
        assert!(err_h.is_err());
        assert!(err_h.unwrap_err().contains("Horizontal margins"));

        let err_v = parse_and_validate_pdf_params(&serde_json::json!({
            "paper_height": 11.0,
            "margin_top": 6.0,
            "margin_bottom": 6.0
        }));
        assert!(err_v.is_err());
        assert!(err_v.unwrap_err().contains("Vertical margins"));
    }

    #[test]
    fn test_pdf_params_page_ranges_syntax() {
        let (opts, cdp) = parse_and_validate_pdf_params(&serde_json::json!({
            "page_ranges": "1-3, 5, 8-10"
        })).unwrap();
        assert_eq!(opts.page_ranges, Some("1-3, 5, 8-10".to_string()));
        assert_eq!(cdp["pageRanges"], "1-3, 5, 8-10");

        let err_inv = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "5-2" }));
        assert!(err_inv.is_err());
        assert!(err_inv.unwrap_err().contains("cannot exceed end page"));

        let err_zero = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "0-3" }));
        assert!(err_zero.is_err());
        assert!(err_zero.unwrap_err().contains("1-indexed"));

        let err_str = parse_and_validate_pdf_params(&serde_json::json!({ "page_ranges": "abc" }));
        assert!(err_str.is_err());
    }
}
