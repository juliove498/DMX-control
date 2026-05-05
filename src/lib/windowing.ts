import {
  getAllWebviewWindows,
  getCurrentWebviewWindow,
  WebviewWindow,
} from "@tauri-apps/api/webviewWindow";

export const POPOUT_VIEWS = [
  "stage",
  "scenes",
  "chaser",
  "movement",
  "preview3d",
  "config",
] as const;
export type PopoutView = (typeof POPOUT_VIEWS)[number];

const TITLES: Record<PopoutView, string> = {
  stage: "DMX — Stage",
  scenes: "DMX — Scenes",
  chaser: "DMX — Chaser",
  movement: "DMX — Movement",
  preview3d: "DMX — Preview 3D",
  config: "DMX — Config",
};

export function readPopoutView(): PopoutView | null {
  const v = new URLSearchParams(window.location.search).get("popout");
  return (POPOUT_VIEWS as readonly string[]).includes(v ?? "") ? (v as PopoutView) : null;
}

export async function openPopout(view: PopoutView): Promise<void> {
  const label = `popout-${view}`;
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.unminimize().catch(() => {});
    await existing.setFocus();
    return;
  }
  // index.html?popout=<view> resolves both in dev (Vite root) and bundled
  // (Tauri asset protocol), so the same React app boots into single-view mode.
  new WebviewWindow(label, {
    url: `index.html?popout=${view}`,
    title: TITLES[view],
    width: 1280,
    height: 800,
  });
}

export async function toggleFullscreen(): Promise<void> {
  const win = getCurrentWebviewWindow();
  const isFs = await win.isFullscreen();
  await win.setFullscreen(!isFs);
}

/// Close every popout, then destroy the current window. Used as the
/// "really quit" path of the main window's close-requested confirm:
/// `destroy()` bypasses the close listener so we don't loop on
/// ourselves, and popouts are closed first so the OS doesn't briefly
/// show stale child windows after the main one is gone.
export async function closeAppCascade(): Promise<void> {
  const me = getCurrentWebviewWindow();
  const all = await getAllWebviewWindows();
  await Promise.allSettled(
    all.filter((w) => w.label !== me.label).map((w) => w.close()),
  );
  await me.destroy();
}
