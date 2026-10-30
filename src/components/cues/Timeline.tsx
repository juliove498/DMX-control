import type { TrackCue } from "@bindings/TrackCue";
import type { TrackGrid } from "@bindings/TrackGrid";
import type { TrackSection } from "@bindings/TrackSection";
import type { WaveformData } from "@bindings/WaveformData";
import { useEffect, useRef } from "react";
import { fmtTime, isUnassigned, kindColor, kindOfAction } from "../../lib/actions";
import { beatAt, secsAt } from "./cueModel";
import type { LoopRegion } from "./useAudioTransport";

// Canvas timeline for the cue editor: zoomable waveform, time ruler,
// optional beat grid, section bands, cue flags, A/B loop shading and the
// playhead, plus a minimap. Everything is drawn from a props ref inside
// one requestAnimationFrame loop so playback never re-renders React.

export interface TimelineView {
  start: number;
  pxPerSec: number;
}

export interface TimelineProps {
  wave: WaveformData | null;
  duration: number;
  timeRef: React.MutableRefObject<number>;
  playing: boolean;
  cues: TrackCue[];
  sections: TrackSection[];
  grid: TrackGrid | null;
  showGrid: boolean;
  selected: ReadonlySet<string>;
  dimmed: ReadonlySet<string>;
  flashing: ReadonlyMap<string, number>;
  loop: LoopRegion;
  view: TimelineView;
  labelOf: (cue: TrackCue) => string;
  onViewChange: (v: TimelineView) => void;
  onWidth: (px: number) => void;
  onSeek: (secs: number) => void;
  onSelect: (ids: string[], additive: boolean) => void;
  onMoveSelected: (deltaSecs: number, commit: boolean) => void;
  onAddCueAt: (secs: number) => void;
}

const RULER_H = 22;
const SECTION_H = 18;
const FLAG_H = 18;
const MINIMAP_H = 44;
export const MIN_PX_PER_SEC = 2;
export const MAX_PX_PER_SEC = 2500;

type DragMode =
  | { kind: "none" }
  | { kind: "scrub" }
  | { kind: "drag"; x0: number; moved: boolean }
  | { kind: "rect"; x0: number; x1: number; additive: boolean }
  | { kind: "pan"; x0: number; start0: number; moved: boolean }
  | { kind: "mini" };

function niceStep(pxPerSec: number): number {
  const candidates = [0.05, 0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300];
  for (const c of candidates) {
    if (c * pxPerSec >= 70) return c;
  }
  return 600;
}

/** Rounded flag outline; plain rect where `roundRect` is missing (Safari < 16). */
function flagPath(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  if (typeof ctx.roundRect === "function") {
    ctx.roundRect(x, y, w, h, r);
  } else {
    ctx.rect(x, y, w, h);
  }
}

function sectionHue(index: number): string {
  return `hsla(${(index * 67) % 360}, 60%, 55%, 0.35)`;
}

