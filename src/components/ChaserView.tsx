import type { AmbientChaser } from "@bindings/AmbientChaser";
import type { Cadence } from "@bindings/Cadence";
import type { ChaserSlot } from "@bindings/ChaserSlot";
import type { ColorMode } from "@bindings/ColorMode";
import type { FadeCurve } from "@bindings/FadeCurve";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import type { PaletteRotation } from "@bindings/PaletteRotation";
import type { Pattern } from "@bindings/Pattern";
import type { Rgb } from "@bindings/Rgb";
import type { Subdivision } from "@bindings/Subdivision";
import type { TempoSource } from "@bindings/TempoSource";
import { useEffect, useState } from "react";
import { useT } from "../i18n";
import type { Translation } from "../i18n/translations";
import { chaserColor } from "../lib/launchpadColors";
import { useShowStore } from "../stores/show";

const FADE_CURVES: Array<{ value: FadeCurve; labelKey: keyof Translation }> = [
  { value: "linear", labelKey: "chaser.fadeCurveLabel.linear" },
  { value: "ease_in_out", labelKey: "chaser.fadeCurveLabel.easeInOut" },
  { value: "ease_in", labelKey: "chaser.fadeCurveLabel.easeIn" },
  { value: "ease_out", labelKey: "chaser.fadeCurveLabel.easeOut" },
  { value: "exponential", labelKey: "chaser.fadeCurveLabel.exponential" },
  { value: "logarithmic", labelKey: "chaser.fadeCurveLabel.logarithmic" },
];

/// Map a backend FadeCurve to a CSS timing function so the preview's CSS
/// transition mirrors the engine's interpolation curve as closely as
/// browsers allow.
function cssCurveFor(curve: FadeCurve): string {
  switch (curve) {
    case "linear":
      return "linear";
    case "ease_in_out":
      return "ease-in-out";
    case "ease_in":
      return "ease-in";
    case "ease_out":
      return "ease-out";
    case "exponential":
      return "cubic-bezier(0.95, 0.05, 0.795, 0.035)";
    case "logarithmic":
      return "cubic-bezier(0.05, 0.95, 0.205, 0.965)";
  }
}

const SUBDIVISIONS: Array<{ value: Subdivision; labelKey: keyof Translation }> = [
  { value: "quarter", labelKey: "chaser.subdivisionLabel.16" },
  { value: "half", labelKey: "chaser.subdivisionLabel.8" },
  { value: "one", labelKey: "chaser.subdivisionLabel.4" },
  { value: "two", labelKey: "chaser.subdivisionLabel.2" },
  { value: "four", labelKey: "chaser.subdivisionLabel.1" },
];

const PATTERNS: Array<{ value: Pattern["type"]; labelKey: keyof Translation }> = [
  { value: "all_together", labelKey: "chaser.patternLabel.allTogether" },
  { value: "alternate", labelKey: "chaser.patternLabel.alternate" },
  { value: "chase", labelKey: "chaser.patternLabel.chase" },
  { value: "chase_reverse", labelKey: "chaser.patternLabel.chaseReverse" },
  { value: "ping_pong", labelKey: "chaser.patternLabel.pingPong" },
  { value: "wave", labelKey: "chaser.patternLabel.wave" },
  { value: "wave_reverse", labelKey: "chaser.patternLabel.waveReverse" },
  { value: "build", labelKey: "chaser.patternLabel.build" },
  { value: "build_reverse", labelKey: "chaser.patternLabel.buildReverse" },
  { value: "center_out", labelKey: "chaser.patternLabel.centerOut" },
  { value: "outside_in", labelKey: "chaser.patternLabel.outsideIn" },
  { value: "pulse_out", labelKey: "chaser.patternLabel.pulseOut" },
  { value: "pulse_in", labelKey: "chaser.patternLabel.pulseIn" },
  { value: "accordion", labelKey: "chaser.patternLabel.accordion" },
  { value: "bowtie", labelKey: "chaser.patternLabel.bowtie" },
  { value: "symmetric", labelKey: "chaser.patternLabel.symmetric" },
  { value: "symmetric_bounce", labelKey: "chaser.patternLabel.symmetricBounce" },
  { value: "dual_chase", labelKey: "chaser.patternLabel.dualChase" },
  { value: "inverted_chase", labelKey: "chaser.patternLabel.invertedChase" },
  { value: "groups_of_two", labelKey: "chaser.patternLabel.groupsOfTwo" },
  { value: "groups_of_three", labelKey: "chaser.patternLabel.groupsOfThree" },
  { value: "half_swap", labelKey: "chaser.patternLabel.halfSwap" },
  { value: "edges", labelKey: "chaser.patternLabel.edges" },
  { value: "random", labelKey: "chaser.patternLabel.random" },
];

