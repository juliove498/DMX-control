// Movement view with one figure-eight enabled across four moving heads
// (so the SVG preview has dots to render) and a second movement off in
// the sidebar. Annotations highlight the list, the parameters column
// and the live preview.

import type { SceneSpec } from "../lib/capture.ts";
import {
  DEMO_LIBRARY_WITH_MOVING,
  DEMO_SHOW_WITH_MOVEMENTS,
} from "../mocks/demoShowWithMovements.ts";

const scene: SceneSpec = {
  id: "movements-overview",
  tab: "movement",
  mock: { show: DEMO_SHOW_WITH_MOVEMENTS, library: DEMO_LIBRARY_WITH_MOVING },
  annotations: [
    { type: "rect", target: '[data-doc="movement-list"]', number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="movement-params"]', number: 2, padding: 4 },
    { type: "rect", target: '[data-doc="movement-preview"]', number: 3, padding: 4 },
  ],
  caption: { es: "Vista general de Movements" },
};

export default scene;