export function Timeline(props: TimelineProps) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const mainRef = useRef<HTMLCanvasElement | null>(null);
  const miniRef = useRef<HTMLCanvasElement | null>(null);
  const propsRef = useRef(props);
  propsRef.current = props;
  const dirtyRef = useRef(true);
  const dragRef = useRef<DragMode>({ kind: "none" });
  const hoverRef = useRef<number | null>(null);
  const sizeRef = useRef({ w: 0, h: 0, dpr: 1 });
  // Flag rectangles as last drawn, so hit-testing matches what's on screen
  // (labels have variable width and stack into rows).
  const flagRectsRef = useRef<{ id: string; x: number; y: number; w: number; h: number }[]>([]);

  // Any prop change → redraw on the next frame.
  useEffect(() => {
    dirtyRef.current = true;
  });

  // Size to the container; keep canvases crisp on HiDPI.
  useEffect(() => {
    const wrap = wrapRef.current;
    const main = mainRef.current;
    const mini = miniRef.current;
    if (!wrap || !main || !mini) return;
    const apply = () => {
      const rect = wrap.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      const w = Math.max(50, Math.floor(rect.width));
      const h = Math.max(120, Math.floor(rect.height) - MINIMAP_H - 6);
      sizeRef.current = { w, h, dpr };
      main.width = Math.floor(w * dpr);
      main.height = Math.floor(h * dpr);
      main.style.width = `${w}px`;
      main.style.height = `${h}px`;
      mini.width = Math.floor(w * dpr);
      mini.height = Math.floor(MINIMAP_H * dpr);
      mini.style.width = `${w}px`;
      mini.style.height = `${MINIMAP_H}px`;
      dirtyRef.current = true;
      propsRef.current.onWidth(w);
    };
    apply();
    const ro = new ResizeObserver(apply);
    ro.observe(wrap);
    return () => ro.disconnect();
  }, []);

  // Draw loop.
  useEffect(() => {
    let raf = 0;
    let lastTime = -1;
    const frame = () => {
      const p = propsRef.current;
      const t = p.timeRef.current;
      // Auto-follow while playing: keep the playhead inside the view.
      if (p.playing && p.wave) {
        const { w } = sizeRef.current;
        const secsVisible = w / p.view.pxPerSec;
        if (t < p.view.start || t > p.view.start + secsVisible * 0.92) {
          p.onViewChange({ start: Math.max(0, t - secsVisible * 0.1), pxPerSec: p.view.pxPerSec });
        }
      }
      if (dirtyRef.current || t !== lastTime || p.flashing.size > 0) {
        draw();
        lastTime = t;
        dirtyRef.current = false;
      }
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const xOf = (secs: number) =>
    (secs - propsRef.current.view.start) * propsRef.current.view.pxPerSec;
  const tOf = (x: number) => propsRef.current.view.start + x / propsRef.current.view.pxPerSec;

  const cueColor = (c: TrackCue) => c.color || kindColor(kindOfAction(c.action));

  function draw() {
    const main = mainRef.current;
    const mini = miniRef.current;
    if (!main || !mini) return;
    const p = propsRef.current;
    const { w, h, dpr } = sizeRef.current;
    const ctx = main.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(main);
    const col = (name: string, fb: string) => css.getPropertyValue(name).trim() || fb;
    const cBg = col("--bg-1", "#111318");
    const cBg2 = col("--bg-2", "#1a1d24");
    const cLine = col("--line-2", "#2a2f3a");
    const cText = col("--text", "#e5e7eb");
    const cDim = col("--text-dim", "#8b93a3");
    const cAccent = col("--accent", "#4ade80");
    const time = p.timeRef.current;
    const now = performance.now();

    ctx.fillStyle = cBg;
    ctx.fillRect(0, 0, w, h);

    const waveTop = RULER_H + SECTION_H;
    const waveH = h - waveTop;
    const mid = waveTop + waveH / 2;

    // Loop region shading.
    if (p.loop.a != null && p.loop.b != null && p.loop.b > p.loop.a) {
      const xa = xOf(p.loop.a);
      const xb = xOf(p.loop.b);
      ctx.fillStyle = p.loop.on ? "rgba(96,165,250,0.14)" : "rgba(96,165,250,0.06)";
      ctx.fillRect(xa, RULER_H, xb - xa, h - RULER_H);
    }

    // Waveform.
    if (p.wave && p.wave.peaks.length > 0) {
      const pps = p.wave.peaks_per_second;
      const peaks = p.wave.peaks;
      const bucketPx = p.view.pxPerSec / pps;
      const playedX = xOf(time);
      if (bucketPx >= 1.5) {
        // Zoomed in past the bucket resolution: one bar per bucket.
        const i0 = Math.max(0, Math.floor(p.view.start * pps));
        const i1 = Math.min(peaks.length, Math.ceil(tOf(w) * pps) + 1);
        for (let i = i0; i < i1; i++) {
          const x = xOf(i / pps);
          const v = peaks[i] / 255;
          const bh = Math.max(1, v * waveH * 0.92);
          ctx.fillStyle = x < playedX ? cAccent : cDim;
          ctx.globalAlpha = x < playedX ? 0.9 : 0.55;
          ctx.fillRect(x, mid - bh / 2, Math.max(1, bucketPx - 0.5), bh);
        }
      } else {
        for (let x = 0; x < w; x++) {
          const a = Math.floor(tOf(x) * pps);
          const b = Math.max(a + 1, Math.floor(tOf(x + 1) * pps));
          if (a >= peaks.length || a < 0) continue;
          let m = 0;
          for (let i = a; i < b && i < peaks.length; i++) if (peaks[i] > m) m = peaks[i];
          const bh = Math.max(1, (m / 255) * waveH * 0.92);
          ctx.fillStyle = x < playedX ? cAccent : cDim;
          ctx.globalAlpha = x < playedX ? 0.9 : 0.55;
          ctx.fillRect(x, mid - bh / 2, 1, bh);
        }
      }
      ctx.globalAlpha = 1;
    } else {
      ctx.fillStyle = cDim;
      ctx.font = "12px system-ui, sans-serif";
      ctx.fillText("…", w / 2, mid);
    }

    // Beat grid.
    if (p.grid && p.showGrid && p.grid.bpm > 0) {
      const beatPx = (p.view.pxPerSec * 60) / p.grid.bpm;
      if (beatPx >= 3) {
        const first = Math.ceil(beatAt(p.grid, p.view.start));
        const last = Math.floor(beatAt(p.grid, tOf(w)));
        ctx.font = "10px system-ui, sans-serif";
        for (let b = first; b <= last; b++) {
          const x = xOf(secsAt(p.grid, b));
          const bar = ((b % 4) + 4) % 4 === 0;
          ctx.strokeStyle = bar ? "rgba(255,255,255,0.22)" : "rgba(255,255,255,0.08)";
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(Math.round(x) + 0.5, waveTop);
          ctx.lineTo(Math.round(x) + 0.5, h);
          ctx.stroke();
          if (bar && beatPx * 4 >= 36) {
            ctx.fillStyle = cDim;
            ctx.fillText(`${Math.floor(b / 4) + 1}`, x + 3, h - 4);
          }
        }
      }
    }

    // Sections band.
    ctx.fillStyle = cBg2;
    ctx.fillRect(0, RULER_H, w, SECTION_H);
    const secs = [...p.sections].sort((a, b) => a.pos_secs - b.pos_secs);
    ctx.font = "11px system-ui, sans-serif";
    secs.forEach((s, i) => {
      const x0 = xOf(s.pos_secs);
      const end = i + 1 < secs.length ? secs[i + 1].pos_secs : p.duration || s.pos_secs + 30;
      const x1 = xOf(end);
      if (x1 < 0 || x0 > w) return;
      ctx.fillStyle = sectionHue(i);
      ctx.fillRect(x0, RULER_H, Math.max(2, x1 - x0), SECTION_H);
      ctx.fillStyle = cText;
      ctx.save();
      ctx.beginPath();
      ctx.rect(Math.max(0, x0), RULER_H, Math.max(0, x1 - Math.max(0, x0)) - 4, SECTION_H);
      ctx.clip();
      ctx.fillText(s.name, Math.max(0, x0) + 4, RULER_H + 13);
      ctx.restore();
    });

    // Ruler.
    ctx.fillStyle = cBg2;
    ctx.fillRect(0, 0, w, RULER_H);
    ctx.strokeStyle = cLine;
    ctx.beginPath();
    ctx.moveTo(0, RULER_H + 0.5);
    ctx.lineTo(w, RULER_H + 0.5);
    ctx.stroke();
    const step = niceStep(p.view.pxPerSec);
    const firstTick = Math.floor(p.view.start / step) * step;
    ctx.font = "10px system-ui, sans-serif";
    for (let tt = firstTick; tt <= tOf(w) + step; tt += step) {
      const x = xOf(tt);
      ctx.strokeStyle = cLine;
      ctx.beginPath();
      ctx.moveTo(Math.round(x) + 0.5, RULER_H - 7);
      ctx.lineTo(Math.round(x) + 0.5, RULER_H);
      ctx.stroke();
      ctx.fillStyle = cDim;
      ctx.fillText(fmtTime(tt, step < 1), x + 3, 11);
      // Half-step minor tick.
      const xm = xOf(tt + step / 2);
      ctx.beginPath();
      ctx.moveTo(Math.round(xm) + 0.5, RULER_H - 3);
      ctx.lineTo(Math.round(xm) + 0.5, RULER_H);
      ctx.stroke();
    }
    // Loop markers on the ruler.
    ctx.font = "bold 10px system-ui, sans-serif";
    for (const [label, pos] of [
      ["A", p.loop.a],
      ["B", p.loop.b],
    ] as const) {
      if (pos == null) continue;
      const x = xOf(pos);
      ctx.fillStyle = "#60a5fa";
      ctx.fillRect(x - 1, 0, 2, RULER_H);
      ctx.fillRect(x, 0, 12, 12);
      ctx.fillStyle = "#0b1020";
      ctx.fillText(label, x + 2, 10);
    }

    // Cues.
    ctx.font = "11px system-ui, sans-serif";
    const sortedCues = [...p.cues].sort((a, b) => a.pos_secs - b.pos_secs);
    let lastFlagRight = Number.NEGATIVE_INFINITY;
    let flagRow = 0;
    const flagRects: { id: string; x: number; y: number; w: number; h: number }[] = [];
    for (const c of sortedCues) {
      const x = xOf(c.pos_secs);
      if (x < -200 || x > w + 200) continue;
      const unassigned = isUnassigned(c.action);
      const color = unassigned ? "#fbbf24" : cueColor(c);
      const sel = p.selected.has(c.id);
      const dim = p.dimmed.has(c.id);
      const flashAt = p.flashing.get(c.id);
      const flash = flashAt != null && now - flashAt < 450;
      ctx.globalAlpha = dim && !sel ? 0.4 : 1;
      ctx.strokeStyle = color;
      ctx.lineWidth = sel ? 2.5 : 1.5;
      if (flash) {
        ctx.save();
        ctx.shadowColor = color;
        ctx.shadowBlur = 16;
        ctx.lineWidth = 4;
      }
      if (unassigned) ctx.setLineDash([5, 4]);
      ctx.beginPath();
      ctx.moveTo(Math.round(x) + 0.5, waveTop);
      ctx.lineTo(Math.round(x) + 0.5, h);
      ctx.stroke();
      ctx.setLineDash([]);
      if (flash) ctx.restore();
      // Flag: stack rows when neighbours overlap.
      const label = p.labelOf(c);
      const tw = Math.min(160, ctx.measureText(label).width + 10);
      if (x < lastFlagRight + 4) flagRow = (flagRow + 1) % 3;
      else flagRow = 0;
      lastFlagRight = Math.max(lastFlagRight, x + tw);
      const fy = waveTop + 2 + flagRow * (FLAG_H + 2);
      flagRects.push({ id: c.id, x, y: fy, w: tw, h: FLAG_H });
      ctx.fillStyle = color;
      ctx.beginPath();
      flagPath(ctx, x, fy, tw, FLAG_H, 4);
      ctx.fill();
      if (sel) {
        ctx.strokeStyle = "#ffffff";
        ctx.lineWidth = 2;
        ctx.stroke();
      }
      ctx.fillStyle = "#0b1020";
      ctx.save();
      ctx.beginPath();
      ctx.rect(x, fy, tw, FLAG_H);
      ctx.clip();
      ctx.fillText(label, x + 5, fy + 13);
      ctx.restore();
      ctx.globalAlpha = 1;
    }

    flagRectsRef.current = flagRects;

    // Rectangle selection.
    const d = dragRef.current;
    if (d.kind === "rect") {
      const x0 = Math.min(d.x0, d.x1);
      const x1 = Math.max(d.x0, d.x1);
      ctx.fillStyle = "rgba(255,255,255,0.08)";
      ctx.fillRect(x0, waveTop, x1 - x0, waveH);
      ctx.strokeStyle = "rgba(255,255,255,0.4)";
      ctx.strokeRect(x0 + 0.5, waveTop + 0.5, x1 - x0, waveH - 1);
    }

    // Hover cursor.
    if (hoverRef.current != null && d.kind === "none") {
      const hx = hoverRef.current;
      ctx.strokeStyle = "rgba(255,255,255,0.18)";
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(Math.round(hx) + 0.5, RULER_H);
      ctx.lineTo(Math.round(hx) + 0.5, h);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.fillStyle = cText;
      ctx.font = "10px system-ui, sans-serif";
      ctx.fillText(fmtTime(tOf(hx)), hx + 4, RULER_H + SECTION_H + 12);
    }

    // Playhead.
    const px = xOf(time);
    if (px >= -2 && px <= w + 2) {
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(px - 1, 0, 2, h);
      ctx.beginPath();
      ctx.moveTo(px - 6, 0);
      ctx.lineTo(px + 6, 0);
      ctx.lineTo(px, 8);
      ctx.closePath();
      ctx.fill();
    }

    // ---- Minimap ----
    const mctx = mini.getContext("2d");
    if (!mctx) return;
    mctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    mctx.fillStyle = cBg2;
    mctx.fillRect(0, 0, w, MINIMAP_H);
    const dur = p.duration || (p.wave ? p.wave.duration_secs : 0);
    if (p.wave && dur > 0) {
      const peaks = p.wave.peaks;
      const per = peaks.length / w;
      mctx.fillStyle = cDim;
      mctx.globalAlpha = 0.6;
      for (let x = 0; x < w; x++) {
        const a = Math.floor(x * per);
        const b = Math.max(a + 1, Math.floor((x + 1) * per));
        let m = 0;
        for (let i = a; i < b && i < peaks.length; i++) if (peaks[i] > m) m = peaks[i];
        const bh = Math.max(1, (m / 255) * (MINIMAP_H - 6));
        mctx.fillRect(x, MINIMAP_H / 2 - bh / 2, 1, bh);
      }
      mctx.globalAlpha = 1;
      for (const c of p.cues) {
        mctx.fillStyle = cueColor(c);
        mctx.fillRect((c.pos_secs / dur) * w, 0, 1, MINIMAP_H);
      }
      const vx0 = (p.view.start / dur) * w;
      const vx1 = ((p.view.start + w / p.view.pxPerSec) / dur) * w;
      mctx.fillStyle = "rgba(255,255,255,0.10)";
      mctx.fillRect(vx0, 0, Math.max(3, vx1 - vx0), MINIMAP_H);
      mctx.strokeStyle = "rgba(255,255,255,0.5)";
      mctx.strokeRect(vx0 + 0.5, 0.5, Math.max(3, vx1 - vx0) - 1, MINIMAP_H - 1);
      mctx.fillStyle = "#ffffff";
      mctx.fillRect((time / dur) * w - 0.5, 0, 1.5, MINIMAP_H);
    }
  }

  // ---- Pointer interaction (main canvas) ----
  const hitCue = (x: number, y: number): string | null => {
    // Flags first (topmost drawn = last in the list wins), then the line.
    const rects = flagRectsRef.current;
    for (let i = rects.length - 1; i >= 0; i--) {
      const r = rects[i];
      if (x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h) return r.id;
    }
    const p = propsRef.current;
    let best: string | null = null;
    let bestDist = 6;
    for (const c of p.cues) {
      const dist = Math.abs(xOf(c.pos_secs) - x);
      if (dist < bestDist) {
        best = c.id;
        bestDist = dist;
      }
    }
    return best;
  };

  const onPointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    e.currentTarget.setPointerCapture(e.pointerId);
    const p = propsRef.current;
    if (y < RULER_H) {
      dragRef.current = { kind: "scrub" };
      p.onSeek(Math.max(0, tOf(x)));
      return;
    }
    const hit = hitCue(x, y);
    if (hit) {
      if (!p.selected.has(hit)) p.onSelect([hit], e.shiftKey);
      dragRef.current = { kind: "drag", x0: x, moved: false };
      return;
    }
    if (e.shiftKey) {
      dragRef.current = { kind: "rect", x0: x, x1: x, additive: true };
      return;
    }
    dragRef.current = { kind: "pan", x0: x, start0: p.view.start, moved: false };
  };

  const onPointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    hoverRef.current = x;
    dirtyRef.current = true;
    const d = dragRef.current;
    const p = propsRef.current;
    switch (d.kind) {
      case "scrub":
        p.onSeek(Math.max(0, tOf(x)));
        break;
      case "drag": {
        const dx = x - d.x0;
        if (!d.moved && Math.abs(dx) < 3) return;
        d.moved = true;
        p.onMoveSelected(dx / p.view.pxPerSec, false);
        break;
      }
      case "rect":
        d.x1 = x;
        break;
      case "pan": {
        const dx = x - d.x0;
        if (!d.moved && Math.abs(dx) < 4) return;
        d.moved = true;
        p.onViewChange({
          start: Math.max(0, d.start0 - dx / p.view.pxPerSec),
          pxPerSec: p.view.pxPerSec,
        });
        break;
      }
      default:
        break;
    }
  };

  const onPointerUp = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const d = dragRef.current;
    const p = propsRef.current;
    dragRef.current = { kind: "none" };
    dirtyRef.current = true;
    switch (d.kind) {
      case "drag":
        if (d.moved) p.onMoveSelected((x - d.x0) / p.view.pxPerSec, true);
        break;
      case "rect": {
        const t0 = tOf(Math.min(d.x0, d.x1));
        const t1 = tOf(Math.max(d.x0, d.x1));
        const ids = p.cues.filter((c) => c.pos_secs >= t0 && c.pos_secs <= t1).map((c) => c.id);
        p.onSelect(ids, d.additive);
        break;
      }
      case "pan":
        if (!d.moved) {
          p.onSeek(Math.max(0, tOf(x)));
          p.onSelect([], false);
        }
        break;
      default:
        break;
    }
  };

  const onDoubleClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    if (y < RULER_H || hitCue(x, y)) return;
    propsRef.current.onAddCueAt(Math.max(0, tOf(x)));
  };

  // Wheel: zoom around the cursor (vertical) / pan (horizontal or shift).
  useEffect(() => {
    const main = mainRef.current;
    if (!main) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const p = propsRef.current;
      const rect = main.getBoundingClientRect();
      const x = e.clientX - rect.left;
      const { w } = sizeRef.current;
      const dur = p.duration || (p.wave ? p.wave.duration_secs : 60);
      const minPps = Math.max(MIN_PX_PER_SEC, w / Math.max(1, dur));
      const panning = e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY);
      if (panning) {
        const delta = (e.shiftKey ? e.deltaY : e.deltaX) / p.view.pxPerSec;
        const maxStart = Math.max(0, dur - w / p.view.pxPerSec);
        p.onViewChange({
          start: Math.min(maxStart, Math.max(0, p.view.start + delta)),
          pxPerSec: p.view.pxPerSec,
        });
        return;
      }
      const factor = Math.exp(-e.deltaY * 0.0025);
      const next = Math.min(MAX_PX_PER_SEC, Math.max(minPps, p.view.pxPerSec * factor));
      const tCursor = p.view.start + x / p.view.pxPerSec;
      const start = Math.max(0, tCursor - x / next);
      p.onViewChange({ start, pxPerSec: next });
    };
    main.addEventListener("wheel", onWheel, { passive: false });
    return () => main.removeEventListener("wheel", onWheel);
  }, []);

  // Minimap: click/drag re-centres the view.
  const miniSeekView = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const p = propsRef.current;
    const dur = p.duration || (p.wave ? p.wave.duration_secs : 0);
    if (dur <= 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const { w } = sizeRef.current;
    const t = ((e.clientX - rect.left) / w) * dur;
    const visible = w / p.view.pxPerSec;
    const maxStart = Math.max(0, dur - visible);
    p.onViewChange({
      start: Math.min(maxStart, Math.max(0, t - visible / 2)),
      pxPerSec: p.view.pxPerSec,
    });
  };

  return (
    <div className="cue-timeline" ref={wrapRef}>
      <canvas
        ref={mainRef}
        className="cue-timeline__main"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerLeave={() => {
          hoverRef.current = null;
          dirtyRef.current = true;
        }}
        onDoubleClick={onDoubleClick}
      />
      <canvas
        ref={miniRef}
        className="cue-timeline__mini"
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          dragRef.current = { kind: "mini" };
          miniSeekView(e);
        }}
        onPointerMove={(e) => {
          if (dragRef.current.kind === "mini") miniSeekView(e);
        }}
        onPointerUp={() => {
          dragRef.current = { kind: "none" };
        }}
      />
    </div>
  );
}
