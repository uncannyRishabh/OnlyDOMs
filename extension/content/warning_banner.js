/**
 * Compatibility shim: upgrades any legacy content/warning_banner.js injection
 * to remove the old top-center banner and initialize the Top-Layer Agent HUD.
 */
(() => {
    const legacy = document.getElementById("onlydoms-banner-host");
    if (legacy && legacy.parentNode) {
        legacy.parentNode.removeChild(legacy);
    }
})();
