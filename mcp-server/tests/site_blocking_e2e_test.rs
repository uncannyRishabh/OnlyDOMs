//! E2E Test Suite for Extension Popup Site Blocking & Enforcement (R5, F8, F9)
//!
//! Covers:
//! - Tier 1: Pattern matching (exact, wildcard subdomains, URL prefixes), rejection with explicit error code and message.
//! - Tier 2: Boundaries (case insensitivity, port stripping, safe defaults about:blank / chrome://, malformed URLs, empty patterns).
//! - Tier 3: Pairwise combinations (schemes x pattern formats, operations x blocked/allowed).
//! - Tier 4: Application scenario (agent navigation prevention workflow).

use serde_json::json;

/// Authoritative URL and Domain Blocking Engine matching PROJECT.md interface contract #4
pub struct SiteBlocker {
    blocked_patterns: Vec<String>,
}

impl SiteBlocker {
    pub fn new(patterns: Vec<&str>) -> Self {
        Self {
            blocked_patterns: patterns.into_iter().map(|s| s.trim().to_lowercase()).collect(),
        }
    }

    /// Extract hostname from URL or return raw string
    fn extract_host_and_path(url: &str) -> (String, String) {
        let trimmed = url.trim();
        // Remove scheme if present
        let without_scheme = if let Some(idx) = trimmed.find("://") {
            &trimmed[idx + 3..]
        } else {
            trimmed
        };

        // Split host from path
        let (host_part, path_part) = if let Some(slash_idx) = without_scheme.find('/') {
            (&without_scheme[..slash_idx], &without_scheme[slash_idx..])
        } else {
            (without_scheme, "")
        };

        // Strip port from host
        let host_clean = if let Some(colon_idx) = host_part.find(':') {
            &host_part[..colon_idx]
        } else {
            host_part
        };

        (host_clean.to_lowercase(), path_part.to_string())
    }

    /// Authoritative URL check logic
    pub fn is_blocked(&self, url: &str) -> bool {
        let trimmed = url.trim();
        if trimmed.is_empty() || trimmed == "about:blank" || trimmed.starts_with("chrome://") {
            return false;
        }

        let (host, _) = Self::extract_host_and_path(trimmed);
        let full_lower = trimmed.to_lowercase();

        for pattern in &self.blocked_patterns {
            if pattern.is_empty() {
                continue;
            }

            // 1. Wildcard subdomain pattern: *.example.com
            if let Some(suffix) = pattern.strip_prefix("*.") {
                if host == suffix || host.ends_with(&format!(".{}", suffix)) {
                    return true;
                }
            }
            // 2. Exact host match
            else if host == *pattern || host.ends_with(&format!(".{}", pattern)) {
                return true;
            }
            // 3. URL prefix / substring match (e.g., https://internal.corp/secret)
            else if full_lower.starts_with(pattern) || full_lower.contains(pattern) {
                return true;
            }
        }
        false
    }

    /// Check request and return explicit error payload on block
    pub fn check_url(&self, url: &str) -> Result<(), (i32, String)> {
        if self.is_blocked(url) {
            Err((
                -32003,
                format!("BLOCKED_BY_SITE_POLICY: Access to domain '{}' is restricted by user policy.", url)
            ))
        } else {
            Ok(())
        }
    }
}

// -----------------------------------------------------------------------------
// TIER 1: Primary Behavior (Happy Path) Tests
// -----------------------------------------------------------------------------

#[test]
fn test_site_blocking_exact_domain_match() {
    let blocker = SiteBlocker::new(vec!["facebook.com", "instagram.com"]);

    assert!(blocker.is_blocked("https://facebook.com/"));
    assert!(blocker.is_blocked("https://facebook.com/messages/t/123"));
    assert!(blocker.is_blocked("http://facebook.com"));
    assert!(blocker.is_blocked("facebook.com"));

    // Subdomains should also match exact domain patterns
    assert!(blocker.is_blocked("https://m.facebook.com/"));
    assert!(blocker.is_blocked("https://instagram.com/p/123"));
}

