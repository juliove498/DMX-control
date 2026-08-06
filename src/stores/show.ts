import type { AiAvailableModels } from "@bindings/AiAvailableModels";
import type { AiConfig } from "@bindings/AiConfig";
import type { AmbientChaser } from "@bindings/AmbientChaser";
import type { ArtNetNodeInfo } from "@bindings/ArtNetNodeInfo";
import type { AudioBpmStatus } from "@bindings/AudioBpmStatus";
import type { AudioInputInfo } from "@bindings/AudioInputInfo";
import type { ButtonBindings } from "@bindings/ButtonBindings";
import type { D2xxDeviceInfo } from "@bindings/D2xxDeviceInfo";
import type { DraftScene } from "@bindings/DraftScene";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import type { GlobalsConfig } from "@bindings/GlobalsConfig";
import type { LoopGroupActiveChange } from "@bindings/LoopGroupActiveChange";
import type { MovementGenerator } from "@bindings/MovementGenerator";
import type { OutputsConfig } from "@bindings/OutputsConfig";
import type { PatchReport } from "@bindings/PatchReport";
import type { ProgrammerStatus } from "@bindings/ProgrammerStatus";
import type { Scene } from "@bindings/Scene";
import type { SceneLoopGroup } from "@bindings/SceneLoopGroup";
import type { SerialPortInfo } from "@bindings/SerialPortInfo";
import type { ShowFileV1 } from "@bindings/ShowFileV1";
import type { Snapshot } from "@bindings/Snapshot";
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
  setChannelRangeImage: (
    definitionId: string,
    modeIndex: number,
    channelIndex: number,
    rangeIndex: number,
    sourcePath: string,
  ) => Promise<void>;

  newShow: () => Promise<void>;
  openShow: (path: string) => Promise<void>;
  saveShow: (path?: string) => Promise<string>;
  renameShow: (name: string) => Promise<void>;

  setOutputs: (outputs: OutputsConfig) => Promise<void>;
  /// Broadcast an ArtPoll and collect ArtPollReply packets for
  /// ~2.5 s. Resolves with every Art-Net node found on the LAN.
  artnetScan: (timeoutMs?: number) => Promise<ArtNetNodeInfo[]>;

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

  createSceneFromState: (
    name: string,
    fixtureIds: string[],
    fadeInMs: number,
    restrictToTouched: boolean,
    captureChaser: boolean,
    captureMovement: boolean,
  ) => Promise<Scene>;
  addSceneStep: (
    sceneId: string,
    fixtureIds: string[],
    fadeInMs: number,
    holdMs: number,
    restrictToTouched: boolean,
  ) => Promise<Scene>;
  removeSceneStep: (sceneId: string, stepId: string) => Promise<Scene>;
  updateSceneStepFromState: (
    sceneId: string,
    stepId: string,
    restrictToTouched: boolean,
  ) => Promise<Scene>;
  updateScene: (scene: Scene) => Promise<void>;
  deleteScene: (id: string) => Promise<void>;
  recallScene: (id: string) => Promise<void>;
  releaseScene: () => Promise<void>;
  activeSceneId: () => Promise<string | null>;
  activeSceneStep: () => Promise<number | null>;

  // Snapshots (whole-rig capture / toggle)
  captureSnapshot: (name?: string) => Promise<Snapshot>;
  updateSnapshotFromState: (id: string) => Promise<Snapshot>;
  renameSnapshot: (id: string, name: string) => Promise<void>;
  deleteSnapshot: (id: string) => Promise<void>;
  activateSnapshot: (id: string) => Promise<void>;
  deactivateSnapshot: () => Promise<void>;
  activeSnapshotId: () => Promise<string | null>;

  // Sequence loop groups (playlists of scenes)
  listLoopGroups: () => Promise<SceneLoopGroup[]>;
  createLoopGroup: (name?: string) => Promise<SceneLoopGroup>;
  updateLoopGroup: (group: SceneLoopGroup) => Promise<void>;
  deleteLoopGroup: (id: string) => Promise<void>;
  startLoopGroup: (id: string) => Promise<void>;
  stopLoopGroup: () => Promise<void>;
  activeLoopGroup: () => Promise<LoopGroupActiveChange>;

  // Button bindings (Launchpad + Stream Deck)
  getButtonBindings: () => Promise<ButtonBindings>;
  updateButtonBindings: (bindings: ButtonBindings) => Promise<void>;
  getDefaultButtonBindings: () => Promise<ButtonBindings>;
  programmerStatus: () => Promise<ProgrammerStatus>;
  programmerClear: () => Promise<void>;
  programmerUntouch: (fixtureId: string) => Promise<void>;

  setBlackout: (active: boolean) => Promise<void>;
  setBlind: (pressed: boolean) => Promise<void>;
  updateGlobals: (config: GlobalsConfig) => Promise<void>;

  setOverallBpm: (bpm: number) => Promise<void>;
  setOverallBpmEnabled: (enabled: boolean) => Promise<void>;
  /// Resolves to the freshly-computed BPM (when 2+ taps in the rolling
  /// window) or `null` (first tap of a fresh window).
  tapOverallBpm: () => Promise<number | null>;
  /// Begin a tempo-pattern recording window. Subsequent calls to
  /// `tapPatternRecord` populate the buffer; `stopPatternRecording`
  /// quantises and commits.
  startPatternRecording: () => Promise<void>;
  tapPatternRecord: () => Promise<void>;
  /// Returns the freshly committed TempoPattern, or `null` if fewer
  /// than 2 taps were recorded (the previous pattern, if any, stays
  /// untouched in that case).
  stopPatternRecording: () => Promise<import("@bindings/TempoPattern").TempoPattern | null>;
  /// Drop the active pattern; chasers go back to plain overall_bpm.
  clearTempoPattern: () => Promise<void>;

  // Audio BPM counter (mic / line-in)
  audioBpmDevices: () => Promise<AudioInputInfo[]>;
  audioBpmStart: (device?: string | null) => Promise<void>;
  audioBpmStop: () => Promise<void>;
  audioBpmStatus: () => Promise<AudioBpmStatus>;
  audioBpmSetAuto: (enabled: boolean) => Promise<void>;

  listMidiDevices: () => Promise<{ name: string; has_input: boolean; has_output: boolean }[]>;
  connectMidiDevice: (name: string) => Promise<void>;
  disconnectMidi: () => Promise<void>;
  sendMidiRaw: (bytes: number[]) => Promise<void>;

  listStreamDeckDevices: () => Promise<{ serial: string; kind: string; key_count: number }[]>;
  connectStreamDeckDevice: (serial?: string) => Promise<void>;
  disconnectStreamDeck: () => Promise<void>;

  // AI scene generation (POC) — keys live off the show file in the OS
  // app-config dir, so these don't touch show state.
  getAiConfig: () => Promise<AiConfig>;
  setAiConfig: (config: AiConfig) => Promise<void>;
  aiListModels: () => Promise<AiAvailableModels>;
  aiTestConnection: () => Promise<string>;
  aiGenerateSceneDraft: (
    prompt: string,
    stepCount: number,
    fixtureIds: string[] | null,
    seed?: DraftScene | null,
  ) => Promise<DraftScene>;
  aiApplyDraftScene: (draft: DraftScene) => Promise<void>;
  aiReplaceScene: (sceneId: string, draft: DraftScene) => Promise<void>;

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

  async setChannelRangeImage(definitionId, modeIndex, channelIndex, rangeIndex, sourcePath) {
    await invoke<string>("set_channel_range_image", {
      definitionId,
      modeIndex,
      channelIndex,
      rangeIndex,
      sourcePath,
    });
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
    // The backend may have updated the show.name (e.g. "Save As" took
    // the filename stem when the show was still on its default name);
    // pull the fresh state so the UI reflects it without flicker.
    await get().refresh();
    return result;
  },

  async renameShow(name) {
    await invoke("rename_show", { name });
    await get().refresh();
  },

  async setOutputs(outputs) {
    await invoke("set_outputs", { outputs });
    await get().refresh();
  },

  async artnetScan(timeoutMs) {
    return invoke<ArtNetNodeInfo[]>("artnet_scan", { timeoutMs: timeoutMs ?? null });
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

  async createSceneFromState(
    name,
    fixtureIds,
    fadeInMs,
    restrictToTouched,
    captureChaser,
    captureMovement,
  ) {
    const s = await invoke<Scene>("create_scene_from_state", {
      name,
      fixtureIds,
      fadeInMs,
      restrictToTouched,
      captureChaser,
      captureMovement,
    });
    await get().refresh();
    return s;
  },

  async addSceneStep(sceneId, fixtureIds, fadeInMs, holdMs, restrictToTouched) {
    const s = await invoke<Scene>("add_scene_step", {
      sceneId,
      fixtureIds,
      fadeInMs,
      holdMs,
      restrictToTouched,
    });
    await get().refresh();
    return s;
  },

  async removeSceneStep(sceneId, stepId) {
    const s = await invoke<Scene>("remove_scene_step", { sceneId, stepId });
    await get().refresh();
    return s;
  },

  async updateSceneStepFromState(sceneId, stepId, restrictToTouched) {
    const s = await invoke<Scene>("update_scene_step_from_state", {
      sceneId,
      stepId,
      restrictToTouched,
    });
    await get().refresh();
    return s;
  },

  async updateScene(scene) {
    await invoke("update_scene", { scene });
    await get().refresh();
  },

  async programmerStatus() {
    return invoke<ProgrammerStatus>("programmer_status");
  },

  async programmerClear() {
    await invoke("programmer_clear");
  },

  async programmerUntouch(fixtureId) {
    await invoke("programmer_untouch", { fixtureId });
  },

  async deleteScene(id) {
    await invoke("delete_scene", { id });
    await get().refresh();
  },

  async recallScene(id) {
    await invoke("recall_scene", { id });
  },

  async releaseScene() {
    await invoke("release_scene");
  },

  async activeSceneId() {
    return invoke<string | null>("active_scene_id");
  },

  async activeSceneStep() {
    return invoke<number | null>("active_scene_step");
  },

  // ---- Snapshots ----
  async captureSnapshot(name) {
    const snap = await invoke<Snapshot>("capture_snapshot", { name: name ?? null });
    await get().refresh();
    return snap;
  },
  async updateSnapshotFromState(id) {
    const snap = await invoke<Snapshot>("update_snapshot_from_state", { id });
    await get().refresh();
    return snap;
  },
  async renameSnapshot(id, name) {
    await invoke("rename_snapshot", { id, name });
    await get().refresh();
  },
  async deleteSnapshot(id) {
    await invoke("delete_snapshot", { id });
    await get().refresh();
  },
  async activateSnapshot(id) {
    await invoke("activate_snapshot", { id });
    await get().refresh();
  },
  async deactivateSnapshot() {
    await invoke("deactivate_snapshot");
    await get().refresh();
  },
  async activeSnapshotId() {
    return invoke<string | null>("active_snapshot_id");
  },

  // ---- Sequence loop groups ----
  async listLoopGroups() {
    return invoke<SceneLoopGroup[]>("list_loop_groups");
  },
  async createLoopGroup(name) {
    const g = await invoke<SceneLoopGroup>("create_loop_group", { name: name ?? null });
    await get().refresh();
    return g;
  },
  async updateLoopGroup(group) {
    await invoke("update_loop_group", { group });
    await get().refresh();
  },
  async deleteLoopGroup(id) {
    await invoke("delete_loop_group", { id });
    await get().refresh();
  },
  async startLoopGroup(id) {
    await invoke("start_loop_group", { id });
  },
  async stopLoopGroup() {
    await invoke("stop_loop_group");
  },
  async activeLoopGroup() {
    return invoke<LoopGroupActiveChange>("active_loop_group");
  },

  // ---- Button bindings ----
  async getButtonBindings() {
    return invoke<ButtonBindings>("get_button_bindings");
  },
  async updateButtonBindings(bindings) {
    await invoke("update_button_bindings", { bindings });
    await get().refresh();
  },
  async getDefaultButtonBindings() {
    return invoke<ButtonBindings>("get_default_button_bindings");
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

  async setOverallBpm(bpm) {
    await invoke("set_overall_bpm", { bpm });
    await get().refresh();
  },

  async setOverallBpmEnabled(enabled) {
    await invoke("set_overall_bpm_enabled", { enabled });
    await get().refresh();
  },

  async tapOverallBpm() {
    const result = await invoke<number | null>("tap_overall_bpm");
    await get().refresh();
    return result;
  },

  async startPatternRecording() {
    await invoke("start_pattern_recording");
  },

  async tapPatternRecord() {
    await invoke("tap_pattern_record");
  },

  async stopPatternRecording() {
    const result = await invoke<import("@bindings/TempoPattern").TempoPattern | null>(
      "stop_pattern_recording",
    );
    await get().refresh();
    return result;
  },

  async clearTempoPattern() {
    await invoke("clear_tempo_pattern");
    await get().refresh();
  },

  // ---- Audio BPM counter ----
  async audioBpmDevices() {
    return invoke<AudioInputInfo[]>("audio_bpm_devices");
  },
  async audioBpmStart(device) {
    await invoke("audio_bpm_start", { device: device ?? null });
  },
  async audioBpmStop() {
    await invoke("audio_bpm_stop");
  },
  async audioBpmStatus() {
    return invoke<AudioBpmStatus>("audio_bpm_status");
  },
  async audioBpmSetAuto(enabled) {
    await invoke("audio_bpm_set_auto", { enabled });
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

  async listStreamDeckDevices() {
    return invoke<{ serial: string; kind: string; key_count: number }[]>("list_streamdeck_devices");
  },

  async connectStreamDeckDevice(serial) {
    await invoke("connect_streamdeck_device", { serial: serial ?? null });
  },

  async disconnectStreamDeck() {
    await invoke("disconnect_streamdeck");
  },

  async getAiConfig() {
    return invoke<AiConfig>("get_ai_config");
  },

  async setAiConfig(config) {
    await invoke("set_ai_config", { config });
  },

  async aiListModels() {
    return invoke<AiAvailableModels>("ai_list_models");
  },

  async aiTestConnection() {
    return invoke<string>("ai_test_connection");
  },

  async aiGenerateSceneDraft(prompt, stepCount, fixtureIds, seed) {
    return invoke<DraftScene>("ai_generate_scene_draft", {
      prompt,
      stepCount,
      fixtureIds,
      seed: seed ?? null,
    });
  },

  async aiApplyDraftScene(draft) {
    await invoke("ai_apply_draft_scene", { draft });
    await get().refresh();
  },

  async aiReplaceScene(sceneId, draft) {
    await invoke("ai_replace_scene", { sceneId, draft });
    await get().refresh();
  },

  async initListeners() {
    const unlisten = await listen("show:updated", () => {
      get().refresh();
    });
    return unlisten;
  },
}));
