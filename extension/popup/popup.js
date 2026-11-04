/**
 * OnlyDOMs Popup Control Center & Site Blacklist Manager
 *
 * Implements:
 * - Real-time WebSocket connection badge & port indicator (F7)
 * - Active agent session leases monitor & Emergency Detach All (F7)
 * - Persistent site blacklist manager with chrome.storage.local (F8)
 */

const STORAGE_KEY_BLACKLIST = "blockedDomains";

// DOM Elements
const connectionPill = document.getElementById("connectionPill");
const connectionText = document.getElementById("connectionText");
const activePortNumber = document.getElementById("activePortNumber");
const sessionCountBadge = document.getElementById("sessionCountBadge");
const sessionsList = document.getElementById("sessionsList");
const emergencyDetachBtn = document.getElementById("emergencyDetachBtn");
const blockedCountBadge = document.getElementById("blockedCountBadge");
const clearAllBtn = document.getElementById("clearAllBtn");
const addPatternForm = document.getElementById("addPatternForm");
const patternInput = document.getElementById("patternInput");
const inputError = document.getElementById("inputError");
const blacklistContainer = document.getElementById("blacklistContainer");
const toggleSettingsBtn = document.getElementById("toggleSettingsBtn");
const settingsDrawer = document.getElementById("settingsDrawer");
const customPortInput = document.getElementById("customPortInput");
const savePortBtn = document.getElementById("savePortBtn");
const statusToast = document.getElementById("statusToast");

let toastTimer = null;
let currentBlockedDomains = [];
let activeSessionsCache = [];

// ---------------------------------------------------------------------------
// Toast Notification
// ---------------------------------------------------------------------------
function showToast(message, durationMs = 2500) {
  if (toastTimer) clearTimeout(toastTimer);
  statusToast.textContent = message;
  statusToast.style.display = "block";
  toastTimer = setTimeout(() => {
    statusToast.style.display = "none";
  }, durationMs);
}

// ---------------------------------------------------------------------------
// Error Display
// ---------------------------------------------------------------------------
function showInputError(msg) {
  inputError.textContent = msg;
  inputError.style.display = "block";
}

function clearInputError() {
  inputError.textContent = "";
  inputError.style.display = "none";
}

// ---------------------------------------------------------------------------
// Site Blacklist Management (chrome.storage.local)
// ---------------------------------------------------------------------------

/**
 * Validates and normalizes pattern string.
 * Supports:
 * - Exact domains: "facebook.com", "sub.domain.org"
 * - Wildcard subdomains: "*.bank.com"
 * - URL prefixes: "https://internal.corp/admin", "github.com/settings"
 */
function normalizePattern(raw) {
  const trimmed = raw.trim();
  if (!trimmed) {
    throw new Error("Pattern cannot be empty.");
  }

  // Safe defaults guard
  const lower = trimmed.toLowerCase();
  if (
    lower === "about:blank" ||
    lower.startsWith("chrome://") ||
    lower.startsWith("chrome-extension://") ||
    lower.startsWith("devtools://") ||
    lower.startsWith("edge://")
  ) {
    throw new Error("Internal browser URLs (about:blank, chrome://) cannot be blocked.");
  }

  // Wildcard validation
  if (trimmed.startsWith("*.")) {
    const root = trimmed.slice(2).trim();
    if (!root || root.includes("*") || root.includes("/")) {
      throw new Error("Invalid wildcard format. Expected format: *.example.com");
    }
    return `*.${root.toLowerCase()}`;
  }

  // If pattern looks like a standard URL or host
  if (trimmed.includes("://")) {
    try {
      const parsed = new URL(trimmed);
      // Normalized url prefix
      return parsed.href.toLowerCase();
    } catch (_) {
      // Fallback
      return trimmed.toLowerCase();
    }
  }

  return trimmed.toLowerCase();
}

async function loadBlacklist() {
  try {
    const data = await chrome.storage.local.get(STORAGE_KEY_BLACKLIST);
    currentBlockedDomains = Array.isArray(data[STORAGE_KEY_BLACKLIST])
      ? data[STORAGE_KEY_BLACKLIST]
      : [];
    renderBlacklist(currentBlockedDomains);
  } catch (err) {
    console.error("[OnlyDOMs] Failed to load blacklist:", err);
  }
}

async function saveBlacklist(domains) {
  currentBlockedDomains = domains;
  await chrome.storage.local.set({ [STORAGE_KEY_BLACKLIST]: domains });
  renderBlacklist(domains);
}

