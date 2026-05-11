// One-off helper: snap a screenshot of the running Astro manual at
// http://localhost:4321/stage/ so we can sanity-check the rendered
// page without a browser handy. Disposable — feel free to delete once
// the manual is wired into the team's preview workflow.
import { chromium } from "playwright";

const url = process.argv[2] ?? "http://localhost:4321/stage/";
const out = process.argv[3] ?? "/tmp/manual-stage.png";

const browser = await chromium.launch();
const ctx = await browser.newContext({ deviceScaleFactor: 2, viewport: { width: 1280, height: 1600 } });
const page = await ctx.newPage();
await page.goto(url, { waitUntil: "networkidle" });
await page.screenshot({ path: out, fullPage: true });
await browser.close();
console.log(`snapped → ${out}`);
