import type { Scene } from "@bindings/Scene";
import type { SceneFxState } from "@bindings/SceneFxState";
import type { SceneStep } from "@bindings/SceneStep";
import { useEffect, useMemo, useState } from "react";
import { useShowStore } from "../stores/show";

/// Scenes UI (Phase 4 iteration 3): two-pane layout.
///
/// Left pane = scene list with GO buttons. Right pane = editor for the
/// currently-selected scene: name, FX capture toggles, step list with
/// per-step fade/hold + recapture/delete, and "Add step from current"
/// at the bottom. The active scene + active step get a dorado halo
/// matching what the Launchpad shows.
export function ScenesView() {
  const show = useShowStore((s) => s.show);
  const createScene = useShowStore((s) => s.createSceneFromState);
  const addStep = useShowStore((s) => s.addSceneStep);
  const removeStep = useShowStore((s) => s.removeSceneStep);
  const updateStepFromState = useShowStore((s) => s.updateSceneStepFromState);
  const updateScene = useShowStore((s) => s.updateScene);
  const deleteScene = useShowStore((s) => s.deleteScene);
  const recallScene = useShowStore((s) => s.recallScene);
  const releaseScene = useShowStore((s) => s.releaseScene);
  const activeSceneIdQuery = useShowStore((s) => s.activeSceneId);
  const activeSceneStepQuery = useShowStore((s) => s.activeSceneStep);
  const programmerStatus = useShowStore((s) => s.programmerStatus);
  const programmerClear = useShowStore((s) => s.programmerClear);

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [activeStep, setActiveStep] = useState<number | null>(null);
  const [touched, setTouched] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  // Auto-select first scene when the list isn't empty and nothing is
  // selected. Reset selection if the selected scene was deleted.
  const scenes = useMemo(() => show?.scenes ?? [], [show?.scenes]);
  useEffect(() => {
    if (scenes.length === 0) {
      if (selectedId !== null) setSelectedId(null);
      return;
    }
    if (!selectedId || !scenes.some((s) => s.id === selectedId)) {
      setSelectedId(scenes[0].id);
    }
  }, [scenes, selectedId]);

  // Poll backend for live state: which scene is active, which step is
  // playing, and the programmer's touched set. 200 ms covers human
  // expectations without flooding IPC.
  useEffect(() => {
    let cancelled = false;
    const tick = () => {
      activeSceneIdQuery()
        .then((id) => {
          if (!cancelled) setActiveId(id ?? null);
        })
        .catch(() => {});
      activeSceneStepQuery()
        .then((idx) => {
          if (!cancelled) setActiveStep(idx ?? null);
        })
        .catch(() => {});
      programmerStatus()
        .then((s) => {
          if (!cancelled) setTouched(s.touched);
        })
        .catch(() => {});
    };
    tick();
    const interval = window.setInterval(tick, 200);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [activeSceneIdQuery, activeSceneStepQuery, programmerStatus]);

  if (!show) return <main className="page">Cargando…</main>;
  const fixtures = show.fixtures;
  const chasers = show.chasers.map((c) => ({ id: c.id, name: c.name }));
  const movements = show.movements.map((m) => ({ id: m.id, name: m.name }));
  const selectedScene = scenes.find((s) => s.id === selectedId) ?? null;

  const handleCreate = async () => {
    setError(null);
    try {
      // Quick-create with sensible defaults: capture FX state, snap
      // (no fade), all currently patched fixtures. The user can tune
      // everything afterwards in the editor on the right.
      const newScene = await createScene("", [], 800, false, true, true);
      setSelectedId(newScene.id);
    } catch (e) {
      setError(`No se pudo crear la escena: ${stringifyError(e)}`);
    }
  };

  return (
    <main className="page scenes-view-v2">
      <header className="page-head">
        <h2>Escenas</h2>
        <span className="meta">
          Multi-step + FX capture · {scenes.length} escena{scenes.length === 1 ? "" : "s"}
        </span>
      </header>

      {error ? (
        <output className="lib-error" aria-live="polite">
          {error}
        </output>
      ) : null}

      <div className="scenes-layout">
        {/* ---- LEFT: list ---- */}
        <aside className="scenes-list-pane">
          <button type="button" className="scenes-new-btn" onClick={handleCreate}>
            + Nueva escena
          </button>
          {scenes.length === 0 ? (
            <p className="empty">
              Sin escenas todavía. Armá un look en Stage y tocá "Nueva escena" para grabarlo.
            </p>
          ) : (
            <ul className="scenes-list">
              {scenes.map((s, i) => (
                <SceneListItem
                  key={s.id}
                  index={i}
                  scene={s}
                  isSelected={s.id === selectedId}
                  isActive={s.id === activeId}
                  onSelect={() => setSelectedId(s.id)}
                  onRecall={() => recallScene(s.id)}
                />
              ))}
            </ul>
          )}
          {activeId ? (
            <div className="scenes-list-footer">
              <span>
                ▶ {scenes.find((s) => s.id === activeId)?.name ?? "—"}
                {activeStep !== null ? (
                  <span className="scene-active-step"> · paso {activeStep + 1}</span>
                ) : null}
              </span>
              <button type="button" onClick={() => releaseScene()}>
                Liberar
              </button>
            </div>
          ) : null}
        </aside>

        {/* ---- RIGHT: editor ---- */}
        <section className="scenes-editor-pane">
          {selectedScene ? (
            <SceneEditor
              scene={selectedScene}
              fixtures={fixtures}
              chasers={chasers}
              movements={movements}
              touched={touched}
              isActive={selectedScene.id === activeId}
              activeStep={activeStep}
              onUpdate={updateScene}
              onAddStep={addStep}
              onRemoveStep={removeStep}
              onUpdateStepFromState={updateStepFromState}
              onDelete={() => deleteScene(selectedScene.id)}
              onRecall={() => recallScene(selectedScene.id)}
              programmerClear={programmerClear}
            />
          ) : (
            <div className="scenes-empty-editor">
              <p>Elegí una escena de la izquierda — o creá una nueva.</p>
            </div>
          )}
        </section>
      </div>

      {touched.length > 0 ? (
        <footer className="programmer-bar">
          <span className="programmer-bar-label">
            <strong>PROG</strong> · {touched.length} fixture{touched.length === 1 ? "" : "s"} tocado
            {touched.length === 1 ? "" : "s"}
          </span>
          <button type="button" onClick={() => programmerClear()}>
            Clear
          </button>
        </footer>
      ) : null}
    </main>
  );
}

function SceneListItem({
  index,
  scene,
  isSelected,
  isActive,
  onSelect,
  onRecall,
}: {
  index: number;
  scene: Scene;
  isSelected: boolean;
  isActive: boolean;
  onSelect: () => void;
  onRecall: () => void;
}) {
  const lpHint = index < 8 ? `LP fila 3, pad ${index + 1}` : null;
  return (
    <li className={`scenes-list-item${isSelected ? " selected" : ""}${isActive ? " active" : ""}`}>
      <button
        type="button"
        className="scenes-list-go"
        onClick={(e) => {
          e.stopPropagation();
          onRecall();
        }}
        title="Recall (▶ GO)"
      >
        ▶
      </button>
      <button
        type="button"
        className="scenes-list-body"
        onClick={onSelect}
        title={lpHint ?? scene.name}
      >
        <span className="scenes-list-name">{scene.name}</span>
        <span className="scenes-list-meta">
          {scene.steps.length} step{scene.steps.length === 1 ? "" : "s"}
        </span>
      </button>
    </li>
  );
}

function SceneEditor({
  scene,
  fixtures,
  chasers,
  movements,
  touched,
  isActive,
  activeStep,
  onUpdate,
  onAddStep,
  onRemoveStep,
  onUpdateStepFromState,
  onDelete,
  onRecall,
  programmerClear,
}: {
  scene: Scene;
  fixtures: { id: string; label: string | null }[];
  chasers: { id: string; name: string }[];
  movements: { id: string; name: string }[];
  touched: string[];
  isActive: boolean;
  activeStep: number | null;
  onUpdate: (s: Scene) => Promise<void>;
  onAddStep: (
    sceneId: string,
    fixtureIds: string[],
    fadeInMs: number,
    holdMs: number,
    restrictToTouched: boolean,
  ) => Promise<Scene>;
  onRemoveStep: (sceneId: string, stepId: string) => Promise<Scene>;
  onUpdateStepFromState: (
    sceneId: string,
    stepId: string,
    restrictToTouched: boolean,
  ) => Promise<Scene>;
  onDelete: () => Promise<void>;
  onRecall: () => void;
  programmerClear: () => Promise<void>;
}) {
  const [name, setName] = useState(scene.name);
  // Local controls for the "+ Add step" footer.
  const [addFade, setAddFade] = useState(800);
  const [addHold, setAddHold] = useState(1500);
  const [addTouchedOnly, setAddTouchedOnly] = useState(false);
  // Sync external scene changes (refresh after a remote update) into
  // the local edit fields without clobbering active edits.
  useEffect(() => {
    setName(scene.name);
  }, [scene.name]);

  const totalCycleMs = scene.steps.reduce((acc, s) => acc + s.fade_in_ms + s.hold_ms, 0);

  const commitName = () => {
    const next = name.trim();
    if (next === "" || next === scene.name) {
      setName(scene.name);
      return;
    }
    onUpdate({ ...scene, name: next });
  };

  const onAdd = () => {
    onAddStep(scene.id, [], Math.max(0, addFade), Math.max(0, addHold), addTouchedOnly);
  };

  return (
    <div className="scene-editor">
      <header className={`scene-editor-head${isActive ? " active" : ""}`}>
        <button
          type="button"
          className="scene-editor-go"
          onClick={onRecall}
          title="Recall esta escena (▶ GO)"
        >
          ▶ GO
        </button>
        <input
          className="scene-editor-name"
          value={name}
          onChange={(e) => setName(e.currentTarget.value)}
          onBlur={commitName}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitName();
            else if (e.key === "Escape") setName(scene.name);
          }}
        />
        <span className="scene-editor-cycle">Ciclo total: {(totalCycleMs / 1000).toFixed(1)}s</span>
        <button
          type="button"
          className="danger"
          onClick={async () => {
            if (window.confirm(`¿Eliminar "${scene.name}"?`)) await onDelete();
          }}
        >
          Eliminar
        </button>
      </header>

      <p className="hint scene-fx-hint">
        Cada step graba el chaser y movement activos en ese instante. Al frenar la escena se
        restaura lo que estaba corriendo antes del recall. Editá el FX state por step abajo.
      </p>

      <div className="scene-steps-wrap">
        <h4>Steps ({scene.steps.length})</h4>
        {scene.steps.length === 0 ? (
          <p className="empty">La escena no tiene steps. Agregá uno desde el bloque de abajo.</p>
        ) : (
          <ul className="scene-steps">
            {scene.steps.map((step, i) => (
              <StepRow
                key={step.id}
                index={i}
                isLive={isActive && activeStep === i}
                step={step}
                touchedCount={touched.length}
                canRemove={scene.steps.length > 1}
                chaserOptions={chasers}
                movementOptions={movements}
                onRename={(stepName) =>
                  onUpdate({
                    ...scene,
                    steps: scene.steps.map((st, j) =>
                      j === i ? { ...st, name: stepName.trim() || null } : st,
                    ),
                  })
                }
                onChangeFade={(fade) =>
                  onUpdate({
                    ...scene,
                    steps: scene.steps.map((st, j) =>
                      j === i ? { ...st, fade_in_ms: Math.max(0, fade) } : st,
                    ),
                  })
                }
                onChangeHold={(hold) =>
                  onUpdate({
                    ...scene,
                    steps: scene.steps.map((st, j) =>
                      j === i ? { ...st, hold_ms: Math.max(0, hold) } : st,
                    ),
                  })
                }
                onChangeChaserState={(state) =>
                  onUpdate({
                    ...scene,
                    steps: scene.steps.map((st, j) =>
                      j === i ? { ...st, chaser_state: state } : st,
                    ),
                  })
                }
                onChangeMovementState={(state) =>
                  onUpdate({
                    ...scene,
                    steps: scene.steps.map((st, j) =>
                      j === i ? { ...st, movement_state: state } : st,
                    ),
                  })
                }
                onUpdateAll={() => onUpdateStepFromState(scene.id, step.id, false)}
                onUpdateTouched={() => onUpdateStepFromState(scene.id, step.id, true)}
                onRemove={() => onRemoveStep(scene.id, step.id)}
              />
            ))}
          </ul>
        )}

        <div className="scene-add-step">
          <h5>Agregar step desde el estado actual</h5>
          <div className="scene-add-row">
            <label>
              Fade in (ms)
              <input
                type="number"
                min={0}
                max={60000}
                step={50}
                value={addFade}
                onChange={(e) => setAddFade(Number(e.currentTarget.value))}
              />
            </label>
            <label>
              Hold (ms)
              <input
                type="number"
                min={0}
                max={60000}
                step={50}
                value={addHold}
                onChange={(e) => setAddHold(Number(e.currentTarget.value))}
              />
            </label>
            <label className="scene-add-touched-toggle">
              <input
                type="checkbox"
                checked={addTouchedOnly}
                onChange={(e) => setAddTouchedOnly(e.currentTarget.checked)}
              />
              Solo touched ({touched.length})
            </label>
            <button
              type="button"
              className="scene-add-btn"
              onClick={onAdd}
              disabled={addTouchedOnly && touched.length === 0}
              title={
                addTouchedOnly && touched.length === 0
                  ? "Tocá fixtures en Stage primero"
                  : "Capturar el estado actual como nuevo step"
              }
            >
              + Add step
            </button>
          </div>
          <p className="hint scene-add-hint">
            Los pasos se reproducen en orden y vuelven al primero al final, formando un loop. El
            siguiente paso arranca cuando termina el <code>hold</code> del actual.
          </p>
        </div>
      </div>

      <p className="hint scene-fixtures-hint">
        {scene.steps.reduce((acc, s) => acc + s.fixtures.length, 0)} writes totales sobre{" "}
        {fixtures.length} fixtures patcheados. Tip: si hace falta, hacé Clear del programmer y usá
        "Solo touched" para iterar steps sin pisarte de más.{" "}
        <button type="button" className="scene-prog-clear-link" onClick={() => programmerClear()}>
          Clear programmer
        </button>
      </p>
    </div>
  );
}

