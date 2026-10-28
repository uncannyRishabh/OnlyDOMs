# Agent Workflows & Prompt Catalog

This guide provides real-world, high-leverage workflows and copy-paste prompt templates for AI coding agents (Claude, Cursor, Windsurf, Antigravity) using **OnlyDOMs**.

---

## 1. Compound Turn Batching: The 80% Token Rule

Conventional browser automation tools require an agent to make 5 to 10 sequential tool calls to complete a single user journey:
1. `create_tab` $\rightarrow$ Wait for agent turn
2. `navigate` $\rightarrow$ Wait for agent turn
3. `wait_for` $\rightarrow$ Wait for agent turn
4. `fill_inputs` $\rightarrow$ Wait for agent turn
5. `click` $\rightarrow$ Wait for agent turn

With OnlyDOMs, agents use `execute_browser_pipeline` to batch the entire operation into a single compound turn:

```json
{
  "name": "execute_browser_pipeline",
  "arguments": {
    "steps": [
      { "action": "navigate", "url": "https://github.com/pulls" },
      { "action": "wait_for", "selector": "input#js-issues-search" },
      { "action": "fill_inputs", "fields": [{ "selector": "input#js-issues-search", "value": "is:pr is:open review:required" }] },
      { "action": "press_key", "key": "Enter" },
      { "action": "wait_network_idle", "timeout_ms": 4000 }
    ]
  }
}
```
**Outcome**: 1 LLM inference turn instead of 5, zero context bloat, ~80% token savings, and sub-second execution.

---

## 2. SDLC & Web Development Workflows

### Scenario A: Local Dev Server Visual QA
Prompt for your coding agent:
```
Verify that our latest CSS updates on http://localhost:3000 did not break the responsive navigation bar:
1. Open http://localhost:3000 in a new tab.
2. Verify the navbar branding and links are visible.
3. Resize/scroll down 600px and confirm sticky header styles apply.
4. Take a screenshot saved to test_reports/navbar_qa.png.
```

### Scenario B: Automated Form Submission Verification
Prompt for your coding agent:
```
Test the signup form on our staging environment:
1. Navigate to https://staging.myapp.com/signup.
2. Fill input[name="email"] with a random test address and input[name="password"] with a valid password.
3. Click the 'Sign Up' button.
4. Wait for redirect to /dashboard and assert the welcome banner is visible.
```

---

## 3. Production Monitoring & DevOps Workflows

### Scenario A: Datadog / Grafana Alert Triage
```
Check our production latency spikes in Datadog:
1. Find the active Datadog tab in Chrome.
2. Filter the APM service graph for 'api-gateway' over the past 15 minutes.
3. Extract the top 3 endpoints with the highest p99 latency using evaluate_expression.
4. Report back the impacted route names and error rates.
```

### Scenario B: AWS CloudWatch Log Extraction
```
Navigate to our AWS Console tab, filter CloudWatch Log Group '/aws/lambda/checkout-service' for errors in the last 1 hour, and extract the latest 5 stack traces.
```

---

## 4. Social & Content Automation Workflows

OnlyDOMs uses your real Chrome session with active logins, avoiding bot captchas and 2FA locks:

### LinkedIn Outreach & Profile Inspection
```
1. Open LinkedIn in a fresh tab.
2. Search for "VP of Engineering" in San Francisco.
3. Filter by People.
4. Scroll the search results twice and extract the top 5 names, titles, and company affiliations into a clean markdown table.
```

### GitHub PR Review & Discussion
```
1. Open PR #42 on our private repository.
2. Check the 'Files changed' tab.
3. Scroll through modified files and evaluate if any secrets or debug logging statements were accidentally committed.
```

---

## 5. Terminal CLI Shortcuts (Zero-Agent Mode)

When interacting with the browser directly from your terminal or shell scripts without an active LLM:

```bash
# Open URL in a clean tab
onlydoms open "https://news.ycombinator.com"

# List active tabs and locks
onlydoms tabs

# Evaluate JavaScript on the active tab
onlydoms eval "document.title"

# Evaluate JavaScript on a specific tab
onlydoms eval "window.location.href" --tab-id 12345

# Check daemon health & extension status
onlydoms status

# Run a compound pipeline directly from bash
onlydoms pipeline '[{"action":"navigate","url":"https://example.com"},{"action":"wait_for","selector":"h1"}]'
```
