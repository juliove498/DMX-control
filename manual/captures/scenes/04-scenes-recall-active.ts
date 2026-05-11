// Same Escenas view, but with one scene live and a multi-step scene
// selected so the operator can see what playback looks like: the
// active footer shows which scene is on, which step is playing, and
// the RELEASE button.
//
// Annotations: just two marker rects — one on the active scene row,
// one on the footer that appears when a scene is live. The MDX prose
// explains what each region tells you.

import type { SceneSpec } from "../lib/capture.ts";
import { DEMO_LIBRARY } from "../mocks/demoShow.ts";
import { DEMO_SHOW_WITH_SCENES, SCENE_IDS } from "../mocks/demoShowWithScenes.ts";

const scene: SceneSpec = {
  id: "scenes-recall-active",
  tab: "scenes",
  mock: {
    show: DEMO_SHOW_WITH_SCENES,
    library: DEMO_LIBRARY,
    activeSceneId: SCENE_IDS.azul,
    activeSceneStep: 1,
  },
  // Click the active scene's row so the editor on the right shows its
  // steps — the active footer + the editor's GO button highlighting
  // tell the same story from two angles.
  interactions: [
    { type: "click", selector: '[data-doc="scene-list-item"][data-doc-active="true"] .scenes-list-body' },
    { type: "wait", ms: 250 },
  ],
  annotations: [
    { type: "rect", target: '[data-doc="scene-list-item"][data-doc-active="true"]', number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="scenes-active-footer"]', number: 2, padding: 4 },
  ],
  caption: { es: "Una escena recallada en vivo" },
};

export default scene;