function StepRow({
  index,
  isLive,
  step,
  touchedCount,
  canRemove,
  chaserOptions,
  movementOptions,
  onRename,
  onChangeFade,
  onChangeHold,
  onChangeChaserState,
  onChangeMovementState,
  onUpdateAll,
  onUpdateTouched,
  onRemove,
}: {
  index: number;
  isLive: boolean;
  step: SceneStep;
  touchedCount: number;
  canRemove: boolean;
  chaserOptions: { id: string; name: string }[];
  movementOptions: { id: string; name: string }[];
  onRename: (name: string) => void;
  onChangeFade: (ms: number) => void;
  onChangeHold: (ms: number) => void;
  onChangeChaserState: (state: SceneFxState) => void;
  onChangeMovementState: (state: SceneFxState) => void;
  onUpdateAll: () => void;
  onUpdateTouched: () => void;
  onRemove: () => void;
}) {
  const [localName, setLocalName] = useState(step.name ?? "");
  const [localFade, setLocalFade] = useState(step.fade_in_ms);
  const [localHold, setLocalHold] = useState(step.hold_ms);
  useEffect(() => setLocalName(step.name ?? ""), [step.name]);
  useEffect(() => setLocalFade(step.fade_in_ms), [step.fade_in_ms]);
  useEffect(() => setLocalHold(step.hold_ms), [step.hold_ms]);

  const fixtureCount = step.fixtures.length;
  const channelCount = step.fixtures.reduce((acc, f) => acc + f.values.length, 0);

  return (
    <li className={`scene-step-card${isLive ? " live" : ""}`}>
      <div className="scene-step-row">
        <span className="scene-step-num">{index + 1}</span>
        <input
          className="scene-step-name"
          placeholder={`Step ${index + 1}`}
          value={localName}
          onChange={(e) => setLocalName(e.currentTarget.value)}
          onBlur={() => onRename(localName)}
          onKeyDown={(e) => {
            if (e.key === "Enter") onRename(localName);
          }}
        />
        <label className="scene-step-time">
          Fade
          <input
            type="number"
            min={0}
            max={60000}
            step={50}
            value={localFade}
            onChange={(e) => setLocalFade(Number(e.currentTarget.value))}
            onBlur={() => onChangeFade(localFade)}
          />
          ms
        </label>
        <label className="scene-step-time">
          Hold
          <input
            type="number"
            min={0}
            max={60000}
            step={50}
            value={localHold}
            onChange={(e) => setLocalHold(Number(e.currentTarget.value))}
            onBlur={() => onChangeHold(localHold)}
          />
          ms
        </label>
        <span className="scene-step-meta">
          {fixtureCount}f · {channelCount}ch
        </span>
        <button
          type="button"
          className="scene-step-update"
          onClick={onUpdateAll}
          title="Re-grabar este step con el estado actual del rig (todos sus fixtures)"
        >
          ⟳
        </button>
        <button
          type="button"
          className="scene-step-update touched"
          onClick={onUpdateTouched}
          disabled={touchedCount === 0}
          title="Re-grabar solo los fixtures touched (resto queda como está)"
        >
          ⟳T
        </button>
        <button
          type="button"
          className="scene-step-del danger"
          onClick={() => {
            if (window.confirm(`¿Eliminar el step ${index + 1}?`)) onRemove();
          }}
          disabled={!canRemove}
          title={canRemove ? "Eliminar este step" : "No se puede eliminar el único step"}
        >
          ×
        </button>
      </div>
      <div className="scene-step-fx">
        <FxStateRow
          label="Chaser"
          state={step.chaser_state}
          options={chaserOptions}
          onChange={onChangeChaserState}
          compact
        />
        <FxStateRow
          label="Movement"
          state={step.movement_state}
          options={movementOptions}
          onChange={onChangeMovementState}
          compact
        />
      </div>
    </li>
  );
}

