import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import { FixturesPanel } from "./FixturesPanel";
import type { StageConfig, TrussSegment } from "./stageConfig";

/// Sidebar to tweak stage dimensions and rigging. Persists via the
/// parent's onChange callback (which writes to localStorage). All
/// edits are applied live so the preview reflects the change as
/// the operator drags a number.
export function StageConfigPanel({
  config,
  onChange,
  fixtures,
  library,
}: {
  config: StageConfig;
  onChange: (next: StageConfig) => void;
  fixtures: FixtureInstance[];
  library: FixtureDefinition[];
}) {
  const updateFloor = (patch: Partial<StageConfig["floor"]>) =>
    onChange({ ...config, floor: { ...config.floor, ...patch } });
  const updateBackWall = (patch: Partial<StageConfig["backWall"]>) =>
    onChange({ ...config, backWall: { ...config.backWall, ...patch } });

  const addTruss = () => {
    const id = `truss-${Date.now().toString(36)}`;
    const next: TrussSegment = {
      id,
      name: `Truss ${config.trusses.length + 1}`,
      fromX: -config.floor.width / 2 + 1,
      fromZ: 0,
      toX: config.floor.width / 2 - 1,
      toZ: 0,
      height: config.rigHeight,
      diameter: 0.25,
    };
    onChange({ ...config, trusses: [...config.trusses, next] });
  };

  const updateTruss = (idx: number, patch: Partial<TrussSegment>) => {
    const trusses = config.trusses.map((t, i) => (i === idx ? { ...t, ...patch } : t));
    onChange({ ...config, trusses });
  };

  const removeTruss = (idx: number) => {
    onChange({ ...config, trusses: config.trusses.filter((_, i) => i !== idx) });
  };

  return (
    <aside className="p3d-panel">
      <h3>Escenario</h3>

      <section className="p3d-section">
        <h4>Piso</h4>
        <NumberRow
          label="Ancho (m)"
          value={config.floor.width}
          step={0.5}
          min={2}
          max={50}
          onChange={(v) => updateFloor({ width: v })}
        />
        <NumberRow
          label="Profundidad (m)"
          value={config.floor.depth}
          step={0.5}
          min={2}
          max={50}
          onChange={(v) => updateFloor({ depth: v })}
        />
        <NumberRow
          label="Grilla (m)"
          value={config.floor.cellSize}
          step={0.25}
          min={0.25}
          max={5}
          onChange={(v) => updateFloor({ cellSize: v })}
        />
        <ColorRow
          label="Color grilla"
          value={config.floor.gridColor}
          onChange={(v) => updateFloor({ gridColor: v })}
        />
        <ColorRow
          label="Color piso"
          value={config.floor.color}
          onChange={(v) => updateFloor({ color: v })}
        />
        <p className="hint">
          Pisos oscuros se tragan el bounce de los pares RGB. Subí a un gris medio para que la luz
          se vea sobre el piso.
        </p>
      </section>

      <section className="p3d-section">
        <h4>Pared de fondo</h4>
        <label className="p3d-check">
          <input
            type="checkbox"
            checked={config.backWall.enabled}
            onChange={(e) => updateBackWall({ enabled: e.currentTarget.checked })}
          />
          Habilitada
        </label>
        {config.backWall.enabled ? (
          <>
            <NumberRow
              label="Altura (m)"
              value={config.backWall.height}
              step={0.5}
              min={1}
              max={20}
              onChange={(v) => updateBackWall({ height: v })}
            />
            <ColorRow
              label="Color pared"
              value={config.backWall.color}
              onChange={(v) => updateBackWall({ color: v })}
            />
          </>
        ) : null}
      </section>

      <section className="p3d-section">
        <h4>Rig general</h4>
        <NumberRow
          label="Altura por defecto (m)"
          value={config.rigHeight}
          step={0.25}
          min={1}
          max={20}
          onChange={(v) => onChange({ ...config, rigHeight: v })}
        />
        <NumberRow
          label="Atmósfera (haze)"
          value={config.atmosphere}
          step={0.05}
          min={0}
          max={1}
          onChange={(v) => onChange({ ...config, atmosphere: v })}
        />
        <NumberRow
          label="Luz ambiente"
          value={config.ambientLevel}
          step={0.05}
          min={0}
          max={1}
          onChange={(v) => onChange({ ...config, ambientLevel: v })}
        />
        <NumberRow
          label="Px por metro"
          value={config.pixelsPerMeter}
          step={5}
          min={20}
          max={500}
          onChange={(v) => onChange({ ...config, pixelsPerMeter: v })}
        />
      </section>

      <section className="p3d-section">
        <h4>Audiencia</h4>
        <label className="p3d-check">
          <input
            type="checkbox"
            checked={config.audience.enabled}
            onChange={(e) =>
              onChange({
                ...config,
                audience: { ...config.audience, enabled: e.currentTarget.checked },
              })
            }
          />
          Mostrar público en la pista
        </label>
        {config.audience.enabled ? (
          <>
            <p className="hint">
              Zona rectangular con siluetas humanoides. La luz del rig las baña — útil para
              visualizar cobertura sobre el público.
            </p>
            <div className="p3d-fixrow-grid p3d-grid-2">
              <NumberRow
                label="X1"
                value={config.audience.zone.x1}
                step={0.25}
                onChange={(v) =>
                  onChange({
                    ...config,
                    audience: { ...config.audience, zone: { ...config.audience.zone, x1: v } },
                  })
                }
              />
              <NumberRow
                label="Z1"
                value={config.audience.zone.z1}
                step={0.25}
                onChange={(v) =>
                  onChange({
                    ...config,
                    audience: { ...config.audience, zone: { ...config.audience.zone, z1: v } },
                  })
                }
              />
              <NumberRow
                label="X2"
                value={config.audience.zone.x2}
                step={0.25}
                onChange={(v) =>
                  onChange({
                    ...config,
                    audience: { ...config.audience, zone: { ...config.audience.zone, x2: v } },
                  })
                }
              />
              <NumberRow
                label="Z2"
                value={config.audience.zone.z2}
                step={0.25}
                onChange={(v) =>
                  onChange({
                    ...config,
                    audience: { ...config.audience, zone: { ...config.audience.zone, z2: v } },
                  })
                }
              />
            </div>
            <NumberRow
              label="Densidad (p/m²)"
              value={config.audience.density}
              step={0.05}
              min={0}
              max={3}
              onChange={(v) =>
                onChange({ ...config, audience: { ...config.audience, density: v } })
              }
            />
            <NumberRow
              label="Altura prom (m)"
              value={config.audience.averageHeight}
              step={0.05}
              min={1}
              max={2.5}
              onChange={(v) =>
                onChange({ ...config, audience: { ...config.audience, averageHeight: v } })
              }
            />
            <ColorRow
              label="Color silueta"
              value={config.audience.color}
              onChange={(v) => onChange({ ...config, audience: { ...config.audience, color: v } })}
            />
          </>
        ) : null}
      </section>

      <section className="p3d-section">
        <div className="p3d-section-head">
          <h4>Trusses ({config.trusses.length})</h4>
          <button type="button" className="p3d-add" onClick={addTruss}>
            + Truss
          </button>
        </div>
        {config.trusses.map((t, i) => (
          <TrussEditor
            key={t.id}
            truss={t}
            onChange={(p) => updateTruss(i, p)}
            onRemove={() => removeTruss(i)}
          />
        ))}
      </section>

      <FixturesPanel fixtures={fixtures} library={library} config={config} onChange={onChange} />
    </aside>
  );
}

