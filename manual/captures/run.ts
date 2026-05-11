// Capture runner. Spawns the Vite dev server, drives every registered
// scene through Playwright, and writes the annotated PNGs to ./output.
//
// Usage:
//   pnpm capture                    # run every scene
//   pnpm capture:scene stage-overview   # run one scene by id
//
// Scene registry: every file in ./scenes that exports a default
// SceneSpec is picked up automatically. The id in the spec drives the
// output filename, so renaming scenes is safe as long as ids are stable.

import { spawn, type ChildProcess } from "node:child_process";
import { mkdir, readdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { chromium, type Browser } from "playwright";
import { runScene, type SceneSpec } from "./lib/capture.ts";

const __dirname = dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = resolve(__dirname, "..", "..");
// Captures go directly into src/assets/ so Astro's image pipeline can
// process them (responsive sizes, format conversion, content hashing).
// Referencing them from MDX uses ~/assets/captures/<id>.png — that is
// what unlocks the build-time optimization.
const OUTPUT_DIR = resolve(__dirname, "..", "src", "assets", "captures");
const DEV_PORT = 1420;
const DEV_URL = `http://localhost:${DEV_PORT}`;

// Wait for the dev server to start serving — `vite dev` prints
// "ready in" once it's bound, but polling the port is more reliable
// across vite versions.
async function waitForServer(url: string, timeoutMs = 30_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(url, { method: "HEAD" });
      if (r.ok || r.status === 304) return;
    } catch {
      // Not ready yet.
    }
    await new Promise((res) => setTimeout(res, 200));
  }
  throw new Error(`Dev server did not become ready at ${url} within ${timeoutMs}ms`);
}

// True if something already answers on the dev port. We reuse it
// instead of failing — operators usually keep `vite dev` running, and
// the doc harness shouldn't fight them for the port.
async function isServerLive(url: string): Promise<boolean> {
  try {
    const r = await fetch(url, { method: "HEAD" });
    return r.ok || r.status === 304;
  } catch {
    return false;
  }
}

async function spawnDevServer(): Promise<ChildProcess> {
  console.log(`[doc] spawning vite dev on :${DEV_PORT}…`);
  const child = spawn("npm", ["run", "dev"], {
    cwd: PROJECT_ROOT,
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, BROWSER: "none" },
  });
  child.stdout?.on("data", (chunk) => {
    const line = chunk.toString().trimEnd();
    if (line) console.log(`[vite] ${line}`);
  });
  child.stderr?.on("data", (chunk) => {
    const line = chunk.toString().trimEnd();
    if (line) console.error(`[vite!] ${line}`);
  });
  await waitForServer(DEV_URL);
  console.log(`[doc] vite ready at ${DEV_URL}`);
  return child;
}

async function loadScenes(): Promise<SceneSpec[]> {
  const dir = resolve(__dirname, "scenes");
  const files = (await readdir(dir)).filter((f) => f.endsWith(".ts")).sort();
  const scenes: SceneSpec[] = [];
  for (const file of files) {
    const mod = await import(pathToFileURL(join(dir, file)).href);
    if (mod.default) scenes.push(mod.default as SceneSpec);
  }
  return scenes;
}

async function main() {
  const args = process.argv.slice(2);
  const sceneFilterIdx = args.indexOf("--scene");
  const sceneFilter = sceneFilterIdx >= 0 ? args[sceneFilterIdx + 1] : null;

  await mkdir(OUTPUT_DIR, { recursive: true });

  const all = await loadScenes();
  const scenes = sceneFilter ? all.filter((s) => s.id === sceneFilter) : all;
  if (scenes.length === 0) {
    console.error(sceneFilter ? `[doc] no scene with id "${sceneFilter}"` : "[doc] no scenes found in ./scenes");
    process.exit(1);
  }

  let devServer: ChildProcess | null = null;
  let browser: Browser | null = null;
  try {
    if (await isServerLive(DEV_URL)) {
      console.log(`[doc] reusing dev server already listening on :${DEV_PORT}`);
    } else {
      devServer = await spawnDevServer();
    }
    browser = await chromium.launch();
    // DSF=1 keeps screenshots at viewport resolution (e.g. 1440×900).
    // Astro's image pipeline produces responsive sizes from the source,
    // so 1× is fine for the docs site, and the smaller PNGs stay under
    // the 2000-px-per-side limit that AI tooling uses to read them back.
    //
    // Locale `es-ES` aligns with the manual's primary language: the app
    // detects the language from navigator.language on first launch, and
    // Playwright defaults to en-US otherwise, which would render every
    // capture in English regardless of the bundled translations.
    const ctx = await browser.newContext({ deviceScaleFactor: 1, locale: "es-ES" });
    // tsx/esbuild wraps named function declarations in __name() helpers
    // for stack-trace fidelity. When Playwright serializes our overlay
    // installer to ship to the page, those wrappers go too — and the
    // helper itself is left behind. Stub it as identity so the page
    // doesn't ReferenceError.
    await ctx.addInitScript(() => {
      (window as unknown as { __name: <T>(fn: T) => T }).__name = (fn) => fn;
    });
    const page = await ctx.newPage();

    // Surface page-side errors so a broken scene fails loudly instead
    // of producing a silent half-rendered screenshot.
    page.on("pageerror", (e) => console.error("[page error]", e.message));
    page.on("console", (msg) => {
      if (msg.type() === "error" || msg.type() === "warning") {
        console.warn(`[page ${msg.type()}]`, msg.text());
      }
    });

    for (const scene of scenes) {
      console.log(`[doc] capturing ${scene.id}…`);
      const out = await runScene(page, scene, { baseUrl: DEV_URL, outputDir: OUTPUT_DIR });
      console.log(`[doc] → ${out}`);
    }

    await browser.close();
    browser = null;
  } finally {
    if (browser) await browser.close().catch(() => {});
    if (devServer) {
      devServer.kill("SIGINT");
      // Give vite a moment to clean up its sockets so the next run can rebind.
      await new Promise((r) => setTimeout(r, 250));
    }
  }
  console.log("[doc] done");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
