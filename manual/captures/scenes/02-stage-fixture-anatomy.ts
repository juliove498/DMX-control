// Detail capture: anatomy of a single fixture card on the Stage.
// Exercises the `circle` and `arrow` annotation primitives so we have
// at least one example of every type in the manual's source. The
// callout types are already covered by 01-stage-overview.

import type { SceneSpec } from "../lib/capture.ts";
import { DEMO_SHOW, DEMO_LIBRARY } from "../mocks/demoShow.ts";

const scene: SceneSpec = {
  id: "stage-fixture-anatomy",
  tab: "stage",
  mock: { show: DEMO_SHOW, library: DEMO_LIBRARY },
  annotations: [
    // Circle the centre fixture so it pops against the dark stage.
    { type: "circle", target: '.stage-fixture[title^="PAR L2"]', number: 1, padding: 18 },
    // Arrow from that fixture to the right-side info pane, which
    // explains where the encoder editor will appear once selected.
    {
      type: "arrow",
      from: '.stage-fixture[title^="PAR L2"]',
      to: ".tab-body > :last-child",
      number: 2,
    },
    {
      type: "callout",
      target: '.stage-fixture[title^="PAR L2"]',
      text: "Cada tarjeta = un fixture en escena · clic para seleccionar",
      placement: "bottom",
      number: 3,
    },
  ],
  caption: { es: "Anatomía de un fixture en el Stage", en: "Anatomy of a fixture on the Stage" },
};

export default scene;
