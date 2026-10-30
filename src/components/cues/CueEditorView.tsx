import type { ButtonAction } from "@bindings/ButtonAction";
import type { CueFollowerStatus } from "@bindings/CueFollowerStatus";
import type { GridEstimate } from "@bindings/GridEstimate";
import type { PoiExportReport } from "@bindings/PoiExportReport";
import type { Scene } from "@bindings/Scene";
import type { Snapshot } from "@bindings/Snapshot";
import type { TrackCue } from "@bindings/TrackCue";
import type { TrackCueSet } from "@bindings/TrackCueSet";
import type { TrackGrid } from "@bindings/TrackGrid";
import type { WaveformData } from "@bindings/WaveformData";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask, open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../../i18n";
import {
  ACTION_KINDS,
  type ActionKind,
  actionId,
  buildAction,
  fmtTime,
  isUnassigned,
  itemsForKind,
  kindColor,
  kindNeedsTarget,
  kindOfAction,
  parseTime,
  targetName,
} from "../../lib/actions";
import { useShowStore } from "../../stores/show";
import { MAX_PX_PER_SEC, MIN_PX_PER_SEC, Timeline, type TimelineView } from "./Timeline";
import {
  EMPTY_HISTORY,
  type History,
  NUDGE_COARSE,
  NUDGE_FINE,
  NUDGE_MED,
  basename,
  beatAt,
  cloneSet,
  crossedCues,
  emptySet,
  fmtBarBeat,
  isOwnedBy,
  newCue,
  newSection,
  nextOwnedName,
  ownerTitle,
  pushHistory,
  snapToGrid,
  sortCues,
  withPos,
} from "./cueModel";
import { type TransportTick, useAudioTransport } from "./useAudioTransport";

// The cue editor ("planning mode"): sit down with the event organiser, play
// the track with the app's own transport, and author cues on a zoomable
// waveform — VirtualDJ optional. Firing at show time stays with the cue
// follower; here, playing with preview on fires the real actions so the
// lights react at the marked moment.

const WAVE_PPS = 400;
const AUDIO_EXTS = ["mp3", "m4a", "aac", "wav", "flac", "aif", "aiff", "ogg", "opus"];
const SAVE_DEBOUNCE_MS = 400;

/** Cross-track clipboard (times relative to the first copied cue). */
let clipboard: { rel: number; cue: TrackCue }[] = [];

function stringify(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e)
    return String((e as { message: unknown }).message);
  return String(e);
}

function isTypingTarget(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  const tag = el.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable;
}

