/**
 * OnlyDOMs Top-Layer Agent HUD (<mcp-agent-hud>)
 * Master Specification § 1, § 2, § 3, § 4, § 6
 * Modern Web Guidance & Award-Grade Developer UX:
 * - Drag & drop repositioning anywhere on screen with dedicated grip handle & viewport clamping
 * - Accordion-expandable telemetry items showing Intent, Agent Request, Browser Response, and subtle Rust token sanitization footnote
 * - Strict hit-test safety: zero unclickable background elements upon dismissal/timeout
 * - Streamlined UI with reclaimed negative space (removed bloated hero card, slim 34px header)
 * - 100% SVG iconography replacing all emojis
 * - Clean typographical hierarchy with zero rainbow-pill clutter
 * - Automatic downward/upward drawer flipping based on viewport position
 * - Closed Shadow DOM isolation with zero page query leakage
 * - Multi-tier semantic design tokens with dark glassmorphism
 */

(() => {
    if (window !== window.top) return;

    // Immediately clean up any legacy banner elements if present
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
    const AUTO_HIDE_DELAY_MS = 6000;
    const UNPIN_HIDE_DELAY_MS = 1200;
    const UNMOUNT_ANIM_MS = 200;
    const STORAGE_KEY_POS = "__onlydoms_hud_pos__";

    // Register Custom Element <mcp-agent-hud> if needed
    try {
        if (typeof customElements !== "undefined" && !customElements.get(HUD_TAG)) {
            class McpAgentHudElement extends HTMLElement {}
            customElements.define(HUD_TAG, McpAgentHudElement);
        }
    } catch (_) {}

    // Clean up any stale host
    const existingHost = document.getElementById(HUD_ID);
    if (existingHost && existingHost.parentNode) {
        existingHost.parentNode.removeChild(existingHost);
    }

    const host = document.createElement(HUD_TAG);
    host.id = HUD_ID;
    host.setAttribute(HUD_ATTR, "true");
    host.setAttribute("popover", "manual");

    // Host initial style
    host.style.setProperty("all", "initial", "important");
    host.style.setProperty("position", "fixed", "important");
    host.style.setProperty("margin", "0", "important");
    host.style.setProperty("padding", "0", "important");
    host.style.setProperty("border", "none", "important");
    host.style.setProperty("background", "transparent", "important");
    host.style.setProperty("overflow", "visible", "important");
    host.style.setProperty("z-index", "2147483647", "important");
    host.style.setProperty("pointer-events", "none", "important");
    host.style.setProperty("display", "none", "important");

    // Closed Shadow DOM
    const shadow = host.attachShadow({ mode: "closed" });

    // Inline SVG Icon Library (100% vector, zero emojis)
    const ICONS = {
        grip: `<svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" aria-hidden="true"><circle cx="5" cy="3.5" r="1.2"/><circle cx="11" cy="3.5" r="1.2"/><circle cx="5" cy="8" r="1.2"/><circle cx="11" cy="8" r="1.2"/><circle cx="5" cy="12.5" r="1.2"/><circle cx="11" cy="12.5" r="1.2"/></svg>`,
        telemetry: `<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M2 4.5l3.5 3.5L2 11.5M7 11.5h7"/></svg>`,
        network: `<svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 5h10M10.5 2.5l2.5 2.5-2.5 2.5M13 11H3M5.5 13.5L3 11l2.5-2.5"/></svg>`,
        copy: `<svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="5" width="8.5" height="8.5" rx="1.5"/><path d="M3 11V3h8"/></svg>`,
        detach: `<svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 2H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h5M11 11l3-3-3-3M6 8h8"/></svg>`,
        pin: `<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><g transform="rotate(45 12 12)"><line x1="12" y1="17" x2="12" y2="22"/><path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z"/></g></svg>`,
        chevron: `<svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 6l4 4 4-4"/></svg>`,
        close: `<svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8"/></svg>`,
        check: `<svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7"/></svg>`,
        info: `<svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="8" cy="8" r="6"/><path d="M8 7v4M8 5h.01"/></svg>`,
        arrowRight: `<svg viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 3l5 5-5 5"/></svg>`
    };

    const styleEl = document.createElement("style");
    styleEl.textContent = `
        :host {
            all: initial !important;
            position: fixed !important;
            z-index: 2147483647 !important;
            pointer-events: none !important;
            margin: 0 !important;
            padding: 0 !important;
            border: none !important;
            background: transparent !important;
            overflow: visible !important;
            font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Geist", "Inter", "Segoe UI", Roboto, sans-serif !important;
            color: #f8fafc !important;
            -webkit-font-smoothing: antialiased !important;
            color-scheme: dark !important;
        }

        /* Strict popover-open gating: when closed, completely remove from layout & pointer events */
        :host([popover]:popover-open) {
            display: block !important;
        }

        :host(:not(:popover-open)),
        :host([data-hidden="true"]),
        .hud-root:not(.mounted),
        .hud-root.unmounting {
            display: none !important;
            visibility: hidden !important;
            pointer-events: none !important;
            opacity: 0 !important;
        }

        :host([data-mcp-cloaked="true"]),
        :host(.cloaked),
        .hud-root.cloaked,
        .hud-root.cloaked * {
            display: none !important;
            visibility: hidden !important;
            opacity: 0 !important;
            pointer-events: none !important;
        }

        * {
            box-sizing: border-box !important;
        }

        /* Root flex container: fixed position, driven by draggable left/top */
        .hud-root {
            position: fixed !important;
            z-index: 2147483647 !important;
            pointer-events: none !important;
            display: flex !important;
            flex-direction: column !important;
            align-items: flex-end !important;
            gap: 8px !important;
            margin: 0 !important;
            padding: 0 !important;
            will-change: transform, opacity, left, top !important;
            opacity: 0;
            transform: translate3d(0, 8px, 0) scale(0.97);
            transition:
                opacity 220ms cubic-bezier(0.16, 1, 0.3, 1),
                transform 220ms cubic-bezier(0.16, 1, 0.3, 1) !important;
        }

        .hud-root.flip-down {
            flex-direction: column-reverse !important;
        }

        .hud-root.mounted {
            opacity: 1 !important;
            transform: translate3d(0, 0, 0) scale(1) !important;
        }

        .hud-root.unmounting {
            opacity: 0 !important;
            transform: translate3d(0, 6px, 0) scale(0.97) !important;
            transition: opacity 160ms ease, transform 160ms ease !important;
        }

        /* -------------------------------------------------------------
           Expandable Telemetry & cURL Drawer
        ------------------------------------------------------------- */
        .hud-drawer {
            pointer-events: none !important;
            width: 440px !important;
            max-width: calc(100vw - 28px) !important;
            height: 380px !important;
            max-height: calc(100vh - 100px) !important;
            background: rgba(10, 14, 23, 0.96) !important;
            backdrop-filter: blur(24px) saturate(190%) !important;
            -webkit-backdrop-filter: blur(24px) saturate(190%) !important;
            border: 1px solid rgba(255, 255, 255, 0.10) !important;
            border-radius: 14px !important;
            box-shadow:
                0 20px 50px -6px rgba(0, 0, 0, 0.75),
                0 0 0 1px rgba(255, 255, 255, 0.04) inset,
                0 2px 4px rgba(0, 0, 0, 0.4) !important;
            display: none !important;
            flex-direction: column !important;
            overflow: hidden !important;
            transform-origin: bottom right !important;
            transform: translate3d(0, 8px, 0) scale(0.97) !important;
            opacity: 0 !important;
            transition:
                display 200ms allow-discrete,
                opacity 200ms cubic-bezier(0.16, 1, 0.3, 1),
                transform 200ms cubic-bezier(0.16, 1, 0.3, 1) !important;
        }

        .hud-root.flip-down .hud-drawer {
            transform-origin: top right !important;
            transform: translate3d(0, -8px, 0) scale(0.97) !important;
        }

        .hud-drawer.open {
            display: flex !important;
            opacity: 1 !important;
            transform: translate3d(0, 0, 0) scale(1) !important;
            pointer-events: auto !important;

            @starting-style {
                opacity: 0;
                transform: translate3d(0, 8px, 0) scale(0.97);
            }
        }

        /* Slim 34px Header */
        .drawer-header {
            display: flex !important;
            align-items: center !important;
            justify-content: space-between !important;
            height: 38px !important;
            padding: 0 10px !important;
            border-bottom: 1px solid rgba(255, 255, 255, 0.08) !important;
            background: rgba(255, 255, 255, 0.02) !important;
            gap: 8px !important;
            flex-shrink: 0 !important;
        }

        .drawer-header-left {
            display: flex !important;
            align-items: center !important;
            gap: 6px !important;
        }

        .drawer-tabs {
            display: flex !important;
            gap: 3px !important;
            background: rgba(0, 0, 0, 0.4) !important;
            padding: 2px !important;
            border-radius: 7px !important;
            border: 1px solid rgba(255, 255, 255, 0.06) !important;
        }

        .drawer-tab-btn {
            all: unset !important;
            cursor: pointer !important;
            font-size: 11px !important;
            font-weight: 600 !important;
            padding: 3px 8px !important;
            border-radius: 5px !important;
            color: #94a3b8 !important;
            display: flex !important;
            align-items: center !important;
            gap: 5px !important;
            transition: background 150ms ease, color 150ms ease !important;
        }

        .drawer-tab-btn:hover {
            color: #f1f5f9 !important;
            background: rgba(255, 255, 255, 0.06) !important;
        }

        .drawer-tab-btn.active {
            background: rgba(56, 189, 248, 0.16) !important;
            color: #7dd3fc !important;
        }

        .tab-count-badge {
            font-size: 10px !important;
            color: inherit !important;
            opacity: 0.8 !important;
        }

        .drawer-header-right {
            display: flex !important;
            align-items: center !important;
            gap: 4px !important;
        }

        .icon-action-btn {
            all: unset !important;
            cursor: pointer !important;
            width: 24px !important;
            height: 24px !important;
            display: inline-flex !important;
            align-items: center !important;
            justify-content: center !important;
            border-radius: 5px !important;
            color: #94a3b8 !important;
            transition: background 150ms ease, color 150ms ease !important;
        }

        .icon-action-btn:hover {
            color: #ffffff !important;
            background: rgba(255, 255, 255, 0.08) !important;
        }

        .icon-action-btn.danger:hover {
            color: #f43f5e !important;
            background: rgba(244, 63, 94, 0.15) !important;
        }

        /* Slim Single-Line Status Sub-Bar */
        .drawer-sub-bar {
            display: flex !important;
            align-items: center !important;
            justify-content: space-between !important;
            padding: 5px 12px !important;
            border-bottom: 1px solid rgba(255, 255, 255, 0.05) !important;
            background: rgba(0, 0, 0, 0.25) !important;
            font-size: 11px !important;
            color: #94a3b8 !important;
            flex-shrink: 0 !important;
        }

        .sub-bar-tab-title {
            color: #e2e8f0 !important;
            font-weight: 500 !important;
            white-space: nowrap !important;
            overflow: hidden !important;
            text-overflow: ellipsis !important;
            max-width: 280px !important;
        }

        .sub-bar-status {
            display: flex !important;
            align-items: center !important;
            gap: 5px !important;
            font-size: 10.5px !important;
            font-weight: 600 !important;
            color: #38bdf8 !important;
        }

        .sub-bar-status.completed {
            color: #34d399 !important;
        }

        .sub-bar-status.error {
            color: #f87171 !important;
        }

        /* Scrollable Telemetry Stream */
        .drawer-body {
            flex: 1 1 auto !important;
            min-height: 0 !important;
            overflow-y: auto !important;
            overscroll-behavior: contain !important;
            padding: 8px 10px !important;
            display: flex !important;
            flex-direction: column !important;
            gap: 6px !important;
            font-size: 11.5px !important;
            scrollbar-width: thin !important;
            scrollbar-color: rgba(255, 255, 255, 0.15) transparent !important;
        }

        .drawer-body::-webkit-scrollbar {
            width: 5px !important;
        }

        .drawer-body::-webkit-scrollbar-thumb {
            background: rgba(255, 255, 255, 0.15) !important;
            border-radius: 999px !important;
        }

        /* -------------------------------------------------------------
           Accordion Telemetry Cards
        ------------------------------------------------------------- */
        .accordion-item {
            flex-shrink: 0 !important;
            border-radius: 8px !important;
            background: rgba(255, 255, 255, 0.03) !important;
            border: 1px solid rgba(255, 255, 255, 0.06) !important;
            display: flex !important;
            flex-direction: column !important;
            overflow: hidden !important;
            transition: background 150ms ease, border-color 150ms ease !important;
        }

        .accordion-item:hover {
            background: rgba(255, 255, 255, 0.05) !important;
            border-color: rgba(255, 255, 255, 0.1) !important;
        }

        .accordion-item.expanded {
            background: rgba(255, 255, 255, 0.05) !important;
            border-color: rgba(56, 189, 248, 0.3) !important;
        }

        .accordion-header {
            display: flex !important;
            align-items: center !important;
            justify-content: space-between !important;
            padding: 7px 9px !important;
            cursor: pointer !important;
            user-select: none !important;
            gap: 8px !important;
        }

        .accordion-title-group {
            display: flex !important;
            align-items: center !important;
            gap: 7px !important;
            overflow: hidden !important;
            min-width: 0 !important;
            flex: 1 !important;
        }

        .accordion-chevron {
            font-size: 10px !important;
            color: #64748b !important;
            transition: transform 180ms ease, color 150ms ease !important;
            display: inline-flex !important;
            align-items: center !important;
            justify-content: center !important;
            flex-shrink: 0 !important;
        }

        .accordion-item.expanded .accordion-chevron {
            transform: rotate(90deg) !important;
            color: #38bdf8 !important;
        }

        .step-index-label {
            font-size: 10.5px !important;
            font-family: ui-monospace, "SF Mono", monospace !important;
            color: #64748b !important;
            flex-shrink: 0 !important;
        }

        .tool-name-label {
            font-family: ui-monospace, "SF Mono", monospace !important;
            font-size: 11px !important;
            font-weight: 600 !important;
            color: #93c5fd !important;
            white-space: nowrap !important;
            overflow: hidden !important;
            text-overflow: ellipsis !important;
        }

        .cmd-preview-label {
            font-size: 11px !important;
            color: #94a3b8 !important;
            white-space: nowrap !important;
            overflow: hidden !important;
            text-overflow: ellipsis !important;
            flex: 1 !important;
        }

        .accordion-meta {
            display: flex !important;
            align-items: center !important;
            gap: 6px !important;
            font-size: 10.5px !important;
            color: #64748b !important;
            flex-shrink: 0 !important;
        }

        .latency-tag {
            font-size: 10px !important;
            color: #94a3b8 !important;
        }

        /* Expanded Accordion Body */
        .accordion-content {
            display: none !important;
            padding: 8px 10px 10px !important;
            border-top: 1px solid rgba(255, 255, 255, 0.05) !important;
            background: rgba(0, 0, 0, 0.3) !important;
            flex-direction: column !important;
            gap: 7px !important;
            font-size: 11px !important;
            max-height: 280px !important;
            overflow-y: auto !important;
            overscroll-behavior: contain !important;
            scrollbar-width: thin !important;
            scrollbar-color: rgba(255, 255, 255, 0.2) transparent !important;
        }

        .accordion-content::-webkit-scrollbar {
            width: 5px !important;
        }

        .accordion-content::-webkit-scrollbar-thumb {
            background: rgba(255, 255, 255, 0.2) !important;
            border-radius: 4px !important;
        }

        .accordion-item.expanded .accordion-content {
            display: flex !important;
        }

        .detail-section {
            display: flex !important;
            flex-direction: column !important;
            gap: 3px !important;
        }

        .detail-heading {
            font-size: 10px !important;
            font-weight: 700 !important;
            text-transform: uppercase !important;
            letter-spacing: 0.04em !important;
            color: #94a3b8 !important;
            display: flex !important;
            align-items: center !important;
            justify-content: space-between !important;
        }

        .detail-copy-btn {
            all: unset !important;
            cursor: pointer !important;
            font-size: 10px !important;
            color: #38bdf8 !important;
            padding: 1px 4px !important;
            border-radius: 3px !important;
            transition: background 150ms ease !important;
        }

        .detail-copy-btn:hover {
            background: rgba(56, 189, 248, 0.15) !important;
        }

        .code-block {
            font-family: ui-monospace, "SF Mono", monospace !important;
            font-size: 10.5px !important;
            color: #cbd5e1 !important;
            background: rgba(0, 0, 0, 0.45) !important;
            border: 1px solid rgba(255, 255, 255, 0.06) !important;
            padding: 6px 8px !important;
            border-radius: 5px !important;
            white-space: pre-wrap !important;
            word-break: break-word !important;
            overflow-wrap: anywhere !important;
            line-height: 1.4 !important;
            max-height: 140px !important;
            overflow: auto !important;
            overscroll-behavior: contain !important;
            scrollbar-width: thin !important;
            scrollbar-color: rgba(255, 255, 255, 0.2) transparent !important;
        }

        .code-block::-webkit-scrollbar {
            width: 5px !important;
            height: 5px !important;
        }

        .code-block::-webkit-scrollbar-thumb {
            background: rgba(255, 255, 255, 0.2) !important;
            border-radius: 4px !important;
        }

        .code-block.response-block {
            max-height: 180px !important;
            background: rgba(2, 6, 23, 0.65) !important;
            border-color: rgba(56, 189, 248, 0.18) !important;
            outline: none !important;
        }

        .code-block.response-block:focus-visible {
            border-color: rgba(56, 189, 248, 0.5) !important;
            box-shadow: 0 0 0 1px rgba(56, 189, 248, 0.3) !important;
        }

        .rust-sanitization-note {
            display: flex !important;
            align-items: flex-start !important;
            gap: 5px !important;
            font-size: 10px !important;
            color: #64748b !important;
            line-height: 1.35 !important;
            margin-top: 2px !important;
            padding-top: 4px !important;
            border-top: 1px dashed rgba(255, 255, 255, 0.06) !important;
        }

        .rust-sanitization-note svg {
            flex-shrink: 0 !important;
            margin-top: 1px !important;
            color: #38bdf8 !important;
        }

        /* Network cURL rows & badges */
        .curl-row {
            display: flex !important;
            align-items: center !important;
            gap: 6px !important;
            font-size: 11px !important;
            padding: 6px 8px !important;
            border-radius: 6px !important;
            background: rgba(255, 255, 255, 0.02) !important;
            border: 1px solid rgba(255, 255, 255, 0.05) !important;
            flex-shrink: 0 !important;
        }

        .method-label {
            font-family: ui-monospace, "SF Mono", monospace !important;
            font-size: 10px !important;
            font-weight: 700 !important;
            color: #34d399 !important;
            flex-shrink: 0 !important;
        }

        .method-label.post {
            color: #38bdf8 !important;
        }

        .method-label.other {
            color: #fbbf24 !important;
        }

        .status-pill {
            font-family: ui-monospace, "SF Mono", monospace !important;
            font-size: 9.5px !important;
            font-weight: 700 !important;
            padding: 1px 5px !important;
            border-radius: 4px !important;
            flex-shrink: 0 !important;
        }

        .status-pill.ok {
            color: #34d399 !important;
            background: rgba(52, 211, 153, 0.12) !important;
            border: 1px solid rgba(52, 211, 153, 0.25) !important;
        }

        .status-pill.err {
            color: #f43f5e !important;
            background: rgba(244, 63, 94, 0.15) !important;
            border: 1px solid rgba(244, 63, 94, 0.3) !important;
        }

        .status-pill.pending {
            color: #94a3b8 !important;
            background: rgba(148, 163, 184, 0.12) !important;
            border: 1px solid rgba(148, 163, 184, 0.25) !important;
        }

        .network-action-badge {
            font-family: ui-monospace, "SF Mono", monospace !important;
            font-size: 9.5px !important;
            font-weight: 600 !important;
            color: #a78bfa !important;
            background: rgba(167, 139, 250, 0.12) !important;
            border: 1px solid rgba(167, 139, 250, 0.25) !important;
            padding: 1px 5px !important;
            border-radius: 4px !important;
            white-space: nowrap !important;
            flex-shrink: 0 !important;
        }

        .curl-url {
            font-family: ui-monospace, "SF Mono", monospace !important;
            font-size: 10.5px !important;
            color: #cbd5e1 !important;
            white-space: nowrap !important;
            overflow: hidden !important;
            text-overflow: ellipsis !important;
            flex: 1 !important;
        }

        .curl-actions {
            display: flex !important;
            gap: 4px !important;
            flex-shrink: 0 !important;
        }

        .mini-btn {
            all: unset !important;
            cursor: pointer !important;
            font-size: 10px !important;
            font-weight: 600 !important;
            padding: 2px 6px !important;
            border-radius: 4px !important;
            background: rgba(255, 255, 255, 0.06) !important;
            color: #e2e8f0 !important;
            transition: background 150ms ease, color 150ms ease !important;
        }

        .mini-btn:hover {
            background: rgba(56, 189, 248, 0.2) !important;
            color: #38bdf8 !important;
        }

        .empty-placeholder {
            padding: 20px 14px !important;
            text-align: center !important;
            color: #64748b !important;
            font-size: 11.5px !important;
            line-height: 1.4 !important;
            border: 1px dashed rgba(255, 255, 255, 0.08) !important;
            border-radius: 8px !important;
        }

        /* -------------------------------------------------------------
           Compact Floating Pill Shell (Reclaimed Negative Space)
        ------------------------------------------------------------- */
        .pill-shell {
            pointer-events: auto !important;
            position: relative !important;
            width: 330px !important;
            max-width: calc(100vw - 28px) !important;
            border-radius: 12px !important;
            padding: 1px !important;
            background: rgba(255, 255, 255, 0.10) !important;
            box-shadow:
                0 12px 30px -4px rgba(0, 0, 0, 0.65),
                0 2px 6px rgba(0, 0, 0, 0.4) !important;
            overflow: hidden !important;
            transition: border-radius 180ms ease, box-shadow 180ms ease !important;
        }

        .pill-shimmer {
            position: absolute !important;
            inset: -140% !important;
            background: conic-gradient(
                from 0deg,
                transparent 0deg,
                transparent 240deg,
                rgba(56, 189, 248, 0.65) 290deg,
                rgba(129, 140, 248, 0.85) 330deg,
                transparent 360deg
            ) !important;
            opacity: 0 !important;
            transition: opacity 250ms ease !important;
            pointer-events: none !important;
        }

        .pill-shell.running .pill-shimmer {
            opacity: 1 !important;
            animation: hudSpin 3s linear infinite !important;
        }

        @keyframes hudSpin {
            to { transform: rotate(360deg); }
        }

        .pill-inner {
            position: relative !important;
            background: rgba(11, 15, 24, 0.94) !important;
            backdrop-filter: blur(20px) saturate(180%) !important;
            -webkit-backdrop-filter: blur(20px) saturate(180%) !important;
            border-radius: 11px !important;
            padding: 6px 10px !important;
            display: flex !important;
            flex-direction: column !important;
            gap: 4px !important;
            cursor: pointer !important;
            user-select: none !important;
            -webkit-user-select: none !important;
            box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08) !important;
        }

        /* Row 1: Header (Grip + Dot + Title + Controls) */
        .pill-header-row {
            display: flex !important;
            align-items: center !important;
            justify-content: space-between !important;
            gap: 6px !important;
            min-height: 20px !important;
        }

        .header-left {
            display: flex !important;
            align-items: center !important;
            gap: 6px !important;
            min-width: 0 !important;
            flex: 1 !important;
        }

        /* Drag Grip Affordance */
        .hud-drag-handle {
            all: unset !important;
            cursor: grab !important;
            display: inline-flex !important;
            align-items: center !important;
            justify-content: center !important;
            color: #64748b !important;
            padding: 2px !important;
            border-radius: 3px !important;
            touch-action: none !important;
            user-select: none !important;
            -webkit-user-select: none !important;
            flex-shrink: 0 !important;
            transition: color 150ms ease, background 150ms ease !important;
        }

        .hud-drag-handle:hover {
            color: #f1f5f9 !important;
            background: rgba(255, 255, 255, 0.08) !important;
        }

        .hud-drag-handle.dragging {
            cursor: grabbing !important;
            color: #38bdf8 !important;
        }

        /* Status Dot */
        .status-dot-wrap {
            position: relative !important;
            width: 10px !important;
            height: 10px !important;
            display: inline-flex !important;
            align-items: center !important;
            justify-content: center !important;
            flex-shrink: 0 !important;
        }

        .status-dot {
            width: 6px !important;
            height: 6px !important;
            border-radius: 999px !important;
            background: #38bdf8 !important;
            box-shadow: 0 0 8px rgba(56, 189, 248, 0.8) !important;
        }

        .status-ring {
            position: absolute !important;
            inset: -2px !important;
            border-radius: 999px !important;
            border: 1px solid rgba(56, 189, 248, 0.6) !important;
            animation: pulseRing 1.8s cubic-bezier(0.22, 1, 0.36, 1) infinite !important;
        }

        @keyframes pulseRing {
            0% { transform: scale(0.7); opacity: 0.9; }
            100% { transform: scale(1.6); opacity: 0; }
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

        .pill-title-text {
            font-size: 11px !important;
            font-weight: 600 !important;
            color: #e2e8f0 !important;
            white-space: nowrap !important;
            overflow: hidden !important;
            text-overflow: ellipsis !important;
            letter-spacing: -0.01em !important;
        }

        .pill-tab-id {
            color: #94a3b8 !important;
            font-family: ui-monospace, "SF Mono", monospace !important;
            font-weight: 500 !important;
            font-size: 10.5px !important;
            margin-right: 4px !important;
        }

        .header-right {
            display: flex !important;
            align-items: center !important;
            gap: 3px !important;
            flex-shrink: 0 !important;
        }

        .icon-btn {
            all: unset !important;
            cursor: pointer !important;
            color: #94a3b8 !important;
            padding: 3px !important;
            border-radius: 4px !important;
            display: inline-flex !important;
            align-items: center !important;
            justify-content: center !important;
            transition: color 150ms ease, background 150ms ease !important;
        }

        .icon-btn:hover {
            color: #ffffff !important;
            background: rgba(255, 255, 255, 0.08) !important;
        }

        .icon-btn.pinned,
        .icon-action-btn.pinned {
            color: #38bdf8 !important;
            background: rgba(56, 189, 248, 0.2) !important;
            box-shadow: 0 0 10px rgba(56, 189, 248, 0.45) !important;
        }

        .icon-btn.pinned svg path,
        .icon-action-btn.pinned svg path {
            fill: #38bdf8 !important;
            fill-opacity: 0.4 !important;
            stroke: #38bdf8 !important;
        }

        .icon-btn.pinned svg line,
        .icon-action-btn.pinned svg line {
            stroke: #38bdf8 !important;
        }

        .details-chevron {
            transition: transform 200ms ease !important;
        }

        .details-chevron.open {
            transform: rotate(180deg) !important;
        }

        /* Row 2: Streamlined Command / Intent Ticker */
        .cmd-ticker-viewport {
            position: relative !important;
            height: 18px !important;
            overflow: hidden !important;
            background: rgba(0, 0, 0, 0.35) !important;
            border: 1px solid rgba(255, 255, 255, 0.05) !important;
            border-radius: 5px !important;
            padding: 0 6px !important;
            display: flex !important;
            align-items: center !important;
            gap: 5px !important;
        }

        .cmd-prompt {
            color: #38bdf8 !important;
            font-family: ui-monospace, "SF Mono", monospace !important;
            font-size: 10px !important;
            font-weight: 700 !important;
            flex-shrink: 0 !important;
        }

        .cmd-ticker-item {
            width: 100% !important;
            font-family: ui-monospace, "SF Mono", monospace !important;
            font-size: 10px !important;
            color: #cbd5e1 !important;
            white-space: nowrap !important;
            overflow: hidden !important;
            text-overflow: ellipsis !important;
            will-change: transform, opacity !important;
            transition:
                transform 140ms cubic-bezier(0.22, 1, 0.36, 1),
                opacity 140ms cubic-bezier(0.22, 1, 0.36, 1) !important;
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

        @media (prefers-reduced-motion: reduce) {
            .hud-root,
            .hud-drawer,
            .pill-shell,
            .cmd-ticker-item,
            .pill-shimmer,
            .status-ring,
            .details-chevron,
            .accordion-chevron {
                animation: none !important;
                transition: none !important;
            }
        }
    `;

    // Build DOM structure inside Closed Shadow Root
    const root = document.createElement("div");
    root.className = "hud-root";

    // 1. Expandable Telemetry Drawer
    const drawer = document.createElement("div");
    drawer.className = "hud-drawer";

    const drawerHeader = document.createElement("div");
    drawerHeader.className = "drawer-header";

    const drawerHeaderLeft = document.createElement("div");
    drawerHeaderLeft.className = "drawer-header-left";

    const drawerDragHandle = document.createElement("button");
    drawerDragHandle.className = "hud-drag-handle";
    drawerDragHandle.innerHTML = ICONS.grip;
    drawerDragHandle.title = "Drag to move HUD anywhere";
    drawerDragHandle.setAttribute("aria-label", "Drag to move HUD anywhere");

    const drawerTabs = document.createElement("div");
    drawerTabs.className = "drawer-tabs";

    const rpcTabBtn = document.createElement("button");
    rpcTabBtn.className = "drawer-tab-btn active";
    rpcTabBtn.innerHTML = `${ICONS.telemetry}<span>Telemetry</span><span id="__hud_rpc_count" class="tab-count-badge">0</span>`;

    const curlTabBtn = document.createElement("button");
    curlTabBtn.className = "drawer-tab-btn";
    curlTabBtn.innerHTML = `${ICONS.network}<span>Network</span><span id="__hud_curl_count" class="tab-count-badge">0</span>`;

    drawerTabs.appendChild(rpcTabBtn);
    drawerTabs.appendChild(curlTabBtn);

    drawerHeaderLeft.appendChild(drawerDragHandle);
    drawerHeaderLeft.appendChild(drawerTabs);

    const drawerHeaderRight = document.createElement("div");
    drawerHeaderRight.className = "drawer-header-right";

    const drawerPinBtn = document.createElement("button");
    drawerPinBtn.className = "icon-action-btn";
    drawerPinBtn.innerHTML = ICONS.pin;
    drawerPinBtn.title = "Pin HUD to stay on screen";

    const copySummaryBtn = document.createElement("button");
    copySummaryBtn.className = "icon-action-btn";
    copySummaryBtn.innerHTML = ICONS.copy;
    copySummaryBtn.title = "Copy task summary to clipboard";

    const killBtn = document.createElement("button");
    killBtn.className = "icon-action-btn danger";
    killBtn.innerHTML = ICONS.detach;
    killBtn.title = "Detach agent debugger from this tab";

    const drawerCloseBtn = document.createElement("button");
    drawerCloseBtn.className = "icon-action-btn";
    drawerCloseBtn.innerHTML = ICONS.close;
    drawerCloseBtn.title = "Close drawer";

    drawerHeaderRight.appendChild(drawerPinBtn);
    drawerHeaderRight.appendChild(copySummaryBtn);
    drawerHeaderRight.appendChild(killBtn);
    drawerHeaderRight.appendChild(drawerCloseBtn);

    drawerHeader.appendChild(drawerHeaderLeft);
    drawerHeader.appendChild(drawerHeaderRight);

    // Slim Single-Line Status Sub-Bar (replaces the bloated 120px hero card)
    const drawerSubBar = document.createElement("div");
    drawerSubBar.className = "drawer-sub-bar";

    const subBarTitle = document.createElement("span");
    subBarTitle.className = "sub-bar-tab-title";
    subBarTitle.textContent = "Tab #1 · Active Tab";

    const subBarStatus = document.createElement("span");
    subBarStatus.className = "sub-bar-status";
    subBarStatus.textContent = "RUNNING";

    drawerSubBar.appendChild(subBarTitle);
    drawerSubBar.appendChild(subBarStatus);

    const drawerBody = document.createElement("div");
    drawerBody.className = "drawer-body";

    drawer.appendChild(drawerHeader);
    drawer.appendChild(drawerSubBar);
    drawer.appendChild(drawerBody);

    // 2. Compact Floating Pill Shell
    const pillShell = document.createElement("div");
    pillShell.className = "pill-shell running";

    const shimmer = document.createElement("div");
    shimmer.className = "pill-shimmer";

    const pillInner = document.createElement("div");
    pillInner.className = "pill-inner";

    // Row 1: Header (Drag Handle + Status + Title + Actions)
    const headerRow = document.createElement("div");
    headerRow.className = "pill-header-row";

    const headerLeft = document.createElement("div");
    headerLeft.className = "header-left";

    const pillDragHandle = document.createElement("button");
    pillDragHandle.className = "hud-drag-handle";
    pillDragHandle.innerHTML = ICONS.grip;
    pillDragHandle.title = "Drag to move HUD anywhere";
    pillDragHandle.setAttribute("aria-label", "Drag to move HUD anywhere");

    const statusWrap = document.createElement("span");
    statusWrap.className = "status-dot-wrap";
    const statusRing = document.createElement("span");
    statusRing.className = "status-ring";
    const statusDot = document.createElement("span");
    statusDot.className = "status-dot";
    statusWrap.appendChild(statusRing);
    statusWrap.appendChild(statusDot);

    const pillTitle = document.createElement("span");
    pillTitle.className = "pill-title-text";
    pillTitle.innerHTML = `<span class="pill-tab-id">#1</span>Active Tab`;

    headerLeft.appendChild(pillDragHandle);
    headerLeft.appendChild(statusWrap);
    headerLeft.appendChild(pillTitle);

    const headerRight = document.createElement("div");
    headerRight.className = "header-right";

    const detailsBtn = document.createElement("button");
    detailsBtn.className = "icon-btn";
    detailsBtn.innerHTML = `<span class="details-chevron">${ICONS.chevron}</span>`;
    detailsBtn.title = "Toggle detailed telemetry drawer";

    const pinBtn = document.createElement("button");
    pinBtn.className = "icon-btn";
    pinBtn.innerHTML = ICONS.pin;
    pinBtn.title = "Pin HUD to stay on screen";

    const closeBtn = document.createElement("button");
    closeBtn.className = "icon-btn";
    closeBtn.innerHTML = ICONS.close;
    closeBtn.title = "Dismiss HUD";

    headerRight.appendChild(detailsBtn);
    headerRight.appendChild(pinBtn);
    headerRight.appendChild(closeBtn);

    headerRow.appendChild(headerLeft);
    headerRow.appendChild(headerRight);

    // Row 2: Streamlined Command Ticker
    const cmdViewport = document.createElement("div");
    cmdViewport.className = "cmd-ticker-viewport";
    const cmdPrompt = document.createElement("span");
    cmdPrompt.className = "cmd-prompt";
    cmdPrompt.textContent = "›";
    const cmdItem = document.createElement("div");
    cmdItem.className = "cmd-ticker-item";
    cmdViewport.appendChild(cmdPrompt);
    cmdViewport.appendChild(cmdItem);

    pillInner.appendChild(headerRow);
    pillInner.appendChild(cmdViewport);

    pillShell.appendChild(shimmer);
    pillShell.appendChild(pillInner);

    root.appendChild(drawer);
    root.appendChild(pillShell);

    shadow.appendChild(styleEl);
    shadow.appendChild(root);

    // Mount host to DOM
    const mountHostToDom = () => {
        removeLegacyBanners();
        const targetParent = document.documentElement || document.body;
        if (targetParent && host.parentNode !== targetParent) {
            targetParent.appendChild(host);
        }
    };

    if (document.documentElement || document.body) {
        mountHostToDom();
    } else {
        window.addEventListener("DOMContentLoaded", mountHostToDom, { once: true });
    }

    // -------------------------------------------------------------
    // Draggable Repositioning Engine with Viewport Clamping
    // -------------------------------------------------------------
    let posX = null;
    let posY = null;
    let isDragging = false;
    let dragStartX = 0;
    let dragStartY = 0;
    let initialLeft = 0;
    let initialTop = 0;

    // Load saved coordinates from sessionStorage
    try {
        const rawPos = sessionStorage.getItem(STORAGE_KEY_POS);
        if (rawPos) {
            const parsed = JSON.parse(rawPos);
            if (typeof parsed.x === "number" && typeof parsed.y === "number") {
                posX = parsed.x;
                posY = parsed.y;
            }
        }
    } catch (_) {}

    function applyPosition(x, y) {
        const winW = window.innerWidth;
        const winH = window.innerHeight;
        const rootW = root.offsetWidth || 340;
        const rootH = root.offsetHeight || 60;

        let clampX = Math.max(10, Math.min(x, winW - rootW - 10));
        let clampY = Math.max(10, Math.min(y, winH - rootH - 10));

        root.style.setProperty("left", `${clampX}px`, "important");
        root.style.setProperty("top", `${clampY}px`, "important");
        root.style.setProperty("right", "auto", "important");
        root.style.setProperty("bottom", "auto", "important");

        // Flip drawer downward if dragged in the upper 45% of screen
        if (clampY < winH * 0.45) {
            root.classList.add("flip-down");
        } else {
            root.classList.remove("flip-down");
        }

        posX = clampX;
        posY = clampY;

        try {
            sessionStorage.setItem(STORAGE_KEY_POS, JSON.stringify({ x: clampX, y: clampY }));
        } catch (_) {}
    }

    function initDrag(handleEl) {
        handleEl.addEventListener("pointerdown", (e) => {
            if (e.button !== 0) return;
            e.stopPropagation();
            e.preventDefault();

            isDragging = true;
            dragStartX = e.clientX;
            dragStartY = e.clientY;

            const rect = root.getBoundingClientRect();
            initialLeft = rect.left;
            initialTop = rect.top;

            handleEl.classList.add("dragging");
            try {
                handleEl.setPointerCapture(e.pointerId);
            } catch (_) {}

            const onPointerMove = (moveEvt) => {
                if (!isDragging) return;
                const deltaX = moveEvt.clientX - dragStartX;
                const deltaY = moveEvt.clientY - dragStartY;
                applyPosition(initialLeft + deltaX, initialTop + deltaY);
            };

            const onPointerUp = (upEvt) => {
                isDragging = false;
                handleEl.classList.remove("dragging");
                try {
                    handleEl.releasePointerCapture(upEvt.pointerId);
                } catch (_) {}
                window.removeEventListener("pointermove", onPointerMove);
                window.removeEventListener("pointerup", onPointerUp);
                window.removeEventListener("pointercancel", onPointerUp);
            };

            window.addEventListener("pointermove", onPointerMove, { passive: true });
            window.addEventListener("pointerup", onPointerUp, { once: true });
            window.addEventListener("pointercancel", onPointerUp, { once: true });
        });
    }

    initDrag(pillDragHandle);
    initDrag(drawerDragHandle);

    // Keep clamped on resize
    window.addEventListener("resize", () => {
        if (posX != null && posY != null) {
            applyPosition(posX, posY);
        }
    }, { passive: true });

    // -------------------------------------------------------------
    // Internal State & Lifecycle Management
    // -------------------------------------------------------------
    let activeTasks = [];
    let selectedTaskIdx = 0;
    let viewerTabId = null;
    let isMounted = false;
    let isDrawerOpen = false;
    let isHovered = false;
    let isPinned = false;
    let activeDrawerTab = "rpc"; // "rpc" | "curl"
    let lastRenderedCmd = "";
    let autoHideTimer = null;
    let unmountTimer = null;
    let hoverLeaveTimer = null;
    let hudPort = null;
    let openTelemetryRpcId = null;
    let openNetworkRequestId = null;

    function clearTimers() {
        if (autoHideTimer) {
            clearTimeout(autoHideTimer);
            autoHideTimer = null;
        }
        if (unmountTimer) {
            clearTimeout(unmountTimer);
            unmountTimer = null;
        }
        if (hoverLeaveTimer) {
            clearTimeout(hoverLeaveTimer);
            hoverLeaveTimer = null;
        }
    }

    function toggleDrawer(forceState) {
        const target = forceState !== undefined ? !!forceState : !isDrawerOpen;
        if (target === isDrawerOpen) return;
        isDrawerOpen = target;

        const chevron = shadow.querySelector(".details-chevron");
        if (chevron) {
            if (isDrawerOpen) {
                chevron.classList.add("open");
            } else {
                chevron.classList.remove("open");
            }
        }

        if (isDrawerOpen) {
            drawer.classList.add("open");
            renderDrawer(activeTasks[selectedTaskIdx] || activeTasks[0]);
        } else {
            drawer.classList.remove("open");
        }
    }

    function setPinnedState(pinned) {
        isPinned = !!pinned;
        if (isPinned) {
            clearTimers();
            pinBtn.classList.add("pinned");
            pinBtn.title = "HUD is pinned. Click to unpin.";
            if (drawerPinBtn) {
                drawerPinBtn.classList.add("pinned");
                drawerPinBtn.title = "HUD is pinned. Click to unpin.";
            }
        } else {
            pinBtn.classList.remove("pinned");
            pinBtn.title = "Pin HUD to screen";
            if (drawerPinBtn) {
                drawerPinBtn.classList.remove("pinned");
                drawerPinBtn.title = "Pin HUD to screen";
            }

            // If tasks are finished, auto-dismiss promptly on unpin
            const allDone = activeTasks.length === 0 || activeTasks.every((t) => t.status === "completed" || t.status === "error");
            if (allDone) {
                scheduleIdleAutoHide(UNPIN_HIDE_DELAY_MS);
            }
        }

        const currentTask = activeTasks[selectedTaskIdx] || activeTasks[0];
        if (currentTask && currentTask.tabId && typeof chrome !== "undefined" && chrome.runtime?.sendMessage) {
            try {
                chrome.runtime.sendMessage({ action: "hud_pin", tabId: currentTask.tabId, pinned: isPinned });
            } catch (_) {}
        }
    }

    function showPopoverTopLayer() {
        mountHostToDom();
        clearTimers();

        host.removeAttribute("data-hidden");
        host.style.setProperty("display", "block", "important");
        host.style.removeProperty("pointer-events");

        try {
            if (typeof host.showPopover === "function" && !host.matches(":popover-open")) {
                host.showPopover();
            }
        } catch (_) {}

        requestAnimationFrame(() => {
            root.classList.remove("unmounting");
            root.classList.add("mounted");
            isMounted = true;

            if (posX != null && posY != null) {
                applyPosition(posX, posY);
            } else {
                // Default position: bottom-right
                const defaultX = window.innerWidth - (root.offsetWidth || 340) - 20;
                const defaultY = window.innerHeight - (root.offsetHeight || 60) - 20;
                applyPosition(defaultX, defaultY);
            }
        });
    }

    function setCloakState(cloaked) {
        if (cloaked) {
            host.setAttribute("data-mcp-cloaked", "true");
            host.classList.add("cloaked");
            host.style.setProperty("display", "none", "important");
            host.style.setProperty("pointer-events", "none", "important");
            root.classList.add("cloaked");
        } else {
            host.removeAttribute("data-mcp-cloaked");
            host.classList.remove("cloaked");
            host.style.setProperty("display", "block", "important");
            root.classList.remove("cloaked");
        }
    }

    function hidePopoverTopLayer() {
        if (isDrawerOpen || isPinned || isHovered) {
            return;
        }
        forceHidePopoverTopLayer();
    }

    function forceHidePopoverTopLayer() {
        clearTimers();
        isPinned = false;
        isHovered = false;
        toggleDrawer(false);
        if (pinBtn) pinBtn.classList.remove("pinned");
        if (drawerPinBtn) drawerPinBtn.classList.remove("pinned");

        // CRITICAL HIT-TEST FIX: Immediately release pointer events across all nodes
        host.setAttribute("data-hidden", "true");
        host.style.setProperty("pointer-events", "none", "important");
        root.style.setProperty("pointer-events", "none", "important");
        pillShell.style.setProperty("pointer-events", "none", "important");

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
            host.style.setProperty("pointer-events", "none", "important");
            root.classList.remove("unmounting");
        }, UNMOUNT_ANIM_MS);
    }

    function scheduleIdleAutoHide(delayMs = AUTO_HIDE_DELAY_MS) {
        if (autoHideTimer) clearTimeout(autoHideTimer);
        if (isDrawerOpen || isHovered || isPinned) return;
        autoHideTimer = setTimeout(() => {
            if (!isDrawerOpen && !isHovered && !isPinned) {
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
            }, 75);
        });
    }

    // Format JSON with safe character bounding and automatic parsing for nested strings
    function formatJsonSafely(val, maxChars = 8000) {
        if (val === undefined || val === null) return "None";
        try {
            let target = val;
            if (typeof target === "string") {
                const trimmed = target.trim();
                if ((trimmed.startsWith("{") && trimmed.endsWith("}")) || (trimmed.startsWith("[") && trimmed.endsWith("]"))) {
                    try {
                        target = JSON.parse(trimmed);
                    } catch (_) {}
                }
            }
            let str = typeof target === "string" ? target : JSON.stringify(target, null, 2);
            if (str.length > maxChars) {
                return str.slice(0, maxChars) + "\n... (truncated for display)";
            }
            return str;
        } catch (_) {
            return String(val);
        }
    }

    function renderDrawer(currentTask) {
        if (!currentTask) return;

        const task = currentTask;
        const status = task.status || "running";

        // Update slim status sub-bar
        subBarTitle.textContent = `Tab #${task.tabId} · ${task.tabTitle || "Active Tab"}`;
        subBarStatus.className = "sub-bar-status";
        if (status === "completed") {
            subBarStatus.classList.add("completed");
            subBarStatus.innerHTML = `${ICONS.check}<span>COMPLETED</span>`;
        } else if (status === "error") {
            subBarStatus.classList.add("error");
            subBarStatus.innerHTML = `<span>ERROR</span>`;
        } else {
            subBarStatus.innerHTML = `<span>RUNNING</span>`;
        }

        const logs = Array.isArray(task.logs) ? task.logs : [];
        const calls = Array.isArray(task.networkCalls) ? task.networkCalls : [];

        const rpcBadge = shadow.getElementById("__hud_rpc_count");
        if (rpcBadge) rpcBadge.textContent = String(logs.length);

        const curlBadge = shadow.getElementById("__hud_curl_count");
        if (curlBadge) curlBadge.textContent = String(calls.length);

        drawerBody.innerHTML = "";

        if (activeDrawerTab === "rpc") {
            if (logs.length === 0) {
                const empty = document.createElement("div");
                empty.className = "empty-placeholder";
                empty.innerHTML = `<strong>Awaiting Tool Invocations</strong><br>Current step: ${task.executingCmd || "Idle"}`;
                drawerBody.appendChild(empty);
                return;
            }

            logs.slice().reverse().forEach((entry, idx) => {
                const entryId = entry.rpcId || (entry.timestamp ? `log_ts_${entry.timestamp}` : `log_orig_${logs.indexOf(entry)}`);
                const isExpanded = openTelemetryRpcId === entryId;

                const item = document.createElement("div");
                item.className = `accordion-item ${isExpanded ? "expanded" : ""}`;

                // Header
                const hdr = document.createElement("div");
                hdr.className = "accordion-header";

                const titleGroup = document.createElement("div");
                titleGroup.className = "accordion-title-group";

                const chevron = document.createElement("span");
                chevron.className = "accordion-chevron";
                chevron.innerHTML = ICONS.arrowRight;

                const stepIdx = document.createElement("span");
                stepIdx.className = "step-index-label";
                stepIdx.textContent = `#${logs.length - idx}`;

                const toolName = document.createElement("span");
                toolName.className = "tool-name-label";
                toolName.textContent = entry.toolName || entry.tool_name || "mcp_tool";

                const cmdPreview = document.createElement("span");
                cmdPreview.className = "cmd-preview-label";
                cmdPreview.textContent = entry.intent || entry.executingCmd || "";

                titleGroup.appendChild(chevron);
                titleGroup.appendChild(stepIdx);
                titleGroup.appendChild(toolName);
                titleGroup.appendChild(cmdPreview);

                const meta = document.createElement("div");
                meta.className = "accordion-meta";
                if (entry.latencyMs != null) {
                    const lat = document.createElement("span");
                    lat.className = "latency-tag";
                    lat.textContent = `${entry.latencyMs}ms`;
                    meta.appendChild(lat);
                }

                hdr.appendChild(titleGroup);
                hdr.appendChild(meta);

                // Content
                const content = document.createElement("div");
                content.className = "accordion-content";

                // 1. Intent Section
                if (entry.intent) {
                    const intentSec = document.createElement("div");
                    intentSec.className = "detail-section";
                    const intentHdr = document.createElement("div");
                    intentHdr.className = "detail-heading";
                    intentHdr.textContent = "Step Intent";
                    const intentBox = document.createElement("div");
                    intentBox.className = "code-block";
                    intentBox.tabIndex = 0;
                    intentBox.textContent = entry.intent;
                    intentSec.appendChild(intentHdr);
                    intentSec.appendChild(intentBox);
                    content.appendChild(intentSec);
                }

                // 2. Agent Request Section
                const reqSec = document.createElement("div");
                reqSec.className = "detail-section";
                const reqHdr = document.createElement("div");
                reqHdr.className = "detail-heading";
                reqHdr.innerHTML = `<span>Agent Request</span><button type="button" class="detail-copy-btn">Copy</button>`;
                const reqBox = document.createElement("div");
                reqBox.className = "code-block";
                reqBox.tabIndex = 0;
                const reqFormatted = formatJsonSafely(entry.requestData || entry.executingCmd || { status: entry.status });
                reqBox.textContent = reqFormatted;
                reqHdr.querySelector("button").addEventListener("click", (e) => {
                    e.stopPropagation();
                    navigator.clipboard.writeText(reqFormatted);
                    reqHdr.querySelector("button").textContent = "✓ Copied";
                    setTimeout(() => { reqHdr.querySelector("button").textContent = "Copy"; }, 1200);
                });
                reqSec.appendChild(reqHdr);
                reqSec.appendChild(reqBox);
                content.appendChild(reqSec);

                // 3. Browser Generated Response Section (Nested Scroll)
                const respSec = document.createElement("div");
                respSec.className = "detail-section";
                const respHdr = document.createElement("div");
                respHdr.className = "detail-heading";
                respHdr.innerHTML = `<span>Browser Generated Response</span><button type="button" class="detail-copy-btn">Copy</button>`;
                const respBox = document.createElement("div");
                respBox.className = "code-block response-block";
                respBox.tabIndex = 0;
                respBox.setAttribute("role", "region");
                respBox.setAttribute("aria-label", "Browser Generated Response");
                respBox.title = "Scrollable response payload (wheel or keyboard arrows)";
                const respFormatted = formatJsonSafely(entry.responseData || { status: entry.status, latency_ms: entry.latencyMs });
                respBox.textContent = respFormatted;
                respHdr.querySelector("button").addEventListener("click", (e) => {
                    e.stopPropagation();
                    navigator.clipboard.writeText(respFormatted);
                    respHdr.querySelector("button").textContent = "✓ Copied";
                    setTimeout(() => { respHdr.querySelector("button").textContent = "Copy"; }, 1200);
                });
                respSec.appendChild(respHdr);
                respSec.appendChild(respBox);

                // Subtle Rust Sanitization Footnote
                const rustNote = document.createElement("div");
                rustNote.className = "rust-sanitization-note";
                rustNote.innerHTML = `${ICONS.info}<span>Raw browser response (scrubbed & sanitized by OnlyDOMs Rust engine before entering LLM context).</span>`;
                respSec.appendChild(rustNote);

                content.appendChild(respSec);

                // Single-open accordion toggle: exactly one open item at a time
                hdr.addEventListener("click", (e) => {
                    e.stopPropagation();
                    const wasExpanded = item.classList.contains("expanded");
                    drawerBody.querySelectorAll(".accordion-item.expanded").forEach((el) => {
                        el.classList.remove("expanded");
                    });
                    if (wasExpanded) {
                        openTelemetryRpcId = null;
                    } else {
                        openTelemetryRpcId = entryId;
                        item.classList.add("expanded");
                    }
                });

                item.appendChild(hdr);
                item.appendChild(content);
                drawerBody.appendChild(item);
            });
        } else {
            if (calls.length === 0) {
                const empty = document.createElement("div");
                empty.className = "empty-placeholder";
                empty.textContent = "No network XHR/Fetch calls intercepted in this window.";
                drawerBody.appendChild(empty);
                return;
            }

            calls.slice().reverse().forEach((net, idx) => {
                const reqId = net.requestId || (net.timestamp ? `net_ts_${net.timestamp}` : `net_orig_${calls.indexOf(net)}`);
                const isExpanded = openNetworkRequestId === reqId;

                const item = document.createElement("div");
                item.className = `accordion-item ${isExpanded ? "expanded" : ""}`;

                // Header
                const hdr = document.createElement("div");
                hdr.className = "accordion-header";

                const titleGroup = document.createElement("div");
                titleGroup.className = "accordion-title-group";

                const chevron = document.createElement("span");
                chevron.className = "accordion-chevron";
                chevron.innerHTML = ICONS.arrowRight;

                const method = (net.method || "GET").toUpperCase();
                const methodLabel = document.createElement("span");
                methodLabel.className = `method-label ${method === "GET" ? "get" : method === "POST" ? "post" : "other"}`;
                methodLabel.textContent = method;

                const statusLabel = document.createElement("span");
                const st = net.status;
                const isSuccess = st && st >= 200 && st < 300;
                const isErr = st && st >= 400;
                statusLabel.className = `status-pill ${isSuccess ? "ok" : isErr ? "err" : "pending"}`;
                statusLabel.textContent = st ? String(st) : "···";

                const actionBadge = document.createElement("span");
                actionBadge.className = "network-action-badge";
                const actionLabel = net.toolName || (net.rpcId ? `#${net.rpcId}` : null);
                if (actionLabel) {
                    actionBadge.textContent = `via ${actionLabel}`;
                    if (net.intent) actionBadge.title = `Triggering Action Intent: ${net.intent}`;
                } else {
                    actionBadge.style.display = "none";
                }

                const urlSpan = document.createElement("span");
                urlSpan.className = "curl-url";
                urlSpan.textContent = net.url || "";
                urlSpan.title = net.url || "";

                titleGroup.appendChild(chevron);
                titleGroup.appendChild(methodLabel);
                titleGroup.appendChild(statusLabel);
                if (actionLabel) titleGroup.appendChild(actionBadge);
                titleGroup.appendChild(urlSpan);

                const actGroup = document.createElement("div");
                actGroup.className = "curl-actions";

                const copyBtn = document.createElement("button");
                copyBtn.className = "mini-btn";
                copyBtn.textContent = "Copy cURL";
                copyBtn.title = "Copy shell-safe cURL to clipboard";
                copyBtn.addEventListener("click", (e) => {
                    e.stopPropagation();
                    if (navigator.clipboard && net.curlCommand) {
                        navigator.clipboard.writeText(net.curlCommand);
                        copyBtn.textContent = "✓ Copied";
                        setTimeout(() => { copyBtn.textContent = "Copy cURL"; }, 1200);
                    }
                });

                const replayBtn = document.createElement("button");
                replayBtn.className = "mini-btn";
                replayBtn.textContent = "Replay";
                replayBtn.title = "Replay cURL via native engine";
                replayBtn.addEventListener("click", (e) => {
                    e.stopPropagation();
                    replayBtn.textContent = "Running...";
                    chrome.runtime.sendMessage(
                        {
                            action: "hud_replay_curl",
                            tabId: task.tabId,
                            requestId: net.requestId,
                            curl: net.curlCommand
                        },
                        (resp) => {
                            const code = resp?.result?.status_code || resp?.result?.status || "OK";
                            replayBtn.textContent = `✓ ${code}`;
                            setTimeout(() => { replayBtn.textContent = "Replay"; }, 1600);
                        }
                    );
                });

                actGroup.appendChild(copyBtn);
                actGroup.appendChild(replayBtn);

                hdr.appendChild(titleGroup);
                hdr.appendChild(actGroup);

                // Body (Request + Response with nested scroll)
                const content = document.createElement("div");
                content.className = "accordion-content";

                // Section 1: Triggering Action Details
                if (net.rpcId || net.toolName || net.intent) {
                    const actSec = document.createElement("div");
                    actSec.className = "detail-section";
                    const actHdr = document.createElement("div");
                    actHdr.className = "detail-heading";
                    actHdr.textContent = "Triggering Agent Action";
                    const actBox = document.createElement("div");
                    actBox.className = "code-block";
                    actBox.tabIndex = 0;
                    actBox.textContent = `Action Tool: ${net.toolName || "Agent Action"} (ID: ${net.rpcId || "untracked"})\nAction Intent: ${net.intent || "No intent description recorded"}`;
                    actSec.appendChild(actHdr);
                    actSec.appendChild(actBox);
                    content.appendChild(actSec);
                }

                // Section 2: Request Details
                const reqSec = document.createElement("div");
                reqSec.className = "detail-section";
                const reqHdr = document.createElement("div");
                reqHdr.className = "detail-heading";
                reqHdr.innerHTML = `<span>Request Details (${method})</span><button type="button" class="detail-copy-btn">Copy</button>`;
                const reqBox = document.createElement("div");
                reqBox.className = "code-block";
                reqBox.tabIndex = 0;
                const reqDetails = {
                    method,
                    url: net.url,
                    headers: net.headers || {},
                    body: net.postData || null
                };
                const reqFormatted = formatJsonSafely(reqDetails);
                reqBox.textContent = reqFormatted;
                reqHdr.querySelector("button").addEventListener("click", (e) => {
                    e.stopPropagation();
                    navigator.clipboard.writeText(reqFormatted);
                    reqHdr.querySelector("button").textContent = "✓ Copied";
                    setTimeout(() => { reqHdr.querySelector("button").textContent = "Copy"; }, 1200);
                });
                reqSec.appendChild(reqHdr);
                reqSec.appendChild(reqBox);
                content.appendChild(reqSec);

                // Section 3: Response Details (Nested Scroll)
                const respSec = document.createElement("div");
                respSec.className = "detail-section";
                const respHdr = document.createElement("div");
                respHdr.className = "detail-heading";
                respHdr.innerHTML = `<span>Response Details [${net.status || "Pending"}]</span><button type="button" class="detail-copy-btn">Copy</button>`;
                const respBox = document.createElement("div");
                respBox.className = "code-block response-block";
                respBox.tabIndex = 0;
                respBox.setAttribute("role", "region");
                respBox.setAttribute("aria-label", "Network Response Details");
                respBox.title = "Scrollable response payload (wheel or keyboard arrows)";
                const respPayload = {
                    status: net.status || "Pending / In-flight",
                    headers: net.responseHeaders || {},
                    body: net.responseBody || "(No response body captured / in-flight)"
                };
                const respFormatted = typeof net.responseBody === "string" && !net.responseHeaders
                    ? net.responseBody
                    : formatJsonSafely(respPayload);
                respBox.textContent = respFormatted;
                respHdr.querySelector("button").addEventListener("click", (e) => {
                    e.stopPropagation();
                    navigator.clipboard.writeText(respFormatted);
                    respHdr.querySelector("button").textContent = "✓ Copied";
                    setTimeout(() => { respHdr.querySelector("button").textContent = "Copy"; }, 1200);
                });
                respSec.appendChild(respHdr);
                respSec.appendChild(respBox);
                content.appendChild(respSec);

                // Single-open accordion toggle: exactly one open item at a time
                hdr.addEventListener("click", (e) => {
                    e.stopPropagation();
                    const wasExpanded = item.classList.contains("expanded");
                    drawerBody.querySelectorAll(".accordion-item.expanded").forEach((el) => {
                        el.classList.remove("expanded");
                    });
                    if (wasExpanded) {
                        openNetworkRequestId = null;
                    } else {
                        openNetworkRequestId = reqId;
                        item.classList.add("expanded");
                    }
                });

                item.appendChild(hdr);
                item.appendChild(content);
                drawerBody.appendChild(item);
            });
        }
    }

    function renderTasks(tasks, currentTabId) {
        if (currentTabId != null) {
            viewerTabId = currentTabId;
        }

        if (Array.isArray(tasks) && tasks.length > 0) {
            activeTasks = tasks;
        } else if (isDrawerOpen || isPinned || isHovered) {
            // Retain while inspecting
        } else {
            activeTasks = [];
        }

        if (activeTasks.length === 0) {
            if (isMounted && !isDrawerOpen && !isPinned && !isHovered) {
                hidePopoverTopLayer();
            }
            return;
        }

        if (selectedTaskIdx >= activeTasks.length) {
            selectedTaskIdx = 0;
        }

        const task = activeTasks[selectedTaskIdx] || activeTasks[0];
        const status = task.status || "running";

        pillShell.classList.remove("running", "completed", "error");
        if (status === "completed") {
            pillShell.classList.add("completed");
        } else if (status === "error") {
            pillShell.classList.add("error");
        } else {
            pillShell.classList.add("running");
        }

        const cleanTitle = (task.tabTitle || "Active Tab").trim();
        pillTitle.innerHTML = `<span class="pill-tab-id">#${task.tabId}</span>${cleanTitle}`;
        pillTitle.title = `Tab #${task.tabId} — ${cleanTitle}`;

        const execStr = task.intent || task.executingCmd || `Tab #${task.tabId} active`;
        animateCmdTicker(execStr);

        if (!isMounted) {
            showPopoverTopLayer();
        }

        if (isDrawerOpen) {
            renderDrawer(task);
        }

        const allDone = activeTasks.every((t) => t.status === "completed" || t.status === "error");
        if (allDone && !isPinned && !isDrawerOpen && !isHovered) {
            scheduleIdleAutoHide(AUTO_HIDE_DELAY_MS);
        } else if (!allDone) {
            if (autoHideTimer) {
                clearTimeout(autoHideTimer);
                autoHideTimer = null;
            }
        }
    }

    // Pointer hover tracking
    const handlePointerEnter = () => {
        if (hoverLeaveTimer) {
            clearTimeout(hoverLeaveTimer);
            hoverLeaveTimer = null;
        }
        isHovered = true;
        if (autoHideTimer) {
            clearTimeout(autoHideTimer);
            autoHideTimer = null;
        }
    };

    const handlePointerLeave = () => {
        if (hoverLeaveTimer) clearTimeout(hoverLeaveTimer);
        hoverLeaveTimer = setTimeout(() => {
            hoverLeaveTimer = null;
            isHovered = false;
            if (!isDrawerOpen && !isPinned) {
                const allDone = activeTasks.every((t) => t.status === "completed" || t.status === "error");
                if (allDone) {
                    scheduleIdleAutoHide(AUTO_HIDE_DELAY_MS);
                }
            }
        }, 800);
    };

    root.addEventListener("mouseenter", handlePointerEnter);
    root.addEventListener("mouseleave", handlePointerLeave);

    // Clicking anywhere on the pill toggles drawer
    pillInner.addEventListener("click", (e) => {
        if (e.target.closest("button")) return;
        toggleDrawer();
    });

    detailsBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        toggleDrawer();
    });

    drawerCloseBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        toggleDrawer(false);
    });

    pinBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        setPinnedState(!isPinned);
    });

    drawerPinBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        setPinnedState(!isPinned);
    });

    closeBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        const currentTask = activeTasks[selectedTaskIdx] || activeTasks[0];
        if (currentTask && currentTask.tabId && typeof chrome !== "undefined" && chrome.runtime?.sendMessage) {
            chrome.runtime.sendMessage({ action: "hud_dismiss", tabId: currentTask.tabId });
        }
        forceHidePopoverTopLayer();
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

    copySummaryBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        const currentTask = activeTasks[selectedTaskIdx] || activeTasks[0];
        if (!currentTask) return;
        const lines = [
            `[OnlyDOMs Task Summary]`,
            `Tab: #${currentTask.tabId} (${currentTask.tabTitle || "Untitled"})`,
            `Status: ${currentTask.status}`,
            `Intent: ${currentTask.intent}`,
            `Command: ${currentTask.executingCmd}`
        ];
        if (Array.isArray(currentTask.logs) && currentTask.logs.length > 0) {
            lines.push(`\nTelemetry Log (${currentTask.logs.length}):`);
            currentTask.logs.forEach((l, i) => {
                lines.push(`  ${i + 1}. [${l.toolName || "tool"}] ${l.intent || l.executingCmd || ""}`);
            });
        }
        if (navigator.clipboard) {
            navigator.clipboard.writeText(lines.join("\n"));
            copySummaryBtn.style.color = "#38bdf8";
            setTimeout(() => { copySummaryBtn.style.removeProperty("color"); }, 1200);
        }
    });

    killBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        const currentTask = activeTasks[selectedTaskIdx] || activeTasks[0];
        if (currentTask && currentTask.tabId) {
            chrome.runtime.sendMessage({ action: "detach_tab", tabId: currentTask.tabId }, () => {
                forceHidePopoverTopLayer();
            });
        } else {
            forceHidePopoverTopLayer();
        }
    });

    // 1. Persistent Port Connection (`mcp-hud-port`)
    function connectHudPort() {
        if (typeof chrome === "undefined" || !chrome.runtime || !chrome.runtime.connect) return;
        try {
            hudPort = chrome.runtime.connect({ name: "mcp-hud-port" });
            hudPort.onMessage.addListener((msg) => {
                if (!msg || typeof msg !== "object") return;
                if (msg.type === "HUD_SYNC") {
                    renderTasks(msg.tasks || [], msg.viewerTabId);
                } else if (msg.type === "CLOAK_HUD") {
                    setCloakState(msg.cloak !== false);
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

    // 2. Direct Message Listener
    if (typeof chrome !== "undefined" && chrome.runtime && chrome.runtime.onMessage) {
        chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
            if (!message || typeof message !== "object") return false;

            if (message.type === "CLOAK_HUD") {
                setCloakState(true);
                requestAnimationFrame(() => {
                    sendResponse({ success: true, cloaked: true });
                });
                return true;
            }
            if (message.type === "UNCLOAK_HUD") {
                setCloakState(false);
                sendResponse({ success: true, cloaked: false });
                return true;
            }
            if (message.type === "HUD_SYNC") {
                renderTasks(message.tasks || [], message.viewerTabId);
                sendResponse({ success: true });
                return true;
            }
            if (message.type === "UPDATE_BANNER") {
                const syntheticTask = {
                    tabId: message.activeTabId || 1,
                    tabTitle: message.activeTabTitle || document.title || "Active Tab",
                    intent: message.intent || `Agent active on Tab #${message.activeTabId || 1}`,
                    executingCmd: message.executingCmd || `Executing :: action on Tab #${message.activeTabId || 1}`,
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
                executingCmd: executingCmd || `Executing :: command on Tab #${tabId}`,
                status,
                updatedAt: Date.now(),
                logs: [],
                networkCalls: []
            }], viewerTabId);
        },
        hide() {
            forceHidePopoverTopLayer();
        }
    };
})();
