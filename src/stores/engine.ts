import type { EngineStats } from "@bindings/EngineStats";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

interface EngineStoreState {
  stats: EngineStats | null;
  master: number;
  blackout: boolean;
  channels: Uint8Array;
  setMaster: (value: number) => Promise<void>;
  setBlackout: (on: boolean) => Promise<void>;
  setChannel: (universe: number, channel: number, value: number) => Promise<void>;
  clearAll: () => Promise<void>;
  refresh: (universe: number) => Promise<void>;
  initListeners: () => Promise<() => void>;
}

const DMX = 512;

export const useEngineStore = create<EngineStoreState>((set, get) => ({
  stats: null,
  master: 255,
  blackout: false,
  channels: new Uint8Array(DMX),

  async setMaster(value) {
    set({ master: value });
    await invoke("set_master", { value });
  },

  async setBlackout(on) {
    set({ blackout: on });
    await invoke("blackout", { on });
  },

  async setChannel(universe, channel, value) {
    const next = new Uint8Array(get().channels);
    next[channel] = value;
    set({ channels: next });
    await invoke("set_channel", { universe, channel, value });
  },

  async clearAll() {
    set({ channels: new Uint8Array(DMX) });
    await invoke("clear_universe", { universe: 0 });
  },

  async refresh(universe) {
    const snap = await invoke<{ id: number; data: number[] }>("get_universe", { universe });
    set({ channels: new Uint8Array(snap.data) });
  },

  async initListeners() {
    const unlisten = await listen<EngineStats>("engine:stats", (e) => {
      set({ stats: e.payload });
    });
    return unlisten;
  },
}));
