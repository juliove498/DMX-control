// Tour of the Escenas view: list of scenes on the left, editor on the
// right, programmer bar at the bottom (when something is touched).
// Annotation strategy: marker rects on the two big regions (panels)
// + short-text callouts on the actionable buttons. Long explanations
// live in the MDX prose, not in the image — the numbered badges are
// the only thing the reader needs to cross-reference.

import type { SceneSpec } from "../lib/capture.ts";
import { DEMO_LIBRARY } from "../mocks/demoShow.ts";
import { DEMO_SHOW_WITH_SCENES } from "../mocks/demoShowWithScenes.ts";

const scene: SceneSpec = {
  id: "scenes-overview",
  tab: "scenes",
  mock: {
    show: DEMO_SHOW_WITH_SCENES,
    library: DEMO_LIBRARY,
  },
  annotations: [
    { type: "rect", target: '[data-doc="scenes-list"]', number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="scenes-editor"]', number: 2, padding: 4 },
    // Inner markers: a rect on the New-scene button and a circle on
    // the first scene's GO. No text — the MDX prose covers the
    // semantics; we just need the reader to know where each numbered
    // bullet point lives in the UI.
    { type: "rect", target: '[data-doc="scenes-new"]', number: 3, padding: 2 },
    { type: "circle", target: '[data-doc="scene-go"]', number: 4, padding: 4 },
  ],
  caption: { es: "Vista general de Escenas" },
};

export default scene;
