import type { ChannelRange } from "@bindings/ChannelRange";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import type { UniverseSnapshot } from "@bindings/UniverseSnapshot";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

/// "What kind of light is this physically?" The 3D renderer needs
/// to know because a moving-head spot has a tight beam that punches
/// through haze, while an RGB par washes a wide area softly. We
/// derive this once from the channel layout — fixtures with a real
/// optical system (zoom / gobo / prism / iris) are spots; everything
/// else (RGB pars, dimmer-only fixtures, RGBW washes) renders as
/// washes.
export type FixtureKind = "wash" | "spot";

/// Per-fixture state derived from the live DMX snapshot. Everything
/// the 3D scene needs to render one fixture is right here — no
/// further lookups in the renderer's hot loop.
export interface FixtureLightState {
  fixtureId: string;
  kind: FixtureKind;
  /// Linear-space colour, 0..1. Pre-multiplied with intensity so the
  /// caller doesn't have to remember to scale. RGBW/A is mixed
  /// additively; if the fixture has a colour wheel and no RGB, the
  /// wheel range's labelled colour is used instead. If both exist
  /// and the wheel is open/white, RGB takes over; otherwise the
  /// wheel wins (matches what a real moving head's beam actually
  /// does — the wheel filters the lamp).
  color: { r: number; g: number; b: number };
  intensity: number;
  pan: number;
  tilt: number;
  /// 0..1 mapping to beam half-angle. Spots: 4° → 12° (narrow → mid).
  /// Washes: 25° → 55° (always wide).
  zoom: number;
  strobe: number;
  /// Raw prism channel value 0..255. The renderer combines this
  /// with per-fixture overrides (threshold + facets) to decide
  /// whether to splat the beam.
  prismValue: number;
  /// Resolved gobo image URL when the live gobo channel selects a
  /// non-open range AND the range carries an image. The renderer
  /// turns this into a `SpotLight.map` texture and projects it.
  goboImage: string | null;
  /// The active gobo range's label, surfaced even when the range
  /// has NO image (common with libraries imported from Freestyler
  /// / QLC+ that ship labels but not the bitmaps). Renderer falls
  /// back to a procedural texture with this label so the operator
  /// can SEE that "Gobo 3" is selected even without artwork.
  /// `null` when the wheel is at an open / no-gobo position.
  goboLabel: string | null;
}

interface ChannelLayout {
  intensity: number | null;
  red: number | null;
  green: number | null;
  blue: number | null;
  white: number | null;
  amber: number | null;
  pan: number | null;
  panFine: number | null;
  tilt: number | null;
  tiltFine: number | null;
  zoom: number | null;
  strobe: number | null;
  /// Color wheel offset + ranges (from the fixture mode definition,
  /// each range labelled "Red", "Blue", "Open" etc.).
  colorWheel: number | null;
  colorWheelRanges: ChannelRange[];
  /// Prism offset. We assume value > 8 means active and 7 facets
  /// (the canonical small-spot prism). Future: read the active
  /// range's label to detect "3-facet" vs "7-facet" prisms.
  prism: number | null;
  /// Gobo channel offset + range table. Each range optionally
  /// carries an `image` (base64 data URL) or an `image_path`
  /// (filesystem path resolved via Tauri's asset protocol). The
  /// renderer projects the active range's image through the
  /// SpotLight as a real gobo.
  gobo: number | null;
  goboRanges: ChannelRange[];
  iris: number | null;
  panDegrees: number;
  tiltDegrees: number;
  hasIntensityChannel: boolean;
  hasRGB: boolean;
  kind: FixtureKind;
}

function roleString(role: unknown): string | null {
  if (typeof role === "string") return role;
  if (role && typeof role === "object" && "other" in role) {
    const inner = (role as { other: unknown }).other;
    if (typeof inner === "string") return inner.toLowerCase();
  }
  return null;
}

function isPrismChannel(roleStr: string | null, name: string | null | undefined): boolean {
  // Prism is conventionally `Other("prism")` because the role enum
  // doesn't include it natively. We also match by channel name for
  // libraries that call it "Prism" / "Prisma" with a generic role.
  if (roleStr && roleStr.includes("prism")) return true;
  if (name && /\bprisma?\b/i.test(name)) return true;
  return false;
}

