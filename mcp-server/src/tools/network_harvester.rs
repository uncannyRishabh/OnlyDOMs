use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};

/// Maximum number of optimistic network calls retained per tab in the ring buffer.
pub const MAX_CAPTURED_REQUESTS_PER_TAB: usize = 50;

/// Strict per-tab memory ceiling (< 2MB) for the network ring buffer.
pub const MAX_TAB_BUFFER_BYTES: usize = 2 * 1024 * 1024;

/// Maximum request postData size captured (64KB matching Network.enable maxPostDataSize).
pub const MAX_POST_DATA_BYTES: usize = 65536;

/// Correlation window (3000ms) linking network calls to the preceding agent tool step's `rpc_id`.
pub const CORRELATION_WINDOW_MS: u64 = 3000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturedRequest {
    pub request_id: String,
    pub tab_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_id: Option<String>,
    pub url: String,
    pub method: String,
    pub resource_type: String,
    pub headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
    pub timestamp_ms: u64,
    pub curl_command: String,
}

impl CapturedRequest {
    pub fn estimated_byte_size(&self) -> usize {
        let mut size = self.request_id.len()
            + self.url.len()
            + self.method.len()
            + self.resource_type.len()
            + self.curl_command.len()
            + 64;
        if let Some(ref r) = self.rpc_id {
            size += r.len();
        }
        for (k, v) in &self.headers {
            size += k.len() + v.len();
        }
        if let Some(ref pd) = self.post_data {
            size += pd.len();
        }
        if let Some(ref rh) = self.response_headers {
            for (k, v) in rh {
                size += k.len() + v.len();
            }
        }
        if let Some(ref rb) = self.response_body {
            size += rb.len();
        }
        size
    }
}