const CADENCES: Array<{ value: Cadence["type"]; labelKey: keyof Translation }> = [
  { value: "every_step", labelKey: "chaser.cadenceLabel.everyStep" },
  { value: "every_n_steps", labelKey: "chaser.cadenceLabel.everyN" },
  { value: "per_slot", labelKey: "chaser.cadenceLabel.perSlot" },
  { value: "alternate_slots", labelKey: "chaser.cadenceLabel.alternateSlots" },
  { value: "chase_per_color", labelKey: "chaser.cadenceLabel.chasePerColor" },
];

const PALETTE_ROTATIONS: Array<{ value: PaletteRotation; labelKey: keyof Translation }> = [
  { value: "per_step", labelKey: "chaser.rotationLabel.perStep" },
  { value: "per_cycle", labelKey: "chaser.rotationLabel.perCycle" },
  { value: "per_slot", labelKey: "chaser.rotationLabel.perSlot" },
];

const COLOR_MODE_TYPES: Array<{ value: ColorMode["type"]; labelKey: keyof Translation }> = [
  { value: "disabled", labelKey: "chaser.colorModeLabel.disabled" },
  { value: "single", labelKey: "chaser.colorModeLabel.single" },
  { value: "two_color_cadence", labelKey: "chaser.colorModeLabel.twoColor" },
  { value: "palette", labelKey: "chaser.colorModeLabel.palette" },
  { value: "rainbow", labelKey: "chaser.colorModeLabel.rainbow" },
];

function bpmOf(t: TempoSource): number {
  // Sub-fase A only ships Fixed; the discriminator keeps later additions
  // (TapTempo / SoundReactive / GlobalTempo) honest.
  if (t.type === "fixed") return t.bpm;
  return 120;
}

function fixtureLabel(f: FixtureInstance): string {
  return f.label ?? f.id;
}

const SUBDIVISION_BEATS: Record<Subdivision, number> = {
  quarter: 0.25,
  half: 0.5,
  one: 1.0,
  two: 2.0,
  four: 4.0,
};

function stepDurationMs(bpm: number, sub: Subdivision): number {
  const safeBpm = Math.max(1, bpm);
  return (60_000 / safeBpm) * SUBDIVISION_BEATS[sub];
}

/// Mirror of `chaser::pattern::evaluate` in Rust. Kept here for the preview
/// only — the actual DMX values come from the engine. If the Rust impl ever
/// changes a pattern, update this too or the preview will lie.
function evaluatePattern(pattern: Pattern, step: number, slot: number, total: number): boolean {
  if (total === 0) return false;
  switch (pattern.type) {
    case "all_together":
      return step % 2 === 0;
    case "alternate":
      return slot % 2 === step % 2;
    case "chase":
      return step % total === slot;
    case "chase_reverse":
      return slot === total - 1 - (step % total);
    case "ping_pong": {
      if (total === 1) return step % 2 === 0;
      const cycle = 2 * total - 2;
      const posInCycle = step % cycle;
      const pos = posInCycle < total ? posInCycle : cycle - posInCycle;
      return slot === pos;
    }
    case "random":
      return slot === randomSlot(step, total);
    case "wave":
    case "wave_reverse": {
      if (total === 1) return true;
      const head = pattern.type === "wave_reverse" ? total - 1 - (step % total) : step % total;
      const tail = (head + 1) % total;
      return slot === head || slot === tail;
    }
    case "build":
    case "build_reverse": {
      const cycle = total + 1;
      const pos = step % cycle;
      if (pos === total) return false; // reset blink
      return pattern.type === "build_reverse" ? slot >= pos : slot <= pos;
    }
    case "center_out": {
      if (total <= 1) return step % 2 === 0;
      const half = Math.ceil(total / 2);
      const cycle = half + 1;
      const pos = step % cycle;
      if (pos === half) return false;
      const radius = pos;
      const centre = (total - 1) / 2;
      return Math.abs(slot - centre) <= radius + 0.5;
    }
    case "symmetric": {
      if (total === 1) return step % 2 === 0;
      const half = Math.ceil(total / 2);
      const pos = step % half;
      return slot === pos || slot === total - 1 - pos;
    }
    case "outside_in": {
      if (total <= 1) return step % 2 === 0;
      const half = Math.ceil(total / 2);
      const cycle = half + 1;
      const pos = step % cycle;
      if (pos === half) return false;
      const centre = (total - 1) / 2;
      const litRadius = half - 0.5 - pos;
      return Math.abs(slot - centre) <= litRadius;
    }
    case "inverted_chase": {
      const dark = step % total;
      return slot !== dark;
    }
    case "groups_of_two":
    case "groups_of_three": {
      const n = pattern.type === "groups_of_two" ? 2 : 3;
      const groups = Math.ceil(total / n);
      const activeGroup = step % groups;
      return Math.floor(slot / n) === activeGroup;
    }
    case "half_swap": {
      const half = Math.ceil(total / 2);
      const inLeft = slot < half;
      const leftOn = step % 2 === 0;
      return (inLeft && leftOn) || (!inLeft && !leftOn);
    }
    case "edges": {
      if (total <= 2) return step % 2 === 0;
      const onStep = step % 2 === 0;
      const isEdge = slot === 0 || slot === total - 1;
      return isEdge && onStep;
    }
    case "pulse_out":
    case "pulse_in": {
      if (total <= 1) return step % 2 === 0;
      const half = Math.ceil(total / 2);
      const cycle = half + 1;
      const pos = step % cycle;
      if (pos === half) return false;
      const radius = pattern.type === "pulse_in" ? half - 1 - pos : pos;
      const centre = (total - 1) / 2;
      const dist = Math.abs(slot - centre);
      return dist >= radius - 0.5 && dist <= radius + 0.5;
    }
    case "accordion": {
      if (total <= 1) return step % 2 === 0;
      const half = Math.ceil(total / 2);
      const cycle = Math.max(1, 2 * half - 1);
      const posInCycle = step % cycle;
      const radius = posInCycle < half ? posInCycle : cycle - 1 - posInCycle;
      const centre = (total - 1) / 2;
      const dist = Math.abs(slot - centre);
      return dist >= radius - 0.5 && dist <= radius + 0.5;
    }
    case "bowtie": {
      if (total <= 1) return step % 2 === 0;
      const half = Math.ceil(total / 2);
      const cycle = half + 1;
      const pos = step % cycle;
      if (pos === half) return false;
      const distToEdge = Math.min(slot, total - 1 - slot);
      return distToEdge <= pos;
    }
    case "dual_chase": {
      if (total <= 1) return step % 2 === 0;
      const half = Math.floor(total / 2);
      const headA = step % total;
      const headB = (headA + half) % total;
      return slot === headA || slot === headB;
    }
    case "symmetric_bounce": {
      if (total <= 1) return step % 2 === 0;
      const half = Math.ceil(total / 2);
      if (half <= 1) return slot === step % 2;
      const cycle = 2 * half - 2;
      const posInCycle = step % cycle;
      const pos = posInCycle < half ? posInCycle : cycle - posInCycle;
      return slot === pos || slot === total - 1 - pos;
    }
  }
}

