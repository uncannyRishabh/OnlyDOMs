const CANDIDATE_PORTS = [9222, 9922, 9223, 9923, 9224, 9924];
let currentPortIndex = 0;
let reconnectDelayMs = 2000;
const MAX_RECONNECT_DELAY_MS = 15000;
let currentWs = null;
let reconnectTimer = null;
const attachedTabs = new Set();

function scheduleReconnect() {
    if (reconnectTimer) clearTimeout(reconnectTimer);
    reconnectTimer = setTimeout(() => {
        connect();
    }, reconnectDelayMs);
    reconnectDelayMs = Math.min(Math.round(reconnectDelayMs * 1.5), MAX_RECONNECT_DELAY_MS);
}

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

    if (!attachedTabs.has(tabId)) {
        try {
            await chrome.debugger.attach({ tabId }, "1.3");
        } catch (e) {
            const msg = e?.message || "";
            if (!msg.includes("Already attached") && !msg.includes("Another debugger")) {
                throw e;
            }
        }
        await Promise.allSettled([
            chrome.debugger.sendCommand({ tabId }, "Page.enable", {}),
            chrome.debugger.sendCommand({ tabId }, "Runtime.enable", {}),
            chrome.debugger.sendCommand({ tabId }, "DOM.enable", {}),
            chrome.debugger.sendCommand({ tabId }, "Input.enable", {})
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
        console.log(`[OnlyDOMs] Attempting connection to ${url}...`);

        let ws;
        try {
            ws = new WebSocket(url);
        } catch (e) {
            console.warn(`[OnlyDOMs] WebSocket init failed: ${e.message}`);
            currentPortIndex++;
            scheduleReconnect();
            return;
        }

        currentWs = ws;

        const connectTimeout = setTimeout(() => {
            if (ws && ws.readyState !== WebSocket.OPEN) {
                console.log(`[OnlyDOMs] Connection timeout on port ${port}`);
                try { ws.close(); } catch (_) {}
            }
        }, 3000);

        ws.onopen = () => {
            clearTimeout(connectTimeout);
            console.log(`[OnlyDOMs] Connected to MCP hub on port ${port}`);
            chrome.storage.session.set({ connected: true, activePort: port, browserWide: true });
            reconnectDelayMs = 2000;
        };

        ws.onmessage = async (event) => {
            let data;
            try {
                data = JSON.parse(event.data);
            } catch (e) {
                console.error("[OnlyDOMs] Parse error:", e);
                return;
            }

            if (data.type === "PING") {
                try { ws.send(JSON.stringify({ id: data.id, result: "pong" })); } catch (_) {}
                return;
            }

            if (data.type === "CDP_CMD") {
                try {
                    const targetTabId = await resolveTargetTab(data.tabId);
                    const res = await chrome.debugger.sendCommand(
                        { tabId: targetTabId },
                        data.method,
                        data.params
                    );
                    ws.send(JSON.stringify({ id: data.id, result: res }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "INJECT_SCRIPT") {
                try {
                    const targetTabId = await resolveTargetTab(data.tabId);
                    await chrome.scripting.executeScript({
                        target: { tabId: targetTabId },
                        files: [data.file]
                    });
                    ws.send(JSON.stringify({ id: data.id, result: { success: true } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "LIST_TABS") {
                try {
                    const tabs = await chrome.tabs.query({});
                    // Token-efficient tab list: strip http/https and query params unless detailed is true
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
                    const resultTabs = data.detailed ? tabs : tabs.map(t => ({
                        id: t.id,
                        title: t.title || "",
                        url: cleanUrl(t.url),
                        active: !!t.active,
                        windowId: t.windowId
                    }));
                    ws.send(JSON.stringify({ id: data.id, result: { tabs: resultTabs } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "CREATE_TAB") {
                try {
                    const url = data.url || "about:blank";
                    const newTab = await chrome.tabs.create({ url, active: true });
                    try {
                        await resolveTargetTab(newTab.id);
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
                    const url = data.url;
                    await chrome.tabs.update(tabId, { url });
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
                    ws.send(JSON.stringify({ id: data.id, result: { success: true } }));
                } catch (e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "ATTACH_TAB") {
                try {
                    const tabId = await resolveTargetTab(data.tabId);
                    await chrome.storage.session.set({ activeSession: tabId });
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

chrome.runtime.onInstalled.addListener(() => {
    chrome.storage.session.set({ activeSession: null, connected: false, browserWide: true });
    connect();
});

chrome.debugger.onDetach.addListener((source, reason) => {
    if (source.tabId) {
        attachedTabs.delete(source.tabId);
        chrome.storage.session.get("activeSession", (result) => {
            if (source.tabId === result?.activeSession) {
                chrome.storage.session.set({ activeSession: null });
            }
        });
    }
});

chrome.tabs.onRemoved.addListener((tabId) => {
    attachedTabs.delete(tabId);
    chrome.storage.session.get("activeSession", (result) => {
        if (tabId === result?.activeSession) {
            chrome.storage.session.set({ activeSession: null });
        }
    });
});

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
    if (msg.action === "attach" && msg.tabId) {
        (async () => {
            const tabId = Number(msg.tabId);
            if (!tabId || isNaN(tabId)) {
                sendResponse({ success: false, error: "Invalid tab ID" });
                return;
            }
            try {
                const current = await chrome.storage.session.get("activeSession");
                if (current && current.activeSession === tabId) {
                    sendResponse({ success: true, message: "Already attached" });
                    return;
                }
                if (current && current.activeSession && current.activeSession !== tabId) {
                    try {
                        await chrome.debugger.detach({ tabId: current.activeSession });
                    } catch (_) {}
                }
                try {
                    await chrome.debugger.attach({ tabId }, "1.3");
                } catch (attachErr) {
                    const errMsg = attachErr?.message || "";
                    if (!errMsg.includes("Already attached") && !errMsg.includes("Another debugger is already attached")) {
                        throw attachErr;
                    }
                }
                await chrome.storage.session.set({ activeSession: tabId });
                
                await Promise.allSettled([
                    chrome.debugger.sendCommand({tabId}, "Page.enable", {}),
                    chrome.debugger.sendCommand({tabId}, "Runtime.enable", {}),
                    chrome.debugger.sendCommand({tabId}, "DOM.enable", {}),
                    chrome.debugger.sendCommand({tabId}, "Debugger.enable", {}),
                    chrome.debugger.sendCommand({tabId}, "Input.enable", {})
                ]);
                sendResponse({ success: true });
            } catch (err) {
                sendResponse({ success: false, error: err.message });
            }
        })();
        return true; 
    } else if (msg.action === "disconnect") {
        (async () => {
            try {
                const res = await chrome.storage.session.get("activeSession");
                if (res && res.activeSession) {
                    try { await chrome.debugger.detach({ tabId: res.activeSession }); } catch (_) {}
                    await chrome.storage.session.set({ activeSession: null });
                }
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
