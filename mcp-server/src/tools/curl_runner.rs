use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::websocket_hub::{SharedHub, send_cdp_command};
use crate::tools::network_harvester::serialize_to_curl;
use crate::filter::hud_sanitizer::sanitize_extension_payload;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedCurlRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

/// Pure-Rust POSIX shell lexer (`shlex`) that tokenizes a raw `curl` command string without
/// spawning any shell or subprocess.
pub fn shlex_split(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' => {
                in_token = true;
                let mut closed = false;
                while let Some(c) = chars.next() {
                    if c == '\'' {
                        closed = true;
                        break;
                    }
                    current.push(c);
                }
                if !closed {
                    return Err("Unterminated single quote in curl command".to_string());
                }
            }
            '"' => {
                in_token = true;
                let mut closed = false;
                while let Some(c) = chars.next() {
                    if c == '"' {
                        closed = true;
                        break;
                    } else if c == '\\' {
                        if let Some(next_c) = chars.next() {
                            match next_c {
                                'n' => current.push('\n'),
                                'r' => current.push('\r'),
                                't' => current.push('\t'),
                                '"' | '\\' | '$' | '`' => current.push(next_c),
                                '\n' => {} // line continuation inside double quotes
                                other => {
                                    current.push('\\');
                                    current.push(other);
                                }
                            }
                        }
                    } else {
                        current.push(c);
                    }
                }
                if !closed {
                    return Err("Unterminated double quote in curl command".to_string());
                }
            }
            '\\' => {
                if let Some(&next_c) = chars.peek() {
                    if next_c == '\n' {
                        chars.next(); // line continuation
                        continue;
                    } else if next_c == '\r' {
                        chars.next();
                        if chars.peek() == Some(&'\n') {
                            chars.next();
                        }
                        continue;
                    }
                }
                if let Some(next_c) = chars.next() {
                    in_token = true;
                    current.push(next_c);
                }
            }
            c if c.is_whitespace() => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            c => {
                in_token = true;
                current.push(c);
            }
        }
    }

    if in_token {
        tokens.push(current);
    }

    Ok(tokens)
}

