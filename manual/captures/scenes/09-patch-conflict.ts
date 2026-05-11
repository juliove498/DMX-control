// Patch view with a deliberate overlap: fx-conflict at U1/3 collides
// with fx-1 (1..4) and fx-2 (5..8). Two PatchConflict entries surface
// the warn status, mark the offending rows red and render the issues
// panel under the table.

import type { SceneSpec } from "../lib/capture.ts";
import { DEMO_LIBRARY } from "../mocks/demoShow.ts";
import {
  DEMO_PATCH_CONFLICT_REPORT,
  DEMO_SHOW_WITH_PATCH_CONFLICT,
} from "../mocks/demoShowWithPatchConflict.ts";

const scene: SceneSpec = {
  id: "patch-conflict",
  tab: "config",
  mock: {
    show: DEMO_SHOW_WITH_PATCH_CONFLICT,
    library: DEMO_LIBRARY,
    patch: DEMO_PATCH_CONFLICT_REPORT,
  },
  interactions: [
    { type: "click", selector: '[data-doc-config-tab="patch"]' },
    { type: "wait", ms: 200 },
  ],
  annotations: [
    { type: "rect", target: '[data-doc="patch-status"]', number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="patch-issues"]', number: 2, padding: 4 },
  ],
  caption: { es: "Patch con conflictos de direcciones" },
};

export default scene;
