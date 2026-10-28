window.onlydomsInput = window.chromapilotInput = {
    fill: function(fields) {
        return new Promise((resolve) => {
            requestAnimationFrame(() => {
                const results = [];
                for (const field of fields) {
                    let el = null;
                    if (field.x !== undefined && field.y !== undefined) {
                        el = document.elementFromPoint(field.x, field.y);
                    } else if (field.selector) {
                        el = document.querySelector(field.selector);
                        if (!el && !field.selector.includes('[') && !field.selector.includes('#') && !field.selector.includes('.')) {
                            // Smart fallback for plain name, id, placeholder, or aria-label
                            el = document.querySelector(`input[name="${field.selector}"], input[id="${field.selector}"], input[placeholder*="${field.selector}" i], textarea[name="${field.selector}"], textarea[id="${field.selector}"], [aria-label*="${field.selector}" i]`);
                        }
                    }
                    
                    if (!el) {
                        results.push({ success: false, error: `Element not found for selector: '${field.selector || ""}'` });
                        continue;
                    }

                    try {
                        if (el.tagName === 'TEXTAREA') {
                            const nativeSetter = Object.getOwnPropertyDescriptor(
                                window.HTMLTextAreaElement.prototype, 'value'
                            )?.set;
                            if (nativeSetter) {
                                nativeSetter.call(el, field.value);
                            } else {
                                el.value = field.value;
                            }
                        } else if (el.tagName === 'INPUT') {
                            const nativeSetter = Object.getOwnPropertyDescriptor(
                                window.HTMLInputElement.prototype, 'value'
                            )?.set;
                            if (nativeSetter) {
                                nativeSetter.call(el, field.value);
                            } else {
                                el.value = field.value;
                            }
                        } else {
                            el.value = field.value;
                        }
                        
                        el.dispatchEvent(new Event('input', { bubbles: true }));
                        el.dispatchEvent(new Event('change', { bubbles: true }));

                        if (field.press_enter) {
                            const enterEvent = new KeyboardEvent('keydown', {
                                bubbles: true, cancelable: true, keyCode: 13, key: 'Enter'
                            });
                            el.dispatchEvent(enterEvent);
                        }

                        results.push({ success: true });
                    } catch (e) {
                        results.push({ success: false, error: e.message });
                    }
                }
                resolve(JSON.stringify(results));
            });
        });
    }
};