function renderBlacklist(domains) {
  blacklistContainer.innerHTML = "";
  blockedCountBadge.textContent = String(domains.length);

  if (domains.length === 0) {
    clearAllBtn.style.display = "none";
    blacklistContainer.innerHTML = `
      <div class="empty-state">
        No blocked sites. All accessible URLs are permitted for agent interactions.
      </div>
    `;
    return;
  }

  clearAllBtn.style.display = "inline-flex";

  domains.forEach((pattern) => {
    const item = document.createElement("div");
    item.className = "blacklist-item";

    const info = document.createElement("div");
    info.className = "blacklist-pattern-info";

    // Determine pattern type tag
    const tag = document.createElement("span");
    tag.className = "pattern-type-tag";
    if (pattern.startsWith("*.")) {
      tag.classList.add("tag-wildcard");
      tag.textContent = "Wildcard";
    } else if (pattern.includes("://") || pattern.includes("/")) {
      tag.classList.add("tag-prefix");
      tag.textContent = "Prefix";
    } else {
      tag.classList.add("tag-exact");
      tag.textContent = "Exact";
    }

    const text = document.createElement("span");
    text.className = "blacklist-pattern-text";
    text.textContent = pattern;
    text.title = pattern;

    info.appendChild(tag);
    info.appendChild(text);

    const deleteBtn = document.createElement("button");
    deleteBtn.className = "delete-pattern-btn";
    deleteBtn.type = "button";
    deleteBtn.innerHTML = "&times;";
    deleteBtn.title = `Remove ${pattern}`;
    deleteBtn.setAttribute("aria-label", `Remove ${pattern}`);
    deleteBtn.addEventListener("click", () => handleRemovePattern(pattern));

    item.appendChild(info);
    item.appendChild(deleteBtn);
    blacklistContainer.appendChild(item);
  });
}

async function handleAddPattern(e) {
  e.preventDefault();
  clearInputError();

  const raw = patternInput.value;
  let normalized;
  try {
    normalized = normalizePattern(raw);
  } catch (err) {
    showInputError(err.message);
    patternInput.focus();
    return;
  }

  // Check duplicate
  const exists = currentBlockedDomains.some(
    (p) => p.toLowerCase() === normalized.toLowerCase()
  );
  if (exists) {
    showInputError(`Pattern "${normalized}" is already in the blacklist.`);
    patternInput.focus();
    return;
  }

  const updated = [normalized, ...currentBlockedDomains];
  await saveBlacklist(updated);
  patternInput.value = "";
  showToast(`Blocked: ${normalized}`);
}

async function handleRemovePattern(patternToRemove) {
  const updated = currentBlockedDomains.filter((p) => p !== patternToRemove);
  await saveBlacklist(updated);
  showToast(`Removed: ${patternToRemove}`);
}

async function handleClearAll() {
  if (currentBlockedDomains.length === 0) return;
  const confirmed = confirm(
    `Remove all ${currentBlockedDomains.length} blocked site pattern(s)?`
  );
  if (!confirmed) return;

  await saveBlacklist([]);
  showToast("Site blacklist cleared");
}

// ---------------------------------------------------------------------------
// Connection & Active Session Monitor
// ---------------------------------------------------------------------------

function updateConnectionUI(connected, port) {
  const displayPort = port || 9222;
  activePortNumber.textContent = `Port ${displayPort}`;
  if (customPortInput && document.activeElement !== customPortInput) {
    customPortInput.value = displayPort;
  }

  connectionPill.className = "status-pill";
  if (connected) {
    connectionPill.classList.add("status-connected");
    connectionText.textContent = `Connected (${displayPort})`;
    connectionPill.title = `Connected to daemon hub on ws://127.0.0.1:${displayPort}/mcp-relay`;
  } else {
    connectionPill.classList.add("status-disconnected");
    connectionText.textContent = "Daemon Offline";
    connectionPill.title = "No active connection to MCP daemon";
  }
}

async function refreshSessionsAndStatus() {
  // Query status & sessions from background service worker
  chrome.runtime.sendMessage({ action: "get_sessions" }, async (res) => {
    if (chrome.runtime.lastError || !res) {
      // Fallback to legacy "status" message
      chrome.runtime.sendMessage({ action: "status" }, async (statusRes) => {
        if (!statusRes) return;
        updateConnectionUI(statusRes.connected, statusRes.activePort);
        await renderSingleOrEmptySession(statusRes.activeSession);
      });
      return;
    }

    updateConnectionUI(res.connected, res.activePort);
    renderActiveSessions(res.sessions || []);
  });
}

async function renderSingleOrEmptySession(singleTabId) {
  if (!singleTabId) {
    renderActiveSessions([]);
    return;
  }
  try {
    const tab = await chrome.tabs.get(singleTabId);
    renderActiveSessions([
      {
        id: tab.id,
        title: tab.title || "Untitled Tab",
        url: tab.url || "",
      },
    ]);
  } catch (_) {
    renderActiveSessions([
      {
        id: singleTabId,
        title: `Tab #${singleTabId}`,
        url: "",
      },
    ]);
  }
}