/// Match a gobo channel via role OR name. Many imported libraries
/// don't use the `gobo` role exactly — they emit `Other("Gobo
/// Wheel")` or set a generic role and put "Gobo 1" in the channel
/// name. We accept either.
function isGoboChannel(roleStr: string | null, name: string | null | undefined): boolean {
  if (roleStr && /gobo/.test(roleStr)) return true;
  if (name && /gobo/i.test(name)) return true;
  return false;
}

function buildLayout(
  fixture: FixtureInstance,
  def: FixtureDefinition | undefined,
): ChannelLayout {
  const layout: ChannelLayout = {
    intensity: null,
    red: null,
    green: null,
    blue: null,
    white: null,
    amber: null,
    pan: null,
    panFine: null,
    tilt: null,
    tiltFine: null,
    zoom: null,
    strobe: null,
    colorWheel: null,
    colorWheelRanges: [],
    prism: null,
    gobo: null,
    goboRanges: [],
    iris: null,
    panDegrees: 540,
    tiltDegrees: 270,
    hasIntensityChannel: false,
    hasRGB: false,
    kind: "wash",
  };
  const mode = def?.modes?.[fixture.mode_index];
  if (!mode) return layout;
  for (let i = 0; i < mode.channels.length; i++) {
    const ch = mode.channels[i];
    const role = roleString(ch.role);
    const name = ch.name ?? null;
    switch (role) {
      case "intensity":
        layout.intensity = i;
        layout.hasIntensityChannel = true;
        break;
      case "red":
        layout.red = i;
        break;
      case "green":
        layout.green = i;
        break;
      case "blue":
        layout.blue = i;
        break;
      case "white":
        layout.white = i;
        break;
      case "amber":
        layout.amber = i;
        break;
      case "pan":
        layout.pan = i;
        break;
      case "pan_fine":
        layout.panFine = i;
        break;
      case "tilt":
        layout.tilt = i;
        break;
      case "tilt_fine":
        layout.tiltFine = i;
        break;
      case "zoom":
        layout.zoom = i;
        break;
      case "strobe":
        layout.strobe = i;
        break;
      case "color_wheel":
      case "color":
        layout.colorWheel = i;
        layout.colorWheelRanges = ch.ranges ?? [];
        break;
      case "gobo":
        // Some fixtures have multiple gobo channels (gobo wheel +
        // gobo rotation). We take the first one with `ranges`
        // present, which is almost always the wheel.
        if (layout.gobo === null || (ch.ranges && ch.ranges.length > 0)) {
          layout.gobo = i;
          layout.goboRanges = ch.ranges ?? [];
        }
        break;
      case "iris":
        layout.iris = i;
        break;
      default:
        if (isPrismChannel(role, name)) {
          layout.prism = i;
        } else if (
          layout.gobo === null &&
          isGoboChannel(role, name) &&
          ch.ranges &&
          ch.ranges.length > 0
        ) {
          // Fallback gobo detection: role is something custom
          // (`Other("Gobo Wheel")`) or the role is generic and the
          // name says "Gobo X". We require ranges to exist —
          // gobo-rotation channels typically don't have wheel
          // labels and we don't want to grab them by mistake.
          layout.gobo = i;
          layout.goboRanges = ch.ranges;
        }
        break;
    }
  }
  if (mode.pan_range) layout.panDegrees = mode.pan_range.physical_degrees;
  if (mode.tilt_range) layout.tiltDegrees = mode.tilt_range.physical_degrees;
  layout.hasRGB =
    layout.red !== null || layout.green !== null || layout.blue !== null;
  // A "spot" is a fixture with a real optical system: zoom narrows
  // the beam, gobo carves shapes into it, prism splits it, iris
  // chokes it. Any of those = treat as a tight-beam spot in the 3D
  // preview. Fixtures without any of those channels are washes
  // (RGB pars, dimmer-only fresnels, etc.) and render with a wide
  // soft cone.
  layout.kind =
    layout.zoom !== null ||
    layout.gobo !== null ||
    layout.prism !== null ||
    layout.iris !== null
      ? "spot"
      : "wash";
  return layout;
}

function readByte(snap: Uint8Array, address: number, offset: number): number {
  const i = (address | 0) + offset - 1;
  return i >= 0 && i < snap.length ? snap[i] : 0;
}

