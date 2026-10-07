// Checks Mermaid code with the real Mermaid parser, in a headless browser.
// Usage: node check_mermaid.js <mermaid.min.js> <chrome> < blocks.json  ->  prints [error or null, ...]
// NODE_PATH must point at a node_modules folder containing playwright.
const { chromium } = require("playwright");
const fs = require("fs");
(async () => {
  const [mermaidJs, chrome] = process.argv.slice(2);
  const blocks = JSON.parse(fs.readFileSync(0, "utf8"));
  const browser = await chromium.launch({ executablePath: chrome });
  const page = await browser.newPage();
  await page.setContent("<!doctype html><html><body></body></html>");
  await page.addScriptTag({ path: mermaidJs });
  const results = await page.evaluate(async (codes) => {
    window.mermaid.initialize({ startOnLoad: false, securityLevel: "strict" });
    const out = [];
    for (const code of codes) {
      try { await window.mermaid.parse(code); out.push(null); }
      catch (e) { out.push(String(e.message || e).split("\n").slice(0, 4).join(" ")); }
    }
    return out;
  }, blocks);
  process.stdout.write(JSON.stringify(results));
  await browser.close();
})().catch((e) => { console.error(e.message); process.exit(1); });
