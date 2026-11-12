const CANDIDATE_PORTS = [9222, 9922, 9223, 9923, 9224, 9924];
let currentPortIndex = 0;
let reconnectDelayMs = 2000;
const MAX_RECONNECT_DELAY_MS = 15000;
let currentWs = null;
let reconnectTimer = null;
const attachedTabs = new Set();

// Master Specification § 1.B: Global Task & Network Store
// Map<tabId, AgentTaskState> in memory, backed by chrome.storage.session
const taskStore = new Map();
const completionTimers = new Map();
const hudPorts = new Map(); // Map<Port, tabId>
const pendingNetworkRequests = new Map(); // Map<requestId, partialReq>
const latestStepByTab = new Map(); // Map<tabId, { rpcId, timestamp }>

const MAX_LOGS_PER_TAB = 25;
const MAX_NETWORK_CALLS_PER_TAB = 50;
const IDLE_COMPLETE_HOLD_MS = 30000;
const NETWORK_CORRELATION_WINDOW_MS = 3000;

const pinnedTabs = new Set();

let activeInteractingTabId = null;
let activeInteractingTabTitle = "";

// Restore task store from session storage upon service worker wake
chrome.storage.session.get(["hudTaskStore"], (res) => {
    if (res?.hudTaskStore && typeof res.hudTaskStore === "object") {
        const now = Date.now();
        for (const [k, val] of Object.entries(res.hudTaskStore)) {
            const tid = Number(k);
            // Only restore tasks updated within the last 10 seconds that are still running
            if (tid && val && now - (val.updatedAt || 0) < 10000 && val.status === "running") {
                taskStore.set(tid, val);
            }
        }
    }
});

function persistTaskStore() {
    const obj = {};
    for (const [tid, state] of taskStore.entries()) {
        obj[tid] = state;
    }
    chrome.storage.session.set({ hudTaskStore: obj }).catch(() => {});
}

// Site Blacklist In-Memory Cache (Feature F9 / Requirement R5)
let blockedPatterns = [];

async function loadBlockedPatterns() {
    try {
        const res = await chrome.storage.local.get("blockedDomains");
        blockedPatterns = Array.isArray(res?.blockedDomains) ? res.blockedDomains : [];
    } catch (_) {
        blockedPatterns = [];
    }
}

loadBlockedPatterns();

chrome.storage.onChanged.addListener(async (changes, areaName) => {
    if (areaName === "local" && changes.blockedDomains) {
        blockedPatterns = Array.isArray(changes.blockedDomains.newValue)
            ? changes.blockedDomains.newValue
            : [];
        await auditAttachedTabsAgainstBlacklist();
    }
});

async function auditAttachedTabsAgainstBlacklist() {
    for (const tabId of Array.from(attachedTabs)) {
        try {
            const tab = await chrome.tabs.get(tabId);
            const url = tab.url || tab.pendingUrl || "";
            if (isUrlBlocked(url, blockedPatterns)) {
                try { await chrome.debugger.detach({ tabId }); } catch (_) {}
                attachedTabs.delete(tabId);
                removeTaskForTab(tabId);
            }
        } catch (_) {}
    }
}

function extractHostAndPath(rawUrl) {
    if (!rawUrl || typeof rawUrl !== "string") {
        return { host: "", path: "" };
    }
    const trimmed = rawUrl.trim();
    const schemeIdx = trimmed.indexOf("://");
    const withoutScheme = schemeIdx !== -1 ? trimmed.slice(schemeIdx + 3) : trimmed;

    const slashIdx = withoutScheme.indexOf("/");
    const hostPart = slashIdx !== -1 ? withoutScheme.slice(0, slashIdx) : withoutScheme;
    const pathPart = slashIdx !== -1 ? withoutScheme.slice(slashIdx) : "";

    const colonIdx = hostPart.indexOf(":");
    const hostClean = colonIdx !== -1 ? hostPart.slice(0, colonIdx) : hostPart;

    return {
        host: hostClean.toLowerCase(),
        path: pathPart
    };
}

function isUrlBlocked(rawUrl, patterns = blockedPatterns) {
    if (!rawUrl || typeof rawUrl !== "string") return false;
    const trimmed = rawUrl.trim();
    if (!trimmed) return false;

    if (
        trimmed === "about:blank" ||
        trimmed.startsWith("about:") ||
        trimmed.startsWith("chrome://") ||
        trimmed.startsWith("chrome-extension://") ||
        trimmed.startsWith("devtools://") ||
        trimmed.startsWith("view-source:")
    ) {
        return false;
    }

    if (trimmed === "://" || trimmed === "http:///" || trimmed === "https:///") {
        return false;
    }

    if (!Array.isArray(patterns) || patterns.length === 0) {
        return false;
    }

    const { host } = extractHostAndPath(trimmed);
    const fullLower = trimmed.toLowerCase();

    for (const rawPattern of patterns) {
        if (!rawPattern || typeof rawPattern !== "string") continue;
        const pattern = rawPattern.trim().toLowerCase();
        if (!pattern) continue;

        if (pattern.startsWith("*.")) {
            const suffix = pattern.slice(2);
            if (host === suffix || host.endsWith("." + suffix)) {
                return true;
            }
        } else if (host === pattern || host.endsWith("." + pattern)) {
            return true;
        } else if (fullLower.startsWith(pattern) || fullLower.includes(pattern)) {
            return true;
        }
    }

    return false;
}

function isScriptableTabUrl(url) {
    if (!url || typeof url !== "string") return false;
    return !(
        url.startsWith("chrome://") ||
        url.startsWith("chrome-extension://") ||
        url.startsWith("edge://") ||
        url.startsWith("devtools://") ||
        url.startsWith("view-source:") ||
        url.startsWith("about:")
    );
}