/// JS port of the Rust `random_slot` so the preview's "random" matches the
/// engine's choice for any given step. Uses BigInt to mirror u64 wrapping.
function randomSlot(step: number, total: number): number {
  if (total <= 1) return 0;
  const MASK = 0xffff_ffff_ffff_ffffn;
  let h = (BigInt(step) * 2654435761n + 0x9e37_79b9_7f4a_7c15n) & MASK;
  h = (h ^ (h >> 33n)) & MASK;
  return Number(h % BigInt(total));
}

function hsvToRgb(h: number, s: number, v: number): Rgb {
  const c = v * s;
  const hp = h / 60;
  const x = c * (1 - Math.abs((((hp % 2) + 2) % 2) - 1));
  let r = 0;
  let g = 0;
  let b = 0;
  const sector = Math.floor(hp);
  if (sector === 0) {
    r = c;
    g = x;
  } else if (sector === 1) {
    r = x;
    g = c;
  } else if (sector === 2) {
    g = c;
    b = x;
  } else if (sector === 3) {
    g = x;
    b = c;
  } else if (sector === 4) {
    r = x;
    b = c;
  } else {
    r = c;
    b = x;
  }
  const m = v - c;
  return {
    r: Math.max(0, Math.min(255, Math.round((r + m) * 255))),
    g: Math.max(0, Math.min(255, Math.round((g + m) * 255))),
    b: Math.max(0, Math.min(255, Math.round((b + m) * 255))),
  };
}

/// Mirror of `chaser::color::color_for_slot`. `null` means "Disabled mode,
/// don't write colour". The preview falls back to white in that case so the
/// dot still indicates intensity.
function colorForSlot(mode: ColorMode, step: number, slot: number, total: number): Rgb | null {
  switch (mode.type) {
    case "disabled":
      return null;
    case "single":
      return mode.color;
    case "two_color_cadence": {
      const aWins = (() => {
        switch (mode.cadence.type) {
          case "every_step":
            return step % 2 === 0;
          case "every_n_steps": {
            const n = Math.max(1, mode.cadence.n);
            return Math.floor(step / n) % 2 === 0;
          }
          case "per_slot":
            return slot < Math.ceil(total / 2);
          case "alternate_slots":
            return slot % 2 === 0;
          case "chase_per_color": {
            const cycle = Math.max(1, total);
            return Math.floor(step / cycle) % 2 === 0;
          }
        }
      })();
      return aWins ? mode.color_a : mode.color_b;
    }
    case "palette": {
      if (mode.colors.length === 0) return { r: 0, g: 0, b: 0 };
      const len = mode.colors.length;
      let idx = 0;
      switch (mode.rotation) {
        case "per_step":
          idx = step % len;
          break;
        case "per_cycle":
          idx = Math.floor(step / Math.max(1, total)) % len;
          break;
        case "per_slot":
          idx = slot % len;
          break;
      }
      return mode.colors[idx];
    }
    case "rainbow": {
      const perSlot = total <= 1 ? 0 : (slot / total) * mode.spread * 360;
      const raw = step * mode.speed + perSlot;
      const hue = ((raw % 360) + 360) % 360;
      return hsvToRgb(hue, 1, 1);
    }
  }
}

