import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import {
  DndContext,
  type DragEndEvent,
  PointerSensor,
  useDraggable,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useRef, useState } from "react";
import { fixtureImageSrc } from "../lib/fixtureImage";
import { chaserColor, movementColor } from "../lib/launchpadColors";
import { useShowStore } from "../stores/show";
import { ColorPicker2D } from "./ColorPicker2D";
import { PanTiltPad } from "./PanTiltPad";

const GRID_SIZE = 32;
/// Fixed footprint of `.stage-fixture` in the CSS — the box is sized
/// rigidly so the marquee hit-test, drag-snap, and auto-grid layout
/// agree. If you change these, update the matching values in App.css.
const FIXTURE_W = 100;
const FIXTURE_H = 120;

type FxHit = { kind: "chaser" | "movement"; index: number; name: string; color: string };

function StageFixture({
  fixture,
  def,
  imageUrl,
  barColor,
  selected,
  effects,
  onSelect,
  onContextMenu,
}: {
  fixture: FixtureInstance;
  def: FixtureDefinition | undefined;
  imageUrl: string | null;
  barColor: string | null;
  selected: boolean;
  effects: FxHit[];
  onSelect: (e: React.MouseEvent) => void;
  onContextMenu: (e: React.MouseEvent) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, isDragging } = useDraggable({
    id: fixture.id,
  });
  const [x, y] = fixture.position;
  const dx = transform?.x ?? 0;
  const dy = transform?.y ?? 0;
  const labelText = fixture.label ?? def?.name ?? fixture.id;
  return (
    <button
      ref={setNodeRef}
      type="button"
      className={`stage-fixture${selected ? " selected" : ""}${isDragging ? " dragging" : ""}${imageUrl ? " has-image" : ""}`}
      onClick={(e) => onSelect(e)}
      onContextMenu={onContextMenu}
      style={{
        left: x,
        top: y,
        transform: `translate(${dx}px, ${dy}px)`,
      }}
      {...attributes}
      {...listeners}
    >
      {barColor ? (
        <span className="color-bar" style={{ background: barColor }} aria-hidden="true" />
      ) : null}
      {imageUrl ? <img className="fixture-thumb" src={imageUrl} alt="" /> : null}
      <div className="label">{labelText}</div>
      <div className="meta">
        U{fixture.universe} · {fixture.address}
      </div>
      {effects.length > 0 ? (
        <div className="fixture-fx-strip" aria-hidden="true">
          {effects.map((e) => (
            <span
              key={`${e.kind}-${e.index}`}
              className={`fixture-fx-dot ${e.kind}`}
              style={{ background: e.color }}
              title={`${e.kind === "chaser" ? "Chaser" : "Movement"}: ${e.name}`}
            />
          ))}
        </div>
      ) : null}
    </button>
  );
}

function TypeCard({
  def,
  count,
  imageUrl,
  active,
  onSelect,
}: {
  def: FixtureDefinition;
  count: number;
  imageUrl: string | null;
  active: boolean;
  onSelect: (e: React.MouseEvent) => void;
}) {
  return (
    <button
      type="button"
      className={`type-card${active ? " active" : ""}`}
      onClick={(e) => onSelect(e)}
      title={`Seleccionar las ${count} unidades (⌘/Ctrl-click para sumar)`}
    >
      <div className="type-thumb">
        {imageUrl ? (
          <img src={imageUrl} alt="" />
        ) : (
          <div className="type-thumb-placeholder" aria-hidden="true">
            ?
          </div>
        )}
        <span className="type-count">×{count}</span>
      </div>
      <div className="type-info">
        <div className="type-name">{def.name}</div>
        <div className="type-meta">{def.manufacturer}</div>
      </div>
    </button>
  );
}

function findRoleIndex(channels: { role: unknown }[], role: string): number {
  return channels.findIndex((c) => c.role === role);
}

/// Display name for a `ChannelRole`. Standard variants come through as
/// snake_case strings ("red", "pan_fine", …) and pass through as-is.
/// Custom roles arrive as `{ other: "function_speed" }` and we surface the
/// inner string instead of the literal label "other" — that way fixtures
/// with manufacturer-specific channels (function, function speed, purple,
/// strobe macros…) read meaningfully.
function roleLabel(role: unknown): string {
  if (typeof role === "string") return role;
  if (role && typeof role === "object" && "other" in role) {
    const inner = (role as { other: unknown }).other;
    if (typeof inner === "string" && inner.length > 0) return inner;
  }
  return "other";
}

/// Subtle unicode glyph hinting at what each role does. Monochromatic on
/// purpose so it doesn't compete with the slider value.
function roleIcon(role: unknown): string {
  const name = roleLabel(role);
  switch (name) {
    case "intensity":
      return "☀";
    case "red":
    case "green":
    case "blue":
    case "white":
    case "amber":
    case "uv":
    case "cyan":
    case "magenta":
    case "yellow":
      return "●";
    case "pan":
    case "pan_fine":
      return "⇄";
    case "tilt":
    case "tilt_fine":
      return "⇅";
    case "color_wheel":
      return "◐";
    case "gobo":
      return "✻";
    case "strobe":
      return "⚡";
    case "zoom":
      return "◌";
    case "focus":
      return "⊙";
    case "iris":
      return "◉";
    case "shutter":
      return "▤";
    default:
      // Custom (Other) channels — keep neutral so the role text carries
      // the meaning.
      return "·";
  }
}

/// Inline colour for the dot icons of named-colour roles, so a "red"
/// channel slider has a tiny red bullet next to it. Returns `null` for
/// roles that aren't a colour, leaving the icon at the default muted tone.
function roleIconColor(role: unknown): string | null {
  switch (roleLabel(role)) {
    case "red":
      return "#ff5050";
    case "green":
      return "#5cd07a";
    case "blue":
      return "#5a85ff";
    case "white":
      return "#e8e8e8";
    case "amber":
      return "#ffa040";
    case "uv":
      return "#9a5cff";
    case "cyan":
      return "#5cd0e0";
    case "magenta":
      return "#ff5cd0";
    case "yellow":
      return "#ffd84d";
    default:
      return null;
  }
}

