// Close-up on the parameter fieldsets that define the look of a
// movement: shape selector, canon (spread + direction) and timing
// (BPM + subdivision). Together those three blocks cover most of what
// an operator tunes day-to-day.

import type { SceneSpec } from "../lib/capture.ts";
import {
  DEMO_LIBRARY_WITH_MOVING,
  DEMO_SHOW_WITH_MOVEMENTS,
} from "../mocks/demoShowWithMovements.ts";

const scene: SceneSpec = {
  id: "movements-shape",
  tab: "movement",
  mock: { show: DEMO_SHOW_WITH_MOVEMENTS, library: DEMO_LIBRARY_WITH_MOVING },
  annotations: [
    { type: "rect", target: '[data-doc="movement-shape"]', number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="movement-canon"]', number: 2, padding: 4 },
    { type: "rect", target: '[data-doc="movement-timing"]', number: 3, padding: 4 },
  ],
  caption: { es: "Parámetros de un movement: shape, canon y timing" },
};

export default scene;