function readWord(snap: Uint8Array, address: number, msb: number, lsb: number | null): number {
  const high = readByte(snap, address, msb);
  if (lsb === null) return (high << 8) >>> 0;
  const low = readByte(snap, address, lsb);
  return ((high << 8) | low) >>> 0;
}

// --- Wheel-label → RGB ---------------------------------------------------
// Mirrors the StageView WHEEL_COLOR_MAP so the 3D preview reads the
// same colours the per-fixture editor draws as bars. Kept local
// (not shared) to keep this module self-contained for the POC.
const WHEEL_RGB: Record<string, [number, number, number]> = {
  white: [1, 1, 1],
  open: [1, 1, 1],
  none: [1, 1, 1],
  off: [0, 0, 0],
  black: [0, 0, 0],
  red: [1, 0.12, 0.12],
  yellow: [1, 0.88, 0],
  green: [0.12, 1, 0.12],
  cyan: [0.12, 1, 1],
  blue: [0.12, 0.38, 1],
  magenta: [1, 0.12, 1],
  purple: [0.63, 0.25, 1],
  violet: [0.56, 0.25, 1],
  pink: [1, 0.5, 0.75],
  orange: [1, 0.5, 0.12],
  amber: [1, 0.67, 0],
  uv: [0.5, 0.25, 1],
  lime: [0.63, 1, 0.12],
  teal: [0.12, 0.63, 0.63],
};

function normalizeWheelLabel(label: string): string {
  return label.toLowerCase().trim().replace(/^color\s+/, "");
}

function wheelColor(value: number, ranges: ChannelRange[]): [number, number, number] | null {
  const range = ranges.find((r) => value >= r.from && value <= r.to);
  if (!range) return null;
  const norm = normalizeWheelLabel(range.label);
  if (WHEEL_RGB[norm]) return WHEEL_RGB[norm];
  // Substring fallback: "Pale Yellow" → yellow, "Deep Blue" → blue.
  for (const key of Object.keys(WHEEL_RGB)) {
    if (norm.includes(key)) return WHEEL_RGB[key];
  }
  return null;
}

/// Resolve the active gobo range's image to a URL the texture
/// loader can fetch:
///   - `data:image/...` URLs are returned verbatim (Three.js loads
///     them directly via the Image element).
///   - Filesystem paths are wrapped in `convertFileSrc` so they go
///     through Tauri's `asset://` protocol (the same scope we
///     declared in tauri.conf.json for fixture thumbnails).
///   - Empty / open ranges return `null` so the renderer skips
///     projection entirely.
/// "Open" labels we should NOT project: "Open", "No Gobo", "Gobo
/// Open", "Open (white)", etc. Anything we'd want to read as
/// "wheel parked at neutral, just send the lamp light through". We
/// match by phrase so labels like "Open Star" still project (they
/// describe an actual gobo).
/// Public capability check used by Preview3D to decide which
/// spotLights should be shadow casters (i.e. enable the
/// SpotLight.map shader path). Each shadow caster reserves a
/// texture unit in the fragment shader; modern WebGL2 reports
/// 16 max, so we only flip on the lights that actually need
/// projection. Fixtures without a gobo wheel skip the path
/// entirely and their light costs zero texture units.
export function fixtureHasGoboWheel(
  fixture: FixtureInstance,
  library: FixtureDefinition[],
): boolean {
  const def = library.find((d) => d.id === fixture.definition_id);
  const mode = def?.modes?.[fixture.mode_index];
  if (!mode) return false;
  for (const ch of mode.channels) {
    const role = roleString(ch.role);
    const name = ch.name ?? null;
    const looksGobo = role === "gobo" || isGoboChannel(role, name);
    if (!looksGobo) continue;
    if (!ch.ranges || ch.ranges.length === 0) continue;
    // At least one non-open range must carry an image, else
    // there's nothing to project even when active.
    const hasProjectable = ch.ranges.some(
      (r) => !isOpenGoboLabel(r.label) && (r.image || r.image_path),
    );
    if (hasProjectable) return true;
  }
  return false;
}

