// Opens each chart page written by the C3 test (MP_HTML_OUT=<in folder>) in headless Chromium with the
// network blocked, checks that it draws without a script error or a request, and saves a PNG of the
// light and of the dark page:  node tools/html_chart_png.js <in folder> <out folder> [names...]
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');

(async () => {
  const [inDir, outDir, ...only] = process.argv.slice(2);
  if (!inDir || !outDir) { console.error('usage: node html_chart_png.js <in folder> <out folder> [names]'); process.exit(2); }
  fs.mkdirSync(outDir, { recursive: true });
  const names = fs.readdirSync(inDir).filter(f => f.endsWith('.html')).map(f => f.slice(0, -5)).filter(n => !only.length || only.includes(n));
  const browser = await chromium.launch();
  let bad = 0;
  for (const name of names) {
    for (const theme of ['light', 'dark']) {
      const ctx = await browser.newContext({ viewport: { width: 1100, height: 700 }, deviceScaleFactor: 1 });
      const page = await ctx.newPage();
      const problems = [];
      page.on('pageerror', e => problems.push('script error: ' + e.message));
      page.on('console', m => { if (m.type() === 'error') problems.push('console: ' + m.text()); });
      await page.route('**/*', route => {
        const u = route.request().url();
        if (u.startsWith('file:') || u.startsWith('data:') || u.startsWith('blob:')) return route.continue();
        problems.push('network request: ' + u); return route.abort();
      });
      await page.goto('file://' + path.resolve(inDir, name + '.html'));
      if (theme === 'dark') await page.click('#mp-theme');
      await page.waitForTimeout(300);
      const el = await page.$('.mp-chart');
      const h = await page.evaluate(() => { const c = document.querySelector('.mp-chart'); return c ? c.getBoundingClientRect().height : 0; });
      const drawn = await page.evaluate(() => { const c = document.querySelector('.mp-chart canvas'); return !!c && c.width > 0; });
      if (!drawn) problems.push('no canvas');
      // hover the middle of the chart: the tooltip must not raise an error
      await page.mouse.move(520, 300); await page.waitForTimeout(100);
      await page.setViewportSize({ width: 1100, height: Math.ceil(h) + 90 });
      await page.waitForTimeout(150);
      await page.screenshot({ path: path.join(outDir, `${name}${theme === 'dark' ? '-dark' : ''}.png`) });
      if (problems.length) { bad++; console.log(name, theme, problems.join(' | ')); }
      await ctx.close();
    }
  }
  await browser.close();
  console.log(bad ? `${bad} page(s) with problems` : `all ${names.length} pages drew in light and dark, no script error, no network request`);
  process.exit(bad ? 1 : 0);
})();
