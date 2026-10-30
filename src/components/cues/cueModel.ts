import type { ButtonAction } from "@bindings/ButtonAction";
import type { TrackCue } from "@bindings/TrackCue";
import type { TrackCueSet } from "@bindings/TrackCueSet";
import type { TrackGrid } from "@bindings/TrackGrid";
import type { TrackSection } from "@bindings/TrackSection";
import { newId } from "../../lib/compat";

// Pure helpers for the cue editor: time/beat conversion against the
// track's grid, snapping, cue construction and the undo history.

export const NUDGE_FINE = 0.01;
export const NUDGE_MED = 0.1;
export const NUDGE_COARSE = 1;
const HISTORY_CAP = 100;

export function beatAt(grid: TrackGrid | null, secs: number): number {
  if (!grid || grid.bpm <= 0) return 0;
  return ((secs - grid.first_beat_secs) * grid.bpm) / 60;
}

export function secsAt(grid: TrackGrid | null, beat: number): number {
  if (!grid || grid.bpm <= 0) return 0;
  return grid.first_beat_secs + (beat * 60) / grid.bpm;
}

/** Nearest grid line at `division` beats (1 = beat, 0.5 = eighth). */
export function snapToGrid(grid: TrackGrid | null, secs: number, division = 1): number {
  if (!grid || grid.bpm <= 0) return secs;
  const b = beatAt(grid, secs);
  return secsAt(grid, Math.round(b / division) * division);
}

/** "bar.beat" (1-based, 4/4) for ruler labels and readouts. */
export function fmtBarBeat(beat: number): string {
  const bar = Math.floor(beat / 4);
  const inBar = beat - bar * 4;
  return `${bar + 1}.${Math.floor(inBar) + 1}`;
}

export function withPos(cue: TrackCue, grid: TrackGrid | null, secs: number): TrackCue {
  const pos = Math.max(0.001, secs);
  return { ...cue, pos_secs: pos, beat: grid ? beatAt(grid, pos) : cue.beat };
}

export function newCue(
  secs: number,
  grid: TrackGrid | null,
  action: ButtonAction,
  name = "",
): TrackCue {
  const pos = Math.max(0.001, secs);
  return {
    id: newId(),
    beat: grid ? beatAt(grid, pos) : 0,
    pos_secs: pos,
    name,
    action,
    color: null,
  };
}

export function newSection(secs: number, name: string): TrackSection {
  return { id: newId(), pos_secs: Math.max(0, secs), name };
}

export function sortCues(cues: TrackCue[]): TrackCue[] {
  return [...cues].sort((a, b) => a.pos_secs - b.pos_secs);
}

export function emptySet(filePath: string, title: string): TrackCueSet {
  return {
    file_path: filePath,
    title,
    cues: [],
    grid: null,
    sections: [],
    duration_secs: 0,
    owned: [],
  };
}

export function isOwnedBy(set: TrackCueSet, kind: string, id: string): boolean {
  return (set.owned ?? []).some((o) => o.kind === kind && o.id === id);
}

/** Title of the (other) track that owns this entity, if any. */
export function ownerTitle(sets: TrackCueSet[], kind: string, id: string): string | null {
  const owner = sets.find((s) => isOwnedBy(s, kind, id));
  return owner ? owner.title || basename(owner.file_path) : null;
}

/** "Apertura · E3" style names for entities created from a cue. */
export function nextOwnedName(set: TrackCueSet, kind: string): string {
  const prefix = { scene: "E", snapshot: "S", loop: "L", chaser: "C", movement: "M" }[kind] ?? "X";
  const n = (set.owned ?? []).filter((o) => o.kind === kind).length + 1;
  return `${set.title || basename(set.file_path)} · ${prefix}${n}`;
}

export function cloneSet(set: TrackCueSet): TrackCueSet {
  return JSON.parse(JSON.stringify(set)) as TrackCueSet;
}

export function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export interface History {
  past: TrackCueSet[];
  future: TrackCueSet[];
}

export const EMPTY_HISTORY: History = { past: [], future: [] };

export function pushHistory(h: History, snapshot: TrackCueSet): History {
  const past = [...h.past, cloneSet(snapshot)];
  if (past.length > HISTORY_CAP) past.shift();
  return { past, future: [] };
}

/** Cues in (from, to] — the crossing test for preview firing. */
export function crossedCues(cues: TrackCue[], from: number, to: number): TrackCue[] {
  if (to <= from) return [];
  return cues.filter((c) => c.pos_secs > from && c.pos_secs <= to);
}
