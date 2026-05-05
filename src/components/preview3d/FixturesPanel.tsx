import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import { useState } from "react";
import {
  defaultFixturePlacement,
  type FixturePlacement,
  type StageConfig,
  type TrussSegment,
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

  if (fixtures.length === 0) {
    return (
      <section className="p3d-section">
        <h4>Fixtures</h4>
        <p className="hint">Sin fixtures patcheados. Agregá fixtures en Stage primero.</p>
      </section>
    );
  }

  return (
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
                <span className="p3d-fixrow-name">
                  {f.label ?? def?.name ?? f.id}
                </span>
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
        min={-90}
        max={90}
        suffix="°"
        onChange={(v) => onChange({ aimPitch: v * RAD })}
        hint="0 = abajo, -90 = horizontal forward, +90 = horizontal back"
      />

      {/* Beam angle override — operator dials in the real fixture's
          spec so the cone matches a Sharpy / Aura / Quantum etc. */}
      <div className="p3d-fixrow-subsection">
        <label className="p3d-check">
          <input
            type="checkbox"
            checked={placement.beamAngle !== null}
            onChange={(e) =>
              onChange({
                beamAngle: e.currentTarget.checked
                  ? placement.beamAngle ?? { minDeg: 4, maxDeg: 14 }
                  : null,
              })
            }
          />
          Override de ángulo de haz
        </label>
        {placement.beamAngle ? (
          <div className="p3d-fixrow-grid p3d-grid-2">
            <NumRow
              label="Mín °"
              value={placement.beamAngle.minDeg}
              step={0.5}
              min={0.5}
              max={120}
              onChange={(v) =>
                onChange({
                  beamAngle: {
                    minDeg: v,
                    maxDeg: Math.max(v, placement.beamAngle?.maxDeg ?? v),
                  },
                })
              }
              hint="Half-angle del beam con zoom cerrado"
            />
            <NumRow
              label="Máx °"
              value={placement.beamAngle.maxDeg}
              step={0.5}
              min={0.5}
              max={120}
              onChange={(v) =>
                onChange({
                  beamAngle: {
                    minDeg: Math.min(v, placement.beamAngle?.minDeg ?? v),
                    maxDeg: v,
                  },
                })
              }
              hint="Half-angle del beam con zoom abierto"
            />
          </div>
        ) : null}
      </div>

      {/* Prism behaviour — threshold above which the prism activates,
          number of facets to splat, angular splay between them. */}
      <div className="p3d-fixrow-subsection">
        <label className="p3d-check">
          <input
            type="checkbox"
            checked={placement.prism !== null}
            onChange={(e) =>
              onChange({
                prism: e.currentTarget.checked
                  ? placement.prism ?? { threshold: 8, facets: 7, splayDeg: 6 }
                  : null,
              })
            }
          />
          Override de prisma
        </label>
        {placement.prism ? (
          <div className="p3d-fixrow-grid">
            <NumRow
              label="Threshold"
              value={placement.prism.threshold}
              step={1}
              min={0}
              max={255}
              onChange={(v) =>
                onChange({ prism: { ...(placement.prism ?? defPrism()), threshold: v } })
              }
              hint="DMX value a partir del cual el prisma se activa"
            />
            <NumRow
              label="Facetas"
              value={placement.prism.facets}
              step={1}
              min={2}
              max={12}
              onChange={(v) =>
                onChange({ prism: { ...(placement.prism ?? defPrism()), facets: v } })
              }
              hint="Cantidad total de haces (incluyendo el central)"
            />
            <NumRow
              label="Splay °"
              value={placement.prism.splayDeg}
              step={0.5}
              min={0}
              max={45}
              onChange={(v) =>
                onChange({ prism: { ...(placement.prism ?? defPrism()), splayDeg: v } })
              }
              hint="Apertura entre haces"
            />
          </div>
        ) : null}
      </div>

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
/// Used for aim Yaw/Pitch where scrubbing is the natural gesture.
function SliderRow({
  label,
  value,
  step,
  min,
  max,
  suffix,
  onChange,
  hint,
}: {
  label: string;
  value: number;
  step?: number;
  min: number;
  max: number;
  suffix?: string;
  onChange: (v: number) => void;
  hint?: string;
}) {
  const display = Number.isFinite(value) ? value.toFixed(0) : "0";
  return (
    <label className="p3d-slider" title={hint}>
      <span className="p3d-slider-label">
        {label} <span className="p3d-slider-value">{display}{suffix ?? ""}</span>
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
