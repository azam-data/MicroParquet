// Clicks the real "Save as SVG" and "Save as PNG" buttons of a chart page in headless Chromium and prints what the
// browser downloaded, as JSON:  node tools/html_svg_click.js <page.html> [index]
// (the toolbox has three icons: Data view, Save as SVG, Save as PNG; each is clicked on a fresh page).
const { chromium } = require('playwright');
const path = require('path');
const fs = require('fs');

(async () => {
  const file = process.argv[2];
  if (!file) { console.error('usage: node html_svg_click.js <page.html>'); process.exit(2); }
  const browser = await chromium.launch();
  const result = {};
  for (const [key, slot] of [['svg', 1], ['png', 2]]) {
    const ctx = await browser.newContext({ viewport: { width: 1100, height: 700 }, acceptDownloads: true });
    const page = await ctx.newPage();
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.goto('file://' + path.resolve(file));
    await page.waitForTimeout(400);
    // where the three toolbox icons are, from the drawn shapes (top right of the chart)
    const at = await page.evaluate(() => {
      const el = document.querySelector('.mp-chart'); const inst = echarts.getInstanceByDom(el); const r0 = el.getBoundingClientRect();
      const icons = [];
      inst.getZr().storage.getDisplayList(true).forEach(d => {
        if (d.type !== 'path' || !d.cursor || d.cursor !== 'pointer') return;
        const r = d.getBoundingRect(); const t = d.transform;
        const x = r.x + (t ? t[4] : 0), y = r.y + (t ? t[5] : 0);
        if (y < 100 && x > r.width) icons.push([r0.left + x + r.width / 2, r0.top + y + r.height / 2]);
      });
      icons.sort((a, b) => a[0] - b[0]);
      return icons;
    });
    if (at.length < 3) { result[key] = { error: 'toolbox icons not found', errors }; await ctx.close(); continue; }
    const [x, y] = at[at.length - 3 + slot];
    await page.mouse.move(x, y, { steps: 3 });
    await page.waitForTimeout(150);
    const dl = page.waitForEvent('download', { timeout: 4000 }).catch(() => null);
    await page.mouse.down(); await page.waitForTimeout(50); await page.mouse.up();
    const d = await dl;
    if (!d) { result[key] = { error: 'no download', errors }; await ctx.close(); continue; }
    const bytes = fs.readFileSync(await d.path());
    result[key] = { name: d.suggestedFilename(), size: bytes.length, start: bytes.slice(0, 8).toString('latin1'), text: key === 'svg' ? bytes.toString('utf8') : '', errors };
    await ctx.close();
  }
  await browser.close();
  console.log(JSON.stringify(result));
})();