/// Parses a raw `curl` command string into a `ParsedCurlRequest` using the pure-Rust lexer.
/// Strictly rejects non-curl commands or dangerous output file flags (`-o`, `--output`, `-O`).
pub fn parse_curl_command(raw_curl: &str) -> Result<ParsedCurlRequest, String> {
    let tokens = shlex_split(raw_curl.trim())?;
    if tokens.is_empty() {
        return Err("Empty curl command".to_string());
    }

    let first = tokens[0].as_str();
    if first != "curl" && !first.ends_with("/curl") {
        return Err(format!(
            "Invalid command '{}': only 'curl' commands are permitted",
            first
        ));
    }

    let mut method: Option<String> = None;
    let mut url: Option<String> = None;
    let mut headers: HashMap<String, String> = HashMap::new();
    let mut body_parts: Vec<String> = Vec::new();

    let mut i = 1;
    while i < tokens.len() {
        let tok = tokens[i].as_str();
        match tok {
            "-X" | "--request" => {
                i += 1;
                let m = tokens
                    .get(i)
                    .ok_or_else(|| format!("Missing HTTP method after {}", tok))?;
                method = Some(m.trim().to_ascii_uppercase());
            }
            "-H" | "--header" => {
                i += 1;
                let h = tokens
                    .get(i)
                    .ok_or_else(|| format!("Missing header value after {}", tok))?;
                if let Some((k, v)) = h.split_once(':') {
                    headers.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
            "-d" | "--data" | "--data-raw" | "--data-binary" | "--data-ascii" | "--data-urlencode" => {
                i += 1;
                let d = tokens
                    .get(i)
                    .ok_or_else(|| format!("Missing body payload after {}", tok))?;
                body_parts.push(d.clone());
            }
            "-b" | "--cookie" => {
                i += 1;
                let c = tokens
                    .get(i)
                    .ok_or_else(|| format!("Missing cookie string after {}", tok))?;
                headers.insert("Cookie".to_string(), c.clone());
            }
            "-A" | "--user-agent" => {
                i += 1;
                let ua = tokens
                    .get(i)
                    .ok_or_else(|| format!("Missing user-agent string after {}", tok))?;
                headers.insert("User-Agent".to_string(), ua.clone());
            }
            "-u" | "--user" => {
                i += 1;
                let creds = tokens
                    .get(i)
                    .ok_or_else(|| format!("Missing credentials after {}", tok))?;
                let encoded = encode_base64_str(creds.as_bytes());
                headers.insert("Authorization".to_string(), format!("Basic {}", encoded));
            }
            "--url" => {
                i += 1;
                let u = tokens
                    .get(i)
                    .ok_or_else(|| "Missing URL after --url".to_string())?;
                url = Some(u.clone());
            }
            "-I" | "--head" => {
                method = Some("HEAD".to_string());
            }
            "-G" | "--get" => {
                method = Some("GET".to_string());
            }
            "-o" | "--output" | "-O" | "--remote-name" => {
                return Err(format!(
                    "Disallowed curl flag '{}': local filesystem writes are prohibited in replay_curl",
                    tok
                ));
            }
            "-s" | "--silent" | "-S" | "--show-error" | "-i" | "--include" | "-v"
            | "--verbose" | "-L" | "--location" | "--compressed" | "-k" | "--insecure" => {
                // Safe no-op flags
            }
            flag if flag.starts_with("-X") && flag.len() > 2 => {
                method = Some(flag[2..].trim().to_ascii_uppercase());
            }
            flag if flag.starts_with('-') => {
                // Ignore unknown short/long informational flags
            }
            positional => {
                if url.is_none()
                    && (positional.starts_with("http://")
                        || positional.starts_with("https://")
                        || positional.starts_with('/'))
                {
                    url = Some(positional.to_string());
                }
            }
        }
        i += 1;
    }

    let resolved_url = url.ok_or_else(|| "No target URL found in curl command".to_string())?;
    let body = if body_parts.is_empty() {
        None
    } else {
        Some(body_parts.join("&"))
    };

    let resolved_method = method.unwrap_or_else(|| {
        if body.is_some() {
            "POST".to_string()
        } else {
            "GET".to_string()
        }
    });

    Ok(ParsedCurlRequest {
        method: resolved_method,
        url: resolved_url,
        headers,
        body,
    })
}

fn encode_base64_str(bytes: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() * 4) / 3 + 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(CHARSET[((n >> 18) & 63) as usize] as char);
        out.push(CHARSET[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARSET[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARSET[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Parses an `http://host[:port][/path[?query]]` URL using pure Rust stdlib.
fn parse_http_url(raw_url: &str) -> Result<(String, u16, String), String> {
    let trimmed = raw_url.trim();
    let without_scheme = if let Some(rest) = trimmed.strip_prefix("http://") {
        rest
    } else if trimmed.starts_with("https://") {
        return Err("Native TCP HTTP client supports http:// endpoints; use browser fetch mode for https://".to_string());
    } else {
        return Err(format!("Invalid HTTP URL '{}': missing http:// scheme", raw_url));
    };

    let (authority, path_and_query) = match without_scheme.find('/') {
        Some(idx) => (&without_scheme[..idx], &without_scheme[idx..]),
        None => (without_scheme, "/"),
    };

    if authority.is_empty() {
        return Err("Missing host in URL".to_string());
    }

    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p_str)) if !h.is_empty() && !p_str.contains(']') => {
            let port = p_str
                .parse::<u16>()
                .map_err(|_| format!("Invalid port in URL '{}'", raw_url))?;
            (h.to_string(), port)
        }
        _ => (authority.to_string(), 80),
    };

    Ok((host, port, path_and_query.to_string()))
}

/// Executes an HTTP/1.1 request over any async byte stream (`TcpStream` or `tokio::io::DuplexStream`).
pub async fn execute_http_over_stream<S>(
    mut stream: S,
    req: &ParsedCurlRequest,
    host: &str,
    port: u16,
    path_and_query: &str,
    timeout_ms: u64,
) -> Result<(u16, HashMap<String, String>, String), String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let body_bytes = req.body.as_deref().unwrap_or("").as_bytes();

    let mut raw_req = format!(
        "{} {} HTTP/1.1\r\nHost: {}:{}\r\nConnection: close\r\n",
        req.method, path_and_query, host, port
    );

    let mut has_content_length = false;
    for (k, v) in &req.headers {
        if k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case("connection") {
            continue;
        }
        if k.eq_ignore_ascii_case("content-length") {
            has_content_length = true;
        }
        raw_req.push_str(&format!("{}: {}\r\n", k, v));
    }
    if !has_content_length && !body_bytes.is_empty() {
        raw_req.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
    }
    raw_req.push_str("\r\n");

    let timeout = Duration::from_millis(timeout_ms.max(500));
    stream
        .write_all(raw_req.as_bytes())
        .await
        .map_err(|e| format!("Failed to write request headers: {}", e))?;
    if !body_bytes.is_empty() {
        stream
            .write_all(body_bytes)
            .await
            .map_err(|e| format!("Failed to write request body: {}", e))?;
    }
    stream.flush().await.ok();

    let mut resp_buf = Vec::new();
    tokio::time::timeout(timeout, stream.read_to_end(&mut resp_buf))
        .await
        .map_err(|_| "Timed out reading HTTP response".to_string())?
        .map_err(|e| format!("Failed to read HTTP response: {}", e))?;

    let (header_end, body_offset) = if let Some(pos) = resp_buf.windows(4).position(|w| w == b"\r\n\r\n") {
        (pos, pos + 4)
    } else if let Some(pos) = resp_buf.windows(2).position(|w| w == b"\n\n") {
        (pos, pos + 2)
    } else {
        return Err("Incomplete HTTP response headers".to_string());
    };

    let header_str = String::from_utf8_lossy(&resp_buf[..header_end]);
    let mut lines = header_str.lines();
    let status_line = lines.next().ok_or_else(|| "Empty HTTP response".to_string())?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(200);

    let mut resp_headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            resp_headers.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    let body_str = String::from_utf8_lossy(&resp_buf[body_offset..]).to_string();
    Ok((status_code, resp_headers, body_str))
}

/// Executes an HTTP/1.1 request natively in Rust over `tokio::net::TcpStream` (zero subprocesses, zero external crates).
pub async fn execute_native_http(req: &ParsedCurlRequest, timeout_ms: u64) -> Result<(u16, HashMap<String, String>, String), String> {
    let (host, port, path_and_query) = parse_http_url(&req.url)?;
    let addr = format!("{}:{}", host, port);
    let timeout = Duration::from_millis(timeout_ms.max(500));
    let stream = tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&addr))
        .await
        .map_err(|_| format!("Connection timed out to {}", addr))?
        .map_err(|e| format!("Failed to connect to {}: {}", addr, e))?;

    execute_http_over_stream(stream, req, &host, port, &path_and_query, timeout_ms).await
}

