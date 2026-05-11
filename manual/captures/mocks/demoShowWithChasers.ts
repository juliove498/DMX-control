// Show enriched with two ambient chasers — one enabled (so the
// "live" border-left highlight shows up), one off. Built on top of
// DEMO_SHOW_WITH_SCENES so captures of the chaser view also have
// scenes available, in case a multi-tab walkthrough wants to switch
// between them without re-hydrating.

import type { ShowFileV1 } from "@bindings/ShowFileV1";
import type { AmbientChaser } from "@bindings/AmbientChaser";
import { DEMO_SHOW_WITH_SCENES } from "./demoShowWithScenes.ts";
import { DEMO_SHOW } from "./demoShow.ts";

const allFixtureSlots = DEMO_SHOW.fixtures.map((fx) => ({
  fixture_id: fx.id,
  use_intensity: true,
  use_color: true,
}));

const cadence: AmbientChaser = {
  id: "chaser-cadence",
  name: "Rojo / Azul cadence",
  enabled: true,
  slots: allFixtureSlots,
  pattern: { type: "chase" },
  color_mode: {
    type: "two_color_cadence",
    color_a: { r: 255, g: 0, b: 0 },
    color_b: { r: 0, g: 80, b: 255 },
    cadence: { type: "every_step" },
  },
  tempo: { type: "fixed", bpm: 120 },
  subdivision: "one",
  master: 1,
  background: 0.1,
  fade: { enabled: true, amount: 0.5, curve: "ease_in_out" },
};

const rainbow: AmbientChaser = {
  id: "chaser-rainbow",
  name: "Arcoíris suave",
  enabled: false,
  slots: allFixtureSlots,
  pattern: { type: "wave" },
  color_mode: { type: "rainbow", speed: 30, spread: 0.6 },
  tempo: { type: "fixed", bpm: 90 },
  subdivision: "half",
  master: 0.8,
  background: 0,
  fade: { enabled: true, amount: 0.7, curve: "linear" },
};

export const DEMO_SHOW_WITH_CHASERS: ShowFileV1 = {
  ...DEMO_SHOW_WITH_SCENES,
  chasers: [cadence, rainbow],
};

export const CHASER_IDS = {
  cadence: "chaser-cadence",
  rainbow: "chaser-rainbow",
} as const;
