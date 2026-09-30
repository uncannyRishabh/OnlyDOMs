function connect(port = 9222) {
    chrome.storage.session.get(["wsUrl"], async (res) => {
        const url = res.wsUrl || `ws://127.0.0.1:${port}/mcp-relay`;
        
        const ws = new WebSocket(url);
        ws.onopen = () => {
            console.log("Connected to MCP");
            chrome.storage.session.set({ connected: true });
        };
        ws.onmessage = async (event) => {
            const data = JSON.parse(event.data);
            if (data.type === "CDP_CMD") {
                const result = await chrome.storage.session.get("activeSession");
                const registeredTabId = result.activeSession;
                if (registeredTabId) {
                    try {
                        const res = await chrome.debugger.sendCommand(
                            { tabId: registeredTabId },
                            data.method,
                            data.params
                        );
                        ws.send(JSON.stringify({ id: data.id, result: res }));
                    } catch (e) {
                        ws.send(JSON.stringify({ id: data.id, error: e.message }));
                    }
                } else {
                    ws.send(JSON.stringify({ id: data.id, error: "No active session" }));
                }
            } else if (data.type === "INJECT_SCRIPT") {
                const result = await chrome.storage.session.get("activeSession");
                const registeredTabId = result.activeSession;
                if (registeredTabId) {
                    try {
                        await chrome.scripting.executeScript({
                            target: { tabId: registeredTabId },
                            files: [data.file]
                        });
                        ws.send(JSON.stringify({ id: data.id, result: { success: true } }));
                    } catch (e) {
                        ws.send(JSON.stringify({ id: data.id, error: e.message }));
                    }
                } else {
                    ws.send(JSON.stringify({ id: data.id, error: "No active session" }));
                }
            } else if (data.type === "LIST_TABS") {
                try {
                    const tabs = await chrome.tabs.query({});
                    ws.send(JSON.stringify({ id: data.id, result: { tabs } }));
                } catch(e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            } else if (data.type === "ATTACH_TAB") {
                try {
                    const tabId = data.tabId;
                    await chrome.debugger.attach({ tabId }, "1.3");
                    await chrome.storage.session.set({ activeSession: tabId });
                    
                    await chrome.debugger.sendCommand({tabId}, "Page.enable", {});
                    await chrome.debugger.sendCommand({tabId}, "Runtime.enable", {});
                    await chrome.debugger.sendCommand({tabId}, "DOM.enable", {});
                    await chrome.debugger.sendCommand({tabId}, "Debugger.enable", {});
                    await chrome.debugger.sendCommand({tabId}, "Input.enable", {});
                    
                    ws.send(JSON.stringify({ id: data.id, result: { success: true } }));
                } catch(e) {
                    ws.send(JSON.stringify({ id: data.id, error: e.message }));
                }
            }
        };
        ws.onclose = () => {
            chrome.storage.session.set({ connected: false });
            chrome.alarms.create("reconnect", { delayInMinutes: 0.05 });
        };
    });
}

chrome.runtime.onInstalled.addListener(() => {
    chrome.storage.session.set({ activeSession: null, connected: false });
    connect();
});

chrome.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === "reconnect") {
        connect();
    }
});

chrome.debugger.onDetach.addListener((source, reason) => {
    chrome.storage.session.get("activeSession", (result) => {
        if (source.tabId === result.activeSession) {
            chrome.storage.session.set({ activeSession: null });
        }
    });
});

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
    if (msg.action === "attach" && msg.tabId) {
        chrome.debugger.attach({ tabId: msg.tabId }, "1.3", async () => {
            if (chrome.runtime.lastError) {
                sendResponse({ success: false, error: chrome.runtime.lastError.message });
            } else {
                await chrome.storage.session.set({ activeSession: msg.tabId });
                try {
                    await chrome.debugger.sendCommand({tabId: msg.tabId}, "Page.enable", {});
                    await chrome.debugger.sendCommand({tabId: msg.tabId}, "Runtime.enable", {});
                    await chrome.debugger.sendCommand({tabId: msg.tabId}, "DOM.enable", {});
                    await chrome.debugger.sendCommand({tabId: msg.tabId}, "Debugger.enable", {});
                    await chrome.debugger.sendCommand({tabId: msg.tabId}, "Input.enable", {});
                    sendResponse({ success: true });
                } catch(e) {
                    sendResponse({ success: false, error: e.message });
                }
            }
        });
        return true; 
    } else if (msg.action === "disconnect") {
        chrome.storage.session.get("activeSession", async (res) => {
            if (res.activeSession) {
                chrome.debugger.detach({ tabId: res.activeSession }, () => {
                    chrome.storage.session.set({ activeSession: null });
                    sendResponse({ success: true });
                });
            } else {
                sendResponse({ success: true });
            }
        });
        return true;
    } else if (msg.action === "status") {
        chrome.storage.session.get(["connected", "activeSession"], (res) => {
            sendResponse({ connected: !!res.connected, activeSession: res.activeSession });
        });
        return true;
    } else if (msg.action === "set_port") {
        const url = `ws://127.0.0.1:${msg.port}/mcp-relay`;
        chrome.storage.session.set({ wsUrl: url }, () => {
            sendResponse({ success: true });
            connect(msg.port);
        });
        return true;
    }
});

connect();