/// Synchronous "is this a spot-class fixture?" check used at mount
/// time. We need this in addition to `fixtureHasGoboWheel` because
/// Three.js's spot light map projection requires
/// `light.castShadow = true` at SHADER COMPILE time, and spots
/// without a gobo (yet) might still need it later. Cheap to flip
/// on for every spot since there's no actual shadow render cost
/// when no receivers are in scope.
export function fixtureIsSpotKind(
  fixture: FixtureInstance,
  library: FixtureDefinition[],
): boolean {
  const def = library.find((d) => d.id === fixture.definition_id);
  const mode = def?.modes?.[fixture.mode_index];
  if (!mode) return false;
  for (const ch of mode.channels) {
    const role = roleString(ch.role);
    const name = ch.name ?? null;
    if (role === "zoom" || role === "iris" || role === "gobo") return true;
    if (isGoboChannel(role, name)) return true;
    if (isPrismChannel(role, name)) return true;
  }
  return false;
}

function isOpenGoboLabel(label: string): boolean {
  const l = label.toLowerCase().trim();
  if (!l) return true;
  if (l === "open" || l === "none" || l === "white" || l === "off" || l === "empty") {
    return true;
  }
  if (/^(no\s*gobo|gobo\s*open|open\s*gobo)/.test(l)) return true;
  if (/^open\s*[-–—:]/.test(l)) return true; // "Open - White", "Open: Lamp"
  return false;
}

/// Resolve the active gobo range to an image URL + label. Either
/// (or both) may be null — the renderer uses the label to draw a
/// procedural fallback texture when the image is missing, so the
/// operator at least sees a labelled patch projected instead of a
/// plain cone.
function resolveGoboRange(
  value: number,
  ranges: ChannelRange[],
): { image: string | null; label: string | null } {
  const range = ranges.find((r) => value >= r.from && value <= r.to);
  if (!range) return { image: null, label: null };
  if (isOpenGoboLabel(range.label)) return { image: null, label: null };
  const raw = range.image ?? range.image_path;
  if (!raw) return { image: null, label: range.label };
  if (raw.startsWith("data:")) return { image: raw, label: range.label };
  return { image: convertFileSrc(raw), label: range.label };
}

