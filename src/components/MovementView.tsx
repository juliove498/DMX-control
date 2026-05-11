import type { Direction } from "@bindings/Direction";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import type { MovementGenerator } from "@bindings/MovementGenerator";
import type { MovementSlot } from "@bindings/MovementSlot";
import type { Shape } from "@bindings/Shape";
import type { SpreadMode } from "@bindings/SpreadMode";
import type { Subdivision } from "@bindings/Subdivision";
import type { TempoSource } from "@bindings/TempoSource";
import type { WaveFunction } from "@bindings/WaveFunction";
import type { Waveform } from "@bindings/Waveform";
import { useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../i18n";
import type { Translation } from "../i18n/translations";
import { movementColor } from "../lib/launchpadColors";
import { useShowStore } from "../stores/show";

const SUBDIVISIONS: Array<{ value: Subdivision; labelKey: keyof Translation }> = [
  { value: "quarter", labelKey: "movement.subdivLabel.16" },
  { value: "half", labelKey: "movement.subdivLabel.8" },
  { value: "one", labelKey: "movement.subdivLabel.1beat" },
  { value: "two", labelKey: "movement.subdivLabel.2beats" },
  { value: "four", labelKey: "movement.subdivLabel.4beats" },
];

const SHAPES: Array<{ value: Shape["type"]; labelKey: keyof Translation }> = [
  { value: "circle", labelKey: "movement.shapeLabel.circle" },
  { value: "polygon", labelKey: "movement.shapeLabel.polygon" },
  { value: "star", labelKey: "movement.shapeLabel.star" },
  { value: "figure_eight", labelKey: "movement.shapeLabel.figureEight" },
  { value: "line_horizontal", labelKey: "movement.shapeLabel.lineH" },
  { value: "line_vertical", labelKey: "movement.shapeLabel.lineV" },
  { value: "sine_combo", labelKey: "movement.shapeLabel.sineCombo" },
];

const SPREAD_MODES: Array<{ value: SpreadMode; labelKey: keyof Translation }> = [
  { value: "none", labelKey: "movement.spreadLabel.none" },
  { value: "even", labelKey: "movement.spreadLabel.even" },
  { value: "symmetric", labelKey: "movement.spreadLabel.symmetric" },
  { value: "pairs", labelKey: "movement.spreadLabel.pairs" },
  { value: "manual", labelKey: "movement.spreadLabel.manual" },
];

const DIRECTIONS: Array<{ value: Direction; labelKey: keyof Translation }> = [
  { value: "forward", labelKey: "movement.directionLabel.forward" },
  { value: "reverse", labelKey: "movement.directionLabel.reverse" },
  { value: "ping_pong", labelKey: "movement.directionLabel.pingPong" },
];

const WAVEFORMS: Array<{ value: Waveform; labelKey: keyof Translation }> = [
  { value: "sine", labelKey: "movement.waveformLabel.sine" },
  { value: "cosine", labelKey: "movement.waveformLabel.cosine" },
  { value: "triangle", labelKey: "movement.waveformLabel.triangle" },
  { value: "square", labelKey: "movement.waveformLabel.square" },
  { value: "sawtooth", labelKey: "movement.waveformLabel.sawtooth" },
  { value: "ramp_up", labelKey: "movement.waveformLabel.rampUp" },
  { value: "ramp_down", labelKey: "movement.waveformLabel.rampDown" },
];

/// Sensible defaults when the user picks a new shape from the dropdown.
function makeDefaultShape(type: Shape["type"]): Shape {
  switch (type) {
    case "circle":
      return { type: "circle" };
    case "polygon":
      return { type: "polygon", sides: 4 };
    case "star":
      return { type: "star", points: 5, inner_ratio: 0.4 };
    case "figure_eight":
      return { type: "figure_eight" };
    case "line_horizontal":
      return { type: "line_horizontal" };
    case "line_vertical":
      return { type: "line_vertical" };
    case "sine_combo":
      return {
        type: "sine_combo",
        pan: {
          waveform: "sine",
          frequency: 1,
          phase_shift: 0,
          amplitude: 1,
          offset: 0,
        },
        tilt: {
          waveform: "cosine",
          frequency: 1,
          phase_shift: 0,
          amplitude: 1,
          offset: 0,
        },
      };
  }
}

const SINE_PRESETS: Array<{ labelKey: keyof Translation; pan: WaveFunction; tilt: WaveFunction }> = [
  {
    labelKey: "movement.presetLabel.circle",
    pan: { waveform: "sine", frequency: 1, phase_shift: 0, amplitude: 1, offset: 0 },
    tilt: { waveform: "cosine", frequency: 1, phase_shift: 0, amplitude: 1, offset: 0 },
  },
  {
    labelKey: "movement.presetLabel.figureEight",
    pan: { waveform: "sine", frequency: 1, phase_shift: 0, amplitude: 1, offset: 0 },
    tilt: { waveform: "sine", frequency: 2, phase_shift: 0, amplitude: 1, offset: 0 },
  },
  {
    labelKey: "movement.presetLabel.lissajous32",
    pan: { waveform: "sine", frequency: 3, phase_shift: 0, amplitude: 1, offset: 0 },
    tilt: { waveform: "sine", frequency: 2, phase_shift: 0.25, amplitude: 1, offset: 0 },
  },
  {
    labelKey: "movement.presetLabel.lissajous54",
    pan: { waveform: "sine", frequency: 5, phase_shift: 0, amplitude: 1, offset: 0 },
    tilt: { waveform: "sine", frequency: 4, phase_shift: 0.125, amplitude: 1, offset: 0 },
  },
  {
    labelKey: "movement.presetLabel.wavePan",
    pan: { waveform: "sine", frequency: 2, phase_shift: 0, amplitude: 1, offset: 0 },
    tilt: { waveform: "sine", frequency: 0, phase_shift: 0, amplitude: 0, offset: 0 },
  },
];

const SUBDIVISION_BEATS: Record<Subdivision, number> = {
  quarter: 0.25,
  half: 0.5,
  one: 1,
  two: 2,
  four: 4,
};

function bpmOf(t: TempoSource): number {
  if (t.type === "fixed") return t.bpm;
  return 60;
}

const TAU = Math.PI * 2;

function evaluateShape(shape: Shape, phase: number): [number, number] {
  switch (shape.type) {
    case "circle": {
      const t = phase * TAU;
      return [Math.cos(t), Math.sin(t)];
    }
    case "polygon":
      return polygon(phase, Math.max(3, shape.sides));
    case "star":
      return star(
        phase,
        Math.max(3, shape.points),
        Math.max(0.05, Math.min(0.95, shape.inner_ratio)),
      );
    case "figure_eight": {
      const t = phase * TAU;
      return [Math.sin(t), Math.sin(2 * t)];
    }
    case "line_horizontal": {
      const x = 1 - 2 * Math.abs(2 * phase - 1);
      return [x, 0];
    }
    case "line_vertical": {
      const y = 1 - 2 * Math.abs(2 * phase - 1);
      return [0, y];
    }
    case "sine_combo":
      return [evaluateWave(shape.pan, phase), evaluateWave(shape.tilt, phase)];
  }
}

function polygon(phase: number, sides: number): [number, number] {
  const total = sides;
  const scaled = phase * total;
  const current = ((Math.floor(scaled) % sides) + sides) % sides;
  const local = scaled - Math.floor(scaled);
  const a = (current / total) * TAU;
  const b = ((current + 1) / total) * TAU;
  const xa = Math.cos(a);
  const ya = Math.sin(a);
  const xb = Math.cos(b);
  const yb = Math.sin(b);
  return [xa + (xb - xa) * local, ya + (yb - ya) * local];
}

function star(phase: number, points: number, innerRatio: number): [number, number] {
  const total = points * 2;
  const scaled = phase * total;
  const segment = ((Math.floor(scaled) % total) + total) % total;
  const local = scaled - Math.floor(scaled);
  const a = (segment / total) * TAU;
  const b = ((segment + 1) / total) * TAU;
  const ra = segment % 2 === 0 ? 1 : innerRatio;
  const rb = segment % 2 === 0 ? innerRatio : 1;
  const xa = Math.cos(a) * ra;
  const ya = Math.sin(a) * ra;
  const xb = Math.cos(b) * rb;
  const yb = Math.sin(b) * rb;
  return [xa + (xb - xa) * local, ya + (yb - ya) * local];
}

function evaluateWave(wave: WaveFunction, phase: number): number {
  const raw = phase * wave.frequency + wave.phase_shift;
  const t = ((raw % 1) + 1) % 1;
  let value: number;
  switch (wave.waveform) {
    case "sine":
      value = Math.sin(t * TAU);
      break;
    case "cosine":
      value = Math.cos(t * TAU);
      break;
    case "triangle":
      value = 1 - 2 * Math.abs(2 * t - 1);
      break;
    case "square":
      value = t < 0.5 ? 1 : -1;
      break;
    case "sawtooth":
    case "ramp_up":
      value = 2 * t - 1;
      break;
    case "ramp_down":
      value = 1 - 2 * t;
      break;
  }
  return value * wave.amplitude + wave.offset;
}

/// Mirror of `movement::pipeline::evaluate_slot` for the local preview.
/// Direction-aware via the global phase, which is computed elsewhere.
function evaluateSlot(
  globalPhase: number,
  slot: MovementSlot,
  gen: MovementGenerator,
): [number, number] {
  const phase = (((globalPhase + slot.phase_offset) % 1) + 1) % 1;
  const [shapeX, shapeY] = evaluateShape(gen.shape, phase);
  let x = shapeX;
  let y = shapeY;
  if (slot.invert_pan) x = -x;
  if (slot.invert_tilt) y = -y;
  const rad = (gen.rotation_deg * Math.PI) / 180;
  const cs = Math.cos(rad);
  const sn = Math.sin(rad);
  const rx = cs * x - sn * y;
  const ry = sn * x + cs * y;
  const sx = Math.max(-1, Math.min(1, rx * gen.size_x + gen.center_x));
  const sy = Math.max(-1, Math.min(1, ry * gen.size_y + gen.center_y));
  return [sx, sy];
}

/// Path of the shape across one full loop, for drawing the trail underneath
/// the live dots.
function shapePath(gen: MovementGenerator, samples = 96): [number, number][] {
  const out: [number, number][] = [];
  const probeSlot: MovementSlot = {
    fixture_id: "",
    phase_offset: 0,
    invert_pan: false,
    invert_tilt: false,
  };
  for (let i = 0; i <= samples; i++) {
    out.push(evaluateSlot(i / samples, probeSlot, gen));
  }
  return out;
}

function WaveEditor({
  axis,
  wave,
  onChange,
}: {
  axis: "pan" | "tilt";
  wave: WaveFunction;
  onChange: (next: WaveFunction) => void;
}) {
  const t = useT();
  return (
    <div className="wave-editor">
      <strong>{axis === "pan" ? t("movement.wave.pan") : t("movement.wave.tilt")}</strong>
      <label>
        {t("movement.wave.waveform")}
        <select
          value={wave.waveform}
          onChange={(e) => onChange({ ...wave, waveform: e.currentTarget.value as Waveform })}
        >
          {WAVEFORMS.map((w) => (
            <option key={w.value} value={w.value}>
              {t(w.labelKey)}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("movement.wave.frequency")}
        <input
          type="range"
          min={0}
          max={8}
          step={0.25}
          value={wave.frequency}
          onChange={(e) => onChange({ ...wave, frequency: Number(e.currentTarget.value) })}
        />
        <span className="val">{wave.frequency.toFixed(2)}×</span>
      </label>
      <label>
        {t("movement.wave.phaseShift")}
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={wave.phase_shift}
          onChange={(e) => onChange({ ...wave, phase_shift: Number(e.currentTarget.value) })}
        />
        <span className="val">{wave.phase_shift.toFixed(2)}</span>
      </label>
      <label>
        {t("movement.wave.amplitude")}
        <input
          type="range"
          min={0}
          max={1}
          step={0.01}
          value={wave.amplitude}
          onChange={(e) => onChange({ ...wave, amplitude: Number(e.currentTarget.value) })}
        />
        <span className="val">{wave.amplitude.toFixed(2)}</span>
      </label>
      <label>
        {t("movement.wave.offset")}
        <input
          type="range"
          min={-1}
          max={1}
          step={0.01}
          value={wave.offset}
          onChange={(e) => onChange({ ...wave, offset: Number(e.currentTarget.value) })}
        />
        <span className="val">{wave.offset.toFixed(2)}</span>
      </label>
    </div>
  );
}

function SineComboEditor({
  pan,
  tilt,
  onChange,
}: {
  pan: WaveFunction;
  tilt: WaveFunction;
  onChange: (pan: WaveFunction, tilt: WaveFunction) => void;
}) {
  const t = useT();
  return (
    <div className="sine-combo-editor">
      <div className="sine-presets">
        <span className="sine-presets-label">{t("movement.preset.label")}</span>
        {SINE_PRESETS.map((p) => {
          const label = t(p.labelKey);
          return (
            <button
              key={p.labelKey}
              type="button"
              onClick={() => onChange(p.pan, p.tilt)}
              title={t("movement.preset.loadHint", { name: label })}
            >
              {label}
            </button>
          );
        })}
      </div>
      <div className="sine-combo-grid">
        <WaveEditor axis="pan" wave={pan} onChange={(next) => onChange(next, tilt)} />
        <WaveEditor axis="tilt" wave={tilt} onChange={(next) => onChange(pan, next)} />
      </div>
    </div>
  );
}

type DragMode = null | { kind: "center" } | { kind: "corner" } | { kind: "rotate" };

function Preview({
  gen,
  fixtures,
  onCommit,
}: {
  gen: MovementGenerator;
  fixtures: FixtureInstance[];
  onCommit: (gen: MovementGenerator) => void;
}) {
  const t = useT();
  const total = gen.fixtures.length;
  const bpm = bpmOf(gen.tempo);
  const beatsPerLoop = SUBDIVISION_BEATS[gen.subdivision];
  const cyclesPerSec = bpm / 60 / beatsPerLoop;

  const svgRef = useRef<SVGSVGElement | null>(null);
  // Local override so dragging shows continuous feedback without firing
  // an IPC roundtrip per pointermove. Persisted to the backend on
  // pointerup. While `draft` is set, we render from it and treat `gen`
  // as the "starting point" we'll snap back to if the gesture is
  // cancelled.
  const [draft, setDraft] = useState<MovementGenerator | null>(null);
  const [drag, setDrag] = useState<DragMode>(null);
  const display = draft ?? gen;

  const [phase, setPhase] = useState(0);
  const lastTickRef = useRef<number | null>(null);
  // Local sign for ping-pong reflection. We mirror the engine's
  // `direction_sign` rather than recompute on every frame so brief BPM
  // bumps don't accidentally flip the bounce.
  const signRef = useRef(1);
  useEffect(() => {
    // When the module is OFF the engine doesn't write DMX. Animating the
    // preview anyway makes the user think something is happening on the
    // rig — confusing. Freeze at phase 0 and let the static dots show
    // where each fixture *would* land at the start of the loop.
    if (!gen.enabled) {
      setPhase(0);
      signRef.current = 1;
      return;
    }
    let raf = 0;
    const step = (now: number) => {
      const last = lastTickRef.current ?? now;
      const delta = (now - last) / 1000;
      lastTickRef.current = now;
      setPhase((p) => {
        if (gen.direction === "ping_pong") {
          let next = p + signRef.current * delta * cyclesPerSec;
          if (next > 1) {
            next = 2 - next;
            signRef.current = -1;
          } else if (next < 0) {
            next = -next;
            signRef.current = 1;
          }
          return Math.max(0, Math.min(1, next));
        }
        const dir = gen.direction === "reverse" ? -1 : 1;
        signRef.current = dir;
        const next = p + dir * delta * cyclesPerSec;
        return ((next % 1) + 1) % 1;
      });
      raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => {
      cancelAnimationFrame(raf);
      lastTickRef.current = null;
    };
  }, [cyclesPerSec, gen.direction, gen.enabled]);

  const path = useMemo(() => shapePath(display), [display]);
  const positions = useMemo(
    () => display.fixtures.map((slot) => evaluateSlot(phase, slot, display)),
    [display, phase],
  );

  // Draw with SVG. viewBox -1.1..1.1 leaves 5% padding around the unit box.
  const padded = 1.1;
  const toView = ([x, y]: [number, number]) => [x, -y]; // flip y to feel like stage

  // Convert a pointer event into the SVG's viewBox coordinate space, then
  // un-flip Y so the result is in the same unit-space the model uses
  // (positive Y up). Returns null only if the SVG isn't laid out yet.
  const svgPoint = (e: React.PointerEvent): { x: number; y: number } | null => {
    const svg = svgRef.current;
    if (!svg) return null;
    const pt = svg.createSVGPoint();
    pt.x = e.clientX;
    pt.y = e.clientY;
    const ctm = svg.getScreenCTM();
    if (!ctm) return null;
    const inv = ctm.inverse();
    const local = pt.matrixTransform(inv);
    return { x: local.x, y: -local.y };
  };

  const clamp01 = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

  // Rotate (x, y) by -rotation_deg so the resulting components are along
  // the bounding box's local axes, regardless of how the user has spun
  // the figure.
  const rotateInverse = (x: number, y: number, deg: number): [number, number] => {
    const rad = (-deg * Math.PI) / 180;
    const cos = Math.cos(rad);
    const sin = Math.sin(rad);
    return [x * cos - y * sin, x * sin + y * cos];
  };

  const startDrag = (mode: NonNullable<DragMode>) => (e: React.PointerEvent) => {
    e.stopPropagation();
    e.preventDefault();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    setDrag(mode);
    setDraft(display);
  };

  const onPointerMove = (e: React.PointerEvent) => {
    if (!drag || !draft) return;
    const p = svgPoint(e);
    if (!p) return;
    if (drag.kind === "center") {
      setDraft({
        ...draft,
        center_x: clamp01(p.x, -1, 1),
        center_y: clamp01(p.y, -1, 1),
      });
    } else if (drag.kind === "corner") {
      // Translate into the bounding box's local frame (rotation undone).
      // Each axis size is the absolute distance from the centre — that
      // matches how the engine applies size_x / size_y as half-widths.
      const dx = p.x - draft.center_x;
      const dy = p.y - draft.center_y;
      const [lx, ly] = rotateInverse(dx, dy, draft.rotation_deg);
      setDraft({
        ...draft,
        size_x: clamp01(Math.abs(lx), 0, 1),
        size_y: clamp01(Math.abs(ly), 0, 1),
      });
    } else if (drag.kind === "rotate") {
      const dx = p.x - draft.center_x;
      const dy = p.y - draft.center_y;
      // atan2 returns radians from +X CCW; convert to degrees and
      // normalise into [0, 360).
      let deg = (Math.atan2(dy, dx) * 180) / Math.PI;
      deg = ((deg % 360) + 360) % 360;
      setDraft({ ...draft, rotation_deg: deg });
    }
  };

  const onPointerUp = (e: React.PointerEvent) => {
    if (drag && draft) {
      onCommit(draft);
    }
    setDrag(null);
    setDraft(null);
    if ((e.currentTarget as Element).hasPointerCapture(e.pointerId)) {
      (e.currentTarget as Element).releasePointerCapture(e.pointerId);
    }
  };

  // Bounding-box corner positions in unit space, with rotation applied.
  // Order is [TL, TR, BR, BL] for the polygon outline; we draw a draggable
  // square on each.
  const cx = display.center_x;
  const cy = display.center_y;
  const sx = Math.max(display.size_x, 0.02);
  const sy = Math.max(display.size_y, 0.02);
  const rad = (display.rotation_deg * Math.PI) / 180;
  const cos = Math.cos(rad);
  const sin = Math.sin(rad);
  const rotPoint = (x: number, y: number): [number, number] => [
    cx + x * cos - y * sin,
    cy + x * sin + y * cos,
  ];
  const corners: [number, number][] = [
    rotPoint(-sx, sy),
    rotPoint(sx, sy),
    rotPoint(sx, -sy),
    rotPoint(-sx, -sy),
  ];
  // Rotation handle: sits on the +X axis of the local frame, just outside
  // the bounding box, so dragging it traces an arc that maps cleanly to
  // the rotation angle.
  const rotateHandleDist = Math.hypot(sx, sy) + 0.18;
  const rotHandle = rotPoint(rotateHandleDist, 0);

  const cornerSize = 0.05;

  return (
    <div className={`movement-preview${gen.enabled ? "" : " off"}`} data-doc="movement-preview">
      {!gen.enabled ? (
        <div className="movement-preview-badge">{t("movement.previewBadgeOff")}</div>
      ) : null}
      <svg
        ref={svgRef}
        viewBox={`-${padded} -${padded} ${padded * 2} ${padded * 2}`}
        role="img"
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        style={{ touchAction: "none" }}
      >
        <title>Movement preview</title>
        <rect x={-1} y={-1} width={2} height={2} fill="none" stroke="#2a2a2a" strokeWidth={0.01} />
        <line x1={-padded} y1={0} x2={padded} y2={0} stroke="#1f1f1f" strokeWidth={0.005} />
        <line x1={0} y1={-padded} x2={0} y2={padded} stroke="#1f1f1f" strokeWidth={0.005} />
        {/* Shape trail */}
        <polyline
          points={path
            .map((p) => {
              const [vx, vy] = toView(p);
              return `${vx.toFixed(4)},${vy.toFixed(4)}`;
            })
            .join(" ")}
          fill="none"
          stroke="#3a3a3a"
          strokeWidth={0.012}
        />
        {/* Bounding box outline + handles. Drawn before the dots so the
            live fixtures stay on top and remain readable while editing. */}
        <polygon
          points={corners
            .map(([x, y]) => {
              const [vx, vy] = toView([x, y]);
              return `${vx.toFixed(4)},${vy.toFixed(4)}`;
            })
            .join(" ")}
          fill="none"
          stroke="#5a5a5a"
          strokeWidth={0.006}
          strokeDasharray="0.02 0.02"
          pointerEvents="none"
        />
        {/* Rotation handle: line from centre to handle, then a ring. */}
        <line
          x1={cx}
          y1={-cy}
          x2={rotHandle[0]}
          y2={-rotHandle[1]}
          stroke="#5a5a5a"
          strokeWidth={0.005}
          strokeDasharray="0.02 0.02"
          pointerEvents="none"
        />
        <circle
          cx={rotHandle[0]}
          cy={-rotHandle[1]}
          r={0.045}
          fill="#1a1a1a"
          stroke="#7a7a7a"
          strokeWidth={0.008}
          style={{ cursor: "grab" }}
          onPointerDown={startDrag({ kind: "rotate" })}
        />
        {/* Corner handles (resize). All four behave identically — drag
            anywhere along the rotated bounding box and the size scales
            symmetrically per axis. */}
        {(["tl", "tr", "br", "bl"] as const).map((label, i) => {
          const [x, y] = corners[i];
          const [vx, vy] = toView([x, y]);
          return (
            <rect
              key={`corner-${label}`}
              x={vx - cornerSize / 2}
              y={vy - cornerSize / 2}
              width={cornerSize}
              height={cornerSize}
              fill="#f5d896"
              stroke="#000"
              strokeWidth={0.005}
              style={{ cursor: "nwse-resize" }}
              onPointerDown={startDrag({ kind: "corner" })}
            />
          );
        })}
        {/* Centre handle. Bigger transparent hit-target wraps a smaller
            visible dot so you can grab it without surgical precision. */}
        <circle
          cx={cx}
          cy={-cy}
          r={0.07}
          fill="transparent"
          style={{ cursor: "move" }}
          onPointerDown={startDrag({ kind: "center" })}
        />
        <circle
          cx={cx}
          cy={-cy}
          r={0.025}
          fill="#f5d896"
          stroke="#000"
          strokeWidth={0.005}
          pointerEvents="none"
        />
        {/* Live dots */}
        {positions.map((p, i) => {
          const [vx, vy] = toView(p);
          const fixture = fixtures.find((f) => f.id === display.fixtures[i].fixture_id);
          return (
            <g key={`${display.fixtures[i].fixture_id}-${i}`} pointerEvents="none">
              <circle cx={vx} cy={vy} r={0.06} fill="#f5d896" opacity={0.9} />
              <text
                x={vx}
                y={vy + 0.005}
                textAnchor="middle"
                dominantBaseline="middle"
                fontSize={0.07}
                fill="#181818"
                fontWeight={700}
              >
                {fixture?.label ?? display.fixtures[i].fixture_id.slice(0, 3)}
              </text>
            </g>
          );
        })}
      </svg>
      <div className="movement-preview-meta">
        {gen.enabled
          ? t("movement.previewMeta", { count: total, phase: (phase * 100).toFixed(0) })
          : t("movement.previewPaused", { count: total })}
        {drag ? ` · ${drag.kind}` : ""}
      </div>
    </div>
  );
}

export function MovementView() {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const library = useShowStore((s) => s.library);
  const updateMovement = useShowStore((s) => s.updateMovement);
  const toggleMovement = useShowStore((s) => s.toggleMovement);
  const createMovement = useShowStore((s) => s.createMovement);
  const deleteMovement = useShowStore((s) => s.deleteMovement);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // Default selection: first movement (if any). When the list grows or
  // shrinks, fall back to the first available; when the list is empty,
  // clear so the editor renders the empty state.
  const movements = show?.movements ?? [];
  useEffect(() => {
    if (movements.length === 0) {
      if (selectedId !== null) setSelectedId(null);
      return;
    }
    if (!selectedId || !movements.some((m) => m.id === selectedId)) {
      setSelectedId(movements[0].id);
    }
  }, [movements, selectedId]);

  if (!show) return <main className="page">{t("common.loading")}</main>;
  const fixtures = show.fixtures;
  const selectedMovement = movements.find((m) => m.id === selectedId) ?? null;

  if (movements.length === 0) {
    return (
      <main className="page movement-view">
        <header className="page-head">
          <h2>{t("movement.title")}</h2>
          <div className="actions">
            <button
              type="button"
              onClick={() => {
                createMovement().then((m) => setSelectedId(m.id));
              }}
            >
              {t("movement.addNew")}
            </button>
          </div>
        </header>
        <p className="empty">{t("movement.empty")}</p>
      </main>
    );
  }

  if (!selectedMovement) {
    return <main className="page">{t("common.loading")}</main>;
  }

  const gen = selectedMovement;

  // Filter the "Add fixture" dropdown to only fixtures whose mode actually
  // has pan or tilt channels. The engine silently drops slots without those
  // roles, so showing them in the dropdown is misleading: the user picks one
  // and nothing happens.
  const libraryById: Record<string, (typeof library)[number]> = {};
  for (const d of library) libraryById[d.id] = d;
  const fixtureHasPanOrTilt = (fixtureId: string) => {
    const f = fixtures.find((x) => x.id === fixtureId);
    if (!f) return false;
    const mode = libraryById[f.definition_id]?.modes[f.mode_index];
    if (!mode) return false;
    return mode.channels.some((c) => c.role === "pan" || c.role === "tilt");
  };
  const assignedIds = new Set(gen.fixtures.map((s) => s.fixture_id));
  const available = fixtures
    .filter((f) => !assignedIds.has(f.id))
    .filter((f) => fixtureHasPanOrTilt(f.id));

  const setBpm = (bpm: number) => updateMovement({ ...gen, tempo: { type: "fixed", bpm } });
  const setSubdivision = (subdivision: Subdivision) => updateMovement({ ...gen, subdivision });
  const setSize = (axis: "x" | "y", value: number) =>
    updateMovement({
      ...gen,
      size_x: axis === "x" ? value : gen.size_x,
      size_y: axis === "y" ? value : gen.size_y,
    });
  const setCenter = (axis: "x" | "y", value: number) =>
    updateMovement({
      ...gen,
      center_x: axis === "x" ? value : gen.center_x,
      center_y: axis === "y" ? value : gen.center_y,
    });
  const setRotation = (deg: number) => updateMovement({ ...gen, rotation_deg: deg });
  const setShape = (shape: Shape) => updateMovement({ ...gen, shape });
  const setShapeType = (type: Shape["type"]) =>
    updateMovement({ ...gen, shape: makeDefaultShape(type) });
  const setSpreadMode = (spread_mode: SpreadMode) => updateMovement({ ...gen, spread_mode });
  const setDirection = (direction: Direction) => updateMovement({ ...gen, direction });
  const addSlot = (fixtureId: string) => {
    if (!fixtureId) return;
    updateMovement({
      ...gen,
      fixtures: [
        ...gen.fixtures,
        {
          fixture_id: fixtureId,
          phase_offset: 0,
          invert_pan: false,
          invert_tilt: false,
        },
      ],
    });
  };
  const removeSlot = (idx: number) =>
    updateMovement({ ...gen, fixtures: gen.fixtures.filter((_, i) => i !== idx) });
  const setSlot = (idx: number, patch: Partial<MovementSlot>) =>
    updateMovement({
      ...gen,
      fixtures: gen.fixtures.map((s, i) => (i === idx ? { ...s, ...patch } : s)),
    });

  const selectedIndex = movements.findIndex((m) => m.id === gen.id);
  const padColor = selectedIndex >= 0 ? movementColor(selectedIndex) : "#888";
  const showsOnLaunchpad = selectedIndex >= 0 && selectedIndex < 8;

  return (
    <main className="page movement-view">
      <header className="page-head">
        <h2>{t("movement.title")}</h2>
        <div className="actions">
          <button
            type="button"
            onClick={() => {
              createMovement().then((m) => setSelectedId(m.id));
            }}
          >
            {t("movement.addNew")}
          </button>
          <button
            type="button"
            className="danger"
            onClick={() => {
              if (window.confirm(t("movement.deleteConfirm", { name: gen.name }))) {
                deleteMovement(gen.id);
              }
            }}
          >
            {t("movement.delete")}
          </button>
        </div>
      </header>

      <div className="movement-layout">
        <aside className="movement-sidebar" aria-label="Movement list" data-doc="movement-list">
          {movements.map((m, i) => {
            const color = movementColor(i);
            const onLp = i < 8;
            return (
              <button
                key={m.id}
                type="button"
                className={`movement-list-item${m.id === gen.id ? " selected" : ""}${m.enabled ? " on" : ""}`}
                onClick={() => setSelectedId(m.id)}
                style={
                  m.enabled
                    ? { borderLeft: `4px solid ${color}` }
                    : { borderLeft: "4px solid transparent" }
                }
              >
                {onLp ? (
                  <span
                    className="lp-pad-badge"
                    style={{ background: color }}
                    title={t("movement.lpHint", { pad: i + 1 })}
                    aria-hidden="true"
                  />
                ) : null}
                <span className="movement-list-name">{m.name}</span>
                <span className="movement-list-meta">{m.fixtures.length}</span>
              </button>
            );
          })}
        </aside>

        <section className="movement-editor" data-doc="movement-editor">
          <div className="movement-editor-head" data-doc="movement-head">
            <button
              type="button"
              className={`chaser-toggle${gen.enabled ? " on" : ""}`}
              onClick={() => toggleMovement(gen.id, !gen.enabled)}
              style={
                gen.enabled
                  ? { background: padColor, borderColor: padColor, color: "#0f0f10" }
                  : { borderColor: padColor, color: padColor }
              }
            >
              {gen.enabled ? t("chaser.toggle.on") : t("chaser.toggle.off")}
            </button>
            {showsOnLaunchpad ? (
              <span
                className="lp-pad-badge"
                style={{ background: padColor }}
                title={t("movement.lpHint", { pad: selectedIndex + 1 })}
                aria-hidden="true"
              />
            ) : null}
            <input
              className="chaser-name"
              value={gen.name}
              onChange={(e) => updateMovement({ ...gen, name: e.currentTarget.value })}
            />
          </div>

          <div className="movement-grid">
            <section className="movement-column" data-doc="movement-params">
              <fieldset className="chaser-fieldset">
                <legend>{t("movement.legend.fixtures", { count: gen.fixtures.length })}</legend>
                {gen.fixtures.length === 0 ? (
                  <p className="empty">{t("movement.fixtures.empty")}</p>
                ) : (
                  <div className="movement-slots">
                    {gen.fixtures.map((slot, i) => {
                      const f = fixtures.find((x) => x.id === slot.fixture_id);
                      return (
                        <div key={`${slot.fixture_id}-${i}`} className="movement-slot-row">
                          <span className="movement-slot-name">{f?.label ?? slot.fixture_id}</span>
                          <label>
                            <input
                              type="checkbox"
                              checked={slot.invert_pan}
                              onChange={(e) => setSlot(i, { invert_pan: e.currentTarget.checked })}
                            />
                            {t("movement.fixtures.invertPan")}
                          </label>
                          <label>
                            <input
                              type="checkbox"
                              checked={slot.invert_tilt}
                              onChange={(e) => setSlot(i, { invert_tilt: e.currentTarget.checked })}
                            />
                            {t("movement.fixtures.invertTilt")}
                          </label>
                          <button type="button" className="danger" onClick={() => removeSlot(i)}>
                            {t("chaser.del")}
                          </button>
                        </div>
                      );
                    })}
                  </div>
                )}
                <select
                  value=""
                  onChange={(e) => {
                    addSlot(e.currentTarget.value);
                    e.currentTarget.value = "";
                  }}
                >
                  <option value="">{t("movement.fixtures.add")}</option>
                  {available.map((f) => (
                    <option key={f.id} value={f.id}>
                      {t("movement.fixtures.fixtureFmt", {
                        label: f.label ?? f.id,
                        universe: f.universe,
                        address: f.address,
                      })}
                    </option>
                  ))}
                </select>
              </fieldset>

              <fieldset className="chaser-fieldset" data-doc="movement-shape">
                <legend>{t("movement.legend.shape")}</legend>
                <select
                  value={gen.shape.type}
                  onChange={(e) => setShapeType(e.currentTarget.value as Shape["type"])}
                >
                  {SHAPES.map((s) => (
                    <option key={s.value} value={s.value}>
                      {t(s.labelKey)}
                    </option>
                  ))}
                </select>
                {gen.shape.type === "polygon" ? (
                  <label>
                    {t("movement.shape.sides")}
                    <input
                      type="number"
                      min={3}
                      max={16}
                      value={gen.shape.sides}
                      onChange={(e) =>
                        setShape({
                          type: "polygon",
                          sides: Math.max(3, Number(e.currentTarget.value)),
                        })
                      }
                    />
                  </label>
                ) : null}
                {gen.shape.type === "star" ? (
                  <>
                    <label>
                      {t("movement.shape.points")}
                      <input
                        type="number"
                        min={3}
                        max={16}
                        value={gen.shape.points}
                        onChange={(e) =>
                          setShape({
                            type: "star",
                            points: Math.max(3, Number(e.currentTarget.value)),
                            inner_ratio: gen.shape.type === "star" ? gen.shape.inner_ratio : 0.4,
                          })
                        }
                      />
                    </label>
                    <label>
                      {t("movement.shape.innerRatio")}
                      <input
                        type="range"
                        min={0.05}
                        max={0.95}
                        step={0.01}
                        value={gen.shape.inner_ratio}
                        onChange={(e) =>
                          setShape({
                            type: "star",
                            points: gen.shape.type === "star" ? gen.shape.points : 5,
                            inner_ratio: Number(e.currentTarget.value),
                          })
                        }
                      />
                      <span className="val">{gen.shape.inner_ratio.toFixed(2)}</span>
                    </label>
                  </>
                ) : null}
                {gen.shape.type === "sine_combo" ? (
                  <SineComboEditor
                    pan={gen.shape.pan}
                    tilt={gen.shape.tilt}
                    onChange={(pan, tilt) => setShape({ type: "sine_combo", pan, tilt })}
                  />
                ) : null}
              </fieldset>

              <fieldset className="chaser-fieldset" data-doc="movement-canon">
                <legend>{t("movement.legend.canon")}</legend>
                <select
                  value={gen.spread_mode}
                  onChange={(e) => setSpreadMode(e.currentTarget.value as SpreadMode)}
                >
                  {SPREAD_MODES.map((m) => (
                    <option key={m.value} value={m.value}>
                      {t(m.labelKey)}
                    </option>
                  ))}
                </select>
                {gen.spread_mode === "manual" && gen.fixtures.length > 0 ? (
                  <div className="movement-manual-offsets">
                    {gen.fixtures.map((slot, i) => {
                      const f = fixtures.find((x) => x.id === slot.fixture_id);
                      return (
                        <label
                          key={`${slot.fixture_id}-${i}-offset`}
                          className="movement-manual-offset"
                        >
                          <span>{f?.label ?? slot.fixture_id}</span>
                          <input
                            type="number"
                            min={0}
                            max={1}
                            step={0.01}
                            value={slot.phase_offset.toFixed(3)}
                            onChange={(e) =>
                              setSlot(i, {
                                phase_offset: Math.max(
                                  0,
                                  Math.min(1, Number(e.currentTarget.value)),
                                ),
                              })
                            }
                          />
                        </label>
                      );
                    })}
                  </div>
                ) : null}
              </fieldset>

              <fieldset className="chaser-fieldset" data-doc="movement-timing">
                <legend>{t("movement.legend.timing")}</legend>
                <label>
                  {t("movement.timing.bpm")}
                  <input
                    type="number"
                    min={20}
                    max={400}
                    step={0.5}
                    value={bpmOf(gen.tempo)}
                    onChange={(e) => setBpm(Number(e.currentTarget.value))}
                  />
                </label>
                <label>
                  {t("movement.timing.loopLength")}
                  <select
                    value={gen.subdivision}
                    onChange={(e) => setSubdivision(e.currentTarget.value as Subdivision)}
                  >
                    {SUBDIVISIONS.map((s) => (
                      <option key={s.value} value={s.value}>
                        {t(s.labelKey)}
                      </option>
                    ))}
                  </select>
                </label>
                <div className="movement-direction">
                  {DIRECTIONS.map((d) => (
                    <button
                      key={d.value}
                      type="button"
                      className={`movement-direction-btn${gen.direction === d.value ? " active" : ""}`}
                      onClick={() => setDirection(d.value)}
                    >
                      {t(d.labelKey)}
                    </button>
                  ))}
                </div>
              </fieldset>

              <fieldset className="chaser-fieldset">
                <legend>{t("movement.legend.transform")}</legend>
                <label>
                  {t("movement.transform.sizeX")}
                  <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={gen.size_x}
                    onChange={(e) => setSize("x", Number(e.currentTarget.value))}
                  />
                  <span className="val">{gen.size_x.toFixed(2)}</span>
                </label>
                <label>
                  {t("movement.transform.sizeY")}
                  <input
                    type="range"
                    min={0}
                    max={1}
                    step={0.01}
                    value={gen.size_y}
                    onChange={(e) => setSize("y", Number(e.currentTarget.value))}
                  />
                  <span className="val">{gen.size_y.toFixed(2)}</span>
                </label>
                <label>
                  {t("movement.transform.centerX")}
                  <input
                    type="range"
                    min={-1}
                    max={1}
                    step={0.01}
                    value={gen.center_x}
                    onChange={(e) => setCenter("x", Number(e.currentTarget.value))}
                  />
                  <span className="val">{gen.center_x.toFixed(2)}</span>
                </label>
                <label>
                  {t("movement.transform.centerY")}
                  <input
                    type="range"
                    min={-1}
                    max={1}
                    step={0.01}
                    value={gen.center_y}
                    onChange={(e) => setCenter("y", Number(e.currentTarget.value))}
                  />
                  <span className="val">{gen.center_y.toFixed(2)}</span>
                </label>
                <label>
                  {t("movement.transform.rotation")}
                  <input
                    type="range"
                    min={0}
                    max={360}
                    step={1}
                    value={gen.rotation_deg}
                    onChange={(e) => setRotation(Number(e.currentTarget.value))}
                  />
                  <span className="val">{gen.rotation_deg.toFixed(0)}°</span>
                </label>
                <button
                  type="button"
                  onClick={() =>
                    updateMovement({
                      ...gen,
                      size_x: 1,
                      size_y: 1,
                      center_x: 0,
                      center_y: 0,
                      rotation_deg: 0,
                    })
                  }
                >
                  {t("movement.transform.reset")}
                </button>
              </fieldset>
            </section>

            <section className="movement-column">
              <Preview gen={gen} fixtures={fixtures} onCommit={updateMovement} />
              <p className="hint">{t("movement.previewHint")}</p>
            </section>
          </div>
        </section>
      </div>
    </main>
  );
}