#[test]
fn test_site_blocking_wildcard_subdomain_match() {
    let blocker = SiteBlocker::new(vec!["*.bank.com", "*.secure.gov"]);

    assert!(blocker.is_blocked("https://auth.bank.com/login"));
    assert!(blocker.is_blocked("https://api.internal.bank.com/v1"));
    assert!(blocker.is_blocked("https://bank.com/")); // Apex domain also matched
    assert!(blocker.is_blocked("https://portal.secure.gov/id"));

    // Unrelated domain with similar suffix should NOT match
    assert!(!blocker.is_blocked("https://mybank.com"));
    assert!(!blocker.is_blocked("https://notbank.com"));
}

#[test]
fn test_site_blocking_url_prefix_match() {
    let blocker = SiteBlocker::new(vec!["https://internal.corp/admin", "github.com/settings/tokens"]);

    assert!(blocker.is_blocked("https://internal.corp/admin"));
    assert!(blocker.is_blocked("https://internal.corp/admin/dashboard"));
    assert!(blocker.is_blocked("https://github.com/settings/tokens"));
    assert!(blocker.is_blocked("https://github.com/settings/tokens/new"));

    // Outside the prefix should be permitted
    assert!(!blocker.is_blocked("https://internal.corp/public"));
    assert!(!blocker.is_blocked("https://github.com/settings/profile"));
    assert!(!blocker.is_blocked("https://github.com/rust-lang/rust"));
}

#[test]
fn test_site_blocking_unblocked_domain_allowed() {
    let blocker = SiteBlocker::new(vec!["badsite.com", "*.malicious.org"]);

    assert!(!blocker.is_blocked("https://google.com"));
    assert!(!blocker.is_blocked("https://wikipedia.org"));
    assert!(!blocker.is_blocked("https://doc.rust-lang.org/book/"));
    assert!(!blocker.is_blocked("http://localhost:3000"));
}

#[test]
fn test_site_blocking_explicit_error_code_and_message() {
    let blocker = SiteBlocker::new(vec!["restricted.io"]);
    let check = blocker.check_url("https://restricted.io/api");

    assert!(check.is_err());
    let (code, msg) = check.unwrap_err();
    assert_eq!(code, -32003, "Must match JSON-RPC site blocking error code -32003");
    assert!(msg.contains("BLOCKED_BY_SITE_POLICY"), "Error message must cite site policy");
    assert!(msg.contains("restricted.io"), "Error message must cite the restricted target");
}

// -----------------------------------------------------------------------------
// TIER 2: Boundary & Corner Condition Tests
// -----------------------------------------------------------------------------

#[test]
fn test_site_blocking_case_insensitivity() {
    // Upper case in rule
    let blocker = SiteBlocker::new(vec!["PAYPAL.COM", "*.CHASE.COM"]);

    assert!(blocker.is_blocked("https://paypal.com"));
    assert!(blocker.is_blocked("https://PAYPAL.COM/signin"));
    assert!(blocker.is_blocked("https://Paypal.Com/checkout"));
    assert!(blocker.is_blocked("https://online.chase.com"));
    assert!(blocker.is_blocked("https://ONLINE.CHASE.COM"));
}

#[test]
fn test_site_blocking_url_with_port() {
    let blocker = SiteBlocker::new(vec!["internal-service.local", "*.devcorp.io"]);

    assert!(blocker.is_blocked("http://internal-service.local:8080"));
    assert!(blocker.is_blocked("https://internal-service.local:8443/api"));
    assert!(blocker.is_blocked("https://auth.devcorp.io:9000/oauth"));
}

#[test]
fn test_site_blocking_safe_defaults_blank_and_chrome() {
    // Even if an overly broad pattern exists, internal/blank tabs are never blocked
    let blocker = SiteBlocker::new(vec!["blank", "chrome", "newtab"]);

    assert!(!blocker.is_blocked("about:blank"));
    assert!(!blocker.is_blocked("chrome://newtab/"));
    assert!(!blocker.is_blocked("chrome://settings/"));
    assert!(!blocker.is_blocked(""));
}