function toHex2(n: number): string {
  return Math.max(0, Math.min(255, Math.round(n)))
    .toString(16)
    .padStart(2, "0");
}

function rgbToHex(r: number, g: number, b: number): string {
  return `#${toHex2(r)}${toHex2(g)}${toHex2(b)}`;
}

type RoleKey = "red" | "green" | "blue" | "intensity" | "pan" | "tilt" | "strobe";

type FixtureCtx = {
  fixture: FixtureInstance;
  def: FixtureDefinition;
  mode: NonNullable<FixtureDefinition["modes"][number]>;
  roleOffsets: Record<RoleKey, number>;
};

function resolveCtx(
  fixture: FixtureInstance,
  libraryById: Record<string, FixtureDefinition>,
): FixtureCtx | null {
  const def = libraryById[fixture.definition_id];
  const mode = def?.modes[fixture.mode_index];
  if (!def || !mode) return null;
  return {
    fixture,
    def,
    mode,
    roleOffsets: {
      red: findRoleIndex(mode.channels, "red"),
      green: findRoleIndex(mode.channels, "green"),
      blue: findRoleIndex(mode.channels, "blue"),
      intensity: findRoleIndex(mode.channels, "intensity"),
      pan: findRoleIndex(mode.channels, "pan"),
      tilt: findRoleIndex(mode.channels, "tilt"),
      strobe: findRoleIndex(mode.channels, "strobe"),
    },
  };
}

