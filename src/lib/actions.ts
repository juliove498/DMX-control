import type { ButtonAction } from "@bindings/ButtonAction";
import type { ShowFileV1 } from "@bindings/ShowFileV1";

// Shared vocabulary for every "pick an action" UI (OS2L mappings, cue
// editor): the kinds an operator thinks in, how they map to ButtonAction
// variants, and how to label/colour them consistently.

export const ACTION_KINDS = [
  "scene",
  "loop",
  "loop_stop",
  "chaser",
  "movement",
  "snapshot",
  "blackout",
  "blind",
  "tap",
  "autopilot",
  "energy_low",
  "energy_mid",
  "energy_high",
  "magic_next",
  "magic_back",
  "magic_hold",
  "magic_burst",
  "magic_drop",
  "magic_movement",
  "magic_palette_auto",
  "magic_palette_warm",
  "magic_palette_cold",
  "magic_palette_rainbow",
  "magic_palette_mono",
] as const;
export type ActionKind = (typeof ACTION_KINDS)[number];

export function kindOfAction(a: ButtonAction): ActionKind | null {
  switch (a.type) {
    case "recall_scene":
    case "recall_scene_by_index":
      return "scene";
    case "start_loop_group":
    case "start_loop_group_by_index":
      return "loop";
    case "stop_loop_group":
      return "loop_stop";
    case "toggle_chaser":
    case "toggle_chaser_by_index":
      return "chaser";
    case "toggle_movement":
    case "toggle_movement_by_index":
      return "movement";
    case "toggle_snapshot":
    case "toggle_snapshot_by_index":
      return "snapshot";
    case "blackout":
      return "blackout";
    case "blind":
      return "blind";
    case "tap":
      return "tap";
    case "toggle_autopilot":
      return "autopilot";
    case "autopilot_energy":
      return a.level <= 0.4 ? "energy_low" : a.level <= 0.7 ? "energy_mid" : "energy_high";
    case "autopilot_nudge":
      return a.kind === "next" ? "magic_next" : "magic_back";
    case "autopilot_hold":
      return "magic_hold";
    case "autopilot_burst":
      return "magic_burst";
    case "autopilot_drop":
      return "magic_drop";
    case "autopilot_movement":
      return "magic_movement";
    case "autopilot_palette":
      return `magic_palette_${a.style}` as ActionKind;
    default:
      return null;
  }
}

export function actionId(a: ButtonAction): string | null {
  return "id" in a ? a.id : null;
}

/** True for `none` and for target-bearing kinds whose target was never picked. */
export function isUnassigned(a: ButtonAction): boolean {
  if (a.type === "none") return true;
  const kind = kindOfAction(a);
  if (kind && kindNeedsTarget(kind)) return "id" in a && !a.id;
  return false;
}

export function kindNeedsTarget(kind: ActionKind): boolean {
  return (
    kind === "scene" ||
    kind === "loop" ||
    kind === "chaser" ||
    kind === "movement" ||
    kind === "snapshot"
  );
}

export function buildAction(kind: ActionKind, id: string | null): ButtonAction {
  switch (kind) {
    case "scene":
      return { type: "recall_scene", id: id ?? "" };
    case "loop":
      return { type: "start_loop_group", id: id ?? "" };
    case "loop_stop":
      return { type: "stop_loop_group" };
    case "chaser":
      return { type: "toggle_chaser", id: id ?? "" };
    case "movement":
      return { type: "toggle_movement", id: id ?? "" };
    case "snapshot":
      return { type: "toggle_snapshot", id: id ?? "" };
    case "blackout":
      return { type: "blackout" };
    case "blind":
      return { type: "blind" };
    case "tap":
      return { type: "tap" };
    case "autopilot":
      return { type: "toggle_autopilot" };
    case "energy_low":
      return { type: "autopilot_energy", level: 0.25 };
    case "energy_mid":
      return { type: "autopilot_energy", level: 0.55 };
    case "energy_high":
      return { type: "autopilot_energy", level: 0.85 };
    case "magic_next":
      return { type: "autopilot_nudge", kind: "next" };
    case "magic_back":
      return { type: "autopilot_nudge", kind: "back" };
    case "magic_hold":
      return { type: "autopilot_hold" };
    case "magic_burst":
      return { type: "autopilot_burst" };
    case "magic_drop":
      return { type: "autopilot_drop" };
    case "magic_movement":
      return { type: "autopilot_movement" };
    case "magic_palette_auto":
      return { type: "autopilot_palette", style: "auto" };
    case "magic_palette_warm":
      return { type: "autopilot_palette", style: "warm" };
    case "magic_palette_cold":
      return { type: "autopilot_palette", style: "cold" };
    case "magic_palette_rainbow":
      return { type: "autopilot_palette", style: "rainbow" };
    case "magic_palette_mono":
      return { type: "autopilot_palette", style: "mono" };
  }
}