function decodeFixture(
  fixture: FixtureInstance,
  layout: ChannelLayout,
  snap: Uint8Array,
): FixtureLightState {
  const intensityRaw =
    layout.intensity !== null ? readByte(snap, fixture.address, layout.intensity) : 255;
  const intensity = layout.hasIntensityChannel ? intensityRaw / 255 : 1;

  // RGBW/A additive mix.
  const r = layout.red !== null ? readByte(snap, fixture.address, layout.red) / 255 : 0;
  const g = layout.green !== null ? readByte(snap, fixture.address, layout.green) / 255 : 0;
  const b = layout.blue !== null ? readByte(snap, fixture.address, layout.blue) / 255 : 0;
  const w = layout.white !== null ? readByte(snap, fixture.address, layout.white) / 255 : 0;
  const a = layout.amber !== null ? readByte(snap, fixture.address, layout.amber) / 255 : 0;

  let baseR = 0;
  let baseG = 0;
  let baseB = 0;

  // Colour resolution priority:
  //   1. Colour wheel selecting a non-open colour → wheel wins (the
  //      lamp is white and the wheel filters it; on a real fixture
  //      this physically replaces the white).
  //   2. RGB(W/A) mix if any channels are non-zero.
  //   3. Wheel "open"/"white" → white.
  //   4. Fallback warm white so the cone is at least visible.
  let wheelRgb: [number, number, number] | null = null;
  let wheelIsOpen = true;
  if (layout.colorWheel !== null) {
    const wv = readByte(snap, fixture.address, layout.colorWheel);
    const wc = wheelColor(wv, layout.colorWheelRanges);
    if (wc) {
      wheelRgb = wc;
      // "Open" / "white" / "none" → treat as transparent so RGB or
      // fallback can speak.
      wheelIsOpen = wc[0] === 1 && wc[1] === 1 && wc[2] === 1;
    }
  }

  if (wheelRgb && !wheelIsOpen) {
    [baseR, baseG, baseB] = wheelRgb;
  } else if (layout.hasRGB || layout.white !== null || layout.amber !== null) {
    baseR = Math.min(1, r + w + a);
    baseG = Math.min(1, g + w + a * 0.5);
    baseB = Math.min(1, b + w);
    // If all colour channels are 0 but there IS an intensity, fall
    // back to wheel (open-white) or warm white. Without this an
    // intensity-up RGB par looks black-on-black until the operator
    // also dials a colour.
    if (baseR === 0 && baseG === 0 && baseB === 0 && intensity > 0) {
      if (wheelRgb) {
        [baseR, baseG, baseB] = wheelRgb;
      } else {
        baseR = 1;
        baseG = 0.85;
        baseB = 0.7;
      }
    }
  } else if (wheelRgb) {
    [baseR, baseG, baseB] = wheelRgb;
  } else {
    baseR = 1;
    baseG = 0.85;
    baseB = 0.7;
  }

  const color = {
    r: baseR * intensity,
    g: baseG * intensity,
    b: baseB * intensity,
  };

  // Pan / tilt 8/16-bit decoding mapped to physical degrees.
  let panRaw = 0;
  let tiltRaw = 0;
  let panMaxRaw = 255;
  let tiltMaxRaw = 255;
  if (layout.pan !== null) {
    panRaw = readWord(snap, fixture.address, layout.pan, layout.panFine);
    panMaxRaw = layout.panFine !== null ? 0xffff : 0xff00;
  }
  if (layout.tilt !== null) {
    tiltRaw = readWord(snap, fixture.address, layout.tilt, layout.tiltFine);
    tiltMaxRaw = layout.tiltFine !== null ? 0xffff : 0xff00;
  }
  const panNorm = panMaxRaw > 0 ? panRaw / panMaxRaw : 0;
  const tiltNorm = tiltMaxRaw > 0 ? tiltRaw / tiltMaxRaw : 0;
  const pan = ((panNorm - 0.5) * layout.panDegrees * Math.PI) / 180;
  const tilt = ((tiltNorm - 0.5) * layout.tiltDegrees * Math.PI) / 180;

  const zoom = layout.zoom !== null ? readByte(snap, fixture.address, layout.zoom) / 255 : 0.5;
  const strobe = layout.strobe !== null ? readByte(snap, fixture.address, layout.strobe) / 255 : 0;

  const prismValue = layout.prism !== null ? readByte(snap, fixture.address, layout.prism) : 0;

  let goboImage: string | null = null;
  let goboLabel: string | null = null;
  if (layout.gobo !== null && layout.goboRanges.length > 0) {
    const gv = readByte(snap, fixture.address, layout.gobo);
    const resolved = resolveGoboRange(gv, layout.goboRanges);
    goboImage = resolved.image;
    goboLabel = resolved.label;
  }

  return {
    fixtureId: fixture.id,
    kind: layout.kind,
    color,
    intensity,
    pan,
    tilt,
    zoom,
    strobe,
    prismValue,
    goboImage,
    goboLabel,
  };
}

export function useFixtureLightStates(
  fixtures: FixtureInstance[],
  library: FixtureDefinition[],
  fps = 30,
): Record<string, FixtureLightState> {
  const [states, setStates] = useState<Record<string, FixtureLightState>>({});

  useEffect(() => {
    if (fixtures.length === 0) {
      setStates({});
      return;
    }
    const libById: Record<string, FixtureDefinition> = {};
    for (const d of library) libById[d.id] = d;
    const layouts: Record<string, ChannelLayout> = {};
    for (const f of fixtures) {
      layouts[f.id] = buildLayout(f, libById[f.definition_id]);
    }
    const universes = Array.from(new Set(fixtures.map((f) => f.universe)));

    let cancelled = false;
    let inFlight = false;
    const tick = async () => {
      if (cancelled || inFlight) return;
      inFlight = true;
      try {
        const snaps = await Promise.all(
          universes.map((u) =>
            invoke<UniverseSnapshot>("get_universe_output", { universe: u }).catch(() => null),
          ),
        );
        if (cancelled) return;
        const byUniverse: Record<number, Uint8Array> = {};
        for (let i = 0; i < universes.length; i++) {
          const s = snaps[i];
          if (s) byUniverse[universes[i]] = new Uint8Array(s.data);
        }
        const next: Record<string, FixtureLightState> = {};
        for (const f of fixtures) {
          const snap = byUniverse[f.universe];
          if (!snap) continue;
          next[f.id] = decodeFixture(f, layouts[f.id], snap);
        }
        setStates(next);
      } finally {
        inFlight = false;
      }
    };
    tick();
    const id = window.setInterval(tick, Math.max(16, Math.floor(1000 / fps)));
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [fixtures, library, fps]);

  return states;
}
