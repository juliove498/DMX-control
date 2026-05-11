// Tour of the Chasers view: list of chasers with one enabled (border
// highlighted) and one off, header with the two creation buttons.
// Marker rects only — prose explains the live-border, the toggle, the
// add buttons.

import type { SceneSpec } from "../lib/capture.ts";
import { DEMO_LIBRARY } from "../mocks/demoShow.ts";
import { DEMO_SHOW_WITH_CHASERS } from "../mocks/demoShowWithChasers.ts";

const scene: SceneSpec = {
  id: "chasers-overview",
  tab: "chaser",
  mock: { show: DEMO_SHOW_WITH_CHASERS, library: DEMO_LIBRARY },
  annotations: [
    { type: "rect", target: '[data-doc="chaser-actions"]', number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="chaser-card"][data-doc-on="true"]', number: 2, padding: 4 },
    { type: "circle", target: '[data-doc="chaser-card"][data-doc-on="true"] [data-doc="chaser-toggle"]', number: 3, padding: 4 },
  ],
  caption: { es: "Vista general de Chasers" },
};

export default scene;
