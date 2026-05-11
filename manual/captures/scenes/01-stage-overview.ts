// First documentation capture: high-level tour of the Stage view.
// Annotates the four anchors a new operator needs to find within the
// first 30 seconds of opening the app — the tabs nav, the global
// transport (BPM/Blackout/Blind), and the stage canvas itself.

import type { SceneSpec } from "../lib/capture.ts";
import { DEMO_SHOW, DEMO_LIBRARY } from "../mocks/demoShow.ts";

const scene: SceneSpec = {
  id: "stage-overview",
  tab: "stage",
  mock: { show: DEMO_SHOW, library: DEMO_LIBRARY },
  annotations: [
    { type: "rect", target: ".tabs .tabs-left", number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="globals"]', number: 2, padding: 4 },
    { type: "rect", target: ".tab-body", number: 3, padding: 4 },
    { type: "callout", target: '[data-doc="blackout"]', text: "Blackout: corta toda la luz al instante", number: 4, placement: "bottom" },
  ],
  caption: { es: "Vista principal del Stage", en: "Main Stage view" },
};

export default scene;