function FxStateRow({
  label,
  state,
  options,
  onChange,
  compact,
}: {
  label: string;
  state: SceneFxState;
  options: { id: string; name: string }[];
  onChange: (state: SceneFxState) => void;
  compact?: boolean;
}) {
  const mode: "inherit" | "disabled" | "enabled" =
    state.type === "inherit" ? "inherit" : state.type === "disabled" ? "disabled" : "enabled";
  const enabledId = state.type === "enabled" ? state.id : "";
  return (
    <div className={`fx-state-row${compact ? " compact" : ""}`}>
      <span className="fx-state-label">{label}</span>
      <div className="fx-state-options">
        <label>
          <input
            type="radio"
            checked={mode === "inherit"}
            onChange={() => onChange({ type: "inherit" })}
          />
          No tocar
        </label>
        <label>
          <input
            type="radio"
            checked={mode === "disabled"}
            onChange={() => onChange({ type: "disabled" })}
          />
          Apagar
        </label>
        <label>
          <input
            type="radio"
            checked={mode === "enabled"}
            onChange={() => onChange({ type: "enabled", id: enabledId || options[0]?.id || "" })}
            disabled={options.length === 0}
          />
          Encender:
        </label>
        {mode === "enabled" ? (
          <select
            value={enabledId}
            onChange={(e) => onChange({ type: "enabled", id: e.currentTarget.value })}
          >
            {options.length === 0 ? <option value="">— sin opciones —</option> : null}
            {options.map((o) => (
              <option key={o.id} value={o.id}>
                {o.name}
              </option>
            ))}
          </select>
        ) : null}
      </div>
    </div>
  );
}

function stringifyError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return JSON.stringify(e);
}
