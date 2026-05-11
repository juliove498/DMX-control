import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import { useMemo, useState } from "react";
import { useT } from "../../i18n";
import {
  type DefinitionRenderOverrides,
  type FixturePlacement,
  type StageConfig,
  type TrussSegment,
  defaultDefinitionRenderOverrides,
  defaultFixturePlacement,
} from "./stageConfig";

const DEG = 180 / Math.PI;
const RAD = Math.PI / 180;

/// Per-fixture placement editor. Shows the current rig of fixtures
/// from the show, lets the operator pin each one to a truss with a
/// 0-1 slider OR set explicit XYZ, plus a base aim (yaw + pitch in
/// degrees) so a fixture rigged "facing forward" lights forward
/// when its DMX pan/tilt are at center.
export function FixturesPanel({
  fixtures,
  library,
  config,
  onChange,
}: {
  fixtures: FixtureInstance[];
  library: FixtureDefinition[];
  config: StageConfig;
  onChange: (next: StageConfig) => void;
}) {
  const t = useT();
  const [openId, setOpenId] = useState<string | null>(null);
  const libById: Record<string, FixtureDefinition> = {};
  for (const d of library) libById[d.id] = d;

  const setPlacement = (id: string, patch: Partial<FixturePlacement>) => {
    const current = config.fixturePlacements[id] ?? defaultFixturePlacement();
    const next: FixturePlacement = { ...current, ...patch };
    onChange({
      ...config,
      fixturePlacements: { ...config.fixturePlacements, [id]: next },
    });
  };
  const reset = (id: string) => {
    const { [id]: _drop, ...rest } = config.fixturePlacements;
    onChange({ ...config, fixturePlacements: rest });
  };

  // Group fixtures by definition so the per-type section only lists
  // each model once. Sorted by display name for stable ordering even
  // when fixtures get added/removed mid-session.
  const definitionGroups = useMemo(() => {
    const lookup: Record<string, FixtureDefinition> = {};
    for (const d of library) lookup[d.id] = d;
    const seen = new Map<string, { def: FixtureDefinition; count: number }>();
    for (const f of fixtures) {
      const def = lookup[f.definition_id];
      if (!def) continue;
      const prev = seen.get(def.id);
      if (prev) prev.count += 1;
      else seen.set(def.id, { def, count: 1 });
    }
    return Array.from(seen.values()).sort((a, b) =>
      `${a.def.manufacturer} ${a.def.name}`.localeCompare(`${b.def.manufacturer} ${b.def.name}`),
    );
  }, [fixtures, library]);

  const setDefOverride = (defId: string, patch: Partial<DefinitionRenderOverrides>) => {
    const current = config.definitionOverrides[defId] ?? defaultDefinitionRenderOverrides();
    const next: DefinitionRenderOverrides = { ...current, ...patch };
    onChange({
      ...config,
      definitionOverrides: { ...config.definitionOverrides, [defId]: next },
    });
  };
  const resetDefOverride = (defId: string) => {
    const { [defId]: _drop, ...rest } = config.definitionOverrides;
    onChange({ ...config, definitionOverrides: rest });
  };

  if (fixtures.length === 0) {
    return (
      <section className="p3d-section">
        <h4>Fixtures</h4>
        <p className="hint">Sin fixtures patcheados. Agregá fixtures en Stage primero.</p>
      </section>
    );
  }

  return (
    <>
      {definitionGroups.length > 0 ? (
        <section className="p3d-section">
          <h4>{t("p3d.types.title", { count: definitionGroups.length })}</h4>
          <p className="hint">{t("p3d.types.intro")}</p>
          <div className="p3d-fixtures-list">
            {definitionGroups.map(({ def, count }) => {
              const ovr = config.definitionOverrides[def.id];
              const has = !!ovr;
              return (
                <DefinitionTypeRow
                  key={def.id}
                  def={def}
                  count={count}
                  override={ovr ?? defaultDefinitionRenderOverrides()}
                  hasOverride={has}
                  onChange={(p) => setDefOverride(def.id, p)}
                  onReset={has ? () => resetDefOverride(def.id) : null}
                />
              );
            })}
          </div>
        </section>
      ) : null}

      <section className="p3d-section">
        <h4>Fixtures ({fixtures.length})</h4>
        <p className="hint">
          Click para fijar posición y orientación base. Sin override usan la posición de la canvas
          2D y aim 0,0 (apuntando al piso).
        </p>
        <div className="p3d-fixtures-list">
          {fixtures.map((f) => {
            const def = libById[f.definition_id];
            const placement = config.fixturePlacements[f.id];
            const open = openId === f.id;
            const hasOverride = !!placement;
            return (
              <div key={f.id} className={`p3d-fixrow${hasOverride ? " has-override" : ""}`}>
                <button
                  type="button"
                  className="p3d-fixrow-head"
                  onClick={() => setOpenId(open ? null : f.id)}
                >
                  <span className="p3d-fixrow-name">{f.label ?? def?.name ?? f.id}</span>
                  <span className="p3d-fixrow-meta">
                    {hasOverride ? badge(placement, config.trusses) : "auto"}
                  </span>
                  <span className="p3d-fixrow-caret">{open ? "▾" : "▸"}</span>
                </button>
                {open ? (
                  <FixtureEditor
                    placement={placement ?? defaultFixturePlacement()}
                    trusses={config.trusses}
                    onChange={(p) => setPlacement(f.id, p)}
                    onReset={hasOverride ? () => reset(f.id) : null}
                  />
                ) : null}
              </div>
            );
          })}
        </div>
      </section>
    </>
  );
}