export function CueEditorView() {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const sets = useMemo(() => show?.track_cues ?? [], [show]);

  const [activePath, setActivePath] = useState<string | null>(null);
  const [working, setWorkingState] = useState<TrackCueSet | null>(null);
  const workingRef = useRef<TrackCueSet | null>(null);
  const setWorking = useCallback((next: TrackCueSet | null) => {
    workingRef.current = next;
    setWorkingState(next);
  }, []);
  const [history, setHistory] = useState<History>(EMPTY_HISTORY);
  const historyRef = useRef(history);
  historyRef.current = history;
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const selectedRef = useRef(selected);
  selectedRef.current = selected;
  const [view, setView] = useState<TimelineView>({ start: 0, pxPerSec: 20 });
  const viewRef = useRef(view);
  viewRef.current = view;
  const widthRef = useRef(800);
  const [snap, setSnap] = useState(false);
  const [showGrid, setShowGrid] = useState(true);
  const [preview, setPreview] = useState(true);
  const [source, setSource] = useState<"audio" | "vdj">("audio");
  const [wave, setWave] = useState<WaveformData | null>(null);
  const [waveErr, setWaveErr] = useState<string | null>(null);
  const [localSrc, setLocalSrc] = useState<string | null>(null);
  const [follower, setFollower] = useState<CueFollowerStatus | null>(null);
  const followerRef = useRef<{ status: CueFollowerStatus | null; at: number }>({
    status: null,
    at: 0,
  });
  const vdjTimeRef = useRef(0);
  const [armKind, setArmKind] = useState<ActionKind>("scene");
  const [armTarget, setArmTarget] = useState("");
  const [toast, setToast] = useState<string | null>(null);
  const [flashing, setFlashing] = useState<ReadonlyMap<string, number>>(new Map());
  const [detecting, setDetecting] = useState(false);
  const [renamingPath, setRenamingPath] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const saveTimer = useRef<number | null>(null);
  const dragBase = useRef<{ snapshot: TrackCueSet; positions: Map<string, number> } | null>(null);

  // ---- toast ----
  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => setToast(null), 4000);
    return () => window.clearTimeout(id);
  }, [toast]);

  // ---- persistence ----
  const persistNow = useCallback((set: TrackCueSet) => {
    invoke("save_track_cue_set", { set }).catch((e) => setToast(stringify(e)));
  }, []);
  const scheduleSave = useCallback(
    (set: TrackCueSet) => {
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => {
        saveTimer.current = null;
        persistNow(set);
      }, SAVE_DEBOUNCE_MS);
    },
    [persistNow],
  );
  useEffect(
    () => () => {
      if (saveTimer.current && workingRef.current) {
        window.clearTimeout(saveTimer.current);
        persistNow(workingRef.current);
      }
    },
    [persistNow],
  );

  /** Apply an edit: undo snapshot + state + debounced save. */
  const commit = useCallback(
    (next: TrackCueSet, pushUndo = true, snapshot?: TrackCueSet) => {
      const base = snapshot ?? workingRef.current;
      if (pushUndo && base) setHistory((h) => pushHistory(h, base));
      setWorking(next);
      scheduleSave(next);
    },
    [scheduleSave, setWorking],
  );

  // ---- follower (VDJ) status ----
  useEffect(() => {
    let alive = true;
    invoke<CueFollowerStatus>("cue_follower_status")
      .then((s) => {
        if (!alive) return;
        setFollower(s);
        followerRef.current = { status: s, at: performance.now() };
      })
      .catch(() => {});
    const un = listen<CueFollowerStatus>("cues:status", (e) => {
      if (!alive) return;
      setFollower(e.payload);
      followerRef.current = { status: e.payload, at: performance.now() };
    });
    return () => {
      alive = false;
      un.then((f) => f());
    };
  }, []);

  // Tell the follower which track is open: while VDJ's HTTP server is
  // down it keeps following this song on VDJ's beats over OS2L.
  useEffect(() => {
    invoke("cue_follower_assume_file", { path: activePath }).catch(() => {});
    return () => {
      invoke("cue_follower_assume_file", { path: null }).catch(() => {});
    };
  }, [activePath]);

  // VDJ playhead interpolation (only meaningful while following VDJ).
  useEffect(() => {
    if (source !== "vdj") return;
    let raf = 0;
    const frame = () => {
      const { status, at } = followerRef.current;
      if (status?.pos_secs != null) {
        const elapsed = status.playing ? (performance.now() - at) / 1000 : 0;
        vdjTimeRef.current = status.pos_secs + elapsed;
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, [source]);

  // ---- own transport + preview firing ----
  const fire = useCallback((cue: TrackCue) => {
    invoke("preview_action", { action: cue.action }).catch(() => {});
    setFlashing((m) => {
      const next = new Map(m);
      next.set(cue.id, performance.now());
      return next;
    });
  }, []);
  const previewRef = useRef(preview);
  previewRef.current = preview;
  const onTick = useCallback(
    (tick: TransportTick) => {
      if (!previewRef.current || tick.discontinuous) return;
      const w = workingRef.current;
      if (!w) return;
      for (const c of crossedCues(w.cues, tick.prev, tick.time)) {
        if (!isUnassigned(c.action)) fire(c);
      }
    },
    [fire],
  );
  const transport = useAudioTransport(source === "audio" ? localSrc : null, onTick);
  const timeRef = source === "audio" ? transport.timeRef : vdjTimeRef;
  const now = useCallback(() => timeRef.current, [timeRef]);

  // Expire flashes.
  useEffect(() => {
    if (flashing.size === 0) return;
    const id = window.setTimeout(() => {
      const cutoff = performance.now() - 500;
      setFlashing((m) => {
        const next = new Map([...m].filter(([, at]) => at > cutoff));
        return next.size === m.size ? m : next;
      });
    }, 600);
    return () => window.clearTimeout(id);
  }, [flashing]);

  // ---- track loading ----
  const fitView = useCallback((duration: number) => {
    const w = widthRef.current;
    if (duration > 0 && w > 0) {
      setView({ start: 0, pxPerSec: Math.max(MIN_PX_PER_SEC, w / duration) });
    }
  }, []);

  const openTrack = useCallback(
    (path: string) => {
      if (saveTimer.current && workingRef.current) {
        window.clearTimeout(saveTimer.current);
        saveTimer.current = null;
        persistNow(workingRef.current);
      }
      const existing = sets.find((s) => s.file_path === path);
      const set = existing ? cloneSet(existing) : emptySet(path, basename(path));
      setActivePath(path);
      setWorking(set);
      setHistory(EMPTY_HISTORY);
      setSelected(new Set());
      setWave(null);
      setWaveErr(null);
      setLocalSrc(null);
      transport.setLoop({ a: null, b: null, on: false });
      if (set.duration_secs > 0) fitView(set.duration_secs);
      invoke<string>("resolve_music_path", { path })
        .then((local) => setLocalSrc(convertFileSrc(local)))
        .catch((e) => setWaveErr(stringify(e)));
      invoke<WaveformData>("waveform_for_file", { path, peaksPerSecond: WAVE_PPS })
        .then((w) => {
          setWave(w);
          fitView(w.duration_secs);
          const cur = workingRef.current;
          if (
            cur &&
            cur.file_path === path &&
            Math.abs(cur.duration_secs - w.duration_secs) > 0.5
          ) {
            const next = { ...cur, duration_secs: w.duration_secs };
            setWorking(next);
            persistNow(next);
          }
        })
        .catch((e) => setWaveErr(stringify(e)));
    },
    [sets, persistNow, setWorking, fitView, transport.setLoop],
  );

  const duration = wave?.duration_secs ?? working?.duration_secs ?? transport.duration ?? 0;
  const grid = working?.grid ?? null;
  const snapFn = useCallback(
    (secs: number) => (snap && grid ? snapToGrid(grid, secs) : secs),
    [snap, grid],
  );

  // ---- editing ops ----
  // A cue whose target was never picked stays UNASSIGNED (and fires
  // nothing) — defaulting to "scene 1" turned every forgotten pick into a
  // wrong look on stage.
  const armedAction = useCallback((): ButtonAction => {
    if (kindNeedsTarget(armKind)) return buildAction(armKind, armTarget || "");
    return buildAction(armKind, null);
  }, [armKind, armTarget]);

  const addCueAt = useCallback(
    (secs: number) => {
      const w = workingRef.current;
      if (!w) return;
      const cue = newCue(snapFn(secs), w.grid ?? null, armedAction());
      commit({ ...w, cues: sortCues([...w.cues, cue]) });
      setSelected(new Set([cue.id]));
    },
    [commit, snapFn, armedAction],
  );
  const markNow = useCallback(() => addCueAt(now()), [addCueAt, now]);

  const deleteSelected = useCallback(() => {
    const w = workingRef.current;
    const sel = selectedRef.current;
    if (!w || sel.size === 0) return;
    commit({ ...w, cues: w.cues.filter((c) => !sel.has(c.id)) });
    setSelected(new Set());
  }, [commit]);

  const moveSelected = useCallback(
    (delta: number, done: boolean) => {
      const w = workingRef.current;
      const sel = selectedRef.current;
      if (!w || sel.size === 0) return;
      if (!dragBase.current) {
        dragBase.current = {
          snapshot: cloneSet(w),
          positions: new Map(w.cues.map((c) => [c.id, c.pos_secs])),
        };
      }
      const base = dragBase.current;
      const next = {
        ...w,
        cues: sortCues(
          w.cues.map((c) =>
            sel.has(c.id)
              ? withPos(c, w.grid ?? null, snapFn((base.positions.get(c.id) ?? c.pos_secs) + delta))
              : c,
          ),
        ),
      };
      if (done) {
        commit(next, true, base.snapshot);
        dragBase.current = null;
      } else {
        setWorking(next);
      }
    },
    [commit, setWorking, snapFn],
  );

  const nudgeSelected = useCallback(
    (delta: number) => {
      const w = workingRef.current;
      const sel = selectedRef.current;
      if (!w || sel.size === 0) return;
      commit({
        ...w,
        cues: sortCues(
          w.cues.map((c) => (sel.has(c.id) ? withPos(c, w.grid ?? null, c.pos_secs + delta) : c)),
        ),
      });
    },
    [commit],
  );

  const updateCue = useCallback(
    (id: string, patch: Partial<TrackCue>) => {
      const w = workingRef.current;
      if (!w) return;
      commit({
        ...w,
        cues: sortCues(w.cues.map((c) => (c.id === id ? { ...c, ...patch } : c))),
      });
    },
    [commit],
  );

  const setCueTime = useCallback(
    (id: string, secs: number) => {
      const w = workingRef.current;
      if (!w) return;
      commit({
        ...w,
        cues: sortCues(w.cues.map((c) => (c.id === id ? withPos(c, w.grid ?? null, secs) : c))),
      });
    },
    [commit],
  );

  const duplicateSelected = useCallback(() => {
    const w = workingRef.current;
    const sel = selectedRef.current;
    if (!w || sel.size === 0) return;
    const copies = w.cues
      .filter((c) => sel.has(c.id))
      .map((c) => newCue(c.pos_secs + 0.5, w.grid ?? null, c.action, c.name));
    commit({ ...w, cues: sortCues([...w.cues, ...copies]) });
    setSelected(new Set(copies.map((c) => c.id)));
  }, [commit]);

  const copySelected = useCallback(() => {
    const w = workingRef.current;
    const sel = selectedRef.current;
    if (!w || sel.size === 0) return;
    const picked = sortCues(w.cues.filter((c) => sel.has(c.id)));
    const t0 = picked[0].pos_secs;
    clipboard = picked.map((c) => ({ rel: c.pos_secs - t0, cue: c }));
    setToast(t("cues.copied", { n: String(picked.length) }));
  }, [t]);

  const pasteAtPlayhead = useCallback(() => {
    const w = workingRef.current;
    if (!w || clipboard.length === 0) return;
    const at = snapFn(now());
    const pasted = clipboard.map(({ rel, cue }) =>
      newCue(at + rel, w.grid ?? null, cue.action, cue.name),
    );
    commit({ ...w, cues: sortCues([...w.cues, ...pasted]) });
    setSelected(new Set(pasted.map((c) => c.id)));
    setToast(t("cues.pasted", { n: String(pasted.length) }));
  }, [commit, now, snapFn, t]);

  const undo = useCallback(() => {
    const h = historyRef.current;
    const w = workingRef.current;
    if (!w || h.past.length === 0) return;
    const past = h.past.slice(0, -1);
    const prev = h.past[h.past.length - 1];
    setHistory({ past, future: [cloneSet(w), ...h.future].slice(0, 100) });
    setWorking(prev);
    scheduleSave(prev);
  }, [scheduleSave, setWorking]);

  const redo = useCallback(() => {
    const h = historyRef.current;
    const w = workingRef.current;
    if (!w || h.future.length === 0) return;
    const [next, ...future] = h.future;
    setHistory({ past: [...h.past, cloneSet(w)], future });
    setWorking(next);
    scheduleSave(next);
  }, [scheduleSave, setWorking]);

  const setGrid = useCallback(
    (g: TrackGrid | null) => {
      const w = workingRef.current;
      if (!w) return;
      // Re-derive every cue's beat from its (authoritative) seconds.
      commit({
        ...w,
        grid: g,
        cues: w.cues.map((c) => ({ ...c, beat: g ? beatAt(g, c.pos_secs) : c.beat })),
      });
    },
    [commit],
  );

  const detectGrid = useCallback(async () => {
    if (!activePath) return;
    setDetecting(true);
    try {
      const est = await invoke<GridEstimate>("estimate_track_grid", { path: activePath });
      setGrid({ bpm: est.bpm, first_beat_secs: est.first_beat_secs });
      setToast(
        t("cues.grid.detected", {
          bpm: est.bpm.toFixed(2),
          conf: String(Math.round(est.confidence * 100)),
        }),
      );
    } catch (e) {
      setToast(stringify(e));
    } finally {
      setDetecting(false);
    }
  }, [activePath, setGrid, t]);

  const gridFromVdj = useCallback(() => {
    const s = followerRef.current.status;
    if (!s || s.bpm == null || s.beat_pos == null || s.pos_secs == null || s.bpm <= 0) return;
    const first = s.pos_secs - (s.beat_pos * 60) / s.bpm;
    setGrid({ bpm: s.bpm, first_beat_secs: first });
  }, [setGrid]);

  /**
   * Capture the current stage look as a NEW scene/snapshot that belongs to
   * this track (named "Tema · E3"), and point the cue at it.
   */
  const createTargetForCue = useCallback(
    async (cueId: string, kind: "scene" | "snapshot") => {
      const w = workingRef.current;
      if (!w) return;
      const name = nextOwnedName(w, kind);
      try {
        let id: string;
        if (kind === "scene") {
          const s = await invoke<Scene>("create_scene_from_state", {
            name,
            fixtureIds: [],
            fadeInMs: 300,
            restrictToTouched: false,
            captureChaser: true,
            captureMovement: true,
          });
          id = s.id;
        } else {
          const s = await invoke<Snapshot>("capture_snapshot", { name });
          id = s.id;
        }
        const cur = workingRef.current;
        if (!cur) return;
        commit({
          ...cur,
          owned: [...(cur.owned ?? []), { kind, id }],
          cues: cur.cues.map((c) => (c.id === cueId ? { ...c, action: buildAction(kind, id) } : c)),
        });
        setToast(t("cues.created", { name }));
      } catch (e) {
        setToast(stringify(e));
      }
    },
    [commit, t],
  );

  /** Mark a reused/global target as "of this track" (an explicit repeat). */
  const adoptTarget = useCallback(
    (c: TrackCue) => {
      const w = workingRef.current;
      const k = kindOfAction(c.action);
      const id = actionId(c.action);
      if (!w || !k || !id || isOwnedBy(w, k, id)) return;
      commit({ ...w, owned: [...(w.owned ?? []), { kind: k, id }] });
    },
    [commit],
  );

  const addSectionAt = useCallback(
    (secs: number, name: string) => {
      const w = workingRef.current;
      if (!w) return;
      const sections = [...w.sections, newSection(secs, name)].sort(
        (a, b) => a.pos_secs - b.pos_secs,
      );
      commit({ ...w, sections });
    },
    [commit],
  );

  const updateSection = useCallback(
    (id: string, patch: { name?: string; pos_secs?: number }) => {
      const w = workingRef.current;
      if (!w) return;
      commit({
        ...w,
        sections: w.sections
          .map((s) => (s.id === id ? { ...s, ...patch } : s))
          .sort((a, b) => a.pos_secs - b.pos_secs),
      });
    },
    [commit],
  );

  const removeSection = useCallback(
    (id: string) => {
      const w = workingRef.current;
      if (!w) return;
      commit({ ...w, sections: w.sections.filter((s) => s.id !== id) });
    },
    [commit],
  );

  // ---- setlist ops ----
  const addTrack = useCallback(async () => {
    let picked: string | string[] | null = null;
    try {
      picked = await openDialog({
        multiple: false,
        filters: [{ name: "Audio", extensions: AUDIO_EXTS }],
      });
    } catch (e) {
      setToast(stringify(e));
      return;
    }
    if (typeof picked !== "string") return;
    const set = emptySet(picked, basename(picked));
    try {
      await invoke("save_track_cue_set", { set });
      openTrack(picked);
    } catch (e) {
      setToast(stringify(e));
    }
  }, [openTrack]);

  const addVdjTrack = useCallback(async () => {
    const s = followerRef.current.status;
    if (!s?.file_path) return;
    const set = emptySet(s.file_path, s.title ?? basename(s.file_path));
    try {
      await invoke("save_track_cue_set", { set });
      openTrack(s.file_path);
    } catch (e) {
      setToast(stringify(e));
    }
  }, [openTrack]);

  const removeTrack = useCallback(
    async (set: TrackCueSet) => {
      const yes = await ask(
        t("cues.removeTrackConfirm", { title: set.title || basename(set.file_path) }),
        {
          title: "DMX Control",
          kind: "warning",
        },
      );
      if (!yes) return;
      const owned = set.owned ?? [];
      if (owned.length > 0) {
        const alsoDelete = await ask(
          t("cues.deleteOwnedConfirm", {
            title: set.title || basename(set.file_path),
            n: String(owned.length),
          }),
          { title: "DMX Control", kind: "warning" },
        );
        if (alsoDelete) {
          for (const o of owned) {
            try {
              if (o.kind === "scene") await invoke("delete_scene", { id: o.id });
              else if (o.kind === "snapshot") await invoke("delete_snapshot", { id: o.id });
            } catch (e) {
              setToast(stringify(e));
            }
          }
        }
      }
      try {
        await invoke("remove_track_cue_set", { filePath: set.file_path });
        if (activePath === set.file_path) {
          setActivePath(null);
          setWorking(null);
          setWave(null);
          setLocalSrc(null);
        }
      } catch (e) {
        setToast(stringify(e));
      }
    },
    [activePath, setWorking, t],
  );

  const moveTrack = useCallback(
    async (path: string, dir: -1 | 1) => {
      const order = sets.map((s) => s.file_path);
      const i = order.indexOf(path);
      const j = i + dir;
      if (i < 0 || j < 0 || j >= order.length) return;
      [order[i], order[j]] = [order[j], order[i]];
      try {
        await invoke("reorder_track_cue_sets", { filePaths: order });
      } catch (e) {
        setToast(stringify(e));
      }
    },
    [sets],
  );

  const commitRename = useCallback(async () => {
    const path = renamingPath;
    setRenamingPath(null);
    if (!path) return;
    const title = renameDraft.trim();
    if (!title) return;
    const target = sets.find((s) => s.file_path === path);
    if (!target) return;
    const next = { ...cloneSet(target), title };
    if (workingRef.current?.file_path === path) {
      const cur = { ...workingRef.current, title };
      setWorking(cur);
      persistNow(cur);
    } else {
      try {
        await invoke("save_track_cue_set", { set: next });
      } catch (e) {
        setToast(stringify(e));
      }
    }
  }, [renamingPath, renameDraft, sets, persistNow, setWorking]);

  const exportSheet = useCallback(async () => {
    try {
      const dest = await saveDialog({
        filters: [{ name: "CSV", extensions: ["csv"] }],
        defaultPath: "cues.csv",
      });
      if (typeof dest !== "string") return;
      const rows = await invoke<number>("export_cue_sheet", { dest });
      setToast(t("cues.exportSheetDone", { n: String(rows) }));
    } catch (e) {
      setToast(stringify(e));
    }
  }, [t]);

  const exportPois = useCallback(async () => {
    try {
      const r = await invoke<PoiExportReport>("export_pois_to_vdj");
      let msg = t("cues.exportPoisDone", {
        tracks: String(r.tracks_written),
        cues: String(r.cues_written),
      });
      if (r.tracks_missing.length > 0) {
        msg += ` ${t("cues.exportPoisMissing", { n: String(r.tracks_missing.length) })}`;
      }
      setToast(msg);
    } catch (e) {
      setToast(stringify(e));
    }
  }, [t]);

  // ---- zoom helpers ----
  const zoomBy = useCallback(
    (factor: number) => {
      const v = viewRef.current;
      const w = widthRef.current;
      const dur = duration || 60;
      const minPps = Math.max(MIN_PX_PER_SEC, w / Math.max(1, dur));
      const next = Math.min(MAX_PX_PER_SEC, Math.max(minPps, v.pxPerSec * factor));
      const centre = now();
      setView({ start: Math.max(0, centre - w / next / 2), pxPerSec: next });
    },
    [duration, now],
  );

  // ---- keyboard ----
  const keyHandlers = useRef({
    toggle: () => {},
    seek: (_d: number) => {},
  });
  keyHandlers.current = {
    toggle: () => {
      if (source === "audio") transport.toggle();
    },
    seek: (d: number) => {
      if (source === "audio") transport.nudge(d);
    },
  };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTypingTarget(e.target)) return;
      const meta = e.metaKey || e.ctrlKey;
      const k = e.key;
      if (meta && (k === "z" || k === "Z")) {
        e.preventDefault();
        if (e.shiftKey) redo();
        else undo();
        return;
      }
      if (meta && k === "c") {
        e.preventDefault();
        copySelected();
        return;
      }
      if (meta && k === "v") {
        e.preventDefault();
        pasteAtPlayhead();
        return;
      }
      if (meta && k === "a") {
        e.preventDefault();
        const w = workingRef.current;
        if (w) setSelected(new Set(w.cues.map((c) => c.id)));
        return;
      }
      if (meta) return;
      switch (k) {
        case " ":
          e.preventDefault();
          keyHandlers.current.toggle();
          break;
        case "m":
        case "M":
          markNow();
          break;
        case "Delete":
        case "Backspace":
          if (selectedRef.current.size > 0) {
            e.preventDefault();
            deleteSelected();
          }
          break;
        case "ArrowLeft":
          e.preventDefault();
          keyHandlers.current.seek(e.shiftKey ? -NUDGE_MED : -NUDGE_COARSE);
          break;
        case "ArrowRight":
          e.preventDefault();
          keyHandlers.current.seek(e.shiftKey ? NUDGE_MED : NUDGE_COARSE);
          break;
        case ",":
        case "<":
          nudgeSelected(e.shiftKey ? -NUDGE_MED : -NUDGE_FINE);
          break;
        case ".":
        case ">":
          nudgeSelected(e.shiftKey ? NUDGE_MED : NUDGE_FINE);
          break;
        case "Escape":
          setSelected(new Set());
          break;
        case "i":
        case "I":
          transport.setLoop({ ...transport.loop, a: now() });
          break;
        case "o":
        case "O":
          transport.setLoop({ ...transport.loop, b: now() });
          break;
        case "l":
        case "L":
          transport.setLoop({ ...transport.loop, on: !transport.loop.on });
          break;
        case "g":
        case "G":
          setShowGrid((v) => !v);
          break;
        case "s":
        case "S":
          setSnap((v) => !v);
          break;
        case "+":
        case "=":
          zoomBy(1.5);
          break;
        case "-":
        case "_":
          zoomBy(1 / 1.5);
          break;
        case "f":
        case "F":
          fitView(duration);
          break;
        case "Home":
          if (source === "audio") transport.seek(0);
          break;
        default:
          break;
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [
    undo,
    redo,
    copySelected,
    pasteAtPlayhead,
    markNow,
    deleteSelected,
    nudgeSelected,
    transport,
    now,
    zoomBy,
    fitView,
    duration,
    source,
  ]);

  // ---- derived for render ----
  const cues = working?.cues ?? [];
  const selectedCues = cues.filter((c) => selected.has(c.id));
  const single = selectedCues.length === 1 ? selectedCues[0] : null;
  const dimmed = useMemo(() => {
    if (source !== "vdj" || !follower || follower.file_path !== activePath)
      return new Set<string>();
    return new Set(follower.fired_cue_ids);
  }, [source, follower, activePath]);
  const kindLabel = useCallback(
    (k: ActionKind | null) => (k ? t(`cues.kind.${k}` as const) : "—"),
    [t],
  );
  /** Title of the OTHER track that owns this cue's target (null = own/global). */
  const reusedFrom = useCallback(
    (c: TrackCue): string | null => {
      const k = kindOfAction(c.action);
      const id = actionId(c.action);
      if (!k || !id || !working || isOwnedBy(working, k, id)) return null;
      return ownerTitle(
        sets.filter((s) => s.file_path !== working.file_path),
        k,
        id,
      );
    },
    [sets, working],
  );
  const labelOf = useCallback(
    (c: TrackCue) => {
      if (isUnassigned(c.action)) return `⚠ ${t("cues.unassigned")}`;
      let base = c.name;
      if (!base) {
        const k = kindOfAction(c.action);
        const target = targetName(show, c.action);
        base = target ? `${kindLabel(k)}: ${target}` : kindLabel(k);
      }
      return reusedFrom(c) ? `↗ ${base}` : base;
    },
    [show, kindLabel, reusedFrom, t],
  );
  const armedLabel = (() => {
    const a = armedAction();
    if (isUnassigned(a)) return t("cues.unassigned");
    const target = targetName(show, a);
    return target ? `${kindLabel(armKind)}: ${target}` : kindLabel(armKind);
  })();
  const displayTime = source === "audio" ? transport.displayTime : (follower?.pos_secs ?? 0);
  const vdjHasOtherTrack = !!(follower?.file_path && follower.file_path !== activePath);
  const vdjTrackNotInSetlist = !!(
    follower?.file_path && !sets.some((s) => s.file_path === follower.file_path)
  );

  const rateOptions = [0.5, 0.75, 1];

  return (
    <main className="page cue-editor">
      {/* ---- Setlist ---- */}
      <aside className="cue-setlist">
        <div className="cue-pane-head">
          <h3>{t("cues.setlist")}</h3>
          <button
            type="button"
            className="primary"
            onClick={addTrack}
            title={t("cues.addTrackHint")}
          >
            {t("cues.addTrack")}
          </button>
        </div>
        {vdjTrackNotInSetlist && follower ? (
          <button type="button" className="cue-vdj-add" onClick={addVdjTrack}>
            {t("cues.addVdjTrack", { title: follower.title ?? "" })}
          </button>
        ) : null}
        {sets.length === 0 ? (
          <p className="hint">{t("cues.noTracks")}</p>
        ) : (
          <ol className="cue-tracks">
            {sets.map((s, i) => (
              <li
                key={s.file_path}
                className={`cue-track${s.file_path === activePath ? " active" : ""}`}
              >
                {renamingPath === s.file_path ? (
                  <input
                    className="cue-track__rename"
                    value={renameDraft}
                    onChange={(e) => setRenameDraft(e.currentTarget.value)}
                    onBlur={commitRename}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") commitRename();
                      if (e.key === "Escape") setRenamingPath(null);
                    }}
                    // biome-ignore lint/a11y/noAutofocus: inline rename field; focus is the whole point
                    autoFocus
                  />
                ) : (
                  <button
                    type="button"
                    className="cue-track__main"
                    onClick={() => openTrack(s.file_path)}
                    onDoubleClick={() => {
                      setRenamingPath(s.file_path);
                      setRenameDraft(s.title || basename(s.file_path));
                    }}
                  >
                    <span className="cue-track__num">{i + 1}</span>
                    <span className="cue-track__title">{s.title || basename(s.file_path)}</span>
                    <span className="cue-track__meta">
                      {s.duration_secs > 0 ? fmtTime(s.duration_secs, false) : "—"} ·{" "}
                      {t("cues.cueCount", { n: String(s.cues.length) })}
                      {s.grid ? ` · ${s.grid.bpm.toFixed(1)} BPM` : ""}
                      {(() => {
                        const un = s.cues.filter((c) => isUnassigned(c.action)).length;
                        return un > 0 ? (
                          <span className="cue-track__warn">
                            {" · "}
                            {t("cues.unassignedCount", { n: String(un) })}
                          </span>
                        ) : null;
                      })()}
                    </span>
                  </button>
                )}
                <span className="cue-track__tools">
                  <button
                    type="button"
                    onClick={() => moveTrack(s.file_path, -1)}
                    title={t("cues.moveUp")}
                  >
                    ↑
                  </button>
                  <button
                    type="button"
                    onClick={() => moveTrack(s.file_path, 1)}
                    title={t("cues.moveDown")}
                  >
                    ↓
                  </button>
                  <button type="button" onClick={() => removeTrack(s)} title={t("cues.remove")}>
                    ✕
                  </button>
                </span>
              </li>
            ))}
          </ol>
        )}
        <div className="cue-export">
          <span className="hint">{t("cues.export")}</span>
          <button type="button" onClick={exportSheet}>
            {t("cues.exportSheet")}
          </button>
          <button type="button" onClick={exportPois}>
            {t("cues.exportPois")}
          </button>
        </div>
      </aside>

      {/* ---- Main ---- */}
      <section className="cue-main">
        <div className="cue-transport">
          <div className="cue-source">
            <button
              type="button"
              className={source === "audio" ? "active" : ""}
              onClick={() => setSource("audio")}
            >
              {t("cues.source.audio")}
            </button>
            <button
              type="button"
              className={source === "vdj" ? "active" : ""}
              onClick={() => setSource("vdj")}
            >
              {t("cues.source.vdj")}
            </button>
          </div>
          <button
            type="button"
            className="cue-tbtn"
            onClick={() => transport.seek(0)}
            disabled={source !== "audio" || !transport.ready}
            title={t("cues.toStart")}
          >
            ⏮
          </button>
          <button
            type="button"
            className="cue-tbtn cue-tbtn--play"
            onClick={transport.toggle}
            disabled={source !== "audio" || !transport.ready}
            title={transport.playing ? t("cues.pause") : t("cues.play")}
          >
            {transport.playing ? "⏸" : "▶"}
          </button>
          <span className="cue-time">
            <strong>{fmtTime(displayTime)}</strong>
            <span className="hint"> / {fmtTime(duration, false)}</span>
            {grid ? (
              <span className="cue-time__beat"> · {fmtBarBeat(beatAt(grid, displayTime))}</span>
            ) : null}
          </span>
          <label className="cue-rate">
            {t("cues.rate")}
            <select
              value={transport.rate}
              onChange={(e) => transport.setRate(Number(e.currentTarget.value))}
              disabled={source !== "audio"}
            >
              {rateOptions.map((r) => (
                <option key={r} value={r}>
                  {r}×
                </option>
              ))}
            </select>
          </label>
          <span className="cue-loop">
            <button
              type="button"
              onClick={() => transport.setLoop({ ...transport.loop, a: now() })}
              title="I"
            >
              {t("cues.loopSetA")}
            </button>
            <button
              type="button"
              onClick={() => transport.setLoop({ ...transport.loop, b: now() })}
              title="O"
            >
              {t("cues.loopSetB")}
            </button>
            <button
              type="button"
              className={transport.loop.on ? "active" : ""}
              onClick={() => transport.setLoop({ ...transport.loop, on: !transport.loop.on })}
              disabled={transport.loop.a == null || transport.loop.b == null}
              title="L"
            >
              {t("cues.loop")}
            </button>
            {transport.loop.a != null || transport.loop.b != null ? (
              <button
                type="button"
                onClick={() => transport.setLoop({ a: null, b: null, on: false })}
                title={t("cues.loopClear")}
              >
                ✕
              </button>
            ) : null}
          </span>
          <span className="cue-toggles">
            <label className="cue-toggle">
              <input
                type="checkbox"
                checked={snap}
                onChange={(e) => setSnap(e.currentTarget.checked)}
              />
              {t("cues.snap")}
            </label>
            <label className="cue-toggle">
              <input
                type="checkbox"
                checked={showGrid}
                onChange={(e) => setShowGrid(e.currentTarget.checked)}
              />
              {t("cues.grid")}
            </label>
            <label className="cue-toggle cue-toggle--preview">
              <input
                type="checkbox"
                checked={preview}
                onChange={(e) => setPreview(e.currentTarget.checked)}
              />
              {t("cues.preview")}
            </label>
          </span>
          <span className="cue-zoom">
            <button type="button" onClick={() => zoomBy(1 / 1.5)} title={t("cues.zoomOut")}>
              −
            </button>
            <button type="button" onClick={() => zoomBy(1.5)} title={t("cues.zoomIn")}>
              +
            </button>
            <button type="button" onClick={() => fitView(duration)} title={t("cues.zoomFit")}>
              ⤢
            </button>
          </span>
          <button
            type="button"
            className="primary cue-mark"
            onClick={markNow}
            disabled={!working}
            title={t("cues.mark")}
          >
            🎯 {t("cues.markAt", { time: fmtTime(displayTime, false) })}
            <span className="cue-mark__armed">· {armedLabel}</span>
          </button>
        </div>

        {source === "vdj" ? (
          <div className={`cue-vdjbar${follower?.playing ? " playing" : ""}`}>
            {follower?.playing
              ? follower.signal === "os2l"
                ? t("cues.vdjPlayingOs2l", { title: follower.title ?? "" })
                : t("cues.vdjPlaying", { title: follower.title ?? "" })
              : t("cues.vdjIdle")}
            {follower?.playing && follower.assumed ? (
              <span className="cue-vdjbar__note">{t("cues.vdjAssumed")}</span>
            ) : null}
            {follower?.last_error ? (
              <span className="cue-vdjbar__note warn" title={follower.last_error}>
                {t("cues.vdjHttpDown", { endpoint: follower.http_endpoint })}
              </span>
            ) : null}
            {vdjHasOtherTrack && follower?.file_path ? (
              <button type="button" onClick={() => openTrack(follower.file_path ?? "")}>
                {t("cues.vdjOtherTrack")}
              </button>
            ) : null}
          </div>
        ) : null}

        {working ? (
          <Timeline
            wave={wave}
            duration={duration}
            timeRef={timeRef}
            playing={source === "audio" ? transport.playing : !!follower?.playing}
            cues={cues}
            sections={working.sections}
            grid={grid}
            showGrid={showGrid}
            selected={selected}
            dimmed={dimmed}
            flashing={flashing}
            loop={transport.loop}
            view={view}
            labelOf={labelOf}
            onViewChange={setView}
            onWidth={(w) => {
              widthRef.current = w;
            }}
            onSeek={(s) => {
              if (source === "audio") transport.seek(s);
            }}
            onSelect={(ids, additive) =>
              setSelected((prev) => {
                if (!additive) return new Set(ids);
                const next = new Set(prev);
                for (const id of ids) {
                  if (next.has(id)) next.delete(id);
                  else next.add(id);
                }
                return next;
              })
            }
            onMoveSelected={moveSelected}
            onAddCueAt={addCueAt}
          />
        ) : (
          <div className="cue-empty">
            <p className="hint">{t("cues.selectTrack")}</p>
          </div>
        )}

        <div className="cue-statusbar">
          {waveErr ? (
            <span className="error">{t("cues.waveError", { err: waveErr })}</span>
          ) : working && !wave ? (
            <span className="hint">{t("cues.noWave")}</span>
          ) : null}
          {transport.error && source === "audio" ? (
            <span className="error">{t("cues.audioError", { err: transport.error })}</span>
          ) : null}
          <span className="hint cue-shortcuts">{t("cues.shortcuts")}</span>
        </div>
      </section>

      {/* ---- Side: inspector + list ---- */}
      <aside className="cue-side">
        <div className="cue-inspector">
          {single ? (
            <CueInspector
              cue={single}
              grid={grid}
              show={show}
              sets={sets}
              set={working}
              kindLabel={kindLabel}
              onCreate={(k) => createTargetForCue(single.id, k)}
              onAdopt={() => adoptTarget(single)}
              onUpdate={(patch) => updateCue(single.id, patch)}
              onSetTime={(s) => setCueTime(single.id, s)}
              onGo={() => {
                if (source === "audio") transport.seek(single.pos_secs);
              }}
              onFire={() => fire(single)}
              onDuplicate={duplicateSelected}
              onDelete={deleteSelected}
            />
          ) : selectedCues.length > 1 ? (
            <div className="cue-multi">
              <h3>{t("cues.inspector.multi", { n: String(selectedCues.length) })}</h3>
              <NudgeButtons onNudge={nudgeSelected} label={t("cues.inspector.nudge")} />
              <div className="cue-row">
                <button type="button" onClick={duplicateSelected}>
                  {t("cues.inspector.duplicate")}
                </button>
                <button type="button" className="danger" onClick={deleteSelected}>
                  {t("cues.inspector.delete")}
                </button>
              </div>
            </div>
          ) : (
            <div className="cue-arm">
              <h3>{t("cues.arm.title")}</h3>
              <ActionPicker
                show={show}
                sets={sets}
                set={working}
                kind={armKind}
                target={armTarget}
                kindLabel={kindLabel}
                onKind={(k) => {
                  setArmKind(k);
                  setArmTarget("");
                }}
                onTarget={setArmTarget}
              />
              <p className="hint">{t("cues.inspector.none")}</p>

              <h3>{t("cues.grid.title")}</h3>
              <GridPanel
                grid={grid}
                detecting={detecting}
                canFromVdj={
                  !!(follower?.bpm && follower.beat_pos != null && follower.pos_secs != null)
                }
                onSet={setGrid}
                onFirstBeatHere={() => setGrid({ bpm: grid?.bpm ?? 120, first_beat_secs: now() })}
                onDetect={detectGrid}
                onFromVdj={gridFromVdj}
              />

              <h3>{t("cues.sections.title")}</h3>
              <SectionsPanel
                sections={working?.sections ?? []}
                onAdd={(name) => addSectionAt(now(), name)}
                onUpdate={updateSection}
                onRemove={removeSection}
                onGo={(s) => {
                  if (source === "audio") transport.seek(s);
                }}
              />
            </div>
          )}
        </div>

        <div className="cue-list">
          <div className="cue-pane-head">
            <h3>{t("cues.list.title")}</h3>
            <span className="hint">{t("cues.cueCount", { n: String(cues.length) })}</span>
          </div>
          {cues.length === 0 ? (
            <p className="hint">{t("cues.list.empty")}</p>
          ) : (
            <ul>
              {cues.map((c) => (
                <li key={c.id}>
                  <button
                    type="button"
                    className={`cue-list__row${selected.has(c.id) ? " selected" : ""}${
                      dimmed.has(c.id) ? " fired" : ""
                    }`}
                    onClick={(e) => {
                      setSelected((prev) => {
                        if (!e.shiftKey) return new Set([c.id]);
                        const next = new Set(prev);
                        if (next.has(c.id)) next.delete(c.id);
                        else next.add(c.id);
                        return next;
                      });
                      if (source === "audio") transport.seek(c.pos_secs);
                    }}
                  >
                    <span
                      className="cue-list__dot"
                      style={{ background: c.color || kindColor(kindOfAction(c.action)) }}
                    />
                    <span className="cue-list__time">{fmtTime(c.pos_secs)}</span>
                    <span className="cue-list__name">{labelOf(c)}</span>
                    <span className="cue-list__action">
                      {c.name ? labelOf({ ...c, name: "" }) : ""}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </aside>

      {toast ? <output className="app-toast">{toast}</output> : null}
    </main>
  );
}

// ---- Sub-components ------------------------------------------------------

function NudgeButtons({ onNudge, label }: { onNudge: (d: number) => void; label: string }) {
  return (
    <div className="cue-nudge">
      <span className="hint">{label}</span>
      <button type="button" onClick={() => onNudge(-NUDGE_COARSE)}>
        −1s
      </button>
      <button type="button" onClick={() => onNudge(-NUDGE_MED)}>
        −100ms
      </button>
      <button type="button" onClick={() => onNudge(-NUDGE_FINE)}>
        −10ms
      </button>
      <button type="button" onClick={() => onNudge(NUDGE_FINE)}>
        +10ms
      </button>
      <button type="button" onClick={() => onNudge(NUDGE_MED)}>
        +100ms
      </button>
      <button type="button" onClick={() => onNudge(NUDGE_COARSE)}>
        +1s
      </button>
    </div>
  );
}

function ActionPicker({
  show,
  sets,
  set,
  kind,
  target,
  kindLabel,
  onKind,
  onTarget,
  onCreate,
  onAdopt,
}: {
  show: ReturnType<typeof useShowStore.getState>["show"];
  sets: TrackCueSet[];
  set: TrackCueSet | null;
  kind: ActionKind;
  target: string;
  kindLabel: (k: ActionKind | null) => string;
  onKind: (k: ActionKind) => void;
  onTarget: (id: string) => void;
  onCreate?: (kind: "scene" | "snapshot") => void;
  onAdopt?: () => void;
}) {
  const t = useT();
  const [filter, setFilter] = useState("");
  // Default scope = this track's own entities, so an act never grabs
  // another act's look by accident; "all" is the explicit reuse path.
  const [scope, setScope] = useState<"own" | "all">("own");
  const owned = (id: string) => (set ? isOwnedBy(set, kind, id) : false);
  const others = sets.filter((s) => s.file_path !== set?.file_path);
  const items = itemsForKind(show, kind);
  const ownItems = items.filter((i) => owned(i.id));
  // A cue pointing at something this track doesn't own must stay visible:
  // flip to the "all" scope instead of showing an empty picker.
  const targetIsForeign = !!target && !owned(target);
  useEffect(() => {
    if (targetIsForeign) setScope("all");
  }, [targetIsForeign]);
  const pool = scope === "own" ? ownItems : items;
  const shown = filter
    ? pool.filter((i) => i.name.toLowerCase().includes(filter.toLowerCase()))
    : pool;
  const targetOwner = target && !owned(target) ? ownerTitle(others, kind, target) : null;
  const needsTarget = kindNeedsTarget(kind);
  return (
    <div className="cue-picker">
      <label>
        {t("cues.inspector.kind")}
        <select value={kind} onChange={(e) => onKind(e.currentTarget.value as ActionKind)}>
          {ACTION_KINDS.map((k) => (
            <option key={k} value={k}>
              {kindLabel(k)}
            </option>
          ))}
        </select>
      </label>
      {needsTarget ? (
        <>
          <div className="cue-seg">
            <button
              type="button"
              className={scope === "own" ? "active" : ""}
              onClick={() => setScope("own")}
            >
              {t("cues.scope.own")} ({ownItems.length})
            </button>
            <button
              type="button"
              className={scope === "all" ? "active" : ""}
              onClick={() => setScope("all")}
            >
              {t("cues.scope.all")}
            </button>
          </div>
          <label>
            {t("cues.inspector.target")}
            <input
              type="search"
              placeholder={t("cues.inspector.filter")}
              value={filter}
              onChange={(e) => setFilter(e.currentTarget.value)}
            />
            <select
              value={target}
              onChange={(e) => onTarget(e.currentTarget.value)}
              size={Math.min(7, Math.max(3, shown.length + 1))}
            >
              <option value="">{t("cues.pickTarget")}</option>
              {shown.map((i) => {
                const from =
                  scope === "all" && !owned(i.id) ? ownerTitle(others, kind, i.id) : null;
                return (
                  <option key={i.id} value={i.id}>
                    {from ? `${i.name}  (↗ ${from})` : i.name}
                  </option>
                );
              })}
            </select>
          </label>
          {targetOwner ? (
            <p className="hint cue-reused">
              {t("cues.reused", { title: targetOwner })}
              {onAdopt ? (
                <button type="button" onClick={onAdopt}>
                  {t("cues.adopt")}
                </button>
              ) : null}
            </p>
          ) : null}
          {onCreate && (kind === "scene" || kind === "snapshot") ? (
            <div className="cue-create">
              <button type="button" className="primary" onClick={() => onCreate(kind)}>
                {kind === "scene" ? t("cues.createScene") : t("cues.createSnapshot")}
              </button>
              <span className="hint">{t("cues.createHint")}</span>
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}

function CueInspector({
  cue,
  grid,
  show,
  sets,
  set,
  kindLabel,
  onUpdate,
  onSetTime,
  onGo,
  onFire,
  onDuplicate,
  onDelete,
  onCreate,
  onAdopt,
}: {
  cue: TrackCue;
  grid: TrackGrid | null;
  show: ReturnType<typeof useShowStore.getState>["show"];
  sets: TrackCueSet[];
  set: TrackCueSet | null;
  kindLabel: (k: ActionKind | null) => string;
  onCreate: (kind: "scene" | "snapshot") => void;
  onAdopt: () => void;
  onUpdate: (patch: Partial<TrackCue>) => void;
  onSetTime: (secs: number) => void;
  onGo: () => void;
  onFire: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
}) {
  const t = useT();
  const kind = kindOfAction(cue.action) ?? "scene";
  const [timeDraft, setTimeDraft] = useState(fmtTime(cue.pos_secs));
  useEffect(() => setTimeDraft(fmtTime(cue.pos_secs)), [cue.pos_secs]);
  const commitTime = () => {
    const parsed = parseTime(timeDraft);
    if (parsed != null && Math.abs(parsed - cue.pos_secs) > 0.0005) onSetTime(parsed);
    else setTimeDraft(fmtTime(cue.pos_secs));
  };
  const autoColor = kindColor(kind);
  return (
    <div className="cue-single">
      <h3>{t("cues.inspector.title")}</h3>
      <label>
        {t("cues.inspector.name")}
        <input
          type="text"
          value={cue.name}
          placeholder={t("cues.inspector.namePh")}
          onChange={(e) => onUpdate({ name: e.currentTarget.value })}
        />
      </label>
      <ActionPicker
        show={show}
        sets={sets}
        set={set}
        kind={kind}
        target={actionId(cue.action) ?? ""}
        kindLabel={kindLabel}
        onKind={(k) => onUpdate({ action: buildAction(k, kindNeedsTarget(k) ? "" : null) })}
        onTarget={(id) => onUpdate({ action: buildAction(kind, id) })}
        onCreate={onCreate}
        onAdopt={onAdopt}
      />
      <div className="cue-row">
        <label>
          {t("cues.inspector.time")}
          <input
            type="text"
            className="cue-time-input"
            value={timeDraft}
            onChange={(e) => setTimeDraft(e.currentTarget.value)}
            onBlur={commitTime}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitTime();
            }}
          />
        </label>
        <span className="hint">
          {t("cues.inspector.beat")}: {grid ? fmtBarBeat(beatAt(grid, cue.pos_secs)) : "—"}
        </span>
      </div>
      <NudgeButtons
        onNudge={(d) => onSetTime(cue.pos_secs + d)}
        label={t("cues.inspector.nudge")}
      />
      <div className="cue-row">
        <label className="cue-color">
          {t("cues.inspector.color")}
          <input
            type="color"
            value={cue.color || autoColor}
            onChange={(e) => onUpdate({ color: e.currentTarget.value })}
          />
        </label>
        <label className="cue-toggle">
          <input
            type="checkbox"
            checked={!cue.color}
            onChange={(e) => onUpdate({ color: e.currentTarget.checked ? null : autoColor })}
          />
          {t("cues.inspector.colorAuto")}
        </label>
      </div>
      <div className="cue-row">
        <button type="button" onClick={onGo}>
          {t("cues.inspector.go")}
        </button>
        <button type="button" onClick={onFire}>
          {t("cues.inspector.fire")}
        </button>
        <button type="button" onClick={onDuplicate}>
          {t("cues.inspector.duplicate")}
        </button>
        <button type="button" className="danger" onClick={onDelete}>
          {t("cues.inspector.delete")}
        </button>
      </div>
    </div>
  );
}

function GridPanel({
  grid,
  detecting,
  canFromVdj,
  onSet,
  onFirstBeatHere,
  onDetect,
  onFromVdj,
}: {
  grid: TrackGrid | null;
  detecting: boolean;
  canFromVdj: boolean;
  onSet: (g: TrackGrid | null) => void;
  onFirstBeatHere: () => void;
  onDetect: () => void;
  onFromVdj: () => void;
}) {
  const t = useT();
  const [bpmDraft, setBpmDraft] = useState(grid ? grid.bpm.toFixed(2) : "");
  useEffect(() => setBpmDraft(grid ? grid.bpm.toFixed(2) : ""), [grid]);
  const commitBpm = () => {
    const v = Number(bpmDraft.replace(",", "."));
    if (Number.isFinite(v) && v > 0) onSet({ bpm: v, first_beat_secs: grid?.first_beat_secs ?? 0 });
  };
  return (
    <div className="cue-gridpanel">
      {!grid ? <p className="hint">{t("cues.grid.none")}</p> : null}
      <div className="cue-row">
        <label>
          {t("cues.grid.bpm")}
          <input
            type="text"
            inputMode="decimal"
            className="cue-bpm-input"
            value={bpmDraft}
            onChange={(e) => setBpmDraft(e.currentTarget.value)}
            onBlur={commitBpm}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitBpm();
            }}
          />
        </label>
        <span className="hint">
          {t("cues.grid.firstBeat")}: {grid ? fmtTime(grid.first_beat_secs) : "—"}
        </span>
      </div>
      <div className="cue-row">
        <button type="button" onClick={onFirstBeatHere}>
          {t("cues.grid.setHere")}
        </button>
        <button type="button" onClick={onDetect} disabled={detecting}>
          {detecting ? t("cues.grid.detecting") : t("cues.grid.detect")}
        </button>
        {canFromVdj ? (
          <button type="button" onClick={onFromVdj}>
            {t("cues.grid.fromVdj")}
          </button>
        ) : null}
        {grid ? (
          <button type="button" onClick={() => onSet(null)}>
            {t("cues.grid.clear")}
          </button>
        ) : null}
      </div>
    </div>
  );
}

function SectionsPanel({
  sections,
  onAdd,
  onUpdate,
  onRemove,
  onGo,
}: {
  sections: TrackCueSet["sections"];
  onAdd: (name: string) => void;
  onUpdate: (id: string, patch: { name?: string; pos_secs?: number }) => void;
  onRemove: (id: string) => void;
  onGo: (secs: number) => void;
}) {
  const t = useT();
  const [name, setName] = useState("");
  return (
    <div className="cue-sections">
      <div className="cue-row">
        <input
          type="text"
          placeholder={t("cues.sections.namePh")}
          value={name}
          onChange={(e) => setName(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && name.trim()) {
              onAdd(name.trim());
              setName("");
            }
          }}
        />
        <button
          type="button"
          onClick={() => {
            if (!name.trim()) return;
            onAdd(name.trim());
            setName("");
          }}
        >
          {t("cues.sections.add")}
        </button>
      </div>
      {sections.length === 0 ? (
        <p className="hint">{t("cues.sections.none")}</p>
      ) : (
        <ul className="cue-sections__list">
          {sections.map((s) => (
            <li key={s.id}>
              <button type="button" className="cue-sections__go" onClick={() => onGo(s.pos_secs)}>
                {fmtTime(s.pos_secs, false)}
              </button>
              <input
                type="text"
                value={s.name}
                onChange={(e) => onUpdate(s.id, { name: e.currentTarget.value })}
              />
              <button type="button" onClick={() => onRemove(s.id)} title={t("cues.remove")}>
                ✕
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
