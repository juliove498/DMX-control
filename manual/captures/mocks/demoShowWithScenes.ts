// Demo show enriched with a small library of scenes for the Escenas
// chapter. Built on top of DEMO_SHOW so the rig (6 PARs) stays
// consistent across captures — only the scenes/programmer state
// differ. Three scenes keep the list visually digestible while
// covering the layout cases we want to teach (multi-step, single
// step, freshly created).

import type { ShowFileV1 } from "@bindings/ShowFileV1";
import type { Scene } from "@bindings/Scene";
import { DEMO_SHOW } from "./demoShow.ts";

const fxInherit = { type: "inherit" } as const;

// Helper: a step that paints every fixture with the same flat colour
// so the captures look coherent without us crafting per-channel data
// for each scene by hand.
const flatColourStep = (id: string, name: string, fadeMs: number, holdMs: number, rgbw: [number, number, number, number]) => ({
  id,
  name,
  fade_in_ms: fadeMs,
  hold_ms: holdMs,
  fixtures: DEMO_SHOW.fixtures.map((fx) => ({
    fixture_id: fx.id,
    values: rgbw.map((value, i) => ({ channel_offset: i, value })),
  })),
  chaser_state: fxInherit,
  movement_state: fxInherit,
});

const SCENES: Scene[] = [
  {
    id: "scene-rojo-profundo",
    name: "Rojo profundo",
    steps: [flatColourStep("step-1", null as unknown as string, 800, 0, [255, 0, 0, 0])],
    chaser_state: fxInherit,
    movement_state: fxInherit,
    fade_in_ms: 800,
    fixtures: [],
  },
  {
    id: "scene-azul-ambient",
    name: "Azul ambient",
    steps: [
      flatColourStep("step-azul-1", "Entrada", 1500, 2000, [0, 40, 200, 20]),
      flatColourStep("step-azul-2", "Pico", 800, 3000, [0, 80, 255, 60]),
      flatColourStep("step-azul-3", "Bajada", 1200, 1500, [0, 20, 120, 0]),
    ],
    chaser_state: fxInherit,
    movement_state: fxInherit,
    fade_in_ms: 1500,
    fixtures: [],
  },
  {
    id: "scene-strobe-blanco",
    name: "Strobe blanco",
    steps: [flatColourStep("step-strobe", null as unknown as string, 0, 200, [255, 255, 255, 255])],
    chaser_state: fxInherit,
    movement_state: fxInherit,
    fade_in_ms: 0,
    fixtures: [],
  },
  {
    id: "scene-cierre-warm",
    name: "Cierre cálido",
    steps: [flatColourStep("step-warm", null as unknown as string, 3000, 0, [180, 60, 0, 200])],
    chaser_state: fxInherit,
    movement_state: fxInherit,
    fade_in_ms: 3000,
    fixtures: [],
  },
];

export const DEMO_SHOW_WITH_SCENES: ShowFileV1 = {
  ...DEMO_SHOW,
  scenes: SCENES,
};

// Convenience exports so individual capture scenes can pick a specific
// scene id when they want to demonstrate "this scene is live".
export const SCENE_IDS = {
  rojo: "scene-rojo-profundo",
  azul: "scene-azul-ambient",
  strobe: "scene-strobe-blanco",
  warm: "scene-cierre-warm",
} as const;