function DefinitionTypeRow({
  def,
  count,
  override,
  hasOverride,
  onChange,
  onReset,
}: {
  def: FixtureDefinition;
  count: number;
  override: DefinitionRenderOverrides;
  hasOverride: boolean;
  onChange: (p: Partial<DefinitionRenderOverrides>) => void;
  onReset: (() => void) | null;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <div className={`p3d-fixrow${hasOverride ? " has-override" : ""}`}>
      <button type="button" className="p3d-fixrow-head" onClick={() => setOpen((v) => !v)}>
        <span className="p3d-fixrow-name">
          {def.manufacturer} {def.name}
        </span>
        <span className="p3d-fixrow-meta">
          {t("p3d.types.metaPrefix", {
            count,
            brightness: override.brightness.toFixed(2),
          })}
          {override.beamAngle ? " · beam" : ""}
          {override.prism ? " · prism" : ""}
        </span>
        <span className="p3d-fixrow-caret">{open ? "▾" : "▸"}</span>
      </button>
      {open ? (
        <div className="p3d-fixrow-body">
          {/* Brightness multiplier — the per-type knob the operator
              uses to dim a model that's blowing out on this monitor
              or boost one that reads dark. */}
          <SliderRow
            label={t("p3d.types.brightness")}
            value={override.brightness}
            step={0.1}
            min={0}
            max={4}
            suffix="×"
            decimals={2}
            onChange={(v) => onChange({ brightness: v })}
            hint={t("p3d.types.brightnessHint")}
          />

          {/* Beam angle override — operator dials in the real
              fixture's spec so the cone matches a Sharpy / Aura /
              Quantum etc. */}
          <div className="p3d-fixrow-subsection">
            <label className="p3d-check">
              <input
                type="checkbox"
                checked={override.beamAngle !== null}
                onChange={(e) =>
                  onChange({
                    beamAngle: e.currentTarget.checked
                      ? (override.beamAngle ?? { minDeg: 4, maxDeg: 14 })
                      : null,
                  })
                }
              />
              {t("p3d.types.beamOverride")}
            </label>
            {override.beamAngle ? (
              <div className="p3d-fixrow-grid p3d-grid-2">
                <NumRow
                  label={t("p3d.types.beamMin")}
                  value={override.beamAngle.minDeg}
                  step={0.5}
                  min={0.5}
                  max={120}
                  onChange={(v) =>
                    onChange({
                      beamAngle: {
                        minDeg: v,
                        maxDeg: Math.max(v, override.beamAngle?.maxDeg ?? v),
                      },
                    })
                  }
                  hint={t("p3d.types.beamMinHint")}
                />
                <NumRow
                  label={t("p3d.types.beamMax")}
                  value={override.beamAngle.maxDeg}
                  step={0.5}
                  min={0.5}
                  max={120}
                  onChange={(v) =>
                    onChange({
                      beamAngle: {
                        minDeg: Math.min(v, override.beamAngle?.minDeg ?? v),
                        maxDeg: v,
                      },
                    })
                  }
                  hint={t("p3d.types.beamMaxHint")}
                />
              </div>
            ) : null}
          </div>

          {/* Prism behaviour. */}
          <div className="p3d-fixrow-subsection">
            <label className="p3d-check">
              <input
                type="checkbox"
                checked={override.prism !== null}
                onChange={(e) =>
                  onChange({
                    prism: e.currentTarget.checked
                      ? (override.prism ?? { threshold: 8, facets: 7, splayDeg: 6 })
                      : null,
                  })
                }
              />
              {t("p3d.types.prismOverride")}
            </label>
            {override.prism ? (
              <div className="p3d-fixrow-grid">
                <NumRow
                  label={t("p3d.types.prismThreshold")}
                  value={override.prism.threshold}
                  step={1}
                  min={0}
                  max={255}
                  onChange={(v) =>
                    onChange({ prism: { ...(override.prism ?? defPrism()), threshold: v } })
                  }
                  hint={t("p3d.types.prismThresholdHint")}
                />
                <NumRow
                  label={t("p3d.types.prismFacets")}
                  value={override.prism.facets}
                  step={1}
                  min={2}
                  max={12}
                  onChange={(v) =>
                    onChange({ prism: { ...(override.prism ?? defPrism()), facets: v } })
                  }
                  hint={t("p3d.types.prismFacetsHint")}
                />
                <NumRow
                  label={t("p3d.types.prismSplay")}
                  value={override.prism.splayDeg}
                  step={0.5}
                  min={0}
                  max={45}
                  onChange={(v) =>
                    onChange({ prism: { ...(override.prism ?? defPrism()), splayDeg: v } })
                  }
                  hint={t("p3d.types.prismSplayHint")}
                />
              </div>
            ) : null}
          </div>

          {onReset ? (
            <button type="button" className="p3d-fixrow-reset" onClick={onReset}>
              {t("p3d.types.reset")}
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function badge(p: FixturePlacement, trusses: TrussSegment[]): string {
  if (p.mode === "truss") {
    const truss = trusses.find((t) => t.id === p.truss.trussId);
    const name = truss?.name ?? truss?.id ?? "?";
    return `${name} · ${(p.truss.t * 100).toFixed(0)}%`;
  }
  return `${p.free.x.toFixed(1)}, ${p.free.y.toFixed(1)}, ${p.free.z.toFixed(1)}`;
}

function FixtureEditor({
  placement,
  trusses,
  onChange,
  onReset,
}: {
  placement: FixturePlacement;
  trusses: TrussSegment[];
  onChange: (p: Partial<FixturePlacement>) => void;
  onReset: (() => void) | null;
}) {
  return (
    <div className="p3d-fixrow-body">
      <div className="p3d-fixrow-mode">
        <label>
          <input
            type="radio"
            checked={placement.mode === "free"}
            onChange={() => onChange({ mode: "free" })}
          />
          Libre (XYZ)
        </label>
        <label>
          <input
            type="radio"
            checked={placement.mode === "truss"}
            onChange={() =>
              onChange({
                mode: "truss",
                truss: {
                  ...placement.truss,
                  trussId: placement.truss.trussId ?? trusses[0]?.id ?? null,
                },
              })
            }
            disabled={trusses.length === 0}
          />
          Atado a truss
        </label>
      </div>

      {placement.mode === "free" ? (
        <div className="p3d-fixrow-grid">
          <NumRow
            label="X"
            value={placement.free.x}
            step={0.1}
            onChange={(v) => onChange({ free: { ...placement.free, x: v } })}
          />
          <NumRow
            label="Y"
            value={placement.free.y}
            step={0.1}
            min={0}
            onChange={(v) => onChange({ free: { ...placement.free, y: v } })}
          />
          <NumRow
            label="Z"
            value={placement.free.z}
            step={0.1}
            onChange={(v) => onChange({ free: { ...placement.free, z: v } })}
          />
        </div>
      ) : (
        <>
          <label className="p3d-num">
            <span>Truss</span>
            <select
              value={placement.truss.trussId ?? ""}
              onChange={(e) =>
                onChange({ truss: { ...placement.truss, trussId: e.currentTarget.value || null } })
              }
            >
              {trusses.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.name ?? t.id}
                </option>
              ))}
            </select>
          </label>
          {/* Position-along-truss slider with synced numeric. */}
          <label className="p3d-num">
            <span>Posición {(placement.truss.t * 100).toFixed(0)}%</span>
            <input
              type="range"
              min={0}
              max={1}
              step={0.01}
              value={placement.truss.t}
              onChange={(e) =>
                onChange({ truss: { ...placement.truss, t: Number(e.currentTarget.value) } })
              }
            />
          </label>
        </>
      )}

      {/* Aim sliders — operator scrubs to point the fixture
          interactively. Numeric value sits next to the label so
          they can read the exact angle they're at. */}
      <SliderRow
        label="Aim Yaw"
        value={placement.aimYaw * DEG}
        step={1}
        min={-180}
        max={180}
        suffix="°"
        onChange={(v) => onChange({ aimYaw: v * RAD })}
        hint="Compass horizontal: 0 = audiencia, 90 = stage right, ±180 = upstage"
      />
      <SliderRow
        label="Aim Pitch"
        value={placement.aimPitch * DEG}
        step={1}
        min={-180}
        max={180}
        suffix="°"
        onChange={(v) => onChange({ aimPitch: v * RAD })}
        hint="0 = abajo, -90 = horizontal forward, +90 = horizontal back, ±180 = arriba"
      />

      {onReset ? (
        <button type="button" className="p3d-fixrow-reset" onClick={onReset}>
          Quitar override (volver a auto)
        </button>
      ) : null}
    </div>
  );
}

function defPrism() {
  return { threshold: 8, facets: 7, splayDeg: 6 };
}

/// Range slider with the live numeric value next to the label.
/// Used for aim Yaw/Pitch and brightness where scrubbing is the
/// natural gesture. `decimals` controls the readout precision; aim
/// values use whole degrees, brightness needs two decimals.
function SliderRow({
  label,
  value,
  step,
  min,
  max,
  suffix,
  decimals,
  onChange,
  hint,
}: {
  label: string;
  value: number;
  step?: number;
  min: number;
  max: number;
  suffix?: string;
  decimals?: number;
  onChange: (v: number) => void;
  hint?: string;
}) {
  const display = Number.isFinite(value) ? value.toFixed(decimals ?? 0) : "0";
  return (
    <label className="p3d-slider" title={hint}>
      <span className="p3d-slider-label">
        {label}{" "}
        <span className="p3d-slider-value">
          {display}
          {suffix ?? ""}
        </span>
      </span>
      <input
        type="range"
        value={Number.isFinite(value) ? value : 0}
        step={step ?? 1}
        min={min}
        max={max}
        onChange={(e) => onChange(Number(e.currentTarget.value))}
      />
    </label>
  );
}

function NumRow({
  label,
  value,
  step,
  min,
  max,
  onChange,
  hint,
}: {
  label: string;
  value: number;
  step?: number;
  min?: number;
  max?: number;
  onChange: (v: number) => void;
  hint?: string;
}) {
  return (
    <label className="p3d-num" title={hint}>
      <span>{label}</span>
      <input
        type="number"
        value={Number.isFinite(value) ? value : 0}
        step={step ?? 1}
        min={min}
        max={max}
        onChange={(e) => {
          const v = Number(e.currentTarget.value);
          if (!Number.isNaN(v)) onChange(v);
        }}
      />
    </label>
  );
}
