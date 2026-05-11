// Show that deliberately collides two fixtures on the same universe so
// the Patch view's conflict panel and the warn status badge both light
// up. fx-conflict starts at universe 1, address 3 — that overlaps the
// last two channels of fx-1 (1..4) and the first two of fx-2 (5..8).
//
// The doc bridge intercepts validate_patch_cmd, so we ship the
// pre-computed report alongside the show; the engine isn't running in
// doc mode and wouldn't recompute it on its own.

import type { ShowFileV1 } from "@bindings/ShowFileV1";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import type { PatchReport } from "@bindings/PatchReport";
import { DEMO_LIBRARY, DEMO_SHOW } from "./demoShow.ts";

const PAR_DEF_ID = DEMO_LIBRARY[0].id;

const conflictFixture: FixtureInstance = {
  id: "fx-conflict",
  definition_id: PAR_DEF_ID,
  mode_index: 0,
  universe: 1,
  address: 3,
  label: "PAR Misplaced",
  position: [560, 120],
};

export const DEMO_SHOW_WITH_PATCH_CONFLICT: ShowFileV1 = {
  ...DEMO_SHOW,
  fixtures: [...DEMO_SHOW.fixtures, conflictFixture],
};

export const DEMO_PATCH_CONFLICT_REPORT: PatchReport = {
  conflicts: [
    {
      fixture_a: "fx-1",
      fixture_b: "fx-conflict",
      universe: 1,
      overlap_start: 3,
      overlap_len: 2,
    },
    {
      fixture_a: "fx-conflict",
      fixture_b: "fx-2",
      universe: 1,
      overlap_start: 5,
      overlap_len: 2,
    },
  ],
  problems: [],
};
