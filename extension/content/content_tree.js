/**
 * OnlyDOMs Browser-Side Pre-Filters & DOM TreeWalker Exclusion Guard
 * Master Specification § 6.B.2 & § 6.B.3
 */
(() => {
    if (window.__onlydomsContentTreeGuardInstalled) return;
    window.__onlydomsContentTreeGuardInstalled = true;

    const HUD_TAG = "mcp-agent-hud";
    const HUD_ID = "__mcp_agent_hud_host__";
    const HUD_ATTR = "data-mcp-internal-ignore";
    const LEGACY_HOST_ID = "onlydoms-banner-host";

    function isHudNode(node) {
        if (!node) return false;
        const nodeName = (node.nodeName || node.tagName || "").toLowerCase();
        if (nodeName === HUD_TAG) return true;
        if (node.id === HUD_ID || node.id === LEGACY_HOST_ID) return true;
        if (typeof node.hasAttribute === "function" && node.hasAttribute(HUD_ATTR)) return true;
        if (typeof node.closest === "function") {
            const anc = node.closest(`${HUD_TAG}, #${HUD_ID}, [${HUD_ATTR}], #${LEGACY_HOST_ID}`);
            if (anc) return true;
        }
        return false;
    }

    // § 6.B.3: DOM TreeWalker Exclusion Filter
    window.__mcpTreeWalkerFilter = function (node) {
        if (isHudNode(node)) {
            return NodeFilter.FILTER_REJECT;
        }
        return NodeFilter.FILTER_ACCEPT;
    };

    // Patch document.createTreeWalker so any agent or page DOM traversal automatically rejects HUD nodes
    const origCreateTreeWalker = Document.prototype.createTreeWalker;
    if (typeof origCreateTreeWalker === "function") {
        Document.prototype.createTreeWalker = function (root, whatToShow, filter, expandEntityReferences) {
            const wrappedFilter = {
                acceptNode(node) {
                    if (isHudNode(node)) {
                        return NodeFilter.FILTER_REJECT;
                    }
                    if (!filter) return NodeFilter.FILTER_ACCEPT;
                    if (typeof filter === "function") return filter(node);
                    if (typeof filter.acceptNode === "function") return filter.acceptNode(node);
                    return NodeFilter.FILTER_ACCEPT;
                }
            };
            return origCreateTreeWalker.call(this, root, whatToShow, wrappedFilter, expandEntityReferences);
        };
    }

    // § 6.B.2: Hit-Test Passthrough for elementFromPoint / elementsFromPoint
    const origElementsFromPoint = Document.prototype.elementsFromPoint;
    if (typeof origElementsFromPoint === "function") {
        Document.prototype.elementsFromPoint = function (x, y) {
            const raw = origElementsFromPoint.call(this, x, y);
            return Array.isArray(raw) ? raw.filter((el) => !isHudNode(el)) : [];
        };
    }

    const origElementFromPoint = Document.prototype.elementFromPoint;
    if (typeof origElementFromPoint === "function") {
        Document.prototype.elementFromPoint = function (x, y) {
            if (typeof origElementsFromPoint === "function") {
                const list = origElementsFromPoint.call(this, x, y);
                for (const el of list) {
                    if (!isHudNode(el)) return el;
                }
            }
            const top = origElementFromPoint.call(this, x, y);
            return isHudNode(top) ? null : top;
        };
    }
})();
