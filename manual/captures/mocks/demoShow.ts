// Realistic-but-tiny mock show used as a baseline by most captures.
// Six fixtures arranged in two rows so the StageView has something
// non-trivial to render. Library has a single matching definition so
// the patch validates and fixtures display sensible labels.

import type { ShowFileV1 } from "@bindings/ShowFileV1";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";

const PAR_RGBW: FixtureDefinition = {
  id: "demo.par-rgbw",
  manufacturer: "Demo",
  name: "Par RGBW",
  image: null,
  modes: [
    {
      name: "4ch",
      channels: [
        { name: "Red", role: "RED", default: 0, ranges: [] },
        { name: "Green", role: "GREEN", default: 0, ranges: [] },
        { name: "Blue", role: "BLUE", default: 0, ranges: [] },
        { name: "White", role: "WHITE", default: 0, ranges: [] },
      ],
    },
    // biome-ignore lint/suspicious/noExplicitAny: ts-rs binding shape varies; the runtime only reads channels[].name during render
  ] as any,
};

const fixtures: FixtureInstance[] = [
  { id: "fx-1", definition_id: PAR_RGBW.id, mode_index: 0, universe: 1, address: 1,  label: "PAR L1", position: [80,  120] },
  { id: "fx-2", definition_id: PAR_RGBW.id, mode_index: 0, universe: 1, address: 5,  label: "PAR L2", position: [240, 120] },
  { id: "fx-3", definition_id: PAR_RGBW.id, mode_index: 0, universe: 1, address: 9,  label: "PAR L3", position: [400, 120] },
  { id: "fx-4", definition_id: PAR_RGBW.id, mode_index: 0, universe: 1, address: 13, label: "PAR R1", position: [80,  280] },
  { id: "fx-5", definition_id: PAR_RGBW.id, mode_index: 0, universe: 1, address: 17, label: "PAR R2", position: [240, 280] },
  { id: "fx-6", definition_id: PAR_RGBW.id, mode_index: 0, universe: 1, address: 21, label: "PAR R3", position: [400, 280] },
];

export const DEMO_SHOW: ShowFileV1 = {
  version: 1,
  name: "Demo Show",
  fixtures,
  outputs: { bindings: [], sacn_cid: null },
  chasers: [],
  movement: null,
  movements: [],
  scenes: [],
  library: [PAR_RGBW],
  globals: {
    blackout: { active: false, fade_in_ms: 500, fade_out_ms: 800, fixtures: [] },
    blind: { fade_in_ms: 80, fade_out_ms: 1500, fixtures: [] },
    master: { fixtures: [] },
    overall_bpm_enabled: false,
    overall_bpm: 120,
  },
};

export const DEMO_LIBRARY: FixtureDefinition[] = [PAR_RGBW];