function renderActiveSessions(sessions) {
  activeSessionsCache = sessions;
  sessionsList.innerHTML = "";
  const count = sessions.length;
  sessionCountBadge.textContent = `${count} active`;
  if (count > 0) {
    sessionCountBadge.classList.add("active");
    emergencyDetachBtn.disabled = false;
  } else {
    sessionCountBadge.classList.remove("active");
    emergencyDetachBtn.disabled = true;
  }

  if (count === 0) {
    sessionsList.innerHTML = `
      <div class="empty-state">
        No active agent sessions. Agents lease tabs dynamically on demand.
      </div>
    `;
    return;
  }

  sessions.forEach((s) => {
    const item = document.createElement("div");
    item.className = "session-item";

    const info = document.createElement("div");
    info.className = "session-info";

    const headerRow = document.createElement("div");
    headerRow.className = "session-header-row";

    const badge = document.createElement("span");
    badge.className = "tab-badge";
    badge.textContent = `#${s.id}`;

    const title = document.createElement("span");
    title.className = "session-title";
    title.textContent = s.title || `Tab ${s.id}`;
    title.title = s.title || `Tab ${s.id}`;

    headerRow.appendChild(badge);
    headerRow.appendChild(title);
    info.appendChild(headerRow);

    if (s.url) {
      const urlSpan = document.createElement("span");
      urlSpan.className = "session-url";
      urlSpan.textContent = s.url;
      urlSpan.title = s.url;
      info.appendChild(urlSpan);
    }

    // Individual release button
    const releaseBtn = document.createElement("button");
    releaseBtn.className = "session-action-btn";
    releaseBtn.type = "button";
    releaseBtn.textContent = "Detach";
    releaseBtn.title = `Detach agent from tab #${s.id}`;
    releaseBtn.addEventListener("click", () => handleDetachSingle(s.id));

    item.appendChild(info);
    item.appendChild(releaseBtn);
    sessionsList.appendChild(item);
  });
}

function handleDetachSingle(tabId) {
  chrome.runtime.sendMessage({ action: "detach_tab", tabId }, () => {
    showToast(`Detached tab #${tabId}`);
    refreshSessionsAndStatus();
  });
}

function handleEmergencyDetachAll() {
  if (activeSessionsCache.length === 0 && emergencyDetachBtn.disabled) return;

  emergencyDetachBtn.disabled = true;
  emergencyDetachBtn.textContent = "Detaching...";

  chrome.runtime.sendMessage({ action: "detach_all" }, (res) => {
    if (chrome.runtime.lastError || (res && !res.success)) {
      // Fallback: try legacy disconnect
      chrome.runtime.sendMessage({ action: "disconnect" }, () => {
        emergencyDetachBtn.textContent = "🚨 Detach All";
        showToast("Agent sessions detached");
        refreshSessionsAndStatus();
      });
      return;
    }
    emergencyDetachBtn.textContent = "🚨 Detach All";
    showToast("All agent sessions detached");
    refreshSessionsAndStatus();
  });
}

// ---------------------------------------------------------------------------
// Port Override Settings Drawer
// ---------------------------------------------------------------------------

function toggleSettings() {
  const isHidden = settingsDrawer.style.display === "none";
  settingsDrawer.style.display = isHidden ? "flex" : "none";
  toggleSettingsBtn.setAttribute("aria-expanded", String(isHidden));
  if (isHidden) {
    customPortInput.focus();
  }
}

function handleSavePort() {
  const port = parseInt(customPortInput.value, 10);
  if (!port || isNaN(port) || port < 1024 || port > 65535) {
    showToast("Invalid port number (1024 - 65535)");
    return;
  }

  chrome.runtime.sendMessage({ action: "set_port", port }, (res) => {
    showToast(`Connecting to port ${port}...`);
    settingsDrawer.style.display = "none";
    toggleSettingsBtn.setAttribute("aria-expanded", "false");
    setTimeout(refreshSessionsAndStatus, 600);
  });
}

// ---------------------------------------------------------------------------
// Event Listeners & Initialization
// ---------------------------------------------------------------------------

function initializeEventListeners() {
  addPatternForm.addEventListener("submit", handleAddPattern);
  patternInput.addEventListener("input", clearInputError);
  clearAllBtn.addEventListener("click", handleClearAll);
  emergencyDetachBtn.addEventListener("click", handleEmergencyDetachAll);
  toggleSettingsBtn.addEventListener("click", toggleSettings);
  savePortBtn.addEventListener("click", handleSavePort);

  // Sync with chrome.storage.local changes
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === "local" && changes[STORAGE_KEY_BLACKLIST]) {
      const newList = changes[STORAGE_KEY_BLACKLIST].newValue || [];
      currentBlockedDomains = newList;
      renderBlacklist(newList);
    }
    if (area === "session" && (changes.connected || changes.activePort || changes.activeSession)) {
      refreshSessionsAndStatus();
    }
  });
}

// Startup
document.addEventListener("DOMContentLoaded", async () => {
  initializeEventListeners();
  await loadBlacklist();
  await refreshSessionsAndStatus();
  // Poll every 2 seconds while popup remains open
  setInterval(refreshSessionsAndStatus, 2000);
});