/// Editor for one or more selected fixtures. When the selection is mixed
/// (different defs/modes), only the controls common to *every* selected
/// fixture are shown — colour if all have R+G+B, pan/tilt if all have those
/// roles, and so on. Writes go to each fixture using its own role offsets, so
/// you can grab a moving head and a par together and flip them both red even
/// though red lives at a different channel inside each. Per-channel raw
/// sliders only appear when every selection shares the same definition+mode,
/// since the channel index has no shared meaning otherwise.
function FixtureChannelEditor({
  fixtures,
  libraryById,
  effectsByFixture,
}: {
  fixtures: FixtureInstance[];
  libraryById: Record<string, FixtureDefinition>;
  effectsByFixture: Record<string, FxHit[]>;
}) {
  const setFixtureChannel = useShowStore((s) => s.setFixtureChannel);
  const getFixtureValues = useShowStore((s) => s.getFixtureValues);
  const libraryDir = useShowStore((s) => s.libraryDir);

  const ctxs = useMemo<FixtureCtx[]>(() => {
    const out: FixtureCtx[] = [];
    for (const f of fixtures) {
      const c = resolveCtx(f, libraryById);
      if (c) out.push(c);
    }
    return out;
  }, [fixtures, libraryById]);

  const primary = ctxs[0] ?? null;

  const [primaryValues, setPrimaryValues] = useState<number[]>(() =>
    (primary?.mode.channels ?? []).map((c) => c.default ?? 0),
  );

  // Poll the engine for the live post-merge channel snapshot while the
  // editor is open. We need this (not just a one-shot read on selection)
  // because the chaser + movement engines animate the output behind our
  // back — a moving head dragged into a circle should *show* the circle
  // in the pan/tilt pad, not freeze at the user's last manual write.
  // 80 ms ≈ 12 Hz, plenty for visual smoothness without flooding IPC.
  useEffect(() => {
    if (!primary) return;
    let cancelled = false;
    let timer: number | null = null;
    const poll = () => {
      getFixtureValues(primary.fixture.id)
        .then((vs) => {
          if (!cancelled) setPrimaryValues(vs);
        })
        .catch(() => {})
        .finally(() => {
          if (!cancelled) {
            timer = window.setTimeout(poll, 80);
          }
        });
    };
    poll();
    return () => {
      cancelled = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [primary, getFixtureValues]);

  const has = (role: RoleKey) => ctxs.length > 0 && ctxs.every((c) => c.roleOffsets[role] >= 0);
  const hasColor = has("red") && has("green") && has("blue");
  const hasPanTilt = has("pan") && has("tilt");
  const hasIntensity = has("intensity");
  const hasStrobe = has("strobe");

  // Same definition + mode → safe to show raw per-channel sliders since the
  // channel index has the same meaning across all selected fixtures.
  const sameDef =
    ctxs.length > 0 &&
    ctxs.every(
      (c) => c.def.id === ctxs[0].def.id && c.fixture.mode_index === ctxs[0].fixture.mode_index,
    );

  // Distinct definitions for the header summary.
  const distinctDefs = useMemo(() => {
    const seen = new Set<string>();
    const list: FixtureDefinition[] = [];
    for (const c of ctxs) {
      if (!seen.has(c.def.id)) {
        seen.add(c.def.id);
        list.push(c.def);
      }
    }
    return list;
  }, [ctxs]);

  if (!primary) return <div className="channel-editor">Definición no disponible.</div>;

  const writeRole = (role: RoleKey, value: number) =>
    Promise.all(
      ctxs.map((c) => {
        const off = c.roleOffsets[role];
        return off >= 0 ? setFixtureChannel(c.fixture.id, off, value) : Promise.resolve();
      }),
    );

  // Optimistically updates the primary's local snapshot so the slider/picker
  // doesn't lag behind while the IPC writes settle.
  const updatePrimary = (writes: { offset: number; value: number }[]) => {
    setPrimaryValues((vs) => {
      const next = [...vs];
      for (const { offset, value } of writes) {
        if (offset >= 0) next[offset] = value;
      }
      return next;
    });
  };

  const writeChannel = async (i: number, v: number) => {
    if (!sameDef) return;
    updatePrimary([{ offset: i, value: v }]);
    await Promise.all(ctxs.map((c) => setFixtureChannel(c.fixture.id, i, v)));
  };

  // Per-channel sliders that the smart top-level widgets already cover.
  // Skip them from the per-channel list so we don't have two controls
  // racing on the same role.
  const coveredRoles = new Set<string>();
  if (hasIntensity) coveredRoles.add("intensity");
  if (hasStrobe) coveredRoles.add("strobe");
  if (hasColor) {
    coveredRoles.add("red");
    coveredRoles.add("green");
    coveredRoles.add("blue");
  }
  if (hasPanTilt) {
    coveredRoles.add("pan");
    coveredRoles.add("tilt");
  }

  const COLOR_ROLES = new Set([
    "red",
    "green",
    "blue",
    "white",
    "amber",
    "uv",
    "cyan",
    "magenta",
    "yellow",
    "color_wheel",
  ]);
  const INT_STROBE_ROLES = new Set(["intensity", "strobe", "shutter"]);

  type ChannelEntry = {
    channel: (typeof primary.mode.channels)[number];
    index: number;
  };
  const remainingChannels: ChannelEntry[] = sameDef
    ? primary.mode.channels
        .map((channel, index) => ({ channel, index }))
        .filter(({ channel }) => !coveredRoles.has(roleLabel(channel.role)))
    : [];
  const intStrobeChannels = remainingChannels.filter((c) =>
    INT_STROBE_ROLES.has(roleLabel(c.channel.role)),
  );
  const colorChannels = remainingChannels.filter((c) => COLOR_ROLES.has(roleLabel(c.channel.role)));
  const extrasChannels = remainingChannels.filter(
    (c) =>
      !INT_STROBE_ROLES.has(roleLabel(c.channel.role)) &&
      !COLOR_ROLES.has(roleLabel(c.channel.role)),
  );

  /// Pick the DMX value to send when the user clicks a range button.
  /// Manufacturers usually treat the midpoint of each range as the
  /// "canonical" value for that step (e.g. for "Red: 13–25" the wheel
  /// definitely sits on red at 19). Using that instead of `from` avoids
  /// landing on a transition edge between two ranges.
  const valueForRange = (r: { from: number; to: number }): number =>
    Math.round((r.from + r.to) / 2);

  const renderChannelControl = (entry: ChannelEntry) => {
    const iconColor = roleIconColor(entry.channel.role);
    const value = primaryValues[entry.index] ?? 0;
    const ranges = entry.channel.ranges ?? [];
    const hasRanges = ranges.length > 0;
    const displayName = entry.channel.name ?? roleLabel(entry.channel.role);
    return (
      <div key={`${primary.fixture.id}-${entry.index}`} className="ch-block">
        <label className="ch">
          <span
            className="role-icon"
            style={iconColor ? { color: iconColor } : undefined}
            aria-hidden="true"
          >
            {roleIcon(entry.channel.role)}
          </span>
          <span className="role" title={entry.channel.description ?? undefined}>
            {displayName}
          </span>
          <input
            type="range"
            min={0}
            max={255}
            value={value}
            onChange={(e) => writeChannel(entry.index, Number(e.currentTarget.value))}
          />
          <span className="val">{value}</span>
        </label>
        {hasRanges ? (
          <div className="range-buttons">
            {ranges.map((r) => {
              const target = valueForRange(r);
              const active = value >= r.from && value <= r.to;
              const thumb = fixtureImageSrc(r.image, libraryDir);
              return (
                <button
                  key={`${entry.index}-${r.from}-${r.to}`}
                  type="button"
                  className={`range-btn${active ? " active" : ""}${thumb ? "" : " no-thumb"}`}
                  title={`${r.label} (${r.from}–${r.to})`}
                  onClick={() => writeChannel(entry.index, target)}
                >
                  {thumb ? (
                    <img src={thumb} alt="" />
                  ) : (
                    <span className="range-btn-fallback" aria-hidden="true">
                      {r.label.slice(0, 2)}
                    </span>
                  )}
                  <span className="range-btn-label">{r.label}</span>
                </button>
              );
            })}
          </div>
        ) : null}
      </div>
    );
  };
  const showIntStrobeSection = hasIntensity || hasStrobe || intStrobeChannels.length > 0;

  const writeColor = async (r: number, g: number, b: number) => {
    if (!hasColor) return;
    updatePrimary([
      { offset: primary.roleOffsets.red, value: r },
      { offset: primary.roleOffsets.green, value: g },
      { offset: primary.roleOffsets.blue, value: b },
    ]);
    await Promise.all([writeRole("red", r), writeRole("green", g), writeRole("blue", b)]);
  };

  const writePanTilt = async (pan: number, tilt: number) => {
    if (!hasPanTilt) return;
    updatePrimary([
      { offset: primary.roleOffsets.pan, value: pan },
      { offset: primary.roleOffsets.tilt, value: tilt },
    ]);
    await Promise.all([writeRole("pan", pan), writeRole("tilt", tilt)]);
  };

  const writeIntensity = async (v: number) => {
    if (!hasIntensity) return;
    updatePrimary([{ offset: primary.roleOffsets.intensity, value: v }]);
    await writeRole("intensity", v);
  };

  const writeStrobe = async (v: number) => {
    if (!hasStrobe) return;
    updatePrimary([{ offset: primary.roleOffsets.strobe, value: v }]);
    await writeRole("strobe", v);
  };

  const primaryRGB = hasColor
    ? {
        r: primaryValues[primary.roleOffsets.red] ?? 0,
        g: primaryValues[primary.roleOffsets.green] ?? 0,
        b: primaryValues[primary.roleOffsets.blue] ?? 0,
      }
    : null;
  const primaryHex = primaryRGB ? rgbToHex(primaryRGB.r, primaryRGB.g, primaryRGB.b) : "#000000";
  const primaryPan = hasPanTilt ? (primaryValues[primary.roleOffsets.pan] ?? 0) : 0;
  const primaryTilt = hasPanTilt ? (primaryValues[primary.roleOffsets.tilt] ?? 0) : 0;
  const primaryIntensity = hasIntensity ? (primaryValues[primary.roleOffsets.intensity] ?? 0) : 0;
  const primaryStrobe = hasStrobe ? (primaryValues[primary.roleOffsets.strobe] ?? 0) : 0;

  return (
    <div className="channel-editor">
      <h4>
        {sameDef ? (
          <>
            {primary.def.manufacturer} {primary.def.name} — {primary.mode.name}
          </>
        ) : (
          <>
            {ctxs.length} fixtures · {distinctDefs.length} tipos
          </>
        )}
        {ctxs.length > 1 ? <span className="multi-badge">×{ctxs.length}</span> : null}
      </h4>

      {!sameDef ? (
        <p className="mixed-hint">
          Mostrando solo los controles comunes a todas las unidades seleccionadas.
        </p>
      ) : null}

      {(() => {
        // Aggregate every distinct effect (chaser/movement) currently
        // driving any selected fixture. We dedupe by `kind+index` so a
        // chaser that drives 5 of the selected pars only shows up once;
        // the count below the chip tells you how many of the selection
        // it touches.
        const seen = new Map<string, { hit: FxHit; touches: number }>();
        for (const f of fixtures) {
          for (const h of effectsByFixture[f.id] ?? []) {
            const k = `${h.kind}-${h.index}`;
            const prev = seen.get(k);
            if (prev) prev.touches += 1;
            else seen.set(k, { hit: h, touches: 1 });
          }
        }
        const entries = Array.from(seen.values());
        if (entries.length === 0) return null;
        return (
          <section className="editor-active-fx">
            <h5>Efectos activos</h5>
            <div className="editor-active-fx-list">
              {entries.map(({ hit, touches }) => (
                <span
                  key={`${hit.kind}-${hit.index}`}
                  className={`editor-fx-chip ${hit.kind}`}
                  style={{ borderColor: hit.color }}
                  title={`${hit.kind === "chaser" ? "Chaser" : "Movement"}: ${hit.name} · afecta ${touches}/${fixtures.length}`}
                >
                  <span className="editor-fx-chip-dot" style={{ background: hit.color }} />
                  <span className="editor-fx-chip-kind">
                    {hit.kind === "chaser" ? "Chaser" : "Move"}
                  </span>
                  <span className="editor-fx-chip-name">{hit.name}</span>
                  {fixtures.length > 1 ? (
                    <span className="editor-fx-chip-count">
                      {touches}/{fixtures.length}
                    </span>
                  ) : null}
                </span>
              ))}
            </div>
          </section>
        );
      })()}

      {hasPanTilt || (hasColor && primaryRGB) ? (
        <div className="editor-grid">
          {hasPanTilt ? (
            <div className="pan-tilt-block">
              <PanTiltPad pan={primaryPan} tilt={primaryTilt} onChange={writePanTilt} />
              <div className="pan-tilt-row">
                <span>P {primaryPan}</span>
                <span>T {primaryTilt}</span>
                <button type="button" onClick={() => writePanTilt(128, 128)}>
                  Center
                </button>
                <button type="button" onClick={() => writePanTilt(0, 0)}>
                  Home
                </button>
              </div>
            </div>
          ) : null}

          {hasColor && primaryRGB ? (
            <div className="color-picker">
              <ColorPicker2D
                r={primaryRGB.r}
                g={primaryRGB.g}
                b={primaryRGB.b}
                onChange={writeColor}
              />
              <div className="color-row">
                <span
                  className="color-swatch"
                  style={{ background: primaryHex }}
                  aria-hidden="true"
                />
                <span className="color-hex">{primaryHex.toUpperCase()}</span>
                <span className="color-rgb">
                  R{primaryRGB.r} G{primaryRGB.g} B{primaryRGB.b}
                </span>
              </div>
              <div className="color-presets">
                {[
                  ["off", 0, 0, 0],
                  ["W", 255, 255, 255],
                  ["R", 255, 0, 0],
                  ["G", 0, 255, 0],
                  ["B", 0, 0, 255],
                  ["A", 255, 128, 0],
                  ["M", 255, 0, 255],
                  ["C", 0, 255, 255],
                  ["Y", 255, 255, 0],
                ].map((p) => {
                  const [label, r, g, b] = p as [string, number, number, number];
                  return (
                    <button
                      type="button"
                      key={label}
                      className="preset"
                      style={{ background: rgbToHex(r, g, b) }}
                      title={label}
                      onClick={() => writeColor(r, g, b)}
                    >
                      {label}
                    </button>
                  );
                })}
              </div>
            </div>
          ) : null}
        </div>
      ) : null}

      {showIntStrobeSection ? (
        <section className="editor-subsection">
          <h5>Intensidad y estrobo</h5>
          <div className="editor-channels">
            {hasIntensity ? (
              <label className="ch">
                <span className="role-icon" aria-hidden="true">
                  {roleIcon("intensity")}
                </span>
                <span className="role">intensity</span>
                <input
                  type="range"
                  min={0}
                  max={255}
                  value={primaryIntensity}
                  onChange={(e) => writeIntensity(Number(e.currentTarget.value))}
                />
                <span className="val">{primaryIntensity}</span>
              </label>
            ) : null}
            {hasStrobe ? (
              <label className="ch">
                <span className="role-icon" aria-hidden="true">
                  {roleIcon("strobe")}
                </span>
                <span className="role">strobe</span>
                <input
                  type="range"
                  min={0}
                  max={255}
                  value={primaryStrobe}
                  onChange={(e) => writeStrobe(Number(e.currentTarget.value))}
                />
                <span className="val">{primaryStrobe}</span>
              </label>
            ) : null}
            {intStrobeChannels.map(renderChannelControl)}
          </div>
        </section>
      ) : null}

      {colorChannels.length > 0 ? (
        <section className="editor-subsection">
          <h5>Color</h5>
          <div className="editor-channels">{colorChannels.map(renderChannelControl)}</div>
        </section>
      ) : null}

      {extrasChannels.length > 0 ? (
        <section className="editor-subsection">
          <h5>Extras</h5>
          <div className="editor-channels">{extrasChannels.map(renderChannelControl)}</div>
        </section>
      ) : null}
    </div>
  );
}

const SIDE_MIN = 260;
const SIDE_MAX = 900;
const SIDE_STORAGE_KEY = "stage.sideWidth";

function clampSideWidth(n: number): number {
  if (!Number.isFinite(n)) return 320;
  return Math.max(SIDE_MIN, Math.min(SIDE_MAX, n));
}

/// Quick-toggle dashboard for FX layers. Always visible at the bottom of
/// the Stage so the operator can flip chasers / the movement generator
/// on/off without leaving the canvas.
function StageFxBar() {
  const show = useShowStore((s) => s.show);
  const toggleChaser = useShowStore((s) => s.toggleChaser);
  const toggleMovement = useShowStore((s) => s.toggleMovement);
  const chasers = show?.chasers ?? [];
  const movements = show?.movements ?? [];

  if (chasers.length === 0 && movements.length === 0) return null;

  return (
    <footer className="stage-fx-bar" aria-label="Active effects">
      {movements.length > 0 ? (
        <div className="stage-fx-section">
          <span className="stage-fx-label">Movements</span>
          {movements.map((m, i) => {
            const padColor = movementColor(i);
            return (
              <button
                key={m.id}
                type="button"
                className={`stage-fx-toggle${m.enabled ? " on" : ""}`}
                onClick={() => toggleMovement(m.id, !m.enabled)}
                title={
                  m.enabled ? `Disable ${m.name}` : `Enable ${m.name} (${m.fixtures.length} slots)`
                }
                style={
                  m.enabled
                    ? { background: padColor, borderColor: padColor, color: "#0f0f10" }
                    : { borderColor: padColor }
                }
              >
                <span
                  className="stage-fx-led"
                  aria-hidden="true"
                  style={{ background: padColor }}
                />
                <span className="stage-fx-name">{m.name}</span>
                <span className="stage-fx-meta">{m.fixtures.length}</span>
              </button>
            );
          })}
        </div>
      ) : null}
      {chasers.length > 0 ? (
        <div className="stage-fx-section">
          <span className="stage-fx-label">Chasers</span>
          {chasers.map((c, i) => {
            const padColor = chaserColor(i);
            const colorPip =
              c.color_mode.type === "single"
                ? `rgb(${c.color_mode.color.r}, ${c.color_mode.color.g}, ${c.color_mode.color.b})`
                : null;
            return (
              <button
                key={c.id}
                type="button"
                className={`stage-fx-toggle${c.enabled ? " on" : ""}`}
                onClick={() => toggleChaser(c.id, !c.enabled)}
                title={
                  c.enabled ? `Disable ${c.name}` : `Enable ${c.name} (${c.slots.length} slots)`
                }
                style={
                  c.enabled
                    ? {
                        background: padColor,
                        borderColor: padColor,
                        color: "#0f0f10",
                      }
                    : { borderColor: padColor }
                }
              >
                <span
                  className="stage-fx-led"
                  aria-hidden="true"
                  style={{ background: padColor }}
                />
                <span className="stage-fx-name">{c.name}</span>
                {colorPip ? (
                  <span
                    className="stage-fx-color-pip"
                    style={{ background: colorPip }}
                    aria-hidden="true"
                  />
                ) : null}
                <span className="stage-fx-meta">{c.slots.length}</span>
              </button>
            );
          })}
        </div>
      ) : null}
    </footer>
  );
}

type Marquee = { x0: number; y0: number; x1: number; y1: number };
type ContextMenuState = { x: number; y: number; fixtureId: string };

/// Right-click menu for one or more fixtures. Closes on outside click and
/// on Escape — both handled via window listeners installed while it's
/// mounted. Position is in viewport (clientX/Y) coords, then nudged left
/// or up if it would clip the screen edge.
function FixtureContextMenu({
  x,
  y,
  multi,
  count,
  onClose,
  onCenterPanTilt,
  onPark,
  onFullIntensity,
  onBlackout,
  onRename,
  onDuplicate,
  onRemove,
}: {
  x: number;
  y: number;
  multi: boolean;
  count: number;
  onClose: () => void;
  onCenterPanTilt: () => void;
  onPark: () => void;
  onFullIntensity: () => void;
  onBlackout: () => void;
  onRename: () => void;
  onDuplicate: () => void;
  onRemove: () => void;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const onDocPointerDown = (e: PointerEvent) => {
      if (!ref.current) return;
      if (!ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("pointerdown", onDocPointerDown, true);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointerdown", onDocPointerDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  // Estimate menu size to keep it on-screen. Real measurement would
  // require a layout pass; this rough cap matches the CSS max-width.
  const W = 220;
  const H = 280;
  const left = Math.min(x, window.innerWidth - W - 4);
  const top = Math.min(y, window.innerHeight - H - 4);

  return (
    <div ref={ref} className="fixture-context-menu" style={{ left, top }} role="menu">
      <div className="fcm-header">{multi ? `${count} fixtures` : "Fixture"}</div>
      <button type="button" className="fcm-item" onClick={onCenterPanTilt}>
        Centrar Pan/Tilt
      </button>
      <button type="button" className="fcm-item" onClick={onPark}>
        Park (defaults)
      </button>
      <div className="fcm-sep" aria-hidden="true" />
      <button type="button" className="fcm-item" onClick={onFullIntensity}>
        Intensidad al máximo
      </button>
      <button type="button" className="fcm-item" onClick={onBlackout}>
        Blackout (intensity 0)
      </button>
      <div className="fcm-sep" aria-hidden="true" />
      {!multi ? (
        <button type="button" className="fcm-item" onClick={onRename}>
          Renombrar…
        </button>
      ) : null}
      <button type="button" className="fcm-item" onClick={onDuplicate}>
        Duplicar{multi ? ` (×${count})` : ""}
      </button>
      <button type="button" className="fcm-item danger" onClick={onRemove}>
        Eliminar{multi ? ` (×${count})` : ""}
      </button>
    </div>
  );
}

export function StageView() {
  const show = useShowStore((s) => s.show);
  const library = useShowStore((s) => s.library);
  const libraryDir = useShowStore((s) => s.libraryDir);
  const moveFixture = useShowStore((s) => s.moveFixture);
  const setFixtureChannel = useShowStore((s) => s.setFixtureChannel);
  const removeFixture = useShowStore((s) => s.removeFixture);
  const updateFixture = useShowStore((s) => s.updateFixture);
  const addFixtures = useShowStore((s) => s.addFixtures);
  const stageRef = useRef<HTMLDivElement | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [marquee, setMarquee] = useState<Marquee | null>(null);
  const [menu, setMenu] = useState<ContextMenuState | null>(null);

  // Right-panel width is user-resizable via the splitter and persisted across
  // sessions. Bounded so the canvas never disappears entirely.
  const [sideWidth, setSideWidth] = useState<number>(() => {
    try {
      const saved = localStorage.getItem(SIDE_STORAGE_KEY);
      return clampSideWidth(saved ? Number(saved) : 320);
    } catch {
      return 320;
    }
  });
  const [splitterActive, setSplitterActive] = useState(false);
  useEffect(() => {
    try {
      localStorage.setItem(SIDE_STORAGE_KEY, String(sideWidth));
    } catch {
      // localStorage unavailable (private mode etc.) — width still works
      // in-session, just won't survive a restart.
    }
  }, [sideWidth]);

  const onSplitterDown = (e: React.PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = sideWidth;
    setSplitterActive(true);
    const onMove = (ev: PointerEvent) => {
      // Splitter sits on the left edge of the side panel: dragging left
      // (negative dx) makes the panel wider.
      const dx = startX - ev.clientX;
      setSideWidth(clampSideWidth(startW + dx));
    };
    const onUp = () => {
      setSplitterActive(false);
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };
    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  };

  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }));

  const libraryById = useMemo(() => {
    const m: Record<string, FixtureDefinition> = {};
    for (const d of library) m[d.id] = d;
    return m;
  }, [library]);

  const imageUrlByDef = useMemo(() => {
    const m: Record<string, string> = {};
    for (const d of library) {
      const url = fixtureImageSrc(d.image, libraryDir);
      if (url) m[d.id] = url;
    }
    return m;
  }, [library, libraryDir]);

  // Group fixtures by definition for the left sidebar. Definition order
  // follows first-appearance in `show.fixtures` so the layout is stable as the
  // user adds units.
  const groups = useMemo(() => {
    const order: string[] = [];
    const map = new Map<string, FixtureInstance[]>();
    for (const f of show?.fixtures ?? []) {
      let arr = map.get(f.definition_id);
      if (!arr) {
        arr = [];
        map.set(f.definition_id, arr);
        order.push(f.definition_id);
      }
      arr.push(f);
    }
    return order
      .map((id) => ({
        defId: id,
        def: libraryById[id],
        fixtures: map.get(id) ?? [],
      }))
      .filter((g) => g.def);
  }, [show?.fixtures, libraryById]);

  const selectedFixtures = useMemo(() => {
    if (!show) return [];
    return show.fixtures.filter((f) => selectedIds.has(f.id));
  }, [show, selectedIds]);

  // Index "this fixture is being driven by these enabled effects, in
  // these colours". Cheap to recompute on every store update — at most
  // a few dozen chasers/movements × a few slots each. Keeping this in
  // one place (instead of two filter passes per fixture) means the
  // stage badges and the editor "Active effects" panel share the same
  // numbering, so the colours always match.
  const effectsByFixture = useMemo(() => {
    type Hit = { kind: "chaser" | "movement"; index: number; name: string; color: string };
    const out: Record<string, Hit[]> = {};
    if (!show) return out;
    show.chasers.forEach((c, i) => {
      if (!c.enabled) return;
      for (const slot of c.slots) {
        if (!out[slot.fixture_id]) out[slot.fixture_id] = [];
        out[slot.fixture_id].push({
          kind: "chaser",
          index: i,
          name: c.name,
          color: chaserColor(i),
        });
      }
    });
    show.movements.forEach((m, i) => {
      if (!m.enabled) return;
      for (const slot of m.fixtures) {
        if (!out[slot.fixture_id]) out[slot.fixture_id] = [];
        out[slot.fixture_id].push({
          kind: "movement",
          index: i,
          name: m.name,
          color: movementColor(i),
        });
      }
    });
    return out;
  }, [show]);

  // Universes that need polling for the per-fixture color bar. Stable under
  // fixture-position drags (which don't change the universe set).
  const universesKey = useMemo(() => {
    const set = new Set<number>();
    for (const f of show?.fixtures ?? []) set.add(f.universe);
    return Array.from(set)
      .sort((a, b) => a - b)
      .join(",");
  }, [show?.fixtures]);

  // Polled universe snapshots. ~5 Hz keeps the bar visibly responsive without
  // saturating IPC; each snapshot is 512 bytes so traffic is negligible.
  const [universesData, setUniversesData] = useState<Record<number, number[]>>({});
  useEffect(() => {
    if (universesKey === "") {
      setUniversesData({});
      return;
    }
    const universes = universesKey.split(",").map(Number);
    let cancelled = false;
    const refresh = async () => {
      try {
        const results = await Promise.all(
          universes.map((u) =>
            invoke<{ id: number; data: number[] }>("get_universe_output", {
              universe: u,
            }).catch(() => null),
          ),
        );
        if (cancelled) return;
        const next: Record<number, number[]> = {};
        for (const r of results) {
          if (r) next[r.id] = r.data;
        }
        setUniversesData(next);
      } catch {
        // best-effort; next tick will retry
      }
    };
    refresh();
    const id = setInterval(refresh, 200);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [universesKey]);

  // Compute the color string shown in each fixture's top bar from the polled
  // universe snapshot. RGB is multiplied by intensity (master/dimmer) when
  // present so the bar reflects the actual output, not just the picker state.
  // Fixtures without RGB fall back to a grey ramp driven by intensity. Returns
  // `null` when there's nothing meaningful to show (e.g. mode without color or
  // intensity channels).
  const barColorByFixture = useMemo(() => {
    const out: Record<string, string | null> = {};
    if (!show) return out;
    for (const f of show.fixtures) {
      const def = libraryById[f.definition_id];
      const mode = def?.modes[f.mode_index];
      const data = universesData[f.universe];
      if (!mode || !data) {
        out[f.id] = null;
        continue;
      }
      const start = f.address - 1;
      const slice = data.slice(start, start + mode.channels.length);
      const ri = findRoleIndex(mode.channels, "red");
      const gi = findRoleIndex(mode.channels, "green");
      const bi = findRoleIndex(mode.channels, "blue");
      const ii = findRoleIndex(mode.channels, "intensity");
      const hasColor = ri >= 0 || gi >= 0 || bi >= 0;
      if (!hasColor && ii < 0) {
        out[f.id] = null;
        continue;
      }
      let r = ri >= 0 ? (slice[ri] ?? 0) : 0;
      let g = gi >= 0 ? (slice[gi] ?? 0) : 0;
      let b = bi >= 0 ? (slice[bi] ?? 0) : 0;
      if (!hasColor) {
        const v = slice[ii] ?? 0;
        r = g = b = v;
      } else if (ii >= 0) {
        const factor = (slice[ii] ?? 0) / 255;
        r = Math.round(r * factor);
        g = Math.round(g * factor);
        b = Math.round(b * factor);
      }
      out[f.id] = `rgb(${r}, ${g}, ${b})`;
    }
    return out;
  }, [show, libraryById, universesData]);

  if (!show) return <main className="page">Cargando…</main>;

  const onDragEnd = (e: DragEndEvent) => {
    const id = String(e.active.id);
    const f = show.fixtures.find((x) => x.id === id);
    if (!f) return;
    const [x, y] = f.position;
    const nx = Math.round((x + e.delta.x) / GRID_SIZE) * GRID_SIZE;
    const ny = Math.round((y + e.delta.y) / GRID_SIZE) * GRID_SIZE;
    moveFixture(id, Math.max(0, nx), Math.max(0, ny));
  };

  // Cmd/Ctrl-click toggles the target into the current selection so the user
  // can compose mixed-type groups (e.g. 4 movers + 2 pars to colour together).
  // A plain click replaces the selection.
  const onFixtureClick = (id: string, e: React.MouseEvent) => {
    if (e.metaKey || e.ctrlKey) {
      setSelectedIds((prev) => {
        const next = new Set(prev);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        return next;
      });
    } else {
      setSelectedIds(new Set([id]));
    }
  };
  const onTypeClick = (fixtures: FixtureInstance[], e: React.MouseEvent) => {
    const ids = fixtures.map((f) => f.id);
    if (e.metaKey || e.ctrlKey) {
      setSelectedIds((prev) => {
        const next = new Set(prev);
        const allIn = ids.every((id) => next.has(id));
        if (allIn) for (const id of ids) next.delete(id);
        else for (const id of ids) next.add(id);
        return next;
      });
    } else {
      setSelectedIds(new Set(ids));
    }
  };

  // Editor key is the sorted selection so React resets local state cleanly on
  // any selection change (single → group, group → single, group A → group B).
  const editorKey = Array.from(selectedIds).sort().join(",");

  // ---- Marquee selection ------------------------------------------------
  // Pointerdown on the empty canvas (not on a fixture) starts a drag
  // rectangle. We deliberately attach pointermove/pointerup to `window`
  // for the duration of the drag — pointer-capture on the React
  // synthetic event would race with dnd-kit's PointerSensor and was
  // dropping events when the cursor crossed a fixture mid-drag.
  // Window listeners + a closure over `start` keep the geometry stable
  // and never fight the draggables.
  const onCanvasPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    if (e.target !== e.currentTarget) return; // bubbled from a fixture/etc.
    const canvas = e.currentTarget;
    const rect = canvas.getBoundingClientRect();
    const toLocal = (cx: number, cy: number) => ({
      x: cx - rect.left + canvas.scrollLeft,
      y: cy - rect.top + canvas.scrollTop,
    });
    const start = toLocal(e.clientX, e.clientY);
    const additive = e.shiftKey;
    setMarquee({ x0: start.x, y0: start.y, x1: start.x, y1: start.y });

    const onMove = (ev: PointerEvent) => {
      const p = toLocal(ev.clientX, ev.clientY);
      setMarquee((m) => (m ? { ...m, x1: p.x, y1: p.y } : m));
    };
    const onUp = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      const end = toLocal(ev.clientX, ev.clientY);
      const minX = Math.min(start.x, end.x);
      const maxX = Math.max(start.x, end.x);
      const minY = Math.min(start.y, end.y);
      const maxY = Math.max(start.y, end.y);
      // Tiny click-drag (< 4px) → treat as a click on empty canvas:
      // clear selection unless additive. Avoids losing a selection just
      // because the user clicked a stray spot.
      if (maxX - minX < 4 && maxY - minY < 4) {
        if (!additive) setSelectedIds(new Set());
        setMarquee(null);
        return;
      }
      const hits = new Set<string>();
      for (const f of show?.fixtures ?? []) {
        // Hit-test against the fixture's actual rendered footprint
        // (defined as constants alongside the CSS) so the marquee
        // catches everything visually inside the rectangle.
        const fx0 = f.position[0];
        const fy0 = f.position[1];
        const fx1 = fx0 + FIXTURE_W;
        const fy1 = fy0 + FIXTURE_H;
        const overlap = fx0 <= maxX && fx1 >= minX && fy0 <= maxY && fy1 >= minY;
        if (overlap) hits.add(f.id);
      }
      setSelectedIds((prev) => {
        if (additive) {
          const next = new Set(prev);
          for (const id of hits) next.add(id);
          return next;
        }
        return hits;
      });
      setMarquee(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
  };

  // ---- Context menu -----------------------------------------------------
  const onFixtureContextMenu = (id: string) => (e: React.MouseEvent) => {
    e.preventDefault();
    // Right-clicking a non-selected fixture selects just that one, so
    // the "Park", "Duplicate", etc. actions don't accidentally fire on
    // the whole previous selection.
    setSelectedIds((prev) => (prev.has(id) ? prev : new Set([id])));
    setMenu({ x: e.clientX, y: e.clientY, fixtureId: id });
  };

  const closeMenu = () => setMenu(null);

  const writeChannelToFixtures = async (
    targets: FixtureInstance[],
    role: string,
    value: number,
  ) => {
    await Promise.all(
      targets.flatMap((f) => {
        const def = libraryById[f.definition_id];
        const mode = def?.modes[f.mode_index];
        if (!mode) return [];
        const offset = mode.channels.findIndex((c) => roleLabel(c.role) === role);
        if (offset < 0) return [];
        return [setFixtureChannel(f.id, offset, value)];
      }),
    );
  };

  const targetsForAction = (): FixtureInstance[] => {
    if (!show || !menu) return [];
    if (selectedIds.has(menu.fixtureId) && selectedIds.size > 1) {
      return show.fixtures.filter((f) => selectedIds.has(f.id));
    }
    const f = show.fixtures.find((x) => x.id === menu.fixtureId);
    return f ? [f] : [];
  };

  const actionCenterPanTilt = async () => {
    const targets = targetsForAction();
    await Promise.all([
      writeChannelToFixtures(targets, "pan", 127),
      writeChannelToFixtures(targets, "pan_fine", 127),
      writeChannelToFixtures(targets, "tilt", 127),
      writeChannelToFixtures(targets, "tilt_fine", 127),
    ]);
    closeMenu();
  };

  const actionPark = async () => {
    const targets = targetsForAction();
    await Promise.all(
      targets.flatMap((f) => {
        const mode = libraryById[f.definition_id]?.modes[f.mode_index];
        if (!mode) return [];
        return mode.channels.map((ch, i) => setFixtureChannel(f.id, i, ch.default ?? 0));
      }),
    );
    closeMenu();
  };

  const actionFullIntensity = async () => {
    await writeChannelToFixtures(targetsForAction(), "intensity", 255);
    closeMenu();
  };

  const actionBlackoutFixture = async () => {
    await writeChannelToFixtures(targetsForAction(), "intensity", 0);
    closeMenu();
  };

  const actionRename = () => {
    if (!show || !menu) return closeMenu();
    const f = show.fixtures.find((x) => x.id === menu.fixtureId);
    if (!f) return closeMenu();
    const next = window.prompt("Nuevo nombre para el fixture:", f.label ?? f.id);
    if (next !== null) {
      const trimmed = next.trim();
      updateFixture({ ...f, label: trimmed === "" ? null : trimmed });
    }
    closeMenu();
  };

  const actionDuplicate = async () => {
    if (!show || !menu) return closeMenu();
    const targets = targetsForAction();
    if (targets.length === 0) return closeMenu();
    const used = new Set(show.fixtures.map((f) => f.id));
    const dupes: FixtureInstance[] = targets.map((f) => {
      let n = 2;
      let candidate = `${f.id}-copy`;
      while (used.has(candidate)) {
        candidate = `${f.id}-copy-${n++}`;
      }
      used.add(candidate);
      return {
        ...f,
        id: candidate,
        position: [f.position[0] + GRID_SIZE * 3, f.position[1]],
        label: f.label ? `${f.label} copy` : null,
      };
    });
    await addFixtures(dupes);
    closeMenu();
  };

  const actionRemove = async () => {
    const targets = targetsForAction();
    if (targets.length === 0) return closeMenu();
    const ok = window.confirm(
      targets.length === 1
        ? `¿Eliminar "${targets[0].label ?? targets[0].id}"?`
        : `¿Eliminar ${targets.length} fixtures?`,
    );
    if (!ok) return closeMenu();
    for (const f of targets) {
      await removeFixture(f.id);
    }
    setSelectedIds((prev) => {
      const next = new Set(prev);
      for (const f of targets) next.delete(f.id);
      return next;
    });
    closeMenu();
  };

  return (
    <main className="page stage-view">
      <header className="page-head">
        <h2>Stage</h2>
        <span className="meta">
          {show.fixtures.length} fixtures · grid {GRID_SIZE}px
          {selectedIds.size > 0 ? ` · ${selectedIds.size} seleccionados` : ""}
        </span>
      </header>
      <div className="stage-and-panel">
        <aside className="stage-types">
          {groups.length === 0 ? (
            <p className="empty">Sin fixtures.</p>
          ) : (
            groups.map(({ defId, def, fixtures }) => {
              const allSelected =
                fixtures.length > 0 && fixtures.every((f) => selectedIds.has(f.id));
              if (!def) return null;
              return (
                <TypeCard
                  key={defId}
                  def={def}
                  count={fixtures.length}
                  imageUrl={imageUrlByDef[defId] ?? null}
                  active={allSelected}
                  onSelect={(e) => onTypeClick(fixtures, e)}
                />
              );
            })
          )}
        </aside>
        <DndContext sensors={sensors} onDragEnd={onDragEnd}>
          <div ref={stageRef} className="stage-canvas" onPointerDown={onCanvasPointerDown}>
            {show.fixtures.map((f) => (
              <StageFixture
                key={f.id}
                fixture={f}
                def={libraryById[f.definition_id]}
                imageUrl={imageUrlByDef[f.definition_id] ?? null}
                barColor={barColorByFixture[f.id] ?? null}
                selected={selectedIds.has(f.id)}
                effects={effectsByFixture[f.id] ?? []}
                onSelect={(e) => onFixtureClick(f.id, e)}
                onContextMenu={onFixtureContextMenu(f.id)}
              />
            ))}
            {marquee ? (
              <div
                className="stage-marquee"
                style={{
                  left: Math.min(marquee.x0, marquee.x1),
                  top: Math.min(marquee.y0, marquee.y1),
                  width: Math.abs(marquee.x1 - marquee.x0),
                  height: Math.abs(marquee.y1 - marquee.y0),
                }}
                aria-hidden="true"
              />
            ) : null}
          </div>
        </DndContext>
        {menu ? (
          <FixtureContextMenu
            x={menu.x}
            y={menu.y}
            multi={selectedIds.has(menu.fixtureId) && selectedIds.size > 1}
            count={selectedIds.has(menu.fixtureId) && selectedIds.size > 1 ? selectedIds.size : 1}
            onClose={closeMenu}
            onCenterPanTilt={actionCenterPanTilt}
            onPark={actionPark}
            onFullIntensity={actionFullIntensity}
            onBlackout={actionBlackoutFixture}
            onRename={actionRename}
            onDuplicate={actionDuplicate}
            onRemove={actionRemove}
          />
        ) : null}
        <div
          className={`stage-splitter${splitterActive ? " active" : ""}`}
          onPointerDown={onSplitterDown}
          aria-hidden="true"
        />
        <aside className="stage-side" style={{ width: sideWidth }}>
          {selectedFixtures.length > 0 ? (
            <FixtureChannelEditor
              key={editorKey}
              fixtures={selectedFixtures}
              libraryById={libraryById}
              effectsByFixture={effectsByFixture}
            />
          ) : (
            <p>
              Seleccioná un fixture o un tipo para abrir sus encoders. ⌘/Ctrl-click suma o quita de
              la selección.
            </p>
          )}
        </aside>
      </div>
      <StageFxBar />
    </main>
  );
}