/// Dispatches the HTTP request inside the target Chrome tab via `Runtime.evaluate(fetch(...))`
/// to preserve browser TLS state and HttpOnly session cookies.
pub async fn execute_browser_fetch(
    state: SharedHub,
    tab_id: Option<u64>,
    req: &ParsedCurlRequest,
) -> Result<(u16, HashMap<String, String>, String), String> {
    let url_json = serde_json::to_string(&req.url).unwrap_or_else(|_| "\"\"".to_string());
    let method_json = serde_json::to_string(&req.method).unwrap_or_else(|_| "\"GET\"".to_string());
    let headers_json = serde_json::to_string(&req.headers).unwrap_or_else(|_| "{}".to_string());
    let body_js = match &req.body {
        Some(b) if req.method != "GET" && req.method != "HEAD" => {
            serde_json::to_string(b).unwrap_or_else(|_| "undefined".to_string())
        }
        _ => "undefined".to_string(),
    };

    let expr = format!(
        "(async () => {{
            const resp = await fetch({url}, {{
                method: {method},
                headers: {headers},
                body: {body},
                credentials: 'include'
            }});
            const text = await resp.text();
            const hdrs = {{}};
            resp.headers.forEach((v, k) => {{ hdrs[k] = v; }});
            return {{
                status: resp.status,
                headers: hdrs,
                body: text
            }};
        }})()",
        url = url_json,
        method = method_json,
        headers = headers_json,
        body = body_js
    );

    let cdp_res = send_cdp_command(
        state,
        tab_id,
        "Runtime.evaluate",
        serde_json::json!({
            "expression": expr,
            "awaitPromise": true,
            "returnByValue": true
        }),
    )
    .await?;

    let val = cdp_res
        .get("result")
        .and_then(|r| r.get("value"))
        .unwrap_or(&cdp_res);

    let status = val
        .get("status")
        .and_then(|s| s.as_u64())
        .unwrap_or(200) as u16;
    let mut headers = HashMap::new();
    if let Some(Value::Object(hmap)) = val.get("headers") {
        for (k, v) in hmap {
            if let Some(s) = v.as_str() {
                headers.insert(k.clone(), s.to_string());
            }
        }
    }
    let body = val
        .get("body")
        .and_then(|b| b.as_str())
        .unwrap_or("")
        .to_string();

    Ok((status, headers, body))
}

