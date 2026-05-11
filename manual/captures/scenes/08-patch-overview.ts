// Patch view inside Config. The doc bridge ships a clean PatchReport
// (no conflicts) so the status badge reads OK and the issues panel is
// hidden — kept here as the "happy path" reference shot.

import type { SceneSpec } from "../lib/capture.ts";
import { DEMO_LIBRARY, DEMO_SHOW } from "../mocks/demoShow.ts";

const scene: SceneSpec = {
  id: "patch-overview",
  tab: "config",
  mock: {
    show: DEMO_SHOW,
    library: DEMO_LIBRARY,
    patch: { conflicts: [], problems: [] },
  },
  interactions: [
    { type: "click", selector: '[data-doc-config-tab="patch"]' },
    { type: "wait", ms: 200 },
  ],
  annotations: [
    { type: "rect", target: '[data-doc="patch-add-form"]', number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="patch-status"]', number: 2, padding: 4 },
    { type: "rect", target: '[data-doc="patch-table"]', number: 3, padding: 4 },
  ],
  caption: { es: "Vista general de Patch" },
};

export default scene;
