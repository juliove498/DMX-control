// Show with a moving-head fixture in the library and one movement
// generator enabled. The Movement view filters its "add fixture"
// dropdown to fixtures whose mode actually has pan/tilt channels, so
// the library entry below uses lowercase "pan" / "tilt" roles.
//
// Built on top of DEMO_SHOW so the Stage and Patch views still have
// their PARs available — captures of the Movement view inherit a
// realistic-looking show file.

import type { ShowFileV1 } from "@bindings/ShowFileV1";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import type { MovementGenerator } from "@bindings/MovementGenerator";
import { DEMO_LIBRARY, DEMO_SHOW } from "./demoShow.ts";

const MOVING_HEAD: FixtureDefinition = {
  id: "demo.moving-head",
  manufacturer: "Demo",
  name: "Moving Head",
  image: null,
  modes: [
    {
      name: "8ch",
      channels: [
        { name: "Pan", role: "pan", default: 128, ranges: [] },
        { name: "Tilt", role: "tilt", default: 128, ranges: [] },
        { name: "Red", role: "red", default: 0, ranges: [] },
        { name: "Green", role: "green", default: 0, ranges: [] },
        { name: "Blue", role: "blue", default: 0, ranges: [] },
        { name: "White", role: "white", default: 0, ranges: [] },
        { name: "Strobe", role: "strobe", default: 0, ranges: [] },
        { name: "Dimmer", role: "intensity", default: 255, ranges: [] },
      ],
    },
    // biome-ignore lint/suspicious/noExplicitAny: ts-rs binding shape varies; the runtime only reads channels[].name during render
  ] as any,
};

const movingHeads: FixtureInstance[] = [
  {
    id: "fx-mh-1",
    definition_id: MOVING_HEAD.id,
    mode_index: 0,
    universe: 2,
    address: 1,
    label: "MH 1",
    position: [80, 440],
  },
  {
    id: "fx-mh-2",
    definition_id: MOVING_HEAD.id,
    mode_index: 0,
    universe: 2,
    address: 9,
    label: "MH 2",
    position: [240, 440],
  },
  {
    id: "fx-mh-3",
    definition_id: MOVING_HEAD.id,
    mode_index: 0,
    universe: 2,
    address: 17,
    label: "MH 3",
    position: [400, 440],
  },
  {
    id: "fx-mh-4",
    definition_id: MOVING_HEAD.id,
    mode_index: 0,
    universe: 2,
    address: 25,
    label: "MH 4",
    position: [560, 440],
  },
];

const figureEight: MovementGenerator = {
  id: "mv-figure-eight",
  name: "Figura ocho",
  enabled: true,
  fixtures: movingHeads.map((fx) => ({
    fixture_id: fx.id,
    phase_offset: 0,
    invert_pan: false,
    invert_tilt: false,
  })),
  shape: { type: "figure_eight" },
  size_x: 0.7,
  size_y: 0.5,
  center_x: 0,
  center_y: 0.1,
  rotation_deg: 0,
  spread_mode: "even",
  tempo: { type: "fixed", bpm: 110 },
  subdivision: "two",
  direction: "forward",
};

const slowCircle: MovementGenerator = {
  id: "mv-slow-circle",
  name: "Slow circle",
  enabled: false,
  fixtures: movingHeads.slice(0, 2).map((fx) => ({
    fixture_id: fx.id,
    phase_offset: 0,
    invert_pan: false,
    invert_tilt: false,
  })),
  shape: { type: "circle" },
  size_x: 0.5,
  size_y: 0.5,
  center_x: 0,
  center_y: 0,
  rotation_deg: 0,
  spread_mode: "even",
  tempo: { type: "fixed", bpm: 60 },
  subdivision: "four",
  direction: "forward",
};

export const DEMO_SHOW_WITH_MOVEMENTS: ShowFileV1 = {
  ...DEMO_SHOW,
  fixtures: [...DEMO_SHOW.fixtures, ...movingHeads],
  library: [...DEMO_LIBRARY, MOVING_HEAD],
  movements: [figureEight, slowCircle],
};

export const DEMO_LIBRARY_WITH_MOVING: FixtureDefinition[] = [
  ...DEMO_LIBRARY,
  MOVING_HEAD,
];
