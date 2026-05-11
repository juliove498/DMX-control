// Detail view of the right pane: editor head (name, GO, AI, delete),
// the steps list with fade/hold per step, and the "+ Add step" footer.
// Selects the multi-step Azul scene so the steps list has enough rows
// to read the layout cleanly.

import type { SceneSpec } from "../lib/capture.ts";
import { DEMO_LIBRARY } from "../mocks/demoShow.ts";
import { DEMO_SHOW_WITH_SCENES } from "../mocks/demoShowWithScenes.ts";

const scene: SceneSpec = {
  id: "scenes-editor-detail",
  tab: "scenes",
  mock: {
    show: DEMO_SHOW_WITH_SCENES,
    library: DEMO_LIBRARY,
  },
  interactions: [
    // Pick the multi-step scene so the editor has more than one row.
    { type: "click", selector: `[data-doc="scene-list-item"]:has(.scenes-list-name:text-is("Azul ambient")) .scenes-list-body` },
    { type: "wait", ms: 250 },
  ],
  annotations: [
    { type: "rect", target: '[data-doc="scene-editor-head"]', number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="scene-steps-wrap"]', number: 2, padding: 4 },
    { type: "rect", target: '[data-doc="scene-add-step"]', number: 3, padding: 4 },
  ],
  caption: { es: "Anatomía del editor de escena" },
};

export default scene;
