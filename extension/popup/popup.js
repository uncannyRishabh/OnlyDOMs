const toggleBtn = document.getElementById('toggleBtn');
const statusDiv = document.getElementById('status');
const wsStatus = document.getElementById('wsStatus');
const portInput = document.getElementById('portInput');
const savePortBtn = document.getElementById('savePortBtn');

function updateStatus() {
    chrome.runtime.sendMessage({ action: "status" }, (res) => {
        if (!res) return;
        wsStatus.innerText = res.connected ? "🟢 Connected" : "🔴 Disconnected";
        if (res.activeSession) {
            toggleBtn.innerText = "Disconnect Session";
        } else {
            toggleBtn.innerText = "Attach Session";
        }
    });
}

setInterval(updateStatus, 1000);
updateStatus();

savePortBtn.addEventListener('click', () => {
    chrome.runtime.sendMessage({ action: "set_port", port: parseInt(portInput.value) }, () => {
        statusDiv.innerText = "Port saved.";
        updateStatus();
    });
});

toggleBtn.addEventListener('click', async () => {
    chrome.runtime.sendMessage({ action: "status" }, async (res) => {
        if (res.activeSession) {
            chrome.runtime.sendMessage({ action: "disconnect" }, () => {
                statusDiv.innerText = "Disconnected.";
                updateStatus();
            });
        } else {
            try {
                const tabs = await chrome.tabs.query({active: true, currentWindow: true});
                if (tabs.length === 0) {
                    statusDiv.innerText = "No active tab found.";
                    return;
                }
                chrome.runtime.sendMessage({ action: "attach", tabId: tabs[0].id }, (response) => {
                    if (chrome.runtime.lastError || !response.success) {
                        statusDiv.innerText = "Error: " + (chrome.runtime.lastError?.message || response?.error);
                    } else {
                        statusDiv.innerText = "Attached successfully!";
                        updateStatus();
                    }
                });
            } catch(e) {
                statusDiv.innerText = "Error: " + e.message;
            }
        }
    });
});