/// Build the CSS colour shown in a preview dot. Mirrors the engine: when
/// the slot is "on" the chaser's resolved colour rides the master multiplier;
/// when "off" the dot dims to the chaser's background level.
function dotColor(
  chaser: AmbientChaser,
  step: number,
  slot: number,
  total: number,
  on: boolean,
): string {
  if (on) {
    const resolved = colorForSlot(chaser.color_mode, step, slot, total) ?? {
      r: 255,
      g: 255,
      b: 255,
    };
    const factor = Math.max(0, Math.min(1, chaser.master));
    return `rgb(${Math.round(resolved.r * factor)}, ${Math.round(resolved.g * factor)}, ${Math.round(resolved.b * factor)})`;
  }
  const bg = chaser.background;
  return `rgb(${bg}, ${bg}, ${bg})`;
}

/// Build a sensible default for a colour mode type when the user switches
/// modes — keeps the previous colour configuration on a per-mode basis is
/// nice but more code; for now reset to safe defaults.
function makeDefaultColorMode(type: ColorMode["type"]): ColorMode {
  switch (type) {
    case "disabled":
      return { type: "disabled" };
    case "single":
      return { type: "single", color: { r: 255, g: 255, b: 255 } };
    case "two_color_cadence":
      return {
        type: "two_color_cadence",
        color_a: { r: 0, g: 200, b: 255 }, // cyan
        color_b: { r: 255, g: 0, b: 200 }, // magenta
        cadence: { type: "every_step" },
      };
    case "palette":
      return {
        type: "palette",
        colors: [
          { r: 255, g: 0, b: 0 },
          { r: 0, g: 255, b: 0 },
          { r: 0, g: 0, b: 255 },
        ],
        rotation: "per_step",
      };
    case "rainbow":
      return { type: "rainbow", speed: 30, spread: 1 };
  }
}

function rgbToHex({ r, g, b }: Rgb): string {
  return `#${r.toString(16).padStart(2, "0")}${g.toString(16).padStart(2, "0")}${b.toString(16).padStart(2, "0")}`;
}

function hexToRgb(hex: string): Rgb {
  const h = hex.replace("#", "");
  return {
    r: Number.parseInt(h.slice(0, 2), 16),
    g: Number.parseInt(h.slice(2, 4), 16),
    b: Number.parseInt(h.slice(4, 6), 16),
  };
}

function ColorPickerInput({ color, onChange }: { color: Rgb; onChange: (c: Rgb) => void }) {
  return (
    <input
      type="color"
      value={rgbToHex(color)}
      onChange={(e) => onChange(hexToRgb(e.currentTarget.value))}
    />
  );
}

function TwoColorCadenceEditor({
  mode,
  onChange,
}: {
  mode: Extract<ColorMode, { type: "two_color_cadence" }>;
  onChange: (m: ColorMode) => void;
}) {
  const t = useT();
  return (
    <div className="color-mode-block">
      <div className="color-pair">
        <span className="color-pair-item">
          A
          <ColorPickerInput
            color={mode.color_a}
            onChange={(c) => onChange({ ...mode, color_a: c })}
          />
        </span>
        <span className="color-pair-item">
          B
          <ColorPickerInput
            color={mode.color_b}
            onChange={(c) => onChange({ ...mode, color_b: c })}
          />
        </span>
      </div>
      <label>
        {t("chaser.cadence")}
        <select
          value={mode.cadence.type}
          onChange={(e) => {
            const next = e.currentTarget.value as Cadence["type"];
            const cadence: Cadence =
              next === "every_n_steps" ? { type: next, n: 4 } : ({ type: next } as Cadence);
            onChange({ ...mode, cadence });
          }}
        >
          {CADENCES.map((c) => (
            <option key={c.value} value={c.value}>
              {t(c.labelKey)}
            </option>
          ))}
        </select>
      </label>
      {mode.cadence.type === "every_n_steps" ? (
        <label>
          {t("chaser.cadenceN")}
          <input
            type="number"
            min={1}
            max={64}
            value={mode.cadence.n}
            onChange={(e) =>
              onChange({
                ...mode,
                cadence: { type: "every_n_steps", n: Math.max(1, Number(e.currentTarget.value)) },
              })
            }
          />
        </label>
      ) : null}
    </div>
  );
}