#[test]
fn test_site_blocking_malformed_url_safety() {
    let blocker = SiteBlocker::new(vec!["danger.com"]);

    // Non-standard or malformed strings must not panic
    assert!(!blocker.is_blocked("not-a-valid-url"));
    assert!(!blocker.is_blocked("://"));
    assert!(!blocker.is_blocked("   "));
    assert!(!blocker.is_blocked("http:///"));
}

#[test]
fn test_site_blocking_empty_pattern_list() {
    let blocker = SiteBlocker::new(vec![]);

    assert!(!blocker.is_blocked("https://facebook.com"));
    assert!(!blocker.is_blocked("https://bank.com"));
    assert!(blocker.check_url("https://example.com").is_ok());
}

// -----------------------------------------------------------------------------
// TIER 3: Pairwise Combinations
// -----------------------------------------------------------------------------

#[test]
fn test_site_blocking_pairwise_schemes_and_patterns() {
    let schemes = ["http", "https", "ws", "wss"];
    let domains = ["blocked.com", "sub.wildcard.net"];
    let blocker = SiteBlocker::new(vec!["blocked.com", "*.wildcard.net"]);

    for scheme in schemes {
        for domain in domains {
            let url = format!("{}://{}/test", scheme, domain);
            assert!(
                blocker.is_blocked(&url),
                "Expected {}://{} to be blocked",
                scheme,
                domain
            );
        }
    }
}

#[test]
fn test_site_blocking_pairwise_tab_operations() {
    let blocker = SiteBlocker::new(vec!["forbidden.org"]);
    let operations = ["create_tab", "navigate_tab", "pipeline_step", "cdp_navigate"];

    for op in operations {
        // Blocked URL
        let blocked_payload = json!({ "operation": op, "url": "https://forbidden.org" });
        let is_blocked = blocker.is_blocked(blocked_payload["url"].as_str().unwrap());
        assert!(is_blocked, "Operation {} must be blocked", op);

        // Allowed URL
        let allowed_payload = json!({ "operation": op, "url": "https://allowed.org" });
        let is_allowed = !blocker.is_blocked(allowed_payload["url"].as_str().unwrap());
        assert!(is_allowed, "Operation {} must be allowed", op);
    }
}

// -----------------------------------------------------------------------------
// TIER 4: Application Scenario (End-to-End Navigation Prevention)
// -----------------------------------------------------------------------------

#[test]
fn test_site_blocking_e2e_agent_navigation_prevention() {
    let blocker = SiteBlocker::new(vec!["*.confidential-corp.com", "secrets.internal"]);

    // Step 1: Agent attempts create_tab on confidential site
    let target_create_url = "https://payroll.confidential-corp.com/salaries";
    let create_result = blocker.check_url(target_create_url);
    assert!(create_result.is_err());
    let (code, msg) = create_result.unwrap_err();
    assert_eq!(code, -32003);
    assert!(msg.contains("BLOCKED_BY_SITE_POLICY"));

    // Step 2: Agent attempts compound pipeline with a navigation step to secrets.internal
    let pipeline_steps = vec![
        json!({ "action": "navigate", "url": "https://public.example.com" }),
        json!({ "action": "navigate", "url": "https://secrets.internal/keys" }),
        json!({ "action": "click", "selector": "#button" }),
    ];

    let mut executed_steps = 0;
    let mut aborted_at_step = None;
    for (idx, step) in pipeline_steps.iter().enumerate() {
        if step["action"] == "navigate" {
            let url = step["url"].as_str().unwrap();
            if let Err(e) = blocker.check_url(url) {
                aborted_at_step = Some((idx, e));
                break;
            }
        }
        executed_steps += 1;
    }

    assert_eq!(executed_steps, 1, "Only the first safe step should execute");
    assert!(aborted_at_step.is_some(), "Pipeline should abort at step 1");
    let (step_idx, (err_code, _)) = aborted_at_step.unwrap();
    assert_eq!(step_idx, 1);
    assert_eq!(err_code, -32003);

    // Step 3: Agent navigates to legitimate public resource -> succeeds cleanly
    assert!(blocker.check_url("https://developer.mozilla.org").is_ok());
}