// ---------------------------------------------------------------------------
// Cross-Tab Floating Pill HUD Synchronization (§ 1.B, § 2, § 3)
// ---------------------------------------------------------------------------

function getActiveTasksArray() {
    return Array.from(taskStore.values()).sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0));
}

async function ensureHudInjectedInTab(tabId) {
    if (!tabId) return;
    try {
        const tab = await chrome.tabs.get(tabId);
        if (!tab || !isScriptableTabUrl(tab.url)) return;

        await chrome.scripting.executeScript({
            target: { tabId },
            files: ["content/content_tree.js", "content/content_hud.js"]
        });
    } catch (_) {}
}

async function broadcastHudStateToAllTabs() {
    const tasks = getActiveTasksArray();

    // 1. Broadcast over all connected persistent `mcp-hud-port` channels
    for (const [port, portTabId] of hudPorts.entries()) {
        try {
            port.postMessage({
                type: "HUD_SYNC",
                tasks,
                viewerTabId: portTabId
            });
        } catch (_) {
            hudPorts.delete(port);
        }
    }

    // 2. Also hydrate the user's currently focused tab in each window (and target tabs)
    //    even if the tab was opened before the extension loaded
    try {
        const activeTabs = await chrome.tabs.query({ active: true });
        for (const tab of activeTabs) {
            if (!tab.id || !isScriptableTabUrl(tab.url)) continue;
            try {
                await chrome.tabs.sendMessage(tab.id, {
                    type: "HUD_SYNC",
                    tasks,
                    viewerTabId: tab.id
                });
            } catch (_) {
                if (tasks.length > 0) {
                    await ensureHudInjectedInTab(tab.id);
                    try {
                        await chrome.tabs.sendMessage(tab.id, {
                            type: "HUD_SYNC",
                            tasks,
                            viewerTabId: tab.id
                        });
                    } catch (_) {}
                }
            }
        }
    } catch (_) {}
}

function scheduleTaskCompletionCleanup(tabId, holdMs = IDLE_COMPLETE_HOLD_MS) {
    if (pinnedTabs.has(Number(tabId))) {
        return;
    }
    if (completionTimers.has(tabId)) {
        clearTimeout(completionTimers.get(tabId));
    }
    const timer = setTimeout(() => {
        completionTimers.delete(tabId);
        if (pinnedTabs.has(Number(tabId))) return;
        const state = taskStore.get(tabId);
        if (state && (state.status === "completed" || state.status === "error")) {
            taskStore.delete(tabId);
            persistTaskStore();
            broadcastHudStateToAllTabs();
        }
    }, holdMs);
    completionTimers.set(tabId, timer);
}

async function upsertAgentTaskState({
    tabId,
    tabTitle,
    intent,
    executingCmd,
    status = "running",
    rpcId = null,
    toolName = null,
    latencyMs = null,
    requestData = null,
    responseData = null
}) {
    const numericTabId = Number(tabId);
    if (!numericTabId || isNaN(numericTabId)) return;

    if (completionTimers.has(numericTabId)) {
        clearTimeout(completionTimers.get(numericTabId));
        completionTimers.delete(numericTabId);
    }

    let resolvedTitle = tabTitle;
    let faviconUrl = null;
    try {
        const tab = await chrome.tabs.get(numericTabId);
        if (tab) {
            if (!resolvedTitle || resolvedTitle === "Active Tab" || resolvedTitle === "Untitled Tab") {
                resolvedTitle = (tab.title && tab.title.trim()) ? tab.title.trim() : "Active Tab";
            }
            faviconUrl = tab.favIconUrl || null;
        }
    } catch (_) {}

    if (!resolvedTitle || !resolvedTitle.trim()) {
        resolvedTitle = "Active Tab";
    }

    const now = Date.now();
    const existing = taskStore.get(numericTabId) || {
        tabId: numericTabId,
        tabTitle: resolvedTitle,
        faviconUrl,
        intent: "",
        executingCmd: "",
        status: "intent_preview",
        updatedAt: now,
        logs: [],
        networkCalls: []
    };

    existing.tabTitle = resolvedTitle;
    if (faviconUrl) existing.faviconUrl = faviconUrl;
    if (intent && intent.trim()) {
        existing.intent = intent.trim().slice(0, 180);
    } else if (!existing.intent) {
        existing.intent = `Automating Tab #${numericTabId} (${resolvedTitle})`.slice(0, 180);
    }

    if (executingCmd && executingCmd.trim()) {
        existing.executingCmd = executingCmd
            .trim()
            .replace(/\(Active Tab\)$/, `(${resolvedTitle})`);
    } else if (!existing.executingCmd) {
        existing.executingCmd = `Executing :: browser action on Tab :: #${numericTabId} (${resolvedTitle})`;
    }

    existing.status = status;
    existing.updatedAt = now;

    if (rpcId || toolName) {
        const existingLog = existing.logs.find(l => l.rpcId === rpcId);
        if (existingLog) {
            existingLog.status = status;
            if (latencyMs != null) existingLog.latencyMs = latencyMs;
            if (requestData != null) existingLog.requestData = requestData;
            if (responseData != null) existingLog.responseData = responseData;
        } else {
            existing.logs.push({
                rpcId: rpcId || `rpc_${now}`,
                toolName: toolName || "cdp_cmd",
                intent: existing.intent,
                executingCmd: existing.executingCmd,
                status,
                latencyMs,
                timestamp: now,
                requestData: requestData || null,
                responseData: responseData || null
            });
            if (existing.logs.length > MAX_LOGS_PER_TAB) {
                existing.logs.shift();
            }
        }
        latestStepByTab.set(numericTabId, {
            rpcId: rpcId || `rpc_${now}`,
            toolName: toolName || "cdp_cmd",
            intent: existing.intent || null,
            timestamp: now
        });
    }

    taskStore.set(numericTabId, existing);
    activeInteractingTabId = numericTabId;
    activeInteractingTabTitle = resolvedTitle;
    persistTaskStore();

    await broadcastHudStateToAllTabs();

    if (status === "completed" || status === "error") {
        scheduleTaskCompletionCleanup(numericTabId, IDLE_COMPLETE_HOLD_MS);
    }
}

