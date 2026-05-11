// Capture engine. A "scene" is a single screenshot's worth of work:
// which route, what mock state to push into the app, what (if any)
// interactions to drive (click a tab, hover an element), and what
// annotations to overlay before the snapshot.
//
// The runner is intentionally synchronous-feeling: hydrate → wait for
// idle → interact → annotate → snapshot. No retries, no parallelism —
// captures are deterministic and re-running is cheap.

import type { Page } from "playwright";
import type { ShowFileV1 } from "@bindings/ShowFileV1";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { PatchReport } from "@bindings/PatchReport";
import type { StreamDeckDeviceInfo } from "@bindings/StreamDeckDeviceInfo";
import type { StreamDeckStatus } from "@bindings/StreamDeckStatus";
import type { TouchedFixture } from "@bindings/TouchedFixture";
import { type Annotation, DEFAULT_COLOR, installOverlayInPage } from "./annotate.ts";

export type Tab = "stage" | "scenes" | "chaser" | "movement" | "preview3d" | "config";

export type SceneInteraction =
  | { type: "click"; selector: string }
  | { type: "hover"; selector: string }
  | { type: "wait"; ms: number };

export interface SceneMock {
  show?: ShowFileV1 | null;
  library?: FixtureDefinition[];
  activeSceneId?: string | null;
  activeSceneStep?: number | null;
  touched?: string[];
  touchedChannels?: TouchedFixture[];
  patch?: PatchReport;
  streamDeckDevices?: StreamDeckDeviceInfo[];
  streamDeckStatus?: StreamDeckStatus;
}

export interface SceneSpec {
  id: string;
  tab?: Tab;
  mock: SceneMock;
  interactions?: SceneInteraction[];
  annotations?: Annotation[];
  caption?: { es: string; en?: string };
  // Override viewport for this specific scene. Defaults to 1440×900,
  // which matches the resolution most demo screenshots are produced at.
  viewport?: { width: number; height: number };
}

export interface RunOptions {
  baseUrl: string;
  outputDir: string;
}

export async function runScene(page: Page, scene: SceneSpec, opts: RunOptions): Promise<string> {
  const viewport = scene.viewport ?? { width: 1440, height: 900 };
  await page.setViewportSize(viewport);

  await page.goto(`${opts.baseUrl}/?doc=1`, { waitUntil: "domcontentloaded" });

  // Wait for the doc bridge to be installed by main.tsx — it's set up
  // before React mounts, but we still race the script load itself.
  await page.waitForFunction(() => Boolean((window as unknown as { __DOC__?: unknown }).__DOC__), null, { timeout: 5000 });

  // Push mock state into the store.
  await page.evaluate(async (mock) => {
    const w = window as unknown as { __DOC__: { hydrate: (s: unknown) => Promise<void> } };
    await w.__DOC__.hydrate(mock);
  }, scene.mock);

  if (scene.tab) {
    // The app's tab state is local React state, not in the store, so we
    // click the visible button. Tabs are labeled via i18n; matching by
    // a stable data attribute we'll seed in App.tsx.
    await page.click(`[data-doc-tab="${scene.tab}"]`).catch(async () => {
      // Fallback: match by visible text in Spanish (default locale).
      const labels: Record<Tab, string> = {
        stage: "Stage",
        scenes: "Escenas",
        chaser: "Chaser",
        movement: "Movement",
        preview3d: "Preview 3D",
        config: "Config",
      };
      await page.getByRole("button", { name: labels[scene.tab!] }).first().click();
    });
  }

  for (const step of scene.interactions ?? []) {
    if (step.type === "click") await page.click(step.selector);
    else if (step.type === "hover") await page.hover(step.selector);
    else if (step.type === "wait") await page.waitForTimeout(step.ms);
  }

  // Settle: small wait for animations + layout. 150ms is enough for
  // CSS transitions in the app and avoids a flaky first-frame snapshot.
  await page.waitForTimeout(150);

  if (scene.annotations?.length) {
    // Ship the installer function source to the page; Playwright handles
    // serialization. Args are JSON-cloned, so they must be plain data.
    await page.evaluate(installOverlayInPage, {
      annotations: scene.annotations,
      viewport,
      defaultColor: DEFAULT_COLOR,
    });
  }

  const outPath = `${opts.outputDir}/${scene.id}.png`;
  await page.screenshot({ path: outPath, fullPage: false });
  return outPath;
}