/** Selectable targets for a kind, from the current show. */
export function itemsForKind(
  show: ShowFileV1 | null,
  kind: ActionKind,
): { id: string; name: string }[] {
  if (!show) return [];
  switch (kind) {
    case "scene":
      return show.scenes.map((s) => ({ id: s.id, name: s.name }));
    case "loop":
      return show.scene_loop_groups.map((l) => ({ id: l.id, name: l.name }));
    case "chaser":
      return show.chasers.map((c) => ({ id: c.id, name: c.name }));
    case "movement":
      return show.movements.map((m) => ({ id: m.id, name: m.name }));
    case "snapshot":
      return show.snapshots.map((s) => ({ id: s.id, name: s.name }));
    default:
      return [];
  }
}

/** Target's current name (ids survive renames; labels shouldn't go stale). */
export function targetName(show: ShowFileV1 | null, a: ButtonAction): string | null {
  const kind = kindOfAction(a);
  if (!kind || !kindNeedsTarget(kind)) return null;
  if ("index" in a) return `#${a.index + 1}`;
  const id = actionId(a);
  if (!id) return null;
  return itemsForKind(show, kind).find((i) => i.id === id)?.name ?? `(${id.slice(0, 8)})`;
}

/**
 * Marker colour per kind. Deliberately a fixed palette (not theme tokens):
 * these are data colours that must stay recognisable across themes, and
 * the operator can override any cue with its own colour.
 */
export function kindColor(kind: ActionKind | null): string {
  switch (kind) {
    case "scene":
      return "#4ade80";
    case "loop":
      return "#60a5fa";
    case "loop_stop":
      return "#93c5fd";
    case "chaser":
      return "#fb923c";
    case "movement":
      return "#c084fc";
    case "snapshot":
      return "#facc15";
    case "blackout":
      return "#f87171";
    case "blind":
      return "#f5f5f5";
    case "tap":
      return "#a3a3a3";
    case "autopilot":
      return "#f472b6";
    case "energy_low":
      return "#fde68a";
    case "energy_mid":
      return "#fbbf24";
    case "energy_high":
      return "#f59e0b";
    case "magic_next":
    case "magic_back":
    case "magic_hold":
    case "magic_burst":
    case "magic_drop":
    case "magic_movement":
    case "magic_palette_auto":
    case "magic_palette_warm":
    case "magic_palette_cold":
    case "magic_palette_rainbow":
    case "magic_palette_mono":
      return "#e879f9";
    default:
      return "#9ca3af";
  }
}

/** m:ss.mmm (or m:ss.d when `fine` is false). */
export function fmtTime(secs: number, fine = true): string {
  if (!Number.isFinite(secs)) return "—";
  const s = Math.max(0, secs);
  const m = Math.floor(s / 60);
  const rest = s - m * 60;
  if (fine) return `${m}:${rest.toFixed(3).padStart(6, "0")}`;
  return `${m}:${rest.toFixed(1).padStart(4, "0")}`;
}

/** Parses "m:ss.mmm", "ss.mmm" or plain seconds. null when unparseable. */
export function parseTime(text: string): number | null {
  const s = text.trim().replace(",", ".");
  if (s === "") return null;
  const parts = s.split(":");
  if (parts.length > 2) return null;
  const nums = parts.map((p) => Number(p));
  if (nums.some((n) => !Number.isFinite(n) || n < 0)) return null;
  return parts.length === 2 ? nums[0] * 60 + nums[1] : nums[0];
}