function removeTaskForTab(tabId) {
    const numericTabId = Number(tabId);
    if (completionTimers.has(numericTabId)) {
        clearTimeout(completionTimers.get(numericTabId));
        completionTimers.delete(numericTabId);
    }
    taskStore.delete(numericTabId);
    persistTaskStore();
    broadcastHudStateToAllTabs();
}

async function clearAllBanners() {
    for (const timer of completionTimers.values()) {
        clearTimeout(timer);
    }
    completionTimers.clear();
    taskStore.clear();
    activeInteractingTabId = null;
    activeInteractingTabTitle = "";
    persistTaskStore();
    await broadcastHudStateToAllTabs();
}

// Helper to describe raw CDP commands when no explicit HUD_STATE_UPDATE was sent prior
function summarizeCdpCommand(method, params) {
    switch (method) {
        case "Page.navigate":
            return `navigate(${(params?.url || "").slice(0, 38)})`;
        case "Runtime.evaluate":
            return "evaluate_expression(js)";
        case "Input.dispatchMouseEvent":
            return `mouse_${params?.type || "click"}(${Math.round(params?.x || 0)}, ${Math.round(params?.y || 0)})`;
        case "Input.dispatchKeyEvent":
            return `press_key(${params?.key || "Key"})`;
        case "Page.captureScreenshot":
            return `take_screenshot(${params?.format || "webp"})`;
        case "Page.printToPDF":
            return "print_to_pdf(pdf)";
        case "Page.handleJavaScriptDialog":
            return `handle_dialog(${params?.accept ? "accept" : "dismiss"})`;
        default:
            return method || "command";
    }
}

async function setHudCloakOnTab(tabId, cloak) {
    if (!tabId) return;
    try {
        await chrome.tabs.sendMessage(tabId, {
            type: cloak ? "CLOAK_HUD" : "UNCLOAK_HUD",
            cloak: !!cloak
        });
        if (cloak) {
            await new Promise((r) => setTimeout(r, 60));
        }
    } catch (_) {}
}

// Shell-safe single-quote cURL builder in extension for immediate HUD drawer display
function escapeSingleQuotes(str) {
    return `'${String(str || "").replace(/'/g, "'\\''")}'`;
}

function buildCurlString(method, url, headers, postData) {
    const parts = ["curl", "-X", (method || "GET").toUpperCase(), escapeSingleQuotes(url)];
    if (headers && typeof headers === "object") {
        for (const [k, v] of Object.entries(headers)) {
            parts.push("-H", escapeSingleQuotes(`${k}: ${v}`));
        }
    }
    if (postData) {
        parts.push("--data-raw", escapeSingleQuotes(postData));
    }
    return parts.join(" ");
}

function scheduleReconnect() {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(() => {
        connect();
    }, reconnectDelayMs);
    reconnectDelayMs = Math.min(Math.round(reconnectDelayMs * 1.5), MAX_RECONNECT_DELAY_MS);
}

/**
 * Gate 1: Resolves target tab, checks blacklist policy, attaches debugger + Network.enable
 */
