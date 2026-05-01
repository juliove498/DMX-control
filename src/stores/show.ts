import type { AmbientChaser } from "@bindings/AmbientChaser";
import type { D2xxDeviceInfo } from "@bindings/D2xxDeviceInfo";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import type { GlobalsConfig } from "@bindings/GlobalsConfig";
import type { MovementGenerator } from "@bindings/MovementGenerator";
import type { OutputsConfig } from "@bindings/OutputsConfig";
import type { PatchReport } from "@bindings/PatchReport";
import type { SerialPortInfo } from "@bindings/SerialPortInfo";
import type { ShowFileV1 } from "@bindings/ShowFileV1";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

interface ShowStoreState {
  show: ShowFileV1 | null;
  showPath: string | null;
  library: FixtureDefinition[];
  libraryDir: string | null;
  patch: PatchReport;
  serialPorts: SerialPortInfo[];
  ftdiDevices: D2xxDeviceInfo[];

  refresh: () => Promise<void>;
  refreshPatch: () => Promise<void>;
  refreshSerialPorts: () => Promise<void>;
  refreshFtdiDevices: () => Promise<void>;
  reloadLibrary: () => Promise<void>;
  setFixtureImage: (definitionId: string, sourcePath: string) => Promise<void>;

  newShow: () => Promise<void>;
  openShow: (path: string) => Promise<void>;
  saveShow: (path?: string) => Promise<string>;

  setOutputs: (outputs: OutputsConfig) => Promise<void>;

  addFixture: (fixture: FixtureInstance) => Promise<void>;
  addFixtures: (fixtures: FixtureInstance[]) => Promise<void>;
  removeFixture: (id: string) => Promise<void>;
  updateFixture: (fixture: FixtureInstance) => Promise<void>;
  moveFixture: (id: string, x: number, y: number) => Promise<void>;
  setFixtureChannel: (id: string, channelOffset: number, value: number) => Promise<void>;
  getFixtureValues: (id: string) => Promise<number[]>;

  createChaser: (name?: string) => Promise<AmbientChaser>;
  updateChaser: (chaser: AmbientChaser) => Promise<void>;
  deleteChaser: (id: string) => Promise<void>;
  toggleChaser: (id: string, enabled: boolean) => Promise<void>;
  addExampleChasers: () => Promise<number>;

  createMovement: (name?: string) => Promise<MovementGenerator>;
  updateMovement: (gen: MovementGenerator) => Promise<void>;
  deleteMovement: (id: string) => Promise<void>;
  toggleMovement: (id: string, enabled: boolean) => Promise<void>;

  setBlackout: (active: boolean) => Promise<void>;
  setBlind: (pressed: boolean) => Promise<void>;
  updateGlobals: (config: GlobalsConfig) => Promise<void>;

  listMidiDevices: () => Promise<{ name: string; has_input: boolean; has_output: boolean }[]>;
  connectMidiDevice: (name: string) => Promise<void>;
  disconnectMidi: () => Promise<void>;
  sendMidiRaw: (bytes: number[]) => Promise<void>;

  initListeners: () => Promise<() => void>;
}

const emptyPatch: PatchReport = { conflicts: [], problems: [] };