/// Zero-Subprocess Rust Replay (`replay_curl` MCP Tool & HUD Action).
///
/// Accepts:
/// - `request_id`: ID of a captured request in the tab's 50-item `NetworkHarvester` ring buffer, OR
/// - `curl` / `curl_command`: A raw or modified `curl` command string parsed via `shlex_split`.
/// - Optional overrides: `method`, `url`, `headers` (object), `body` / `post_data` (string).
/// - Optional `action`: `"list"` to inspect captured optimistic cURL requests for the tab.
pub async fn replay_curl(
    state: SharedHub,
    tab_id: Option<u64>,
    params: Value,
) -> Result<Value, String> {
    let target_tab = params
        .get("tab_id")
        .or_else(|| params.get("tabId"))
        .and_then(|v| v.as_u64())
        .or(tab_id)
        .unwrap_or(1);

    // Support listing captured cURLs from the tab's ring buffer when action == "list", "query", or "get_traffic"
    let action_str = params.get("action").and_then(|v| v.as_str());
    if action_str == Some("list")
        || action_str == Some("query")
        || action_str == Some("get_traffic")
        || action_str == Some("get_network_traffic")
        || params.get("list_captured").and_then(|v| v.as_bool()) == Some(true)
    {
        let rpc_filter = params
            .get("action_id")
            .or_else(|| params.get("actionId"))
            .or_else(|| params.get("rpc_id"))
            .or_else(|| params.get("rpcId"))
            .and_then(|v| v.as_str());
        let req_filter = params
            .get("request_id")
            .or_else(|| params.get("requestId"))
            .and_then(|v| v.as_str());
        let url_filter = params
            .get("url_filter")
            .or_else(|| params.get("url"))
            .and_then(|v| v.as_str());

        let lock = state.lock().await;
        let mut captured = lock.network_harvester.list_tab_requests(target_tab);
        if let Some(rf) = rpc_filter {
            captured.retain(|r| r.rpc_id.as_deref() == Some(rf));
        }
        if let Some(qid) = req_filter {
            captured.retain(|r| r.request_id == qid);
        }
        if let Some(uf) = url_filter {
            let uf_lower = uf.to_lowercase();
            captured.retain(|r| r.url.to_lowercase().contains(&uf_lower));
        }

        return Ok(serde_json::json!({
            "success": true,
            "tab_id": target_tab,
            "count": captured.len(),
            "network_calls": captured,
            "network_traffic": captured
        }));
    }

    let request_id_opt = params
        .get("request_id")
        .or_else(|| params.get("requestId"))
        .and_then(|v| v.as_str());
    let raw_curl_opt = params
        .get("curl")
        .or_else(|| params.get("curl_command"))
        .or_else(|| params.get("command"))
        .and_then(|v| v.as_str());

    let mut parsed_req = if let Some(raw_curl) = raw_curl_opt {
        parse_curl_command(raw_curl)?
    } else if let Some(req_id) = request_id_opt {
        let lock = state.lock().await;
        let cap = lock
            .network_harvester
            .get_request(Some(target_tab), req_id)
            .ok_or_else(|| format!("Captured request '{}' not found in ring buffer", req_id))?;
        ParsedCurlRequest {
            method: cap.method,
            url: cap.url,
            headers: cap.headers,
            body: cap.post_data,
        }
    } else {
        return Err(
            "Missing required parameter: provide either 'request_id' from the ring buffer or a 'curl' command string"
                .to_string(),
        );
    };

    // Apply optional agent/user modifications (method, url, headers, body)
    if let Some(m) = params.get("method").and_then(|v| v.as_str()) {
        if !m.trim().is_empty() {
            parsed_req.method = m.trim().to_ascii_uppercase();
        }
    }
    if let Some(u) = params.get("url").and_then(|v| v.as_str()) {
        if !u.trim().is_empty() {
            parsed_req.url = u.trim().to_string();
        }
    }
    if let Some(Value::Object(hdr_overrides)) = params.get("headers") {
        for (k, v) in hdr_overrides {
            if let Some(s) = v.as_str() {
                parsed_req.headers.insert(k.clone(), s.to_string());
            } else if v.is_null() {
                parsed_req.headers.remove(k);
            } else {
                parsed_req.headers.insert(k.clone(), v.to_string());
            }
        }
    }
    if let Some(body_override) = params.get("body").or_else(|| params.get("post_data")) {
        if let Some(s) = body_override.as_str() {
            parsed_req.body = Some(s.to_string());
        } else if body_override.is_object() || body_override.is_array() {
            parsed_req.body = Some(body_override.to_string());
        }
    }

    let timeout_ms = params
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000);
    let mode = params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");

    let has_ws_tx = {
        let lock = state.lock().await;
        lock.tx.is_some()
    };

    let start = Instant::now();
    let (status_code, resp_headers, raw_body) = if mode == "browser" {
        execute_browser_fetch(state.clone(), Some(target_tab), &parsed_req).await?
    } else if mode == "native" {
        execute_native_http(&parsed_req, timeout_ms).await?
    } else if parsed_req.url.starts_with("http://127.0.0.1")
        || parsed_req.url.starts_with("http://localhost")
        || !has_ws_tx
    {
        match execute_native_http(&parsed_req, timeout_ms).await {
            Ok(res) => res,
            Err(native_err) if has_ws_tx => {
                execute_browser_fetch(state.clone(), Some(target_tab), &parsed_req)
                    .await
                    .map_err(|_| native_err)?
            }
            Err(e) => return Err(e),
        }
    } else {
        execute_browser_fetch(state.clone(), Some(target_tab), &parsed_req).await?
    };
    let elapsed_ms = start.elapsed().as_millis() as u64;

    // Sanitize body through hud_sanitizer and inline style stripper
    let mut body_val = Value::String(raw_body);
    sanitize_extension_payload(&mut body_val);
    let sanitized_body = body_val.as_str().unwrap_or("").to_string();
    let body_json = serde_json::from_str::<Value>(&sanitized_body).ok();

    let canonical_curl = serialize_to_curl(
        &parsed_req.method,
        &parsed_req.url,
        &parsed_req.headers,
        parsed_req.body.as_deref(),
    );

    Ok(serde_json::json!({
        "success": true,
        "tab_id": target_tab,
        "request_id": request_id_opt,
        "method": parsed_req.method,
        "url": parsed_req.url,
        "curl_command": canonical_curl,
        "status": status_code,
        "status_code": status_code,
        "response_headers": resp_headers,
        "elapsed_ms": elapsed_ms,
        "sanitized_body": sanitized_body,
        "body_json": body_json
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shlex_split_and_parse_curl_command() {
        let raw = r#"curl -X POST 'https://staging.example.com/api/login' \
            -H 'Content-Type: application/json' \
            -H "X-Custom-Token: secret_123" \
            --data-raw '{"username":"admin","password":"p'\''ass"}'"#;

        let parsed = parse_curl_command(raw).unwrap();
        assert_eq!(parsed.method, "POST");
        assert_eq!(parsed.url, "https://staging.example.com/api/login");
        assert_eq!(
            parsed.headers.get("Content-Type").map(|s| s.as_str()),
            Some("application/json")
        );
        assert_eq!(
            parsed.headers.get("X-Custom-Token").map(|s| s.as_str()),
            Some("secret_123")
        );
        assert_eq!(
            parsed.body.as_deref(),
            Some("{\"username\":\"admin\",\"password\":\"p'ass\"}")
        );
    }

    #[test]
    fn test_parse_curl_rejects_non_curl_and_output_flags() {
        assert!(parse_curl_command("rm -rf /").is_err());
        assert!(parse_curl_command("curl https://example.com -o /etc/passwd").is_err());
    }

    #[tokio::test]
    async fn test_query_captured_network_traffic_with_action_correlation() {
        use crate::websocket_hub::HubState;
        use std::sync::Arc;
        use tokio::sync::Mutex;
        use serde_json::json;

        let hub = Arc::new(Mutex::new(HubState::new()));
        {
            let mut lock = hub.lock().await;
            lock.network_harvester.push_or_update(crate::tools::network_harvester::CapturedRequest {
                request_id: "req_101".to_string(),
                tab_id: 2,
                rpc_id: Some("action_btn_click".to_string()),
                tool_name: Some("click".to_string()),
                intent: Some("Click login button".to_string()),
                url: "https://auth.example.com/api/v1/tokens".to_string(),
                method: "POST".to_string(),
                resource_type: "XHR".to_string(),
                headers: HashMap::new(),
                post_data: Some("{\"user\":\"test\"}".to_string()),
                status: Some(200),
                response_headers: Some(HashMap::new()),
                response_body: Some("{\"token\":\"xyz\"}".to_string()),
                elapsed_ms: Some(45),
                timestamp_ms: 10_000,
                curl_command: "curl -X POST https://auth.example.com/api/v1/tokens".to_string(),
            });
            lock.network_harvester.push_or_update(crate::tools::network_harvester::CapturedRequest {
                request_id: "req_102".to_string(),
                tab_id: 2,
                rpc_id: Some("action_fetch_feed".to_string()),
                tool_name: Some("navigate".to_string()),
                intent: Some("Navigate to user dashboard".to_string()),
                url: "https://auth.example.com/api/v1/feed".to_string(),
                method: "GET".to_string(),
                resource_type: "Fetch".to_string(),
                headers: HashMap::new(),
                post_data: None,
                status: Some(200),
                response_headers: Some(HashMap::new()),
                response_body: Some("{\"items\":[]}".to_string()),
                elapsed_ms: Some(30),
                timestamp_ms: 10_500,
                curl_command: "curl -X GET https://auth.example.com/api/v1/feed".to_string(),
            });
        }

        // Query by action_id
        let res = replay_curl(
            hub.clone(),
            Some(2),
            json!({
                "action": "get_network_traffic",
                "action_id": "action_btn_click"
            }),
        )
        .await
        .unwrap();

        assert_eq!(res["success"], true);
        assert_eq!(res["count"], 1);
        let calls = res["network_traffic"].as_array().unwrap();
        assert_eq!(calls[0]["request_id"], "req_101");
        assert_eq!(calls[0]["rpc_id"], "action_btn_click");
        assert_eq!(calls[0]["tool_name"], "click");
        assert_eq!(calls[0]["response_body"], "{\"token\":\"xyz\"}");

        // Query by url_filter
        let feed_res = replay_curl(
            hub.clone(),
            Some(2),
            json!({
                "action": "list",
                "url_filter": "feed"
            }),
        )
        .await
        .unwrap();
        assert_eq!(feed_res["count"], 1);
        assert_eq!(feed_res["network_traffic"][0]["request_id"], "req_102");
    }
}