async function resolveTargetTab(requestedTabId) {
    let tabId = Number(requestedTabId);
    if (!tabId || isNaN(tabId)) {
        const [activeTab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
        if (activeTab) {
            tabId = activeTab.id;
        } else {
            const tabs = await chrome.tabs.query({});
            if (tabs.length > 0) {
                tabId = tabs[0].id;
            }
        }
    }
    if (!tabId) {
        throw new Error("No browser tab available");
    }

    const targetTab = await chrome.tabs.get(tabId);
    const targetUrl = targetTab.url || targetTab.pendingUrl || "";
    if (isUrlBlocked(targetUrl, blockedPatterns)) {
        throw new Error(`BLOCKED_BY_SITE_POLICY: Access to '${targetUrl}' is restricted by site blacklist policy.`);
    }

    if (!attachedTabs.has(tabId)) {
        try {
            await chrome.debugger.attach({ tabId }, "1.3");
        } catch (e) {
            const msg = e?.message || "";
            if (!msg.includes("Already attached") && !msg.includes("Another debugger")) {
                throw e;
            }
        }
        // § 4.A.1: Enable Network.enable({ maxPostDataSize: 65536 }) alongside Page, Runtime, DOM, Input
        await Promise.allSettled([
            chrome.debugger.sendCommand({ tabId }, "Page.enable", {}),
            chrome.debugger.sendCommand({ tabId }, "Runtime.enable", {}),
            chrome.debugger.sendCommand({ tabId }, "DOM.enable", {}),
            chrome.debugger.sendCommand({ tabId }, "Input.enable", {}),
            chrome.debugger.sendCommand({ tabId }, "Network.enable", { maxPostDataSize: 65536 })
        ]);
        attachedTabs.add(tabId);
    }

    return tabId;
}

function connect(overridePort) {
    if (currentWs && (currentWs.readyState === WebSocket.OPEN || currentWs.readyState === WebSocket.CONNECTING)) {
        return;
    }

    chrome.storage.session.get(["wsUrl", "preferredPort"], async (res) => {
        let port = overridePort || res.preferredPort;
        if (!port) {
            port = CANDIDATE_PORTS[currentPortIndex % CANDIDATE_PORTS.length];
        }
        const url = `ws://127.0.0.1:${port}/mcp-relay`;

        let ws;
        try {
            ws = new WebSocket(url);
        } catch (_) {
            currentPortIndex++;
            scheduleReconnect();
            return;
        }

        currentWs = ws;

        const connectTimeout = setTimeout(() => {
            if (ws && ws.readyState !== WebSocket.OPEN) {
                try { ws.close(); } catch (_) {}
            }
        }, 3000);

        ws.onopen = () => {
            clearTimeout(connectTimeout);
            chrome.storage.session.set({ connected: true, activePort: port, browserWide: true });
            reconnectDelayMs = 2000;
        };

        ws.onmessage = async (event) => {
            let data;
            try {
                data = JSON.parse(event.data);
            } catch (_) {
                return;
            }

            if (data.type === "PING") {
                try { ws.send(JSON.stringify({ id: data.id, result: "pong" })); } catch (_) {}
                return;
            }

            // § 3.A.2: Pre-Flight & Step-by-Step HUD_STATE_UPDATE from Rust Daemon
            if (data.type === "HUD_STATE_UPDATE") {
                const tid = Number(data.tab_id || data.tabId || activeInteractingTabId || 0);
                if (tid > 0) {
                    await upsertAgentTaskState({
                        tabId: tid,
                        intent: data.intent,
                        executingCmd: data.executing_cmd || data.executingCmd,
                        status: data.status || "running",
                        rpcId: data.rpc_id || `hud_${Date.now()}`,
                        toolName: data.tool_name || data.toolName || "mcp_tool",
                        latencyMs: data.latency_ms || data.latencyMs || null,
                        requestData: data.request_data || null,
                        responseData: data.response_data || null
                    });
                }
                return;
            }

            // § 6.B.1: Synchronous HUD Cloaking before screenshots & PDF exports
            if (data.type === "CLOAK_HUD" || data.type === "UNCLOAK_HUD") {
                const shouldCloak = data.type === "CLOAK_HUD" && data.cloak !== false;
                const tid = Number(data.tabId || activeInteractingTabId || 0);
                if (tid > 0) {
                    await setHudCloakOnTab(tid, shouldCloak);
                }
                ws.send(JSON.stringify({ id: data.id, result: { success: true, cloaked: shouldCloak } }));
                return;
            }

            if (data.type === "CDP_CMD") {
                try {
                    if (data.method === "Page.navigate" && data.params?.url) {
                        const navUrl = data.params.url;
                        if (isUrlBlocked(navUrl, blockedPatterns)) {
                            ws.send(JSON.stringify({
                                id: data.id,
                                error: `BLOCKED_BY_SITE_POLICY: Access to '${navUrl}' is restricted by site blacklist policy.`
                            }));
                            return;
                        }
                    }

                    const targetTabId = await resolveTargetTab(data.tabId);
                    const currentTab = await chrome.tabs.get(targetTabId);
                    const currentUrl = currentTab.url || currentTab.pendingUrl || "";
                    if (isUrlBlocked(currentUrl, blockedPatterns)) {
                        ws.send(JSON.stringify({
                            id: data.id,
                            error: `BLOCKED_BY_SITE_POLICY: Access to '${currentUrl}' is restricted by site blacklist policy.`
                        }));
                        return;
                    }

                    const tabTitle = currentTab.title || "Active Tab";
                    const isVisualCapture = data.method === "Page.captureScreenshot" || data.method === "Page.printToPDF";

                    // Automatically cloak HUD during screenshots / PDFs so zero HUD trace ever leaks
                    if (isVisualCapture) {
                        await setHudCloakOnTab(targetTabId, true);
                    } else {
                        const cmdSummary = summarizeCdpCommand(data.method, data.params);
                        const existingTask = taskStore.get(targetTabId);
                        await upsertAgentTaskState({
                            tabId: targetTabId,
                            tabTitle,
                            intent: existingTask?.intent || `Running ${cmdSummary} on ${tabTitle}`,
                            executingCmd: `Executing :: ${cmdSummary} on Tab :: #${targetTabId} (${tabTitle})`,
                            status: "running",
                            rpcId: data.id,
                            toolName: data.method,
                            requestData: data.params || null
                        });
                    }

                    const res = await chrome.debugger.sendCommand(
                        { tabId: targetTabId },
                        data.method,
                        data.params
                    );

                    if (isVisualCapture) {
                        await setHudCloakOnTab(targetTabId, false);
                    } else {
                        await upsertAgentTaskState({
                            tabId: targetTabId,
                            rpcId: data.id,
                            toolName: data.method,
                            status: "completed",
                            responseData: res || null
                        });
                    }

                    // Schedule automatic transition to completed -> auto-hide if no subsequent command arrives within 600ms
                    scheduleTaskCompletionCleanup(targetTabId, IDLE_COMPLETE_HOLD_MS);
                    const taskAfter = taskStore.get(targetTabId);
                    if (taskAfter) {
                        setTimeout(() => {
                            const current = taskStore.get(targetTabId);
                            if (current && Date.now() - current.updatedAt >= 450 && current.status === "running") {
                                upsertAgentTaskState({
                                    tabId: targetTabId,
                                    tabTitle: current.tabTitle,
                                    intent: current.intent,
                                    executingCmd: current.executingCmd,
                                    status: "completed"
                                });
                            }
                        }, 500);
                    }

                    ws.send(JSON.stringify({ id: data.id, result: res }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "INJECT_SCRIPT") {
                try {
                    const targetTabId = await resolveTargetTab(data.tabId);
                    await upsertAgentTaskState({
                        tabId: targetTabId,
                        executingCmd: `Executing :: inject(${data.file}) on Tab :: #${targetTabId} (Active Tab)`,
                        status: "running",
                        rpcId: data.id,
                        toolName: "INJECT_SCRIPT"
                    });
                    await chrome.scripting.executeScript({
                        target: { tabId: targetTabId },
                        files: [data.file],
                        world: "MAIN"
                    });
                    ws.send(JSON.stringify({ id: data.id, result: { success: true } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "LIST_TABS") {
                try {
                    const tabs = await chrome.tabs.query({});
                    const cleanUrl = (rawUrl) => {
                        if (!rawUrl) return "";
                        try {
                            const u = new URL(rawUrl);
                            if (u.protocol === "http:" || u.protocol === "https:") {
                                let path = u.pathname;
                                if (path === "/") path = "";
                                return `${u.host}${path}`;
                            }
                            return `${u.protocol}//${u.host || ""}${u.pathname}`;
                        } catch (_) {
                            return rawUrl.replace(/^https?:\/\//i, "").split(/[?#]/)[0];
                        }
                    };
                    const resultTabs = data.detailed ? tabs : tabs.map((t) => ({
                        id: t.id,
                        title: t.title || "",
                        url: cleanUrl(t.url),
                        active: !!t.active,
                        windowId: t.windowId,
                        openerTabId: t.openerTabId || null
                    }));
                    ws.send(JSON.stringify({ id: data.id, result: { tabs: resultTabs } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "CREATE_TAB") {
                try {
                    const url = data.url || "about:blank";
                    if (isUrlBlocked(url, blockedPatterns)) {
                        ws.send(JSON.stringify({
                            id: data.id,
                            error: `BLOCKED_BY_SITE_POLICY: Access to '${url}' is restricted by site blacklist policy.`
                        }));
                        return;
                    }

                    const activeFlag = data.active !== undefined ? !!data.active : true;
                    const newTab = await chrome.tabs.create({ url, active: activeFlag });
                    try {
                        await resolveTargetTab(newTab.id);
                        await upsertAgentTaskState({
                            tabId: newTab.id,
                            tabTitle: newTab.title || url,
                            intent: `Open ${url} in new tab #${newTab.id}`,
                            executingCmd: `Executing :: create_tab(${url.slice(0, 36)}) on Tab :: #${newTab.id} (${newTab.title || "New Tab"})`,
                            status: "completed",
                            rpcId: data.id,
                            toolName: "create_tab"
                        });
                    } catch (attachErr) {
                        const msg = attachErr?.message || "";
                        if (!msg.includes("Cannot access") && !msg.includes("chrome://")) {
                            throw attachErr;
                        }
                    }
                    ws.send(JSON.stringify({ id: data.id, result: { success: true, tabId: newTab.id } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "SWITCH_TAB") {
                try {
                    const tabId = Number(data.tabId);
                    await chrome.tabs.update(tabId, { active: true });
                    try {
                        await resolveTargetTab(tabId);
                        await upsertAgentTaskState({
                            tabId,
                            intent: `Switch agent focus to Tab #${tabId}`,
                            executingCmd: `Executing :: switch_tab(#${tabId}) on Tab :: #${tabId} (Active Tab)`,
                            status: "completed",
                            rpcId: data.id,
                            toolName: "switch_tab"
                        });
                    } catch (attachErr) {
                        const msg = attachErr?.message || "";
                        if (!msg.includes("Cannot access") && !msg.includes("chrome://")) {
                            throw attachErr;
                        }
                    }
                    ws.send(JSON.stringify({ id: data.id, result: { success: true, tabId } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "NAVIGATE_TAB") {
                try {
                    const tabId = Number(data.tabId);
                    const url = data.url || "";
                    if (isUrlBlocked(url, blockedPatterns)) {
                        ws.send(JSON.stringify({
                            id: data.id,
                            error: `BLOCKED_BY_SITE_POLICY: Access to '${url}' is restricted by site blacklist policy.`
                        }));
                        return;
                    }

                    await chrome.tabs.update(tabId, { url });
                    await upsertAgentTaskState({
                        tabId,
                        intent: `Navigate Tab #${tabId} to ${url}`,
                        executingCmd: `Executing :: navigate(${url.slice(0, 36)}) on Tab :: #${tabId} (Active Tab)`,
                        status: "completed",
                        rpcId: data.id,
                        toolName: "navigate"
                    });
                    setTimeout(async () => {
                        try { await resolveTargetTab(tabId); } catch (_) {}
                    }, 500);
                    ws.send(JSON.stringify({ id: data.id, result: { success: true, tabId, url } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "CLOSE_TAB") {
                try {
                    const tabId = Number(data.tabId);
                    await chrome.tabs.remove(tabId);
                    attachedTabs.delete(tabId);
                    removeTaskForTab(tabId);
                    ws.send(JSON.stringify({ id: data.id, result: { success: true } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "ATTACH_TAB") {
                try {
                    const tabId = await resolveTargetTab(data.tabId);
                    await chrome.storage.session.set({ activeSession: tabId });
                    await upsertAgentTaskState({
                        tabId,
                        intent: `Attached debugger to Tab #${tabId}`,
                        executingCmd: `Executing :: select_session(#${tabId}) on Tab :: #${tabId} (Active Tab)`,
                        status: "completed"
                    });
                    ws.send(JSON.stringify({ id: data.id, result: { success: true, tabId } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "DETACH_TAB") {
                try {
                    const tabId = Number(data.tabId);
                    if (tabId && attachedTabs.has(tabId)) {
                        try { await chrome.debugger.detach({ tabId }); } catch (_) {}
                        attachedTabs.delete(tabId);
                    }
                    removeTaskForTab(tabId);
                    ws.send(JSON.stringify({ id: data.id, result: { success: true } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "RELOAD_EXTENSION") {
                ws.send(JSON.stringify({ id: data.id, result: { success: true } }));
                setTimeout(() => chrome.runtime.reload(), 100);
            }
        };

        ws.onclose = () => {
            clearTimeout(connectTimeout);
            currentWs = null;
            chrome.storage.session.set({ connected: false });
            clearAllBanners();
            if (!overridePort && !res.preferredPort) {
                currentPortIndex++;
            }
            scheduleReconnect();
        };

        ws.onerror = () => {
            try { ws.close(); } catch (_) {}
        };
    });
}

// Persistent HUD Port Listener (`mcp-hud-port`) for instant cross-tab hydration
chrome.runtime.onConnect.addListener((port) => {
    if (port.name !== "mcp-hud-port") return;
    const senderTabId = port.sender?.tab?.id || null;
    hudPorts.set(port, senderTabId);

    // Immediately hydrate newly connected tab content script
    try {
        port.postMessage({
            type: "HUD_SYNC",
            tasks: getActiveTasksArray(),
            viewerTabId: senderTabId
        });
    } catch (_) {}

    port.onDisconnect.addListener(() => {
        hudPorts.delete(port);
    });
});

// § 1.B.2: Instant Tab-Switch Hydration whenever the user switches tabs
chrome.tabs.onActivated.addListener(async (activeInfo) => {
    const tasks = getActiveTasksArray();
    if (tasks.length === 0) return;

    await ensureHudInjectedInTab(activeInfo.tabId);
    try {
        await chrome.tabs.sendMessage(activeInfo.tabId, {
            type: "HUD_SYNC",
            tasks,
            viewerTabId: activeInfo.tabId
        });
    } catch (_) {}
});

// Clean up any legacy warning banner across existing open tabs on install/startup
async function cleanupLegacyBannersAcrossAllTabs() {
    try {
        const tabs = await chrome.tabs.query({});
        for (const tab of tabs) {
            if (!tab.id || !isScriptableTabUrl(tab.url)) continue;
            try {
                await chrome.scripting.executeScript({
                    target: { tabId: tab.id },
                    files: ["content/content_tree.js", "content/content_hud.js"]
                });
            } catch (_) {}
        }
    } catch (_) {}
}

chrome.runtime.onInstalled.addListener(() => {
    chrome.storage.session.set({ activeSession: null, connected: false, browserWide: true });
    loadBlockedPatterns();
    cleanupLegacyBannersAcrossAllTabs();
    connect();
});

cleanupLegacyBannersAcrossAllTabs();

chrome.tabs.onUpdated.addListener(async (tabId, changeInfo, tab) => {
    const newUrl = changeInfo.url || tab.url || tab.pendingUrl;

    if (newUrl && attachedTabs.has(tabId) && isUrlBlocked(newUrl, blockedPatterns)) {
        try { await chrome.debugger.detach({ tabId }); } catch (_) {}
        attachedTabs.delete(tabId);
        removeTaskForTab(tabId);
        if (currentWs && currentWs.readyState === WebSocket.OPEN) {
            try {
                currentWs.send(JSON.stringify({
                    type: "EVENT",
                    event: "TAB_BLOCKED_DETACHED",
                    tabId,
                    url: newUrl,
                    error: `BLOCKED_BY_SITE_POLICY: Access to '${newUrl}' is restricted by site blacklist policy.`
                }));
            } catch (_) {}
        }
        return;
    }

    if (taskStore.has(tabId) && changeInfo.title) {
        const existing = taskStore.get(tabId);
        if (existing) {
            existing.tabTitle = changeInfo.title;
            taskStore.set(tabId, existing);
            persistTaskStore();
            await broadcastHudStateToAllTabs();
        }
    }
});

// § 4.A: Optimistic CDP Network Interception & Dialog Event Forwarder
chrome.debugger.onEvent.addListener((source, method, params) => {
    const tabId = source.tabId;
    if (!tabId) return;

    if (method === "Page.javascriptDialogOpening" || method === "Page.javascriptDialogClosed") {
        if (currentWs && currentWs.readyState === WebSocket.OPEN) {
            try {
                currentWs.send(JSON.stringify({
                    type: "EVENT",
                    event: method,
                    tabId,
                    payload: params || {}
                }));
            } catch (_) {}
        }
        return;
    }

    // § 4.A.2 & 4.A.3: Capture only XHR, Fetch, and Document requests within 3000ms of an agent step
    if (method === "Network.requestWillBeSent") {
        const resourceType = params?.type || "Fetch";
        const lowerType = String(resourceType).toLowerCase();
        if (lowerType !== "xhr" && lowerType !== "fetch" && lowerType !== "document") {
            return;
        }
        const now = Date.now();
        const latestStep = latestStepByTab.get(tabId);
        const isCorrelated = latestStep && now - latestStep.timestamp <= NETWORK_CORRELATION_WINDOW_MS;
        const correlatedRpcId = isCorrelated ? latestStep.rpcId : null;
        const correlatedToolName = isCorrelated ? (latestStep.toolName || null) : null;
        const correlatedIntent = isCorrelated ? (latestStep.intent || null) : null;

        const reqObj = params?.request || {};
        const requestId = params?.requestId || `req_${now}`;
        const url = reqObj.url || "";
        const httpMethod = (reqObj.method || "GET").toUpperCase();
        const headers = reqObj.headers || {};
        const postData = reqObj.postData ? String(reqObj.postData).slice(0, 65536) : null;
        const curlCommand = buildCurlString(httpMethod, url, headers, postData);

        const captured = {
            requestId,
            tabId,
            rpcId: correlatedRpcId,
            toolName: correlatedToolName,
            intent: correlatedIntent,
            resourceType,
            url,
            method: httpMethod,
            headers,
            postData,
            status: null,
            responseHeaders: null,
            responseBody: null,
            timestamp: now,
            curlCommand
        };
        pendingNetworkRequests.set(requestId, captured);
        if (pendingNetworkRequests.size > 150) {
            const oldestKey = pendingNetworkRequests.keys().next().value;
            pendingNetworkRequests.delete(oldestKey);
        }

        const task = taskStore.get(tabId);
        if (task) {
            task.networkCalls = Array.isArray(task.networkCalls) ? task.networkCalls : [];
            task.networkCalls.push(captured);
            if (task.networkCalls.length > MAX_NETWORK_CALLS_PER_TAB) {
                task.networkCalls.shift();
            }
            taskStore.set(tabId, task);
            broadcastHudStateToAllTabs();
        }

        if (currentWs && currentWs.readyState === WebSocket.OPEN) {
            try {
                currentWs.send(JSON.stringify({
                    type: "EVENT",
                    event: "NETWORK_CAPTURE",
                    tabId,
                    payload: {
                        requestId,
                        rpc_id: correlatedRpcId,
                        tool_name: correlatedToolName,
                        intent: correlatedIntent,
                        resourceType,
                        url,
                        method: httpMethod,
                        headers,
                        postData,
                        timestamp_ms: now
                    }
                }));
            } catch (_) {}
        }
    } else if (method === "Network.responseReceived") {
        const requestId = params?.requestId;
        const status = params?.response?.status || null;
        const responseHeaders = params?.response?.headers || {};
        if (requestId && pendingNetworkRequests.has(requestId)) {
            const cap = pendingNetworkRequests.get(requestId);
            cap.status = status;
            cap.responseHeaders = responseHeaders;

            // Attempt early response body extraction
            try {
                chrome.debugger.sendCommand({ tabId }, "Network.getResponseBody", { requestId }, (bodyRes) => {
                    if (!chrome.runtime.lastError && bodyRes && bodyRes.body) {
                        const bodyText = bodyRes.base64Encoded ? "[base64 binary]" : bodyRes.body;
                        cap.responseBody = bodyText.length > 102400 ? bodyText.slice(0, 102400) + "\n... (truncated)" : bodyText;
                        broadcastHudStateToAllTabs();
                    }
                });
            } catch (_) {}

            if (currentWs && currentWs.readyState === WebSocket.OPEN) {
                try {
                    currentWs.send(JSON.stringify({
                        type: "EVENT",
                        event: "Network.responseReceived",
                        tabId,
                        payload: {
                            requestId,
                            resourceType: cap.resourceType,
                            response: {
                                status,
                                headers: responseHeaders
                            }
                        }
                    }));
                } catch (_) {}
            }
            broadcastHudStateToAllTabs();
        }
    } else if (method === "Network.loadingFinished") {
        const requestId = params?.requestId;
        if (requestId && pendingNetworkRequests.has(requestId)) {
            const cap = pendingNetworkRequests.get(requestId);
            try {
                chrome.debugger.sendCommand({ tabId }, "Network.getResponseBody", { requestId }, (bodyRes) => {
                    if (!chrome.runtime.lastError && bodyRes && bodyRes.body) {
                        const bodyText = bodyRes.base64Encoded ? "[base64 binary]" : bodyRes.body;
                        cap.responseBody = bodyText.length > 102400 ? bodyText.slice(0, 102400) + "\n... (truncated)" : bodyText;

                        if (currentWs && currentWs.readyState === WebSocket.OPEN) {
                            try {
                                currentWs.send(JSON.stringify({
                                    type: "EVENT",
                                    event: "NETWORK_CAPTURE",
                                    tabId,
                                    payload: {
                                        requestId: cap.requestId,
                                        rpc_id: cap.rpcId,
                                        tool_name: cap.toolName,
                                        intent: cap.intent,
                                        resourceType: cap.resourceType,
                                        url: cap.url,
                                        method: cap.method,
                                        headers: cap.headers,
                                        postData: cap.postData,
                                        status: cap.status,
                                        responseHeaders: cap.responseHeaders,
                                        responseBody: cap.responseBody,
                                        timestamp_ms: cap.timestamp
                                    }
                                }));
                            } catch (_) {}
                        }
                        broadcastHudStateToAllTabs();
                    }
                    pendingNetworkRequests.delete(requestId);
                });
            } catch (_) {
                pendingNetworkRequests.delete(requestId);
            }
        }
    } else if (method === "Network.loadingFailed") {
        const requestId = params?.requestId;
        if (requestId && pendingNetworkRequests.has(requestId)) {
            pendingNetworkRequests.delete(requestId);
        }
    }
});

chrome.tabs.onCreated.addListener((tab) => {
    if (currentWs && currentWs.readyState === WebSocket.OPEN) {
        try {
            currentWs.send(JSON.stringify({
                type: "EVENT",
                event: "Target.targetCreated",
                tabId: tab.id,
                openerTabId: tab.openerTabId || null,
                url: tab.url || tab.pendingUrl || "",
                isPopup: !!tab.openerTabId
            }));
        } catch (_) {}
    }
});

chrome.debugger.onDetach.addListener((source) => {
    if (source.tabId) {
        attachedTabs.delete(source.tabId);
        removeTaskForTab(source.tabId);
    }
});

chrome.tabs.onRemoved.addListener((tabId) => {
    attachedTabs.delete(tabId);
    removeTaskForTab(tabId);
});

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
    if (msg.action === "hud_pin") {
        const tid = Number(msg.tabId);
        if (tid) {
            if (msg.pinned) {
                pinnedTabs.add(tid);
                if (completionTimers.has(tid)) {
                    clearTimeout(completionTimers.get(tid));
                    completionTimers.delete(tid);
                }
            } else {
                pinnedTabs.delete(tid);
                const task = taskStore.get(tid);
                const isDone = !task || task.status === "completed" || task.status === "error";
                scheduleTaskCompletionCleanup(tid, isDone ? 1500 : IDLE_COMPLETE_HOLD_MS);
            }
        }
        sendResponse({ success: true, pinned: tid ? pinnedTabs.has(tid) : false });
        return true;
    } else if (msg.action === "hud_dismiss") {
        const tid = Number(msg.tabId);
        if (tid) {
            pinnedTabs.delete(tid);
            removeTaskForTab(tid);
        }
        sendResponse({ success: true });
        return true;
    } else if (msg.action === "hud_focus_tab" && msg.tabId) {
        chrome.tabs.update(Number(msg.tabId), { active: true }).then(() => {
            sendResponse({ success: true });
        }).catch((e) => sendResponse({ success: false, error: e.message }));
        return true;
    } else if (msg.action === "hud_replay_curl") {
        if (currentWs && currentWs.readyState === WebSocket.OPEN) {
            const id = `replay_${Date.now()}`;
            currentWs.send(JSON.stringify({
                type: "REPLAY_CURL",
                id,
                tabId: msg.tabId,
                request_id: msg.requestId,
                curl: msg.curl
            }));
            sendResponse({ success: true, result: { status: "Dispatched" } });
        } else {
            sendResponse({ success: false, error: "Daemon disconnected" });
        }
        return true;
    } else if (msg.action === "attach" && msg.tabId) {
        (async () => {
            const tabId = Number(msg.tabId);
            if (!tabId || isNaN(tabId)) {
                sendResponse({ success: false, error: "Invalid tab ID" });
                return;
            }
            try {
                await resolveTargetTab(tabId);
                await chrome.storage.session.set({ activeSession: tabId });
                await upsertAgentTaskState({
                    tabId,
                    intent: `Attached to Tab #${tabId}`,
                    executingCmd: `Executing :: attach(#${tabId}) on Tab :: #${tabId} (Active Tab)`,
                    status: "completed"
                });
                sendResponse({ success: true });
            } catch (err) {
                sendResponse({ success: false, error: err.message });
            }
        })();
        return true;
    } else if (msg.action === "disconnect" || msg.action === "detach_all") {
        (async () => {
            try {
                await clearAllBanners();
                const count = attachedTabs.size;
                for (const tabId of Array.from(attachedTabs)) {
                    try { await chrome.debugger.detach({ tabId }); } catch (_) {}
                    attachedTabs.delete(tabId);
                }
                await chrome.storage.session.set({ activeSession: null });
                sendResponse({ success: true, detachedCount: count });
            } catch (err) {
                sendResponse({ success: false, error: err.message });
            }
        })();
        return true;
    } else if (msg.action === "get_sessions") {
        (async () => {
            try {
                const sessionRes = await chrome.storage.session.get(["activePort"]);
                const isConnected = !!currentWs && currentWs.readyState === WebSocket.OPEN;
                const port = sessionRes?.activePort || 9222;

                const sessions = [];
                for (const tabId of attachedTabs) {
                    try {
                        const tab = await chrome.tabs.get(tabId);
                        sessions.push({
                            id: tab.id,
                            title: tab.title || `Tab #${tab.id}`,
                            url: tab.url || ""
                        });
                    } catch (_) {
                        sessions.push({
                            id: tabId,
                            title: `Tab #${tabId}`,
                            url: ""
                        });
                    }
                }
                sendResponse({
                    connected: isConnected,
                    activePort: port,
                    sessions
                });
            } catch (_) {
                sendResponse({ connected: false, activePort: 9222, sessions: [] });
            }
        })();
        return true;
    } else if (msg.action === "detach_tab") {
        (async () => {
            try {
                const tabId = Number(msg.tabId);
                if (tabId && attachedTabs.has(tabId)) {
                    try { await chrome.debugger.detach({ tabId }); } catch (_) {}
                    attachedTabs.delete(tabId);
                }
                removeTaskForTab(tabId);
                sendResponse({ success: true });
            } catch (err) {
                sendResponse({ success: false, error: err.message });
            }
        })();
        return true;
    } else if (msg.action === "status") {
        chrome.storage.session.get(["connected", "activeSession", "activePort"], (res) => {
            sendResponse({ connected: !!res.connected, activeSession: res.activeSession, activePort: res.activePort });
        });
        return true;
    } else if (msg.action === "set_port") {
        const port = Number(msg.port);
        chrome.storage.session.set({ preferredPort: port }, () => {
            sendResponse({ success: true });
            if (currentWs) {
                try { currentWs.close(); } catch (_) {}
            }
            reconnectDelayMs = 500;
            connect(port);
        });
        return true;
    }
});

connect();
