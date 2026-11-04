/**
 * OnlyDOMs Top-Layer Agent HUD (<mcp-agent-hud>)
 * Master Specification § 1, § 2, § 3, § 4, § 6
 *
 * Features:
 * - Immutable Sentinel Markers: <mcp-agent-hud id="__mcp_agent_hud_host__" data-mcp-internal-ignore="true" popover="manual">
 * - Closed Shadow DOM: attachShadow({ mode: "closed" })
 * - Top-Layer Mounting via native HTML Popover API (.showPopover()) at bottom: 20px; right: 20px
 * - Minimal, non-invasive modern glassmorphic pill displaying:
 *     1. Tab Header (Tab #<id> · <title>) + Cross-Tab [Switch ↗] button + Multi-Tab ⚡ N Tabs badge
 *     2. Task Intent Header (🎯 <180-char intent>)
 *     3. Live Command Status Ticker (Executing :: <cmd>) with 60fps rAF vertical crossfade
 * - Automatic Idle Auto-Hide: Morphs to ✓ Done on completion, holds 2500ms, then unmounts cleanly (never stuck on page)
 * - Expandable Glassmorphic Telemetry & Optimistic cURL Drawer on click
 */

(() => {
    if (window !== window.top) return;

    // Immediately clean up any legacy warning banner element if present on the page
    const removeLegacyBanners = () => {
        const legacy = document.getElementById("onlydoms-banner-host");
        if (legacy && legacy.parentNode) {
            legacy.parentNode.removeChild(legacy);
        }
    };
    removeLegacyBanners();

    if (window.__mcpAgentHudInitialized) {
        return;
    }
    window.__mcpAgentHudInitialized = true;

    const HUD_TAG = "mcp-agent-hud";
    const HUD_ID = "__mcp_agent_hud_host__";
    const HUD_ATTR = "data-mcp-internal-ignore";
    const AUTO_HIDE_DELAY_MS = 2500;
    const UNMOUNT_ANIM_MS = 240;

    // Define Custom Element <mcp-agent-hud> if not already registered
    try {
        if (typeof customElements !== "undefined" && !customElements.get(HUD_TAG)) {
            class McpAgentHudElement extends HTMLElement {}
            customElements.define(HUD_TAG, McpAgentHudElement);
        }
    } catch (_) {}

    // Remove any stale HUD host before mounting fresh closed Shadow DOM instance
    const existingHost = document.getElementById(HUD_ID);
    if (existingHost && existingHost.parentNode) {
        existingHost.parentNode.removeChild(existingHost);
    }

    const host = document.createElement(HUD_TAG);
    host.id = HUD_ID;
    host.setAttribute(HUD_ATTR, "true");
    host.setAttribute("popover", "manual");

    // Outer host styles: fixed in #top-layer at bottom-right, pointer-events: none for hit-test passthrough
    host.style.setProperty("all", "initial", "important");
    host.style.setProperty("position", "fixed", "important");
    host.style.setProperty("inset", "auto 20px 20px auto", "important");
    host.style.setProperty("bottom", "20px", "important");
    host.style.setProperty("right", "20px", "important");
    host.style.setProperty("margin", "0", "important");
    host.style.setProperty("padding", "0", "important");
    host.style.setProperty("border", "none", "important");
    host.style.setProperty("background", "transparent", "important");
    host.style.setProperty("overflow", "visible", "important");
    host.style.setProperty("z-index", "2147483647", "important");
    host.style.setProperty("pointer-events", "none", "important");
    host.style.setProperty("display", "none", "important");

    // Closed Shadow DOM prevents page JS & querySelectorAll from traversing internal HUD nodes
    const shadow = host.attachShadow({ mode: "closed" });

    const styleEl = document.createElement("style");
    styleEl.textContent = `
        :host {
            all: initial !important;
            display: block !important;
            pointer-events: none !important;
            font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Inter", "Segoe UI", Roboto, sans-serif !important;
            color: #f8fafc !important;
            -webkit-font-smoothing: antialiased !important;
        }

        * {
            box-sizing: border-box !important;
        }

        .hud-root {
            position: relative !important;
            display: flex !important;
            flex-direction: column !important;
            align-items: flex-end !important;
            gap: 8px !important;
            pointer-events: none !important;
            will-change: transform, opacity, filter !important;
            opacity: 0;
            transform: translate3d(0, 16px, 0) scale(0.94);
            filter: blur(4px);
            transition:
                opacity 320ms cubic-bezier(0.22, 1, 0.36, 1),
                transform 320ms cubic-bezier(0.22, 1, 0.36, 1),
                filter 320ms cubic-bezier(0.22, 1, 0.36, 1) !important;
        }

        .hud-root.mounted {
            opacity: 1 !important;
            transform: translate3d(0, 0, 0) scale(1) !important;
            filter: blur(0px) !important;
        }

        .hud-root.unmounting {
            opacity: 0 !important;
            transform: translate3d(0, 8px, 0) scale(0.96) !important;
            filter: blur(2px) !important;
            transition:
                opacity 240ms cubic-bezier(0.4, 0, 1, 1),
                transform 240ms cubic-bezier(0.4, 0, 1, 1),
                filter 240ms cubic-bezier(0.4, 0, 1, 1) !important;
        }

        /* Expandable Glassmorphic Telemetry & cURL Drawer */
        .hud-drawer {
            pointer-events: auto !important;
            width: 380px !important;
            max-width: calc(100vw - 40px) !important;
            height: 320px !important;
            background: rgba(13, 15, 22, 0.92) !important;
            backdrop-filter: blur(20px) saturate(180%) !important;
            -webkit-backdrop-filter: blur(20px) saturate(180%) !important;
            border: 1px solid rgba(255, 255, 255, 0.11) !important;
            border-radius: 14px !important;
            box-shadow: 0 20px 45px rgba(0, 0, 0, 0.55), 0 0 0 1px rgba(255, 255, 255, 0.04) inset !important;
            display: flex !important;
            flex-direction: column !important;
            overflow: hidden !important;
            transform-origin: bottom right !important;
            transform: scale(0.92) translate3d(0, 10px, 0) !important;
            opacity: 0 !important;
            visibility: hidden !important;
            transition:
                transform 260ms cubic-bezier(0.16, 1, 0.3, 1),
                opacity 220ms ease,
                visibility 220ms ease !important;
        }

        .hud-drawer.open {
            transform: scale(1) translate3d(0, 0, 0) !important;
            opacity: 1 !important;
            visibility: visible !important;
        }

        .drawer-header {
            display: flex !important;
            align-items: center !important;
            justify-content: space-between !important;
            padding: 8px 12px !important;
            border-bottom: 1px solid rgba(255, 255, 255, 0.08) !important;
            background: rgba(255, 255, 255, 0.02) !important;
        }

        .drawer-tabs {
            display: flex !important;
            gap: 4px !important;
        }

        .drawer-tab-btn {
            all: unset !important;
            cursor: pointer !important;
            font-size: 11px !important;
            font-weight: 600 !important;
            padding: 4px 9px !important;
            border-radius: 6px !important;
            color: #94a3b8 !important;
            transition: background 150ms ease, color 150ms ease !important;
        }

        .drawer-tab-btn.active {
            background: rgba(99, 102, 241, 0.22) !important;
            color: #e0e7ff !important;
        }

        .drawer-actions {
            display: flex !important;
            align-items: center !important;
            gap: 6px !important;
        }

        .kill-btn {
            all: unset !important;
            cursor: pointer !important;
            font-size: 10.5px !important;
            font-weight: 600 !important;
            color: #fda4af !important;
            background: rgba(244, 63, 94, 0.16) !important;
            border: 1px solid rgba(244, 63, 94, 0.3) !important;
            padding: 3px 8px !important;
            border-radius: 6px !important;
            transition: background 150ms ease !important;
        }

        .kill-btn:hover {
            background: rgba(244, 63, 94, 0.28) !important;
        }

        .drawer-body {
            flex: 1 !important;
            overflow-y: auto !important;
            padding: 8px 10px !important;
            display: flex !important;
            flex-direction: column !important;
            gap: 6px !important;
            font-size: 11px !important;
        }

        .log-item {
            padding: 6px 8px !important;
            border-radius: 8px !important;
            background: rgba(255, 255, 255, 0.035) !important;
            border: 1px solid rgba(255, 255, 255, 0.06) !important;
            display: flex !important;
            flex-direction: column !important;
            gap: 3px !important;
        }

        .log-top {
            display: flex !important;
            justify-content: space-between !important;
            align-items: center !important;
            color: #cbd5e1 !important;
            font-weight: 600 !important;
        }

        .log-cmd {
            font-family: ui-monospace, SFMono-Regular, Menlo, monospace !important;
            font-size: 10.5px !important;
            color: #93c5fd !important;
            word-break: break-all !important;
        }

        .curl-actions {
            display: flex !important;
            gap: 6px !important;
            margin-top: 4px !important;
        }

        .mini-btn {
            all: unset !important;
            cursor: pointer !important;
            font-size: 10px !important;
            font-weight: 600 !important;
            padding: 2px 7px !important;
            border-radius: 5px !important;
            background: rgba(255, 255, 255, 0.08) !important;
            color: #e2e8f0 !important;
        }

        .mini-btn:hover {
            background: rgba(99, 102, 241, 0.3) !important;
        }

        /* Floating Pill Container */
        .pill-shell {
            pointer-events: auto !important;
            position: relative !important;
            width: 320px !important;
            max-width: calc(100vw - 32px) !important;
            border-radius: 14px !important;
            padding: 1px !important;
            background: rgba(255, 255, 255, 0.12) !important;
            box-shadow:
                0 10px 28px -4px rgba(0, 0, 0, 0.5),
                0 2px 8px rgba(0, 0, 0, 0.3) !important;
            overflow: hidden !important;
            transition:
                width 280ms cubic-bezier(0.16, 1, 0.3, 1),
                height 280ms cubic-bezier(0.16, 1, 0.3, 1),
                border-radius 240ms ease !important;
        }

        .pill-shell.multi-tab {
            width: 356px !important;
        }

        /* Ambient Conic Border Shimmer when running */
        .pill-shimmer {
            position: absolute !important;
            inset: -120% !important;
            background: conic-gradient(
                from 0deg,
                transparent 0deg,
                transparent 240deg,
                rgba(56, 189, 248, 0.75) 295deg,
                rgba(129, 140, 248, 0.9) 335deg,
                transparent 360deg
            ) !important;
            opacity: 0 !important;
            transition: opacity 250ms ease !important;
            pointer-events: none !important;
        }

        .pill-shell.running .pill-shimmer {
            opacity: 1 !important;
            animation: hudSpin 2.6s linear infinite !important;
        }

        @keyframes hudSpin {
            to { transform: rotate(360deg); }
        }

        .pill-inner {
            position: relative !important;
            background: rgba(15, 17, 24, 0.90) !important;
            backdrop-filter: blur(16px) saturate(180%) !important;
            -webkit-backdrop-filter: blur(16px) saturate(180%) !important;
            border-radius: 13px !important;
            padding: 8px 11px !important;
            display: flex !important;
            flex-direction: column !important;
            gap: 5px !important;
            cursor: pointer !important;
            user-select: none !important;
            -webkit-user-select: none !important;
        }

        /* Row 1: Tab Header + Status Dot + Multi-tab badge + Actions */
        .pill-header-row {
            display: flex !important;
            align-items: center !important;
            justify-content: space-between !important;
            gap: 6px !important;
            min-height: 18px !important;
        }

        .header-left {
            display: flex !important;
            align-items: center !important;
            gap: 6px !important;
            min-width: 0 !important;
            flex: 1 !important;
        }

        .status-dot-wrap {
            position: relative !important;
            width: 14px !important;
            height: 14px !important;
            display: inline-flex !important;
            align-items: center !important;
            justify-content: center !important;
            flex-shrink: 0 !important;
        }

        .status-dot {
            width: 7px !important;
            height: 7px !important;
            border-radius: 999px !important;
            background: #38bdf8 !important;
            box-shadow: 0 0 8px rgba(56, 189, 248, 0.8) !important;
        }

        .status-ring {
            position: absolute !important;
            inset: 1px !important;
            border-radius: 999px !important;
            border: 1.5px solid rgba(56, 189, 248, 0.65) !important;
            animation: pulseRing 1.6s cubic-bezier(0.22, 1, 0.36, 1) infinite !important;
        }

        @keyframes pulseRing {
            0% { transform: scale(0.75); opacity: 0.9; }
            100% { transform: scale(1.55); opacity: 0; }
        }

        .pill-shell.completed .status-dot {
            background: #10b981 !important;
            box-shadow: 0 0 8px rgba(16, 185, 129, 0.8) !important;
        }

        .pill-shell.completed .status-ring {
            display: none !important;
        }

        .pill-shell.error .status-dot {
            background: #f43f5e !important;
            box-shadow: 0 0 8px rgba(244, 63, 94, 0.8) !important;
        }

        .done-check {
            display: none !important;
            font-size: 11px !important;
            font-weight: 800 !important;
            color: #10b981 !important;
            line-height: 1 !important;
        }

        .pill-shell.completed .done-check {
            display: inline-block !important;
        }

        .pill-shell.completed .status-dot {
            display: none !important;
        }

        .multi-tab-badge {
            display: none !important;
            align-items: center !important;
            gap: 3px !important;
            font-size: 10px !important;
            font-weight: 700 !important;
            padding: 1px 6px !important;
            border-radius: 999px !important;
            background: rgba(99, 102, 241, 0.24) !important;
            color: #c7d2fe !important;
            border: 1px solid rgba(129, 140, 248, 0.35) !important;
            flex-shrink: 0 !important;
        }

        .pill-shell.multi-tab .multi-tab-badge {
            display: inline-flex !important;
        }

        .tab-header-label {
            font-size: 11px !important;
            font-weight: 600 !important;
            color: #e2e8f0 !important;
            white-space: nowrap !important;
            overflow: hidden !important;
            text-overflow: ellipsis !important;
            letter-spacing: -0.01em !important;
        }

        .tab-id-pill {
            color: #94a3b8 !important;
            font-weight: 500 !important;
            font-family: ui-monospace, SFMono-Regular, Menlo, monospace !important;
            font-size: 10.5px !important;
        }

        .header-right {
            display: flex !important;
            align-items: center !important;
            gap: 4px !important;
            flex-shrink: 0 !important;
        }

        .jump-btn {
            all: unset !important;
            cursor: pointer !important;
            font-size: 10px !important;
            font-weight: 600 !important;
            color: #7dd3fc !important;
            background: rgba(56, 189, 248, 0.14) !important;
            border: 1px solid rgba(56, 189, 248, 0.28) !important;
            padding: 2px 6px !important;
            border-radius: 5px !important;
            transition: background 150ms ease !important;
        }

        .jump-btn:hover {
            background: rgba(56, 189, 248, 0.26) !important;
        }

        .icon-btn {
            all: unset !important;
            cursor: pointer !important;
            color: #64748b !important;
            font-size: 12px !important;
            line-height: 1 !important;
            padding: 2px 4px !important;
            border-radius: 4px !important;
            transition: color 150ms ease, background 150ms ease !important;
        }

        .icon-btn:hover {
            color: #e2e8f0 !important;
            background: rgba(255, 255, 255, 0.08) !important;
        }

        /* Row 2: Task Intent Banner */
        .intent-row {
            display: flex !important;
            align-items: center !important;
            gap: 5px !important;
            font-size: 11px !important;
            line-height: 1.3 !important;
            color: #cbd5e1 !important;
            white-space: nowrap !important;
            overflow: hidden !important;
            text-overflow: ellipsis !important;
        }

        .intent-prefix {
            color: #a5b4fc !important;
            font-weight: 600 !important;
            flex-shrink: 0 !important;
        }

        .intent-text {
            overflow: hidden !important;
            text-overflow: ellipsis !important;
            white-space: nowrap !important;
            color: #f1f5f9 !important;
        }

        /* Row 3: Live Command Ticker with 60fps crossfade */
        .cmd-ticker-viewport {
            position: relative !important;
            height: 18px !important;
            overflow: hidden !important;
            background: rgba(255, 255, 255, 0.04) !important;
            border: 1px solid rgba(255, 255, 255, 0.06) !important;
            border-radius: 6px !important;
            padding: 0 7px !important;
            display: flex !important;
            align-items: center !important;
        }

        .cmd-ticker-item {
            width: 100% !important;
            font-family: ui-monospace, SFMono-Regular, Menlo, monospace !important;
            font-size: 10px !important;
            color: #7dd3fc !important;
            white-space: nowrap !important;
            overflow: hidden !important;
            text-overflow: ellipsis !important;
            will-change: transform, opacity !important;
            transition:
                transform 160ms cubic-bezier(0.22, 1, 0.36, 1),
                opacity 160ms cubic-bezier(0.22, 1, 0.36, 1) !important;
        }

        .cmd-ticker-item.slide-out {
            transform: translateY(-100%) !important;
            opacity: 0 !important;
        }

        .cmd-ticker-item.slide-in-start {
            transform: translateY(100%) !important;
            opacity: 0 !important;
            transition: none !important;
        }

        /* Multi-tab horizontal swipe carousel controls */
        .carousel-nav {
            display: none !important;
            align-items: center !important;
            justify-content: space-between !important;
            padding-top: 2px !important;
            font-size: 10px !important;
            color: #94a3b8 !important;
        }

        .pill-shell.multi-tab .carousel-nav {
            display: flex !important;
        }

        @media (prefers-reduced-motion: reduce) {
            .hud-root,
            .hud-drawer,
            .pill-shell,
            .cmd-ticker-item,
            .pill-shimmer,
            .status-ring {
                animation: none !important;
                transition: none !important;
            }
        }
    `;

    // Build DOM inside Closed Shadow Root
    const root = document.createElement("div");
    root.className = "hud-root";

    // Expandable Drawer
    const drawer = document.createElement("div");
    drawer.className = "hud-drawer";

    const drawerHeader = document.createElement("div");
    drawerHeader.className = "drawer-header";

    const drawerTabs = document.createElement("div");
    drawerTabs.className = "drawer-tabs";

    const rpcTabBtn = document.createElement("button");
    rpcTabBtn.className = "drawer-tab-btn active";
    rpcTabBtn.textContent = "Live RPC Stream";

    const curlTabBtn = document.createElement("button");
    curlTabBtn.className = "drawer-tab-btn";
    curlTabBtn.textContent = "Network & cURL";

    drawerTabs.appendChild(rpcTabBtn);
    drawerTabs.appendChild(curlTabBtn);

    const drawerActions = document.createElement("div");
    drawerActions.className = "drawer-actions";

    const killBtn = document.createElement("button");
    killBtn.className = "kill-btn";
    killBtn.textContent = "Detach ⏹";
    killBtn.title = "Pause and detach agent from this tab";

    drawerActions.appendChild(killBtn);
    drawerHeader.appendChild(drawerTabs);
    drawerHeader.appendChild(drawerActions);

    const drawerBody = document.createElement("div");
    drawerBody.className = "drawer-body";

    drawer.appendChild(drawerHeader);
    drawer.appendChild(drawerBody);

    // Compact Pill Shell
    const pillShell = document.createElement("div");
    pillShell.className = "pill-shell running";

    const shimmer = document.createElement("div");
    shimmer.className = "pill-shimmer";

    const pillInner = document.createElement("div");
    pillInner.className = "pill-inner";

    // Row 1: Tab Header
    const headerRow = document.createElement("div");
    headerRow.className = "pill-header-row";

    const headerLeft = document.createElement("div");
    headerLeft.className = "header-left";

    const statusWrap = document.createElement("span");
    statusWrap.className = "status-dot-wrap";
    const statusRing = document.createElement("span");
    statusRing.className = "status-ring";
    const statusDot = document.createElement("span");
    statusDot.className = "status-dot";
    const doneCheck = document.createElement("span");
    doneCheck.className = "done-check";
    doneCheck.textContent = "✓";
    statusWrap.appendChild(statusRing);
    statusWrap.appendChild(statusDot);
    statusWrap.appendChild(doneCheck);

    const multiTabBadge = document.createElement("span");
    multiTabBadge.className = "multi-tab-badge";
    multiTabBadge.textContent = "⚡ 1 Tab";

    const tabHeaderLabel = document.createElement("span");
    tabHeaderLabel.className = "tab-header-label";

    headerLeft.appendChild(statusWrap);
    headerLeft.appendChild(multiTabBadge);
    headerLeft.appendChild(tabHeaderLabel);

    const headerRight = document.createElement("div");
    headerRight.className = "header-right";

    const jumpBtn = document.createElement("button");
    jumpBtn.className = "jump-btn";
    jumpBtn.textContent = "Switch ↗";
    jumpBtn.style.setProperty("display", "none", "important");

    const prevTabBtn = document.createElement("button");
    prevTabBtn.className = "icon-btn";
    prevTabBtn.textContent = "‹";
    prevTabBtn.title = "Previous active tab task";
    prevTabBtn.style.setProperty("display", "none", "important");

    const nextTabBtn = document.createElement("button");
    nextTabBtn.className = "icon-btn";
    nextTabBtn.textContent = "›";
    nextTabBtn.title = "Next active tab task";
    nextTabBtn.style.setProperty("display", "none", "important");

    const closeBtn = document.createElement("button");
    closeBtn.className = "icon-btn";
    closeBtn.textContent = "×";
    closeBtn.title = "Hide HUD pill";

    headerRight.appendChild(prevTabBtn);
    headerRight.appendChild(nextTabBtn);
    headerRight.appendChild(jumpBtn);
    headerRight.appendChild(closeBtn);

    headerRow.appendChild(headerLeft);
    headerRow.appendChild(headerRight);

    // Row 2: Task Intent
    const intentRow = document.createElement("div");
    intentRow.className = "intent-row";
    const intentPrefix = document.createElement("span");
    intentPrefix.className = "intent-prefix";
    intentPrefix.textContent = "🎯 Intent";
    const intentText = document.createElement("span");
    intentText.className = "intent-text";
    intentRow.appendChild(intentPrefix);
    intentRow.appendChild(intentText);

    // Row 3: Command Ticker
    const cmdViewport = document.createElement("div");
    cmdViewport.className = "cmd-ticker-viewport";
    const cmdItem = document.createElement("div");
    cmdItem.className = "cmd-ticker-item";
    cmdViewport.appendChild(cmdItem);

    pillInner.appendChild(headerRow);
    pillInner.appendChild(intentRow);
    pillInner.appendChild(cmdViewport);

    pillShell.appendChild(shimmer);
    pillShell.appendChild(pillInner);

    root.appendChild(drawer);
    root.appendChild(pillShell);

    shadow.appendChild(styleEl);
    shadow.appendChild(root);

    const mountHostToDom = () => {
        removeLegacyBanners();
        const parent = document.documentElement || document.body;
        if (parent && !host.parentNode) {
            parent.appendChild(host);
        }
    };

    if (document.documentElement || document.body) {
        mountHostToDom();
    } else {
        window.addEventListener("DOMContentLoaded", mountHostToDom, { once: true });
    }

    // Internal HUD State
    let activeTasks = [];
    let selectedTaskIdx = 0;
    let viewerTabId = null;
    let isMounted = false;
    let isDrawerOpen = false;
    let activeDrawerTab = "rpc"; // "rpc" | "curl"
    let lastRenderedCmd = "";
    let autoHideTimer = null;
    let unmountTimer = null;
    let hudPort = null;

    function clearTimers() {
        if (autoHideTimer) {
            clearTimeout(autoHideTimer);
            autoHideTimer = null;
        }
        if (unmountTimer) {
            clearTimeout(unmountTimer);
            unmountTimer = null;
        }
    }

    function showPopoverTopLayer() {
        mountHostToDom();
        clearTimers();
        host.style.setProperty("display", "block", "important");
        try {
            if (typeof host.showPopover === "function") {
                host.showPopover();
            }
        } catch (_) {}
        requestAnimationFrame(() => {
            root.classList.remove("unmounting");
            root.classList.add("mounted");
            isMounted = true;
        });
    }

    function hidePopoverTopLayer() {
        clearTimers();
        isDrawerOpen = false;
        drawer.classList.remove("open");
        root.classList.remove("mounted");
        root.classList.add("unmounting");
        isMounted = false;

        unmountTimer = setTimeout(() => {
            try {
                if (typeof host.hidePopover === "function") {
                    host.hidePopover();
                }
            } catch (_) {}
            host.style.setProperty("display", "none", "important");
            root.classList.remove("unmounting");
        }, UNMOUNT_ANIM_MS);
    }

    function scheduleIdleAutoHide(delayMs = AUTO_HIDE_DELAY_MS) {
        if (autoHideTimer) clearTimeout(autoHideTimer);
        autoHideTimer = setTimeout(() => {
            if (!isDrawerOpen) {
                hidePopoverTopLayer();
            }
        }, delayMs);
    }

    function animateCmdTicker(newCmdText) {
        const clean = (newCmdText || "").trim();
        if (!clean || clean === lastRenderedCmd) {
            if (!cmdItem.textContent) cmdItem.textContent = clean;
            return;
        }
        if (!lastRenderedCmd) {
            lastRenderedCmd = clean;
            cmdItem.textContent = clean;
            return;
        }
        lastRenderedCmd = clean;
        requestAnimationFrame(() => {
            cmdItem.classList.add("slide-out");
            setTimeout(() => {
                cmdItem.classList.remove("slide-out");
                cmdItem.classList.add("slide-in-start");
                cmdItem.textContent = clean;
                requestAnimationFrame(() => {
                    requestAnimationFrame(() => {
                        cmdItem.classList.remove("slide-in-start");
                    });
                });
            }, 90);
        });
    }

    function renderDrawer(currentTask) {
        if (!isDrawerOpen || !currentTask) return;
        drawerBody.innerHTML = "";

        if (activeDrawerTab === "rpc") {
            const logs = Array.isArray(currentTask.logs) ? currentTask.logs : [];
            if (logs.length === 0) {
                const empty = document.createElement("div");
                empty.style.color = "#64748b";
                empty.textContent = "No RPC tool calls recorded yet.";
                drawerBody.appendChild(empty);
                return;
            }
            for (const entry of logs.slice().reverse()) {
                const item = document.createElement("div");
                item.className = "log-item";

                const top = document.createElement("div");
                top.className = "log-top";
                const toolSpan = document.createElement("span");
                toolSpan.textContent = entry.toolName || entry.tool_name || "mcp_tool";
                const latSpan = document.createElement("span");
                latSpan.style.color = "#94a3b8";
                latSpan.textContent = entry.latencyMs != null ? `${entry.latencyMs}ms` : (entry.status || "running");
                top.appendChild(toolSpan);
                top.appendChild(latSpan);

                const cmd = document.createElement("div");
                cmd.className = "log-cmd";
                cmd.textContent = entry.executingCmd || entry.executing_cmd || "";

                item.appendChild(top);
                item.appendChild(cmd);
                drawerBody.appendChild(item);
            }
        } else {
            const calls = Array.isArray(currentTask.networkCalls) ? currentTask.networkCalls : [];
            if (calls.length === 0) {
                const empty = document.createElement("div");
                empty.style.color = "#64748b";
                empty.textContent = "No correlated XHR/Fetch calls captured in the last 3000ms.";
                drawerBody.appendChild(empty);
                return;
            }
            for (const net of calls.slice().reverse()) {
                const item = document.createElement("div");
                item.className = "log-item";

                const top = document.createElement("div");
                top.className = "log-top";
                const methodSpan = document.createElement("span");
                methodSpan.textContent = `${net.method || "GET"} ${net.status ? `[${net.status}]` : ""}`;
                const typeSpan = document.createElement("span");
                typeSpan.style.color = "#94a3b8";
                typeSpan.textContent = net.resourceType || "Fetch";
                top.appendChild(methodSpan);
                top.appendChild(typeSpan);

                const urlDiv = document.createElement("div");
                urlDiv.className = "log-cmd";
                urlDiv.textContent = net.url || "";

                const actions = document.createElement("div");
                actions.className = "curl-actions";

                const copyBtn = document.createElement("button");
                copyBtn.className = "mini-btn";
                copyBtn.textContent = "📋 Copy cURL";
                copyBtn.addEventListener("click", (e) => {
                    e.stopPropagation();
                    if (navigator.clipboard && net.curlCommand) {
                        navigator.clipboard.writeText(net.curlCommand);
                        copyBtn.textContent = "✓ Copied";
                        setTimeout(() => { copyBtn.textContent = "📋 Copy cURL"; }, 1200);
                    }
                });

                const replayBtn = document.createElement("button");
                replayBtn.className = "mini-btn";
                replayBtn.textContent = "⚡ Replay via Rust";
                replayBtn.addEventListener("click", (e) => {
                    e.stopPropagation();
                    replayBtn.textContent = "Running...";
                    chrome.runtime.sendMessage(
                        {
                            action: "hud_replay_curl",
                            tabId: currentTask.tabId,
                            requestId: net.requestId,
                            curl: net.curlCommand
                        },
                        (resp) => {
                            const code = resp?.result?.status_code || resp?.result?.status || "OK";
                            replayBtn.textContent = `✓ ${code}`;
                            setTimeout(() => { replayBtn.textContent = "⚡ Replay via Rust"; }, 1800);
                        }
                    );
                });

                actions.appendChild(copyBtn);
                actions.appendChild(replayBtn);

                item.appendChild(top);
                item.appendChild(urlDiv);
                item.appendChild(actions);
                drawerBody.appendChild(item);
            }
        }
    }

    function renderTasks(tasks, currentTabId) {
        if (currentTabId != null) {
            viewerTabId = currentTabId;
        }
        activeTasks = Array.isArray(tasks) ? tasks : [];

        if (activeTasks.length === 0) {
            if (isMounted) {
                hidePopoverTopLayer();
            }
            return;
        }

        // Prefer showing the task on the current viewer tab first, or most recently updated task
        if (selectedTaskIdx >= activeTasks.length) {
            selectedTaskIdx = 0;
        }

        const task = activeTasks[selectedTaskIdx] || activeTasks[0];
        const status = task.status || "running";

        pillShell.classList.remove("running", "completed", "error", "multi-tab");
        if (status === "completed") {
            pillShell.classList.add("completed");
        } else if (status === "error") {
            pillShell.classList.add("error");
        } else {
            pillShell.classList.add("running");
        }

        if (activeTasks.length > 1) {
            pillShell.classList.add("multi-tab");
            multiTabBadge.textContent = `⚡ ${activeTasks.length} Tabs`;
            prevTabBtn.style.setProperty("display", "inline-flex", "important");
            nextTabBtn.style.setProperty("display", "inline-flex", "important");
        } else {
            prevTabBtn.style.setProperty("display", "none", "important");
            nextTabBtn.style.setProperty("display", "none", "important");
        }

        const cleanTitle = (task.tabTitle || "Active Tab").trim();
        tabHeaderLabel.innerHTML = "";
        const idSpan = document.createElement("span");
        idSpan.className = "tab-id-pill";
        idSpan.textContent = `Tab #${task.tabId}`;
        const sepSpan = document.createElement("span");
        sepSpan.textContent = ` · ${cleanTitle}`;
        tabHeaderLabel.appendChild(idSpan);
        tabHeaderLabel.appendChild(sepSpan);
        tabHeaderLabel.title = `Tab #${task.tabId} — ${cleanTitle}`;

        // Show [Switch ↗] button whenever the user is viewing a different tab than the task's tab
        if (viewerTabId && task.tabId && Number(viewerTabId) !== Number(task.tabId)) {
            jumpBtn.style.setProperty("display", "inline-flex", "important");
            jumpBtn.onclick = (e) => {
                e.stopPropagation();
                chrome.runtime.sendMessage({ action: "hud_focus_tab", tabId: task.tabId });
            };
        } else {
            jumpBtn.style.setProperty("display", "none", "important");
        }

        // Intent row
        const intentStr = (task.intent || "Executing browser automation step").slice(0, 180);
        intentPrefix.textContent = status === "completed" ? "✓ Done ::" : "🎯 Intent ::";
        intentText.textContent = intentStr;
        intentRow.title = intentStr;

        // Live command ticker
        const execStr = task.executingCmd || `Executing :: active step on Tab :: #${task.tabId} (${cleanTitle})`;
        animateCmdTicker(execStr);

        if (!isMounted) {
            showPopoverTopLayer();
        }

        if (isDrawerOpen) {
            renderDrawer(task);
        }

        // Automatic idle auto-hide 2500ms after completion so the pill is NEVER stuck permanently
        const allDone = activeTasks.every((t) => t.status === "completed" || t.status === "error");
        if (allDone) {
            scheduleIdleAutoHide(AUTO_HIDE_DELAY_MS);
        } else {
            if (autoHideTimer) {
                clearTimeout(autoHideTimer);
                autoHideTimer = null;
            }
        }
    }

    // Event Listeners inside Closed Shadow DOM
    pillInner.addEventListener("click", (e) => {
        e.stopPropagation();
        isDrawerOpen = !isDrawerOpen;
        if (isDrawerOpen) {
            if (autoHideTimer) {
                clearTimeout(autoHideTimer);
                autoHideTimer = null;
            }
            drawer.classList.add("open");
            renderDrawer(activeTasks[selectedTaskIdx] || activeTasks[0]);
        } else {
            drawer.classList.remove("open");
        }
    });

    prevTabBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        if (activeTasks.length > 1) {
            selectedTaskIdx = (selectedTaskIdx - 1 + activeTasks.length) % activeTasks.length;
            renderTasks(activeTasks, viewerTabId);
        }
    });

    nextTabBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        if (activeTasks.length > 1) {
            selectedTaskIdx = (selectedTaskIdx + 1) % activeTasks.length;
            renderTasks(activeTasks, viewerTabId);
        }
    });

    closeBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        hidePopoverTopLayer();
    });

    rpcTabBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        activeDrawerTab = "rpc";
        rpcTabBtn.classList.add("active");
        curlTabBtn.classList.remove("active");
        renderDrawer(activeTasks[selectedTaskIdx] || activeTasks[0]);
    });

    curlTabBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        activeDrawerTab = "curl";
        curlTabBtn.classList.add("active");
        rpcTabBtn.classList.remove("active");
        renderDrawer(activeTasks[selectedTaskIdx] || activeTasks[0]);
    });

    killBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        const currentTask = activeTasks[selectedTaskIdx] || activeTasks[0];
        if (currentTask && currentTask.tabId) {
            chrome.runtime.sendMessage({ action: "detach_tab", tabId: currentTask.tabId }, () => {
                hidePopoverTopLayer();
            });
        } else {
            hidePopoverTopLayer();
        }
    });

    // 1. Persistent Port Connection (`mcp-hud-port`) for instant cross-tab hydration
    function connectHudPort() {
        if (typeof chrome === "undefined" || !chrome.runtime || !chrome.runtime.connect) return;
        try {
            hudPort = chrome.runtime.connect({ name: "mcp-hud-port" });
            hudPort.onMessage.addListener((msg) => {
                if (!msg || typeof msg !== "object") return;
                if (msg.type === "HUD_SYNC") {
                    renderTasks(msg.tasks || [], msg.viewerTabId);
                } else if (msg.type === "CLOAK_HUD") {
                    host.style.setProperty("visibility", msg.cloak ? "hidden" : "visible", "important");
                } else if (msg.type === "CLEAR_BANNER") {
                    hidePopoverTopLayer();
                }
            });
            hudPort.onDisconnect.addListener(() => {
                hudPort = null;
                setTimeout(connectHudPort, 1500);
            });
        } catch (_) {}
    }
    connectHudPort();

    // 2. Direct Message Listener (CLOAK_HUD, HUD_SYNC, and legacy UPDATE_BANNER / CLEAR_BANNER compatibility)
    if (typeof chrome !== "undefined" && chrome.runtime && chrome.runtime.onMessage) {
        chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
            if (!message || typeof message !== "object") return false;

            if (message.type === "CLOAK_HUD") {
                host.style.setProperty("visibility", "hidden", "important");
                requestAnimationFrame(() => {
                    sendResponse({ success: true, cloaked: true });
                });
                return true;
            }
            if (message.type === "UNCLOAK_HUD") {
                host.style.setProperty("visibility", "visible", "important");
                sendResponse({ success: true, cloaked: false });
                return true;
            }
            if (message.type === "HUD_SYNC") {
                renderTasks(message.tasks || [], message.viewerTabId);
                sendResponse({ success: true });
                return true;
            }
            if (message.type === "UPDATE_BANNER") {
                // Upgrade legacy UPDATE_BANNER into a sleek HUD task state with automatic 2500ms idle hide
                const syntheticTask = {
                    tabId: message.activeTabId || 1,
                    tabTitle: message.activeTabTitle || document.title || "Active Tab",
                    intent: message.intent || `Agent active on Tab #${message.activeTabId || 1}`,
                    executingCmd: message.executingCmd || `Executing :: browser action on Tab :: #${message.activeTabId || 1} (${message.activeTabTitle || document.title || "Tab"})`,
                    status: message.status || "running",
                    updatedAt: Date.now(),
                    logs: [],
                    networkCalls: []
                };
                renderTasks([syntheticTask], message.viewerTabId);
                sendResponse({ success: true });
                return true;
            }
            if (message.type === "CLEAR_BANNER") {
                hidePopoverTopLayer();
                sendResponse({ success: true });
                return true;
            }
            return false;
        });
    }

    // Backwards-compatible controller handle
    window.__onlydomsBanner = {
        update(tabId, title, intent, executingCmd, status = "running") {
            renderTasks([{
                tabId,
                tabTitle: title || "Untitled Tab",
                intent: intent || `Inspecting Tab #${tabId}`,
                executingCmd: executingCmd || `Executing :: command on Tab :: #${tabId} (${title || "Untitled Tab"})`,
                status,
                updatedAt: Date.now(),
                logs: [],
                networkCalls: []
            }], viewerTabId);
        },
        hide() {
            hidePopoverTopLayer();
        }
    };
})();
