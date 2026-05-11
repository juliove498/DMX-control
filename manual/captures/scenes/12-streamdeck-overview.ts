// Stream Deck config tab inside Config. Mock a connected XL device so
// the page shows the intro hint, the device list with the Disconnect
// action and the bottom status section. Uses DEMO_SHOW_WITH_CHASERS so
// the per-row chaser/movement/scene mapping referenced in the prose
// has data to point at if a future capture wants to focus on that.

import type { SceneSpec } from "../lib/capture.ts";
import { DEMO_LIBRARY } from "../mocks/demoShow.ts";
import { DEMO_SHOW_WITH_CHASERS } from "../mocks/demoShowWithChasers.ts";

const scene: SceneSpec = {
  id: "streamdeck-overview",
  tab: "config",
  mock: {
    show: DEMO_SHOW_WITH_CHASERS,
    library: DEMO_LIBRARY,
    streamDeckDevices: [
      { serial: "BL12K3A04321", kind: "Stream Deck XL", key_count: 32 },
    ],
    streamDeckStatus: { connected: "BL12K3A04321", kind: "Stream Deck XL", key_count: 32 },
  },
  interactions: [
    { type: "click", selector: '[data-doc-config-tab="streamdeck"]' },
    { type: "wait", ms: 200 },
  ],
  annotations: [
    { type: "rect", target: '[data-doc="streamdeck-intro"]', number: 1, padding: 4 },
    { type: "rect", target: '[data-doc="streamdeck-devices"]', number: 2, padding: 4 },
    { type: "rect", target: '[data-doc="streamdeck-status"]', number: 3, padding: 4 },
  ],
  caption: { es: "Stream Deck conectado en Config" },
};

export default scene;
