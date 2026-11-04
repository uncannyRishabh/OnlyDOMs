use serde::{Deserialize, Serialize};
use std::path::Path;
use crate::websocket_hub::{SharedHub, send_hub_command};
use crate::tools::visual::print_to_pdf;
use crate::filter::hud_sanitizer::scrub_hud_html;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportStepEntry {
    pub step_number: usize,
    pub name: String,
    pub action: String,
    pub intent: String,
    pub executing_cmd: String,
    pub status: String,
    pub duration_ms: u64,
    pub error: Option<String>,
    /// Relative asset path inside `<run_id>/`, e.g. `"assets/step_1.webp"`.
    pub asset_rel_path: Option<String>,
    pub curl_snippets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedReportBundle {
    pub run_id: String,
    pub overview_html_path: String,
    pub overview_pdf_path: Option<String>,
    pub html_report_url: String,
    pub pdf_document_url: String,
    pub instant_print_url: String,
}

fn escape_html(input: &str) -> String {
    scrub_hud_html(input)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Compiles step intents (`_intent`), executed commands (`Executing :: xyz on Tab :: xyz`),
/// optimistic cURL snippets, and `<img src="assets/step_<n>.webp">` into a self-contained,
/// print-styled HTML document (`overview.html`) with `@media print` page-break rules and
/// `?print=1` auto-trigger support.
pub fn compile_overview_html(
    run_id: &str,
    test_name: &str,
    global_intent: &str,
    passed: bool,
    total_duration_ms: u64,
    steps: &[ReportStepEntry],
) -> String {
    let verdict_text = if passed { "PASSED" } else { "FAILED" };
    let verdict_color = if passed { "#16a34a" } else { "#dc2626" };

    let mut steps_html = String::new();
    for step in steps {
        let badge_bg = match step.status.as_str() {
            "passed" => "#dcfce7",
            "failed" => "#fee2e2",
            _ => "#f3f4f6",
        };
        let badge_fg = match step.status.as_str() {
            "passed" => "#166534",
            "failed" => "#991b1b",
            _ => "#374151",
        };

        let error_block = if let Some(ref err) = step.error {
            format!(
                r#"<div class="error-banner"><strong>Error:</strong> {}</div>"#,
                escape_html(err)
            )
        } else {
            String::new()
        };

        let curl_block = if !step.curl_snippets.is_empty() {
            let joined = step
                .curl_snippets
                .iter()
                .map(|c| escape_html(c))
                .collect::<Vec<_>>()
                .join("\n\n");
            format!(
                r#"<div class="curl-section">
                    <div class="section-label">Optimistic Network cURL</div>
                    <pre class="curl-code"><code>{}</code></pre>
                </div>"#,
                joined
            )
        } else {
            String::new()
        };

        let img_block = if let Some(ref rel_asset) = step.asset_rel_path {
            format!(
                r#"<div class="screenshot-frame">
                    <img src="{}" alt="Step {} Screenshot" loading="eager" />
                </div>"#,
                escape_html(rel_asset),
                step.step_number
            )
        } else {
            String::new()
        };

        steps_html.push_str(&format!(
            r#"<section class="step-card">
                <div class="step-header">
                    <span class="step-num">Step {:02}: {}</span>
                    <span class="step-badge" style="background:{};color:{};">{} ({} ms)</span>
                </div>
                <div class="meta-row">
                    <div><strong>🎯 Intent:</strong> {}</div>
                    <div class="cmd-ticker"><code>{}</code></div>
                </div>
                {}
                {}
                {}
            </section>"#,
            step.step_number,
            escape_html(&step.name),
            badge_bg,
            badge_fg,
            escape_html(&step.status.to_uppercase()),
            step.duration_ms,
            escape_html(&step.intent),
            escape_html(&step.executing_cmd),
            error_block,
            curl_block,
            img_block
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>OnlyDOMs Visual Test Overview — {test_name}</title>
  <style>
    :root {{
      color-scheme: light;
    }}
    * {{
      box-sizing: border-box;
    }}
    body {{
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
      margin: 0;
      padding: 24px;
      background: #f8fafc;
      color: #0f172a;
    }}
    .report-header {{
      background: #ffffff;
      border: 1px solid #e2e8f0;
      border-radius: 12px;
      padding: 20px 24px;
      margin-bottom: 24px;
      display: flex;
      justify-content: space-between;
      align-items: center;
    }}
    .verdict-pill {{
      padding: 6px 14px;
      border-radius: 9999px;
      color: #ffffff;
      font-weight: 700;
      background: {verdict_color};
    }}
    .step-card {{
      background: #ffffff;
      border: 1px solid #e2e8f0;
      border-radius: 12px;
      padding: 20px;
      margin-bottom: 20px;
      break-inside: avoid;
      page-break-inside: avoid;
    }}
    .step-header {{
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 12px;
    }}
    .step-num {{
      font-size: 1.1rem;
      font-weight: 700;
    }}
    .step-badge {{
      padding: 4px 10px;
      border-radius: 6px;
      font-size: 0.85rem;
      font-weight: 600;
    }}
    .meta-row {{
      background: #f1f5f9;
      border-radius: 8px;
      padding: 10px 14px;
      margin-bottom: 12px;
      font-size: 0.92rem;
    }}
    .cmd-ticker {{
      margin-top: 6px;
      color: #334155;
      font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
      font-size: 0.85rem;
    }}
    .error-banner {{
      background: #fef2f2;
      border-left: 4px solid #ef4444;
      color: #991b1b;
      padding: 10px 12px;
      margin-bottom: 12px;
      border-radius: 4px;
    }}
    .curl-section {{
      margin-bottom: 12px;
    }}
    .section-label {{
      font-size: 0.8rem;
      font-weight: 600;
      text-transform: uppercase;
      color: #475569;
      margin-bottom: 4px;
    }}
    .curl-code {{
      background: #0f172a;
      color: #e2e8f0;
      padding: 10px 12px;
      border-radius: 6px;
      overflow-x: auto;
      font-size: 0.8rem;
      margin: 0;
    }}
    .screenshot-frame img {{
      max-width: 100%;
      height: auto;
      border-radius: 8px;
      border: 1px solid #cbd5e1;
      display: block;
    }}
    @media print {{
      @page {{
        size: A4;
        margin: 12mm;
      }}
      body {{
        background: #ffffff;
        padding: 0;
      }}
      .step-card {{
        page-break-after: always;
        break-after: page;
        page-break-inside: avoid;
        break-inside: avoid;
        box-shadow: none;
      }}
      .step-card:last-of-type {{
        page-break-after: auto;
        break-after: auto;
      }}
    }}
  </style>
  <script>
    window.addEventListener('DOMContentLoaded', () => {{
      const params = new URLSearchParams(window.location.search);
      if (params.get('print') === '1') {{
        window.print();
      }}
    }});
  </script>
</head>
<body>
  <header class="report-header">
    <div>
      <h1 style="margin:0 0 6px 0;font-size:1.4rem;">Visual Test &amp; Feature Overview: {test_name}</h1>
      <div style="color:#475569;font-size:0.9rem;">Run ID: <code>{run_id}</code> &bull; Duration: {total_duration_ms} ms</div>
      <div style="margin-top:6px;font-size:0.95rem;"><strong>🎯 Intent:</strong> {global_intent}</div>
    </div>
    <div class="verdict-pill">{verdict_text}</div>
  </header>
  <main>
    {steps_html}
  </main>
</body>
</html>"#,
        test_name = escape_html(test_name),
        run_id = escape_html(run_id),
        total_duration_ms = total_duration_ms,
        global_intent = escape_html(global_intent),
        verdict_color = verdict_color,
        verdict_text = verdict_text,
        steps_html = steps_html
    )
}

/// Compiles `overview.html`, generates `overview.pdf` silently via CDP `Page.printToPDF`
/// (with `printBackground: true, preferCSSPageSize: true`), and returns the openable daemon
/// links on port 9222 (or the active hub port).
pub async fn build_visual_report_bundle(
    state: SharedHub,
    tab_id: Option<u64>,
    output_dir: &Path,
    run_id: &str,
    test_name: &str,
    global_intent: &str,
    passed: bool,
    total_duration_ms: u64,
    steps: &[ReportStepEntry],
) -> Result<GeneratedReportBundle, String> {
    tokio::fs::create_dir_all(output_dir.join("assets"))
        .await
        .map_err(|e| format!("Failed to create assets directory: {}", e))?;

    let html_content = compile_overview_html(
        run_id,
        test_name,
        global_intent,
        passed,
        total_duration_ms,
        steps,
    );

    let overview_html_file = output_dir.join("overview.html");
    tokio::fs::write(&overview_html_file, &html_content)
        .await
        .map_err(|e| format!("Failed to write overview.html: {}", e))?;

    let (active_port, conn_token) = {
        let lock = state.lock().await;
        (lock.active_port, lock.conn_token)
    };
    let port = if active_port == 0 { 9222 } else { active_port };

    let base_url = format!("http://127.0.0.1:{}/reports/{}", port, run_id);
    let html_report_url = format!("{}/overview.html", base_url);
    let pdf_document_url = format!("{}/overview.pdf", base_url);
    let instant_print_url = format!("{}/overview.html?print=1", base_url);

    let overview_pdf_file = output_dir.join("overview.pdf");
    let overview_pdf_str = overview_pdf_file.to_string_lossy().to_string();

    // Generate overview.pdf via silent CDP Page.printToPDF
    let mut temp_tab_id: Option<u64> = None;
    if conn_token > 0 {
        if let Ok(create_res) = send_hub_command(
            state.clone(),
            None,
            "CREATE_TAB",
            "",
            serde_json::json!({ "url": html_report_url, "active": false }),
        )
        .await
        {
            temp_tab_id = create_res.get("tabId").and_then(|v| v.as_u64());
        }
    }

    let pdf_target_tab = temp_tab_id.or(tab_id);
    let pdf_res = print_to_pdf(
        state.clone(),
        pdf_target_tab,
        serde_json::json!({
            "print_background": true,
            "prefer_css_page_size": true,
            "save_path": overview_pdf_str,
            "include_base64": false
        }),
    )
    .await;

    if let Some(tid) = temp_tab_id {
        let _ = send_hub_command(
            state,
            Some(tid),
            "CLOSE_TAB",
            "",
            serde_json::json!({ "tabId": tid }),
        )
        .await;
    }

    let overview_pdf_path = if pdf_res.is_ok() && overview_pdf_file.exists() {
        Some(overview_pdf_str)
    } else {
        None
    };

    Ok(GeneratedReportBundle {
        run_id: run_id.to_string(),
        overview_html_path: overview_html_file.to_string_lossy().to_string(),
        overview_pdf_path,
        html_report_url,
        pdf_document_url,
        instant_print_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_overview_html_contains_print_styles_and_zero_hud_traces() {
        let steps = vec![ReportStepEntry {
            step_number: 1,
            name: "Submit login form".to_string(),
            action: "click".to_string(),
            intent: "Submit login credentials on staging portal".to_string(),
            executing_cmd: "Executing :: click(#login-btn) on Tab :: #142 (Login)<mcp-agent-hud id=\"__mcp_agent_hud_host__\">leak</mcp-agent-hud>".to_string(),
            status: "passed".to_string(),
            duration_ms: 185,
            error: None,
            asset_rel_path: Some("assets/step_1.webp".to_string()),
            curl_snippets: vec!["curl -X POST 'https://staging.example.com/api/login'".to_string()],
        }];

        let html = compile_overview_html(
            "run_login_01",
            "login_flow",
            "Validate staging login flow and PDF overview generation",
            true,
            185,
            &steps,
        );

        assert!(html.contains("@media print"));
        assert!(html.contains("assets/step_1.webp"));
        assert!(html.contains("params.get('print') === '1'"));
        assert!(html.contains("window.print()"));
        assert!(html.contains("curl -X POST"));
        // Verify zero trace of HUD pill in compiled HTML
        assert!(!html.contains("mcp-agent-hud"));
        assert!(!html.contains("__mcp_agent_hud_host__"));
    }
}