function PaletteEditor({
  mode,
  onChange,
}: {
  mode: Extract<ColorMode, { type: "palette" }>;
  onChange: (m: ColorMode) => void;
}) {
  const t = useT();
  return (
    <div className="color-mode-block">
      <label>
        {t("chaser.rotation")}
        <select
          value={mode.rotation}
          onChange={(e) =>
            onChange({ ...mode, rotation: e.currentTarget.value as PaletteRotation })
          }
        >
          {PALETTE_ROTATIONS.map((r) => (
            <option key={r.value} value={r.value}>
              {t(r.labelKey)}
            </option>
          ))}
        </select>
      </label>
      <div className="palette-list">
        {mode.colors.map((c, i) => (
          <div key={`${i}-${rgbToHex(c)}`} className="palette-item">
            <ColorPickerInput
              color={c}
              onChange={(next) =>
                onChange({
                  ...mode,
                  colors: mode.colors.map((cc, j) => (j === i ? next : cc)),
                })
              }
            />
            <button
              type="button"
              className="danger"
              onClick={() => onChange({ ...mode, colors: mode.colors.filter((_, j) => j !== i) })}
              disabled={mode.colors.length <= 1}
              title={t("chaser.removeColor")}
            >
              ×
            </button>
          </div>
        ))}
        <button
          type="button"
          onClick={() =>
            onChange({ ...mode, colors: [...mode.colors, { r: 255, g: 255, b: 255 }] })
          }
        >
          {t("chaser.addColor")}
        </button>
      </div>
    </div>
  );
}

function RainbowEditor({
  mode,
  onChange,
}: {
  mode: Extract<ColorMode, { type: "rainbow" }>;
  onChange: (m: ColorMode) => void;
}) {
  const t = useT();
  return (
    <div className="color-mode-block">
      <label>
        {t("chaser.rainbowSpeed")}
        <input
          type="range"
          min={0}
          max={180}
          step={1}
          value={mode.speed}
          onChange={(e) => onChange({ ...mode, speed: Number(e.currentTarget.value) })}
        />
        <span className="val">{mode.speed.toFixed(0)}°</span>
      </label>
      <label>
        {t("chaser.rainbowSpread")}
        <input
          type="range"
          min={0}
          max={3}
          step={0.05}
          value={mode.spread}
          onChange={(e) => onChange({ ...mode, spread: Number(e.currentTarget.value) })}
        />
        <span className="val">{mode.spread.toFixed(2)}×</span>
      </label>
    </div>
  );
}

function ChaserPreview({
  chaser,
  fixtures,
}: {
  chaser: AmbientChaser;
  fixtures: FixtureInstance[];
}) {
  const t = useT();
  const total = chaser.slots.length;
  const bpm = bpmOf(chaser.tempo);
  const stepMs = stepDurationMs(bpm, chaser.subdivision);

  // Each chaser keeps its own visual step counter. We re-init when timing
  // changes so a BPM bump doesn't replay the past in fast-forward.
  const [step, setStep] = useState(0);
  useEffect(() => {
    setStep(0);
    if (!Number.isFinite(stepMs) || stepMs <= 0) return;
    const id = window.setInterval(() => {
      setStep((s) => (s + 1) >>> 0);
    }, stepMs);
    return () => window.clearInterval(id);
  }, [stepMs]);

  if (total === 0) {
    return <div className="chaser-preview empty">{t("chaser.preview.empty")}</div>;
  }

  // When fade is on, mirror the engine's fade duration via a CSS
  // transition so the preview blends smoothly between steps. With fade
  // off we keep the existing 80 ms cosmetic transition (purely so the
  // hard flash doesn't read as flicker on a 60 Hz monitor).
  const fadeMs = chaser.fade.enabled ? Math.max(0, Math.min(0.9, chaser.fade.amount)) * stepMs : 80;
  const fadeCurve = chaser.fade.enabled ? cssCurveFor(chaser.fade.curve) : "linear";
  const transition = `background ${fadeMs}ms ${fadeCurve}, box-shadow ${fadeMs}ms ${fadeCurve}`;

  return (
    <div className="chaser-preview">
      {chaser.slots.map((slot, i) => {
        const on = evaluatePattern(chaser.pattern, step, i, total);
        const color = dotColor(chaser, step, i, total, on);
        const fixture = fixtures.find((f) => f.id === slot.fixture_id);
        return (
          <span
            key={`${slot.fixture_id}-${i}`}
            className={`preview-dot${on ? " lit" : ""}`}
            style={{
              background: color,
              boxShadow: on ? `0 0 10px 1px ${color}` : "none",
              transition,
            }}
            title={fixture ? fixtureLabel(fixture) : slot.fixture_id}
          />
        );
      })}
    </div>
  );
}

