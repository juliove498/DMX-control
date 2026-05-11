// One chaser card expanded so the Pattern/Timing/Slots tabs are
// visible. The capture clicks the Edit button on the second card
// (the rainbow one) so the live-border highlight on the first card
// stays as a separate visual cue.

import type { SceneSpec } from "../lib/capture.ts";
import { DEMO_LIBRARY } from "../mocks/demoShow.ts";
import { DEMO_SHOW_WITH_CHASERS, CHASER_IDS } from "../mocks/demoShowWithChasers.ts";

const scene: SceneSpec = {
  id: "chasers-card-anatomy",
  tab: "chaser",
  mock: { show: DEMO_SHOW_WITH_CHASERS, library: DEMO_LIBRARY },
  interactions: [
    // Click the Edit button on the rainbow chaser — its row is the
    // second `chaser-row`, and Edit is the second-to-last button on
    // that row (before Delete). Easier to target by aria/text via
    // Playwright's selector engine.
    { type: "click", selector: `[data-doc="chaser-card"]:nth-of-type(2) [data-doc="chaser-edit-toggle"]` },
    { type: "wait", ms: 250 },
  ],
  annotations: [
    { type: "rect", target: '[data-doc="chaser-card"]:nth-of-type(2) [data-doc="chaser-row"]', number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="chaser-tabs"]', number: 2, padding: 4 },
    { type: "rect", target: '[data-doc="chaser-edit"] .chaser-tab-body', number: 3, padding: 4 },
  ],
  caption: { es: "Anatomía de un chaser expandido" },
};

export default scene;