function TrussEditor({
  truss,
  onChange,
  onRemove,
}: {
  truss: TrussSegment;
  onChange: (p: Partial<TrussSegment>) => void;
  onRemove: () => void;
}) {
  return (
    <div className="p3d-truss">
      <div className="p3d-truss-head">
        <input
          className="p3d-truss-name"
          value={truss.name ?? ""}
          placeholder="Truss"
          onChange={(e) => onChange({ name: e.currentTarget.value })}
        />
        <button type="button" className="danger" onClick={onRemove} title="Quitar truss">
          ×
        </button>
      </div>
      <div className="p3d-truss-grid">
        <NumberRow
          label="X1"
          value={truss.fromX}
          step={0.25}
          onChange={(v) => onChange({ fromX: v })}
        />
        <NumberRow
          label="Z1"
          value={truss.fromZ}
          step={0.25}
          onChange={(v) => onChange({ fromZ: v })}
        />
        <NumberRow
          label="X2"
          value={truss.toX}
          step={0.25}
          onChange={(v) => onChange({ toX: v })}
        />
        <NumberRow
          label="Z2"
          value={truss.toZ}
          step={0.25}
          onChange={(v) => onChange({ toZ: v })}
        />
        <NumberRow
          label="Altura"
          value={truss.height}
          step={0.25}
          min={0.5}
          max={20}
          onChange={(v) => onChange({ height: v })}
        />
        <NumberRow
          label="Grosor"
          value={truss.diameter}
          step={0.05}
          min={0.05}
          max={1}
          onChange={(v) => onChange({ diameter: v })}
        />
      </div>
    </div>
  );
}

function NumberRow({
  label,
  value,
  step,
  min,
  max,
  onChange,
}: {
  label: string;
  value: number;
  step?: number;
  min?: number;
  max?: number;
  onChange: (v: number) => void;
}) {
  return (
    <label className="p3d-num">
      <span>{label}</span>
      <input
        type="number"
        value={value}
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

function ColorRow({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <label className="p3d-num">
      <span>{label}</span>
      <input type="color" value={value} onChange={(e) => onChange(e.currentTarget.value)} />
    </label>
  );
}