type EditTab = "slots" | "pattern" | "timing";

function ChaserCard({
  chaser,
  index,
  fixtures,
  expanded,
  onToggleExpanded,
}: {
  chaser: AmbientChaser;
  index: number;
  fixtures: FixtureInstance[];
  expanded: boolean;
  onToggleExpanded: () => void;
}) {
  const t = useT();
  const padColor = chaserColor(index);
  const showsOnLaunchpad = index < 8;
  const updateChaser = useShowStore((s) => s.updateChaser);
  const deleteChaser = useShowStore((s) => s.deleteChaser);
  const toggleChaser = useShowStore((s) => s.toggleChaser);
  const [tab, setTab] = useState<EditTab>("pattern");

  const bpm = bpmOf(chaser.tempo);
  const colorPreview = (() => {
    if (chaser.color_mode.type === "single") {
      const { r, g, b } = chaser.color_mode.color;
      return `rgb(${r}, ${g}, ${b})`;
    }
    return null;
  })();

  const setName = (name: string) => updateChaser({ ...chaser, name });
  const setBpm = (b: number) => updateChaser({ ...chaser, tempo: { type: "fixed", bpm: b } });
  const setSubdivision = (subdivision: Subdivision) => updateChaser({ ...chaser, subdivision });
  const setMaster = (master: number) => updateChaser({ ...chaser, master });
  const setBackground = (background: number) => updateChaser({ ...chaser, background });
  const setPattern = (type: Pattern["type"]) =>
    updateChaser({ ...chaser, pattern: { type } as Pattern });
  const setColorMode = (mode: ColorMode) => updateChaser({ ...chaser, color_mode: mode });
  const setFade = (fade: AmbientChaser["fade"]) => updateChaser({ ...chaser, fade });

  const addSlot = () => {
    const fixture = fixtures.find((f) => !chaser.slots.some((s) => s.fixture_id === f.id));
    if (!fixture) return;
    const slot: ChaserSlot = {
      fixture_id: fixture.id,
      use_intensity: true,
      use_color: chaser.color_mode.type !== "disabled",
    };
    updateChaser({ ...chaser, slots: [...chaser.slots, slot] });
  };
  const removeSlot = (idx: number) => {
    updateChaser({ ...chaser, slots: chaser.slots.filter((_, i) => i !== idx) });
  };
  const setSlot = (idx: number, patch: Partial<ChaserSlot>) => {
    updateChaser({
      ...chaser,
      slots: chaser.slots.map((s, i) => (i === idx ? { ...s, ...patch } : s)),
    });
  };

  return (
    <div
      data-doc="chaser-card"
      data-doc-on={chaser.enabled ? "true" : undefined}
      className={`chaser-card${chaser.enabled ? " on" : ""}`}
      style={chaser.enabled ? { borderLeft: `4px solid ${padColor}` } : undefined}
    >
      <div className="chaser-row" data-doc="chaser-row">
        <button
          type="button"
          data-doc="chaser-toggle"
          className={`chaser-toggle${chaser.enabled ? " on" : ""}`}
          onClick={() => toggleChaser(chaser.id, !chaser.enabled)}
          aria-label={chaser.enabled ? t("chaser.toggle.disable") : t("chaser.toggle.enable")}
          style={
            chaser.enabled
              ? { background: padColor, borderColor: padColor, color: "#0f0f10" }
              : { borderColor: padColor, color: padColor }
          }
        >
          {chaser.enabled ? t("chaser.toggle.on") : t("chaser.toggle.off")}
        </button>
        {showsOnLaunchpad ? (
          <span
            className="lp-pad-badge"
            title={t("chaser.lpHint", { pad: index + 1 })}
            style={{ background: padColor }}
            aria-hidden="true"
          />
        ) : null}
        <input
          className="chaser-name"
          value={chaser.name}
          onChange={(e) => setName(e.currentTarget.value)}
        />
        <span className="chaser-summary">
          {t("chaser.summary", { bpm: bpm.toFixed(0), slots: chaser.slots.length })}
          {chaser.fade.enabled ? t("chaser.summary.fade") : ""}
        </span>
        {colorPreview ? (
          <span
            className="chaser-color-pip"
            style={{ background: colorPreview }}
            aria-hidden="true"
          />
        ) : null}
        <button type="button" data-doc="chaser-edit-toggle" onClick={onToggleExpanded}>
          {expanded ? t("chaser.hide") : t("chaser.edit")}
        </button>
        <button type="button" className="danger" onClick={() => deleteChaser(chaser.id)}>
          {t("chaser.del")}
        </button>
      </div>

      <ChaserPreview chaser={chaser} fixtures={fixtures} />

      {expanded ? (
        <div className="chaser-edit" data-doc="chaser-edit">
          <nav className="chaser-tabs" data-doc="chaser-tabs">
            {(
              [
                { id: "pattern", labelKey: "chaser.tab.pattern" as const },
                { id: "timing", labelKey: "chaser.tab.timing" as const },
                { id: "slots", labelKey: "chaser.tab.slots" as const },
              ] as Array<{ id: EditTab; labelKey: keyof Translation }>
            ).map((meta) => (
              <button
                key={meta.id}
                type="button"
                className={`chaser-tab${tab === meta.id ? " active" : ""}`}
                onClick={() => setTab(meta.id)}
              >
                {meta.id === "slots"
                  ? t(meta.labelKey, { count: chaser.slots.length })
                  : t(meta.labelKey)}
              </button>
            ))}
          </nav>

          {tab === "pattern" ? (
            <div className="chaser-tab-body">
              <div className="chaser-section">
                <h5>{t("chaser.section.pattern")}</h5>
                {/* The pattern list grew past a comfortable radio
                    grid (18 entries), so we use a <select>. The
                    options stay sorted in PATTERNS to keep related
                    looks adjacent (chase / chase_reverse, build /
                    build_reverse, center_out / outside_in). */}
                <select
                  value={chaser.pattern.type}
                  onChange={(e) => setPattern(e.currentTarget.value as Pattern["type"])}
                >
                  {PATTERNS.map((p) => (
                    <option key={p.value} value={p.value}>
                      {t(p.labelKey)}
                    </option>
                  ))}
                </select>
              </div>
              <div className="chaser-section">
                <h5>{t("chaser.section.colorMode")}</h5>
                <select
                  value={chaser.color_mode.type}
                  onChange={(e) =>
                    setColorMode(makeDefaultColorMode(e.currentTarget.value as ColorMode["type"]))
                  }
                >
                  {COLOR_MODE_TYPES.map((c) => (
                    <option key={c.value} value={c.value}>
                      {t(c.labelKey)}
                    </option>
                  ))}
                </select>
                {chaser.color_mode.type === "single" ? (
                  <ColorPickerInput
                    color={chaser.color_mode.color}
                    onChange={(color) => setColorMode({ type: "single", color })}
                  />
                ) : null}
                {chaser.color_mode.type === "two_color_cadence" ? (
                  <TwoColorCadenceEditor
                    mode={chaser.color_mode}
                    onChange={(m) => setColorMode(m)}
                  />
                ) : null}
                {chaser.color_mode.type === "palette" ? (
                  <PaletteEditor mode={chaser.color_mode} onChange={(m) => setColorMode(m)} />
                ) : null}
                {chaser.color_mode.type === "rainbow" ? (
                  <RainbowEditor mode={chaser.color_mode} onChange={(m) => setColorMode(m)} />
                ) : null}
              </div>
            </div>
          ) : null}

          {tab === "timing" ? (
            <div className="chaser-tab-body">
              <div className="chaser-section">
                <h5>{t("chaser.section.tempo")}</h5>
                <div className="chaser-edit-grid">
                  <label>
                    {t("chaser.bpm")}
                    <input
                      type="number"
                      min={20}
                      max={400}
                      step={0.5}
                      value={bpm}
                      onChange={(e) => setBpm(Number(e.currentTarget.value))}
                    />
                  </label>
                  <label>
                    {t("chaser.subdivision")}
                    <select
                      value={chaser.subdivision}
                      onChange={(e) => setSubdivision(e.currentTarget.value as Subdivision)}
                    >
                      {SUBDIVISIONS.map((s) => (
                        <option key={s.value} value={s.value}>
                          {t(s.labelKey)}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
              </div>
              <div className="chaser-section">
                <h5>{t("chaser.section.levels")}</h5>
                <div className="chaser-edit-grid">
                  <label>
                    {t("chaser.master")}
                    <input
                      type="range"
                      min={0}
                      max={1}
                      step={0.01}
                      value={chaser.master}
                      onChange={(e) => setMaster(Number(e.currentTarget.value))}
                    />
                    <span className="val">{Math.round(chaser.master * 100)}%</span>
                  </label>
                  <label>
                    {t("chaser.background")}
                    <input
                      type="range"
                      min={0}
                      max={255}
                      value={chaser.background}
                      onChange={(e) => setBackground(Number(e.currentTarget.value))}
                    />
                    <span className="val">{chaser.background}</span>
                  </label>
                </div>
              </div>
              <div className="chaser-section">
                <h5>
                  {t("chaser.section.fade")}
                  <label className="chaser-fade-toggle">
                    <input
                      type="checkbox"
                      checked={chaser.fade.enabled}
                      onChange={(e) =>
                        setFade({ ...chaser.fade, enabled: e.currentTarget.checked })
                      }
                    />
                    {t("chaser.fadeEnabled")}
                  </label>
                </h5>
                <div className="chaser-edit-grid">
                  <label>
                    {t("chaser.fadeAmount")}
                    <input
                      type="range"
                      min={0}
                      max={0.9}
                      step={0.01}
                      value={chaser.fade.amount}
                      disabled={!chaser.fade.enabled}
                      onChange={(e) =>
                        setFade({ ...chaser.fade, amount: Number(e.currentTarget.value) })
                      }
                    />
                    <span className="val">{Math.round(chaser.fade.amount * 100)}%</span>
                  </label>
                  <label>
                    {t("chaser.fadeCurve")}
                    <select
                      value={chaser.fade.curve}
                      disabled={!chaser.fade.enabled}
                      onChange={(e) =>
                        setFade({
                          ...chaser.fade,
                          curve: e.currentTarget.value as FadeCurve,
                        })
                      }
                    >
                      {FADE_CURVES.map((c) => (
                        <option key={c.value} value={c.value}>
                          {t(c.labelKey)}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <p className="chaser-hint">{t("chaser.fadeHint")}</p>
              </div>
            </div>
          ) : null}

          {tab === "slots" ? (
            <div className="chaser-tab-body">
              <div className="chaser-section">
                {chaser.slots.length === 0 ? (
                  <p className="empty">{t("chaser.slots.empty")}</p>
                ) : (
                  <div className="chaser-slots">
                    {chaser.slots.map((slot, i) => (
                      <div key={`${slot.fixture_id}-${i}`} className="chaser-slot-row">
                        <select
                          value={slot.fixture_id}
                          onChange={(e) => setSlot(i, { fixture_id: e.currentTarget.value })}
                        >
                          {fixtures.map((f) => (
                            <option key={f.id} value={f.id}>
                              {t("chaser.slots.fixtureFmt", {
                                label: fixtureLabel(f),
                                universe: f.universe,
                                address: f.address,
                              })}
                            </option>
                          ))}
                        </select>
                        <label>
                          <input
                            type="checkbox"
                            checked={slot.use_intensity}
                            onChange={(e) => setSlot(i, { use_intensity: e.currentTarget.checked })}
                          />
                          {t("chaser.slots.intLabel")}
                        </label>
                        <label>
                          <input
                            type="checkbox"
                            checked={slot.use_color}
                            onChange={(e) => setSlot(i, { use_color: e.currentTarget.checked })}
                          />
                          {t("chaser.slots.colorLabel")}
                        </label>
                        <button type="button" className="danger" onClick={() => removeSlot(i)}>
                          {t("chaser.del")}
                        </button>
                      </div>
                    ))}
                  </div>
                )}
                <button
                  type="button"
                  onClick={addSlot}
                  disabled={chaser.slots.length >= fixtures.length}
                >
                  {t("chaser.slots.add")}
                </button>
              </div>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

export function ChaserView() {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const createChaser = useShowStore((s) => s.createChaser);
  const addExampleChasers = useShowStore((s) => s.addExampleChasers);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [exampleNotice, setExampleNotice] = useState<string | null>(null);

  if (!show) return <main className="page">{t("common.loading")}</main>;
  const chasers: AmbientChaser[] = show.chasers ?? [];
  const fixtures = show.fixtures;

  const onNew = async () => {
    const c = await createChaser();
    setExpandedId(c.id);
  };

  const onAddExamples = async () => {
    const n = await addExampleChasers();
    if (fixtures.length === 0) {
      setExampleNotice(t("chaser.exampleNoticeNoFixtures", { count: n }));
    } else {
      setExampleNotice(
        t("chaser.exampleNoticeWithFixtures", { count: n, fixtures: fixtures.length }),
      );
    }
  };

  return (
    <main className="page chaser-view">
      <header className="page-head" data-doc="chaser-head">
        <h2>{t("chaser.title", { count: chasers.length })}</h2>
        <div className="actions" data-doc="chaser-actions">
          <button type="button" data-doc="chaser-add-example" onClick={onAddExamples}>
            {t("chaser.addExample")}
          </button>
          <button type="button" data-doc="chaser-add-new" onClick={onNew}>
            {t("chaser.addNew")}
          </button>
        </div>
      </header>
      {exampleNotice ? (
        <output className="chaser-notice">
          {exampleNotice}
          <button
            type="button"
            className="chaser-notice-dismiss"
            onClick={() => setExampleNotice(null)}
            aria-label={t("common.close")}
          >
            ×
          </button>
        </output>
      ) : null}
      {chasers.length === 0 ? (
        <p className="empty">{t("chaser.empty")}</p>
      ) : (
        <div className="chaser-list" data-doc="chaser-list">
          {chasers.map((c, i) => (
            <ChaserCard
              key={c.id}
              chaser={c}
              index={i}
              fixtures={fixtures}
              expanded={expandedId === c.id}
              onToggleExpanded={() => setExpandedId((cur) => (cur === c.id ? null : c.id))}
            />
          ))}
        </div>
      )}
    </main>
  );
}
