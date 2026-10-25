function drawGrid() {
    return new Promise((resolve) => {
        requestAnimationFrame(() => {
            const cols = 16;
            const w = window.innerWidth;
            const h = window.innerHeight;
            const rows = Math.round(16 * (h / w));
            const dpr = window.devicePixelRatio || 1;
            
            const canvas = document.createElement('canvas');
            canvas.id = 'chromapilot-grid-overlay';
            canvas.width = w; canvas.height = h;
            canvas.style.cssText = 'position:fixed;top:0;left:0;z-index:2147483647;pointer-events:none;';
            
            const ctx = canvas.getContext('2d');
            ctx.strokeStyle = 'rgba(0, 0, 0, 0.2)';
            ctx.lineWidth = 1;
            ctx.font = '10px monospace';
            ctx.fillStyle = 'rgba(0, 0, 0, 0.5)';
            
            const cellW = w / cols;
            const cellH = h / rows;
            const letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
            
            for (let i = 0; i < cols; i++) {
                for (let j = 0; j < rows; j++) {
                    ctx.strokeRect(i * cellW, j * cellH, cellW, cellH);
                    let colStr = "";
                    let temp = i;
                    while (temp >= 0) {
                        colStr = letters[temp % 26] + colStr;
                        temp = Math.floor(temp / 26) - 1;
                    }
                    const label = `${colStr}${j+1}`;
                    ctx.fillText(label, i * cellW + 2, j * cellH + 10);
                }
            }
            document.body.appendChild(canvas);
            
            resolve(JSON.stringify({
                cols: cols,
                rows: rows,
                viewport_css: { width: w, height: h },
                cell_size_css: { width: cellW, height: cellH },
                dpr: dpr
            }));
        });
    });
}

function removeGrid() {
    return new Promise((resolve) => {
        requestAnimationFrame(() => {
            const el = document.getElementById('chromapilot-grid-overlay');
            if (el) el.remove();
            resolve(true);
        });
    });
}
window.chromapilotGrid = { drawGrid, removeGrid };