/// Shell-safe single-quote escaping for POSIX shells (`'...'` with `'` escaped as `'\''`).
pub fn shell_escape_single_quoted(input: &str) -> String {
    let mut out = String::with_capacity(input.len() + 2);
    out.push('\'');
    for ch in input.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// Serializes a `CapturedRequest` into a shell-safe, deterministic `curl` command string.
pub fn serialize_to_curl(
    method: &str,
    url: &str,
    headers: &HashMap<String, String>,
    post_data: Option<&str>,
) -> String {
    let clean_method = if method.trim().is_empty() {
        "GET"
    } else {
        method.trim()
    };

    let mut parts = Vec::new();
    parts.push("curl".to_string());
    parts.push("-X".to_string());
    parts.push(clean_method.to_ascii_uppercase());
    parts.push(shell_escape_single_quoted(url.trim()));

    // Sort header keys for deterministic output
    let mut header_keys: Vec<&String> = headers.keys().collect();
    header_keys.sort();
    for k in header_keys {
        if let Some(v) = headers.get(k) {
            let header_pair = format!("{}: {}", k, v);
            parts.push("-H".to_string());
            parts.push(shell_escape_single_quoted(&header_pair));
        }
    }

    if let Some(body) = post_data {
        if !body.is_empty() {
            parts.push("--data-raw".to_string());
            parts.push(shell_escape_single_quoted(body));
        }
    }

    parts.join(" ")
}

/// Returns `true` only for `XHR`, `Fetch`, and `Document` requests (ignoring images, fonts, stylesheets, media).
pub fn is_allowed_resource_type(resource_type: &str) -> bool {
    matches!(
        resource_type.trim().to_ascii_lowercase().as_str(),
        "xhr" | "fetch" | "document" | "xmlhttprequest"
    )
}

#[derive(Default, Debug)]
pub struct NetworkHarvester {
    /// Per-tab ring buffer of captured requests (max 50 items, < 2MB ceiling).
    pub tab_buffers: HashMap<u64, VecDeque<CapturedRequest>>,
    /// Tracks the latest `(rpc_id, timestamp_ms)` per tab for 3000ms optimistic correlation.
    pub latest_agent_step: HashMap<u64, (String, u64)>,
}

impl NetworkHarvester {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that an agent tool/step with `rpc_id` executed on `tab_id` at `timestamp_ms`.
    pub fn record_agent_step(&mut self, tab_id: u64, rpc_id: &str, timestamp_ms: u64) {
        if tab_id > 0 && !rpc_id.trim().is_empty() {
            self.latest_agent_step
                .insert(tab_id, (rpc_id.trim().to_string(), timestamp_ms));
        }
    }

    /// Resolves the correlated `rpc_id` if the request occurred within 3000ms of an agent step.
    pub fn resolve_correlated_rpc_id(&self, tab_id: u64, req_timestamp_ms: u64) -> Option<String> {
        if let Some((rpc_id, step_ts)) = self.latest_agent_step.get(&tab_id) {
            let delta = req_timestamp_ms.saturating_sub(*step_ts);
            if delta <= CORRELATION_WINDOW_MS {
                return Some(rpc_id.clone());
            }
        }
        None
    }

    /// Inserts or updates a captured request in the tab's ring buffer, enforcing both the
    /// 50-item count limit and the < 2MB memory ceiling.
    pub fn push_or_update(&mut self, mut req: CapturedRequest) -> CapturedRequest {
        // Enforce 64KB postData cap
        if let Some(ref mut pd) = req.post_data {
            if pd.len() > MAX_POST_DATA_BYTES {
                pd.truncate(MAX_POST_DATA_BYTES);
            }
        }

        // Correlate with latest agent step within 3000ms if not already tagged
        if req.rpc_id.is_none() {
            req.rpc_id = self.resolve_correlated_rpc_id(req.tab_id, req.timestamp_ms);
        }

        if req.curl_command.is_empty() {
            req.curl_command = serialize_to_curl(
                &req.method,
                &req.url,
                &req.headers,
                req.post_data.as_deref(),
            );
        }

        let buf = self
            .tab_buffers
            .entry(req.tab_id)
            .or_insert_with(|| VecDeque::with_capacity(MAX_CAPTURED_REQUESTS_PER_TAB));

        // If an entry with the same request_id already exists (e.g. responseReceived updating requestWillBeSent), update it in place
        if let Some(existing) = buf.iter_mut().find(|r| r.request_id == req.request_id) {
            if req.status.is_some() {
                existing.status = req.status;
            }
            if req.response_headers.is_some() {
                existing.response_headers = req.response_headers.clone();
            }
            if req.response_body.is_some() {
                existing.response_body = req.response_body.clone();
            }
            if req.elapsed_ms.is_some() {
                existing.elapsed_ms = req.elapsed_ms;
            }
            return existing.clone();
        }

        if buf.len() >= MAX_CAPTURED_REQUESTS_PER_TAB {
            buf.pop_front();
        }
        buf.push_back(req.clone());

        // Enforce < 2MB per-tab memory ceiling
        while buf.len() > 1 {
            let total_bytes: usize = buf.iter().map(|r| r.estimated_byte_size()).sum();
            if total_bytes <= MAX_TAB_BUFFER_BYTES {
                break;
            }
            buf.pop_front();
        }

        req
    }

    /// Ingests an extension network event (`Network.requestWillBeSent`, `Network.responseReceived`,
    /// or `NETWORK_CAPTURE` envelope). Returns `Some(CapturedRequest)` if accepted, or `None` if filtered out.
    pub fn ingest_extension_event(&mut self, val: &Value) -> Option<CapturedRequest> {
        let tab_id = val
            .get("tabId")
            .or_else(|| val.get("tab_id"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if tab_id == 0 {
            return None;
        }

        let payload = val.get("payload").unwrap_or(val);
        let event_name = val
            .get("event")
            .or_else(|| val.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("NETWORK_CAPTURE");

        let resource_type = payload
            .get("resourceType")
            .or_else(|| payload.get("resource_type"))
            .or_else(|| payload.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("Fetch");

        if !is_allowed_resource_type(resource_type) {
            return None;
        }

        let request_id = payload
            .get("requestId")
            .or_else(|| payload.get("request_id"))
            .or_else(|| val.get("requestId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if request_id.is_empty() {
            return None;
        }

        let now_ms = payload
            .get("timestamp_ms")
            .or_else(|| payload.get("timestamp"))
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64
            });

        // Case 1: Network.responseReceived updating an existing request
        if event_name == "Network.responseReceived" {
            let resp_obj = payload.get("response").unwrap_or(payload);
            let status = resp_obj
                .get("status")
                .and_then(|v| v.as_u64())
                .map(|s| s as u16);
            let response_headers = extract_headers_map(resp_obj.get("headers"));
            let elapsed_ms = payload.get("elapsed_ms").and_then(|v| v.as_u64());

            if let Some(buf) = self.tab_buffers.get_mut(&tab_id) {
                if let Some(existing) = buf.iter_mut().find(|r| r.request_id == request_id) {
                    existing.status = status;
                    if !response_headers.is_empty() {
                        existing.response_headers = Some(response_headers);
                    }
                    if elapsed_ms.is_some() {
                        existing.elapsed_ms = elapsed_ms;
                    }
                    return Some(existing.clone());
                }
            }
        }

        // Case 2: Network.requestWillBeSent or compact NETWORK_CAPTURE envelope
        let req_obj = payload.get("request").unwrap_or(payload);
        let url = req_obj
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if url.is_empty() {
            return None;
        }

        let method = req_obj
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_ascii_uppercase();
        let headers = extract_headers_map(req_obj.get("headers"));
        let post_data = req_obj
            .get("postData")
            .or_else(|| req_obj.get("post_data"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let status = payload
            .get("status")
            .and_then(|v| v.as_u64())
            .map(|s| s as u16);
        let response_headers_map = extract_headers_map(payload.get("responseHeaders").or_else(|| payload.get("response_headers")));
        let response_headers = if response_headers_map.is_empty() {
            None
        } else {
            Some(response_headers_map)
        };
        let response_body = payload
            .get("responseBody")
            .or_else(|| payload.get("response_body"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let elapsed_ms = payload
            .get("elapsedMs")
            .or_else(|| payload.get("elapsed_ms"))
            .and_then(|v| v.as_u64());
        let explicit_rpc_id = payload
            .get("rpc_id")
            .or_else(|| payload.get("rpcId"))
            .or_else(|| val.get("rpc_id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let curl_command = serialize_to_curl(&method, &url, &headers, post_data.as_deref());

        let captured = CapturedRequest {
            request_id,
            tab_id,
            rpc_id: explicit_rpc_id,
            url,
            method,
            resource_type: resource_type.to_string(),
            headers,
            post_data,
            status,
            response_headers,
            response_body,
            elapsed_ms,
            timestamp_ms: now_ms,
            curl_command,
        };

        Some(self.push_or_update(captured))
    }

    pub fn get_request(&self, tab_id: Option<u64>, request_id: &str) -> Option<CapturedRequest> {
        if let Some(tid) = tab_id {
            if let Some(buf) = self.tab_buffers.get(&tid) {
                if let Some(found) = buf.iter().find(|r| r.request_id == request_id) {
                    return Some(found.clone());
                }
            }
        }
        for buf in self.tab_buffers.values() {
            if let Some(found) = buf.iter().find(|r| r.request_id == request_id) {
                return Some(found.clone());
            }
        }
        None
    }

    pub fn list_tab_requests(&self, tab_id: u64) -> Vec<CapturedRequest> {
        self.tab_buffers
            .get(&tab_id)
            .map(|b| b.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn list_requests_by_rpc_id(&self, tab_id: u64, rpc_id: &str) -> Vec<CapturedRequest> {
        self.tab_buffers
            .get(&tab_id)
            .map(|b| {
                b.iter()
                    .filter(|r| r.rpc_id.as_deref() == Some(rpc_id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn extract_headers_map(val: Option<&Value>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(Value::Object(obj)) = val {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                map.insert(k.clone(), s.to_string());
            } else {
                map.insert(k.clone(), v.to_string());
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_resource_type_filtering_only_xhr_fetch_document() {
        assert!(is_allowed_resource_type("XHR"));
        assert!(is_allowed_resource_type("Fetch"));
        assert!(is_allowed_resource_type("Document"));
        assert!(!is_allowed_resource_type("Image"));
        assert!(!is_allowed_resource_type("Font"));
        assert!(!is_allowed_resource_type("Stylesheet"));
        assert!(!is_allowed_resource_type("Media"));
    }

    #[test]
    fn test_ring_buffer_max_50_and_rpc_correlation() {
        let mut harvester = NetworkHarvester::new();
        harvester.record_agent_step(42, "rpc_step_1", 10_000);

        for i in 0..60 {
            let ev = json!({
                "type": "EVENT",
                "event": "NETWORK_CAPTURE",
                "tabId": 42,
                "payload": {
                    "requestId": format!("req_{}", i),
                    "resourceType": "Fetch",
                    "url": format!("https://api.example.com/items/{}", i),
                    "method": "POST",
                    "headers": {
                        "Content-Type": "application/json"
                    },
                    "postData": "{\"qty\":1}",
                    "timestamp_ms": 10_000 + (i * 40) // first 50 are within 3000ms
                }
            });
            let res = harvester.ingest_extension_event(&ev);
            assert!(res.is_some());
        }

        let list = harvester.list_tab_requests(42);
        assert_eq!(list.len(), MAX_CAPTURED_REQUESTS_PER_TAB);
        assert_eq!(list[0].request_id, "req_10");
        assert_eq!(list[49].request_id, "req_59");
        assert_eq!(list[0].rpc_id.as_deref(), Some("rpc_step_1"));
    }

    #[test]
    fn test_shell_safe_curl_serializer_escapes_single_quotes() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer tok'en".to_string());
        let curl = serialize_to_curl(
            "POST",
            "https://api.example.com/login",
            &headers,
            Some("{\"name\":\"O'Reilly\"}"),
        );
        assert!(curl.starts_with("curl -X POST 'https://api.example.com/login'"));
        assert!(curl.contains("'Authorization: Bearer tok'\\''en'"));
        assert!(curl.contains("--data-raw '{\"name\":\"O'\\''Reilly\"}'"));
    }
}
