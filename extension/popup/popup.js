const toggleBtn = document.getElementById('toggleBtn');
const statusDiv = document.getElementById('status');
const wsStatus = document.getElementById('wsStatus');
const portInput = document.getElementById('portInput');
const savePortBtn = document.getElementById('savePortBtn');
const tabSelect = document.getElementById('tabSelect');

let availableTabs = [];
let currentActiveSession = null;

async function refreshTabList() {
    try {
        const tabs = await chrome.tabs.query({});
        availableTabs = tabs.filter(t => {
            const url = t.url || "";
            return !url.startsWith("chrome://") && 
                   !url.startsWith("edge://") && 
                   !url.startsWith("devtools://") && 
                   !url.startsWith("chrome-extension://") &&
                   !url.includes("chromewebstore.google.com");
        });

        const [activeTab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
        const activeTabId = activeTab ? activeTab.id : null;

        tabSelect.innerHTML = "";
        if (availableTabs.length === 0) {
            tabSelect.innerHTML = `<option value="">No attachable tabs found (open a webpage)</option>`;
            return;
        }

        availableTabs.forEach(t => {
            const opt = document.createElement("option");
            opt.value = t.id;
            const title = t.title ? (t.title.length > 38 ? t.title.slice(0, 38) + "..." : t.title) : t.url;
            opt.textContent = `${title} (ID: ${t.id})`;
            if (currentActiveSession === t.id) {
                opt.selected = true;
            } else if (!currentActiveSession && t.id === activeTabId) {
                opt.selected = true;
            }
            tabSelect.appendChild(opt);
        });
    } catch (e) {
        console.error("Failed to list tabs:", e);
    }
}

function updateStatus() {
    chrome.runtime.sendMessage({ action: "status" }, (res) => {
        if (!res) return;
        currentActiveSession = res.activeSession;
        if (res.activePort && document.activeElement !== portInput) {
            portInput.value = res.activePort;
        }
        wsStatus.innerText = res.connected 
            ? `🟢 Connected (port ${res.activePort || portInput.value})` 
            : "🔴 Disconnected";
            
        if (res.activeSession) {
            toggleBtn.innerText = `Disconnect Session (Tab ${res.activeSession})`;
            toggleBtn.classList.add("disconnect");
            if (tabSelect.value !== String(res.activeSession)) {
                tabSelect.value = res.activeSession;
            }
        } else {
            toggleBtn.innerText = "Attach Session";
            toggleBtn.classList.remove("disconnect");
        }
    });
}

savePortBtn.addEventListener('click', () => {
    const port = parseInt(portInput.value, 10);
    if (!port || isNaN(port)) {
        statusDiv.innerText = "Please enter a valid numeric port.";
        return;
    }
    chrome.runtime.sendMessage({ action: "set_port", port: port }, () => {
        statusDiv.innerText = `Connecting to port ${port}...`;
        setTimeout(updateStatus, 500);
    });
});

toggleBtn.addEventListener('click', async () => {
    chrome.runtime.sendMessage({ action: "status" }, async (res) => {
        if (res && res.activeSession) {
            chrome.runtime.sendMessage({ action: "disconnect" }, () => {
                statusDiv.innerText = "Session detached.";
                updateStatus();
            });
        } else {
            const selectedTabId = parseInt(tabSelect.value, 10);
            if (!selectedTabId || isNaN(selectedTabId)) {
                statusDiv.innerText = "Please select a valid tab from the list.";
                return;
            }
            statusDiv.innerText = `Attaching to tab ${selectedTabId}...`;
            chrome.runtime.sendMessage({ action: "attach", tabId: selectedTabId }, (response) => {
                if (chrome.runtime.lastError || !response || !response.success) {
                    const err = chrome.runtime.lastError?.message || response?.error || "Unknown error";
                    statusDiv.innerText = "Error: " + err;
                } else {
                    statusDiv.innerText = "Attached successfully! 🚀";
                    updateStatus();
                }
            });
        }
    });
});

refreshTabList();
updateStatus();
setInterval(updateStatus, 2000);