export const useShowStore = create<ShowStoreState>((set, get) => ({
  show: null,
  showPath: null,
  library: [],
  libraryDir: null,
  patch: emptyPatch,
  serialPorts: [],
  ftdiDevices: [],

  async refresh() {
    const [show, showPath, library, patch, ports, ftdi, libraryDir] = await Promise.all([
      invoke<ShowFileV1>("get_show"),
      invoke<string | null>("get_show_path"),
      invoke<FixtureDefinition[]>("list_fixture_definitions"),
      invoke<PatchReport>("validate_patch_cmd"),
      invoke<SerialPortInfo[]>("list_serial_ports_cmd"),
      invoke<D2xxDeviceInfo[]>("list_ftdi_devices"),
      invoke<string | null>("get_library_dir"),
    ]);
    library.sort((a, b) =>
      `${a.manufacturer} ${a.name}`.localeCompare(`${b.manufacturer} ${b.name}`),
    );
    set({ show, showPath, library, libraryDir, patch, serialPorts: ports, ftdiDevices: ftdi });
  },

  async refreshPatch() {
    const patch = await invoke<PatchReport>("validate_patch_cmd");
    set({ patch });
  },

  async refreshSerialPorts() {
    const ports = await invoke<SerialPortInfo[]>("list_serial_ports_cmd");
    set({ serialPorts: ports });
  },

  async refreshFtdiDevices() {
    const ftdi = await invoke<D2xxDeviceInfo[]>("list_ftdi_devices");
    set({ ftdiDevices: ftdi });
  },

  async reloadLibrary() {
    await invoke<number>("reload_library");
    const library = await invoke<FixtureDefinition[]>("list_fixture_definitions");
    library.sort((a, b) =>
      `${a.manufacturer} ${a.name}`.localeCompare(`${b.manufacturer} ${b.name}`),
    );
    set({ library });
  },

  async setFixtureImage(definitionId, sourcePath) {
    await invoke<string>("set_fixture_image", { definitionId, sourcePath });
    await get().reloadLibrary();
  },

  async newShow() {
    await invoke("new_show");
    await get().refresh();
  },

  async openShow(path) {
    await invoke("open_show", { path });
    await get().refresh();
  },

  async saveShow(path) {
    const result = await invoke<string>("save_show", { path: path ?? null });
    set({ showPath: result });
    return result;
  },

  async setOutputs(outputs) {
    await invoke("set_outputs", { outputs });
    await get().refresh();
  },

  async addFixture(fixture) {
    await invoke("add_fixture", { fixture });
    await get().refresh();
  },

  async addFixtures(fixtures) {
    await invoke("add_fixtures", { fixtures });
    await get().refresh();
  },

  async removeFixture(id) {
    await invoke("remove_fixture", { id });
    await get().refresh();
  },

  async updateFixture(fixture) {
    await invoke("update_fixture", { fixture });
    await get().refresh();
  },

  async moveFixture(id, x, y) {
    await invoke("move_fixture", { id, x, y });
    // Optimistic local update so the drag UI doesn't snap-back.
    const show = get().show;
    if (show) {
      set({
        show: {
          ...show,
          fixtures: show.fixtures.map((f) => (f.id === id ? { ...f, position: [x, y] } : f)),
        },
      });
    }
  },

  async setFixtureChannel(id, channelOffset, value) {
    await invoke("set_fixture_channel", {
      fixtureId: id,
      channelOffset,
      value,
    });
  },

  async getFixtureValues(id) {
    return invoke<number[]>("get_fixture_values", { fixtureId: id });
  },

  async createChaser(name) {
    const chaser = await invoke<AmbientChaser>("create_chaser", { name: name ?? null });
    await get().refresh();
    return chaser;
  },

  async updateChaser(chaser) {
    await invoke("update_chaser", { chaser });
    await get().refresh();
  },

  async deleteChaser(id) {
    await invoke("delete_chaser", { id });
    await get().refresh();
  },

  async toggleChaser(id, enabled) {
    await invoke("toggle_chaser", { id, enabled });
    await get().refresh();
  },

  async addExampleChasers() {
    const n = await invoke<number>("add_example_chasers");
    await get().refresh();
    return n;
  },

  async createMovement(name) {
    const m = await invoke<MovementGenerator>("create_movement", { name: name ?? null });
    await get().refresh();
    return m;
  },

  async updateMovement(gen) {
    await invoke("update_movement", { generator: gen });
    await get().refresh();
  },

  async deleteMovement(id) {
    await invoke("delete_movement", { id });
    await get().refresh();
  },

  async toggleMovement(id, enabled) {
    await invoke("toggle_movement", { id, enabled });
    await get().refresh();
  },

  async setBlackout(active) {
    await invoke("set_blackout", { on: active });
    await get().refresh();
  },

  async setBlind(pressed) {
    // No refresh: blind is momentary and not persisted, so the show file
    // wouldn't change anyway. Skipping the round-trip keeps the press feel
    // tight on long-running shows with bigger snapshots.
    await invoke("set_blind", { pressed });
  },

  async updateGlobals(config) {
    await invoke("update_globals", { config });
    await get().refresh();
  },

  async listMidiDevices() {
    return invoke<{ name: string; has_input: boolean; has_output: boolean }[]>("list_midi_devices");
  },

  async connectMidiDevice(name) {
    await invoke("connect_midi_device", { name });
  },

  async disconnectMidi() {
    await invoke("disconnect_midi");
  },

  async sendMidiRaw(bytes) {
    await invoke("send_midi_raw", { bytes });
  },

  async initListeners() {
    const unlisten = await listen("show:updated", () => {
      get().refresh();
    });
    return unlisten;
  },
}));
