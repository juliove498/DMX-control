// Documentation mode: when the URL carries `?doc=1`, the app boots inside
// a plain browser (vite dev) instead of Tauri's webview, so the manual's
// capture pipeline can drive it with Playwright. Tauri's IPC bridge is
// absent in that environment, so any `invoke()` call would throw — we
// install a minimal stub that resolves with empty/null defaults and
// expose `window.__DOC__.hydrate()` so the capture script can seed the
// Zustand store with realistic mock state before each snapshot.
//
// This file has zero effect outside doc mode: `installDocStubs()` returns
// early when the flag is absent, so the production Tauri build is
// unaffected and the bundle cost is just the dead-code-eliminable shim.

import type { ShowFileV1 } from "@bindings/ShowFileV1";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { PatchReport } from "@bindings/PatchReport";
import type { StreamDeckDeviceInfo } from "@bindings/StreamDeckDeviceInfo";
import type { StreamDeckStatus } from "@bindings/StreamDeckStatus";
import type { TouchedFixture } from "@bindings/TouchedFixture";

declare global {
  interface Window {
    __DOC__?: DocBridge;
    __TAURI_INTERNALS__?: unknown;
    __TAURI_EVENT_PLUGIN_INTERNALS__?: unknown;
  }
}

interface DocBridge {
  show: ShowFileV1 | null;
  library: FixtureDefinition[];
  activeSceneId: string | null;
  activeSceneStep: number | null;
  touched: string[];
  touchedChannels: TouchedFixture[];
  patch: PatchReport;
  streamDeckDevices: StreamDeckDeviceInfo[];
  streamDeckStatus: StreamDeckStatus;
  hydrate(state: Partial<Omit<DocBridge, "hydrate">>): Promise<void>;
}

export const isDocMode = (): boolean =>
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).get("doc") === "1";

export function installDocStubs(): void {
  if (!isDocMode()) return;

  let nextCallbackId = 0;
  const callbacks = new Map<number, (payload: unknown) => void>();

  // Router for invoke() calls. Most setters are no-ops; getters look up
  // the current __DOC__ snapshot so the script can mutate state and have
  // subsequent invokes reflect it. Unknown commands warn once and
  // resolve to null so adding a new view doesn't immediately break doc.
  const warned = new Set<string>();
  const invoke = async (cmd: string, _args?: unknown): Promise<unknown> => {
    const doc = window.__DOC__!;
    switch (cmd) {
      case "get_show":
        return doc.show;
      case "get_show_path":
        return null;
      case "list_fixture_definitions":
        return doc.library;
      case "get_library_dir":
        return null;
      case "validate_patch_cmd":
        return doc.patch;
      case "list_serial_ports_cmd":
        return [];
      case "list_ftdi_devices":
        return [];
      case "active_scene_id":
        return doc.activeSceneId;
      case "active_scene_step":
        return doc.activeSceneStep;
      case "programmer_status":
        return { touched: doc.touched, channels: doc.touchedChannels };
      case "get_fixture_values":
        return [];
      case "list_midi_devices":
        return [];
      case "list_streamdeck_devices":
        return doc.streamDeckDevices;
      case "get_streamdeck_status":
        return doc.streamDeckStatus;
      // Setters and event-plugin commands: silently no-op.
      default:
        if (cmd.startsWith("plugin:event|")) return cmd === "plugin:event|listen" ? ++nextCallbackId : null;
        if (!warned.has(cmd)) {
          warned.add(cmd);
          console.debug(`[doc] unmocked invoke: ${cmd}`);
        }
        return null;
    }
  };

  window.__TAURI_INTERNALS__ = {
    invoke,
    transformCallback: (cb: (payload: unknown) => void) => {
      const id = ++nextCallbackId;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id: number) => {
      callbacks.delete(id);
    },
    convertFileSrc: (filePath: string) => filePath,
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
    },
  };

  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: () => {},
  };

  const bridge: DocBridge = {
    show: null,
    library: [],
    activeSceneId: null,
    activeSceneStep: null,
    touched: [],
    touchedChannels: [],
    patch: { conflicts: [], problems: [] },
    streamDeckDevices: [],
    streamDeckStatus: { connected: null, kind: null, key_count: null },
    async hydrate(state) {
      if (state.show !== undefined) bridge.show = state.show;
      if (state.library !== undefined) bridge.library = state.library;
      if (state.activeSceneId !== undefined) bridge.activeSceneId = state.activeSceneId;
      if (state.activeSceneStep !== undefined) bridge.activeSceneStep = state.activeSceneStep;
      if (state.touched !== undefined) bridge.touched = state.touched;
      if (state.touchedChannels !== undefined) bridge.touchedChannels = state.touchedChannels;
      if (state.patch !== undefined) bridge.patch = state.patch;
      if (state.streamDeckDevices !== undefined) bridge.streamDeckDevices = state.streamDeckDevices;
      if (state.streamDeckStatus !== undefined) bridge.streamDeckStatus = state.streamDeckStatus;
      // Push into the Zustand store so reactive selectors update without
      // waiting for the next refresh() round-trip.
      const { useShowStore } = await import("../stores/show");
      const storePatch: Record<string, unknown> = {};
      if (state.show !== undefined) storePatch.show = state.show;
      if (state.library !== undefined) storePatch.library = state.library;
      if (state.patch !== undefined) storePatch.patch = state.patch;
      useShowStore.setState(storePatch as never);
    },
  };
  window.__DOC__ = bridge;
}
