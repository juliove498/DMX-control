import type { DraftScene } from "@bindings/DraftScene";
import type { LoopGroupActiveChange } from "@bindings/LoopGroupActiveChange";
import type { Scene } from "@bindings/Scene";
import type { SceneFxState } from "@bindings/SceneFxState";
import type { SceneStep } from "@bindings/SceneStep";
import type { Snapshot } from "@bindings/Snapshot";
import { useEffect, useMemo, useState } from "react";
import { useT } from "../i18n";
import type { Translation } from "../i18n/translations";
import { useShowStore } from "../stores/show";
import { AiGenerateModal, type AiGenerateModalSeed } from "./AiGenerateModal";

// Spanish "fixtures" / "scenes" / "tocados" all switch their plural
// suffix on count === 1 → empty, otherwise → "s". English does the same
// for "scene" / "step" / "fixture". Single helper for both — passed to
// `t()` as the `plural` substitution.
const plural = (n: number) => (n === 1 ? "" : "s");

/// Project a live Scene into the DraftScene shape the LLM iteration
/// flow expects. Drops chaser/movement state — the iterator focuses
/// on light values; FX layers are preserved unchanged when the
/// scene is replaced via `aiReplaceScene`.
function sceneToDraft(scene: Scene): DraftScene {
  return {
    name: scene.name,
    steps: scene.steps.map((step) => ({
      name: step.name ?? null,
      fade_in_ms: step.fade_in_ms,
      hold_ms: step.hold_ms,
      fixtures: step.fixtures.map((fx) => ({
        fixture_id: fx.fixture_id,
        values: fx.values.map((v) => ({
          channel_offset: v.channel_offset,
          value: v.value,
        })),
      })),
    })),
  };
}

/// Scenes UI (Phase 4 iteration 3): two-pane layout.
///
/// Left pane = scene list with GO buttons. Right pane = editor for the
/// currently-selected scene: name, FX capture toggles, step list with
/// per-step fade/hold + recapture/delete, and "Add step from current"
/// at the bottom. The active scene + active step get a dorado halo
/// matching what the Launchpad shows.
export function ScenesView() {
  const t = useT();
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
  const [aiOpen, setAiOpen] = useState(false);
  // When set, the modal opens in iterate mode with this seed instead
  // of the from-scratch form. Cleared on close.
  const [aiSeed, setAiSeed] = useState<AiGenerateModalSeed | undefined>(undefined);
  // Live state of any running loop group (the playlist driver). Polled
  // alongside scene state so the panel stays in sync.
  const [activeLoop, setActiveLoop] = useState<LoopGroupActiveChange>({
    active_group_id: null,
    current_index: null,
    current_scene_id: null,
  });
  // Loop groups themselves are managed in the dedicated Loops tab now;
  // here we only listen to the active one so the sidebar can show a
  // "Running: X [Stop]" footer when one is playing.
  const activeLoopGroupQuery = useShowStore((s) => s.activeLoopGroup);
  const stopLoopGroup = useShowStore((s) => s.stopLoopGroup);

  // Snapshots: whole-rig captures the operator can toggle like a
  // super-cue. Activation remembers the pre-state; deactivation puts
  // everything back.
  const captureSnapshot = useShowStore((s) => s.captureSnapshot);
  const updateSnapshotFromState = useShowStore((s) => s.updateSnapshotFromState);
  const renameSnapshot = useShowStore((s) => s.renameSnapshot);
  const deleteSnapshot = useShowStore((s) => s.deleteSnapshot);
  const activateSnapshot = useShowStore((s) => s.activateSnapshot);
  const deactivateSnapshot = useShowStore((s) => s.deactivateSnapshot);
  const activeSnapshotIdQuery = useShowStore((s) => s.activeSnapshotId);
  const [activeSnapshot, setActiveSnapshot] = useState<string | null>(null);

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

  // Loop-group state poll: independent from the scene poll so the
  // panel updates even when no scene is active.
  useEffect(() => {
    let cancelled = false;
    const tick = () => {
      activeLoopGroupQuery()
        .then((s) => {
          if (!cancelled) setActiveLoop(s);
        })
        .catch(() => {});
      activeSnapshotIdQuery()
        .then((id) => {
          if (!cancelled) setActiveSnapshot(id ?? null);
        })
        .catch(() => {});
    };
    tick();
    const interval = window.setInterval(tick, 300);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [activeLoopGroupQuery, activeSnapshotIdQuery]);

  if (!show) return <main className="page">{t("common.loading")}</main>;
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
      setError(t("scenes.errCreate", { err: stringifyError(e) }));
    }
  };

  return (
    <main className="page scenes-view-v2">
      <header className="page-head">
        <h2>{t("scenes.title")}</h2>
        <span className="meta">
          {t("scenes.metaSummary", { count: scenes.length, plural: plural(scenes.length) })}
        </span>
        <div className="actions">
          <button
            type="button"
            className="ai-trigger-btn"
            onClick={() => {
              setAiSeed(undefined);
              setAiOpen(true);
            }}
            title={t("scenes.aiTriggerHint")}
          >
            {t("scenes.aiTrigger")}
          </button>
        </div>
      </header>

      {error ? (
        <output className="lib-error" aria-live="polite">
          {error}
        </output>
      ) : null}

      <div className="scenes-layout">
        {/* ---- LEFT: list ---- */}
        <aside className="scenes-list-pane" data-doc="scenes-list">
          <button
            type="button"
            className="scenes-new-btn"
            data-doc="scenes-new"
            onClick={handleCreate}
          >
            {t("scenes.list.new")}
          </button>
          {scenes.length === 0 ? (
            <p className="empty">{t("scenes.list.empty")}</p>
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
            <div className="scenes-list-footer" data-doc="scenes-active-footer">
              <span>
                {t("scenes.list.activePrefix", {
                  name: scenes.find((s) => s.id === activeId)?.name ?? "—",
                })}
                {activeStep !== null ? (
                  <span className="scene-active-step">
                    {t("scenes.list.activeStep", { step: activeStep + 1 })}
                  </span>
                ) : null}
              </span>
              <button type="button" data-doc="scenes-release" onClick={() => releaseScene()}>
                {t("scenes.list.release")}
              </button>
            </div>
          ) : null}

          {activeLoop.active_group_id ? (
            <div
              className="scenes-list-footer scenes-list-loop-footer"
              data-doc="active-loop-footer"
            >
              <span>
                {t("scenes.list.activeLoopPrefix")}
                <strong>
                  {show.scene_loop_groups?.find((g) => g.id === activeLoop.active_group_id)?.name ??
                    "—"}
                </strong>
                {activeLoop.current_index !== null ? (
                  <span className="scene-active-step"> ({activeLoop.current_index + 1})</span>
                ) : null}
              </span>
              <button
                type="button"
                onClick={async () => {
                  try {
                    await stopLoopGroup();
                  } catch (e) {
                    setError(stringifyError(e));
                  }
                }}
              >
                {t("scenes.list.activeLoopStop")}
              </button>
            </div>
          ) : null}

          {/* ---- Snapshots: whole-rig capture/toggle ---- */}
          <div className="snapshots-section" data-doc="snapshots">
            <div className="snapshots-head">
              <h4>{t("snapshots.heading")}</h4>
              <button
                type="button"
                className="snapshots-capture-btn"
                data-doc="snapshot-capture"
                title={t("snapshots.captureHint")}
                onClick={async () => {
                  setError(null);
                  try {
                    await captureSnapshot();
                  } catch (e) {
                    setError(stringifyError(e));
                  }
                }}
              >
                {t("snapshots.capture")}
              </button>
            </div>
            {(show.snapshots ?? []).length === 0 ? (
              <p className="empty">{t("snapshots.empty")}</p>
            ) : (
              <ul className="snapshots-list">
                {(show.snapshots ?? []).map((snap) => (
                  <SnapshotRow
                    key={snap.id}
                    snapshot={snap}
                    isActive={snap.id === activeSnapshot}
                    onActivate={async () => {
                      setError(null);
                      try {
                        await activateSnapshot(snap.id);
                        setActiveSnapshot(snap.id);
                      } catch (e) {
                        setError(stringifyError(e));
                      }
                    }}
                    onDeactivate={async () => {
                      setError(null);
                      try {
                        await deactivateSnapshot();
                        setActiveSnapshot(null);
                      } catch (e) {
                        setError(stringifyError(e));
                      }
                    }}
                    onRecapture={async () => {
                      setError(null);
                      try {
                        await updateSnapshotFromState(snap.id);
                      } catch (e) {
                        setError(stringifyError(e));
                      }
                    }}
                    onRename={async (name) => {
                      try {
                        await renameSnapshot(snap.id, name);
                      } catch (e) {
                        setError(stringifyError(e));
                      }
                    }}
                    onDelete={async () => {
                      try {
                        await deleteSnapshot(snap.id);
                      } catch (e) {
                        setError(stringifyError(e));
                      }
                    }}
                  />
                ))}
              </ul>
            )}
          </div>
        </aside>

        {/* ---- RIGHT: editor ---- */}
        <section className="scenes-editor-pane" data-doc="scenes-editor">
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
              onIterateWithAi={() => {
                setAiSeed({
                  sceneId: selectedScene.id,
                  sceneName: selectedScene.name,
                  draft: sceneToDraft(selectedScene),
                });
                setAiOpen(true);
              }}
            />
          ) : (
            <div className="scenes-empty-editor">
              <p>{t("scenes.editor.empty")}</p>
            </div>
          )}
        </section>
      </div>

      {touched.length > 0 ? (
        <footer className="programmer-bar" data-doc="programmer-bar">
          <span className="programmer-bar-label">
            {t("scenes.programmer.label", {
              count: touched.length,
              plural: plural(touched.length),
            })}
          </span>
          <button type="button" onClick={() => programmerClear()}>
            {t("scenes.programmer.clear")}
          </button>
        </footer>
      ) : null}
      {aiOpen ? (
        <AiGenerateModal
          initialSeed={aiSeed}
          onClose={() => {
            setAiOpen(false);
            setAiSeed(undefined);
          }}
        />
      ) : null}
    </main>
  );
}

function SnapshotRow({
  snapshot,
  isActive,
  onActivate,
  onDeactivate,
  onRecapture,
  onRename,
  onDelete,
}: {
  snapshot: Snapshot;
  isActive: boolean;
  onActivate: () => void;
  onDeactivate: () => void;
  onRecapture: () => void;
  onRename: (name: string) => void;
  onDelete: () => void;
}) {
  const t = useT();
  const [name, setName] = useState(snapshot.name);
  useEffect(() => setName(snapshot.name), [snapshot.name]);

  const commitName = () => {
    const next = name.trim();
    if (next === "" || next === snapshot.name) {
      setName(snapshot.name);
      return;
    }
    onRename(next);
  };

  return (
    <li
      className={`snapshot-item${isActive ? " active" : ""}`}
      data-doc="snapshot-item"
      data-doc-active={isActive ? "true" : undefined}
    >
      <button
        type="button"
        className="snapshot-toggle"
        onClick={isActive ? onDeactivate : onActivate}
        title={isActive ? t("snapshots.deactivateHint") : t("snapshots.activateHint")}
      >
        {isActive ? "■" : "▶"}
      </button>
      <input
        className="snapshot-name"
        value={name}
        onChange={(e) => setName(e.currentTarget.value)}
        onBlur={commitName}
        onKeyDown={(e) => {
          if (e.key === "Enter") commitName();
          else if (e.key === "Escape") setName(snapshot.name);
        }}
      />
      {isActive ? (
        <span className="snapshot-active-badge">{t("snapshots.activeBadge")}</span>
      ) : null}
      <button
        type="button"
        className="snapshot-recapture"
        onClick={onRecapture}
        title={t("snapshots.recaptureHint")}
      >
        ↻
      </button>
      <button
        type="button"
        className="snapshot-del danger"
        onClick={() => {
          if (window.confirm(t("snapshots.deleteConfirm", { name: snapshot.name }))) onDelete();
        }}
        title={t("snapshots.deleteHint")}
      >
        ×
      </button>
    </li>
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
  const t = useT();
  const lpHint = index < 8 ? t("scenes.list.lpHint", { pad: index + 1 }) : null;
  return (
    <li
      data-doc="scene-list-item"
      data-doc-active={isActive ? "true" : undefined}
      className={`scenes-list-item${isSelected ? " selected" : ""}${isActive ? " active" : ""}`}
    >
      <button
        type="button"
        className="scenes-list-go"
        data-doc="scene-go"
        onClick={(e) => {
          e.stopPropagation();
          onRecall();
        }}
        title={t("scenes.list.recallHint")}
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
          {t("scenes.list.stepCount", {
            count: scene.steps.length,
            plural: plural(scene.steps.length),
          })}
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
  onIterateWithAi,
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
  /** Open the AI modal seeded with this scene so the operator can
   *  iterate over its values with a tweak prompt. */
  onIterateWithAi: () => void;
}) {
  const t = useT();
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
      <header
        data-doc="scene-editor-head"
        className={`scene-editor-head${isActive ? " active" : ""}`}
      >
        <button
          type="button"
          className="scene-editor-go"
          data-doc="scene-editor-go"
          onClick={onRecall}
          title={t("scenes.editor.recallHint")}
        >
          {t("scenes.editor.go")}
        </button>
        <input
          className="scene-editor-name"
          data-doc="scene-editor-name"
          value={name}
          onChange={(e) => setName(e.currentTarget.value)}
          onBlur={commitName}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitName();
            else if (e.key === "Escape") setName(scene.name);
          }}
        />
        <span className="scene-editor-cycle">
          {t("scenes.editor.cycleTotal", { seconds: (totalCycleMs / 1000).toFixed(1) })}
        </span>
        <button
          type="button"
          className="ai-trigger-btn scene-editor-ai"
          onClick={onIterateWithAi}
          title={t("scenes.editor.aiIterateHint")}
        >
          {t("scenes.editor.aiIterate")}
        </button>
        <button
          type="button"
          className="danger"
          onClick={async () => {
            if (window.confirm(t("scenes.editor.deleteConfirm", { name: scene.name })))
              await onDelete();
          }}
        >
          {t("scenes.editor.delete")}
        </button>
      </header>

      <p className="hint scene-fx-hint">{t("scenes.editor.fxHint")}</p>

      <div className="scene-steps-wrap" data-doc="scene-steps-wrap">
        <h4>{t("scenes.editor.stepsHeading", { count: scene.steps.length })}</h4>
        {scene.steps.length === 0 ? (
          <p className="empty">{t("scenes.editor.stepsEmpty")}</p>
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

        <div className="scene-add-step" data-doc="scene-add-step">
          <h5>{t("scenes.editor.addStepHeading")}</h5>
          <div className="scene-add-row">
            <label>
              {t("scenes.editor.fadeIn")}
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
              {t("scenes.editor.hold")}
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
              {t("scenes.editor.touchedOnly", { count: touched.length })}
            </label>
            <button
              type="button"
              className="scene-add-btn"
              onClick={onAdd}
              disabled={addTouchedOnly && touched.length === 0}
              title={
                addTouchedOnly && touched.length === 0
                  ? t("scenes.editor.addDisabledHint")
                  : t("scenes.editor.addEnabledHint")
              }
            >
              {t("scenes.editor.addStep")}
            </button>
          </div>
          <p className="hint scene-add-hint">{t("scenes.editor.loopHint")}</p>
        </div>
      </div>

      <p className="hint scene-fixtures-hint">
        {t("scenes.editor.fixturesHint", {
          writes: scene.steps.reduce((acc, s) => acc + s.fixtures.length, 0),
          fixtures: fixtures.length,
        })}{" "}
        <button type="button" className="scene-prog-clear-link" onClick={() => programmerClear()}>
          {t("scenes.editor.clearProg")}
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
  const t = useT();
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
          placeholder={t("scenes.step.placeholder", { n: index + 1 })}
          value={localName}
          onChange={(e) => setLocalName(e.currentTarget.value)}
          onBlur={() => onRename(localName)}
          onKeyDown={(e) => {
            if (e.key === "Enter") onRename(localName);
          }}
        />
        <label className="scene-step-time">
          {t("scenes.step.fade")}
          <input
            type="number"
            min={0}
            max={60000}
            step={50}
            value={localFade}
            onChange={(e) => setLocalFade(Number(e.currentTarget.value))}
            onBlur={() => onChangeFade(localFade)}
          />
          {t("scenes.step.ms")}
        </label>
        <label className="scene-step-time">
          {t("scenes.step.hold")}
          <input
            type="number"
            min={0}
            max={60000}
            step={50}
            value={localHold}
            onChange={(e) => setLocalHold(Number(e.currentTarget.value))}
            onBlur={() => onChangeHold(localHold)}
          />
          {t("scenes.step.ms")}
        </label>
        <span className="scene-step-meta">
          {t("scenes.step.metaFmt", { fixtures: fixtureCount, channels: channelCount })}
        </span>
        <button
          type="button"
          className="scene-step-update"
          onClick={onUpdateAll}
          title={t("scenes.step.updateAllHint")}
        >
          {t("scenes.step.update")}
        </button>
        <button
          type="button"
          className="scene-step-update touched"
          onClick={onUpdateTouched}
          disabled={touchedCount === 0}
          title={t("scenes.step.updateTouchedHint")}
        >
          {t("scenes.step.updateTouched")}
        </button>
        <button
          type="button"
          className="scene-step-del danger"
          onClick={() => {
            if (window.confirm(t("scenes.step.removeConfirm", { n: index + 1 }))) onRemove();
          }}
          disabled={!canRemove}
          title={canRemove ? t("scenes.step.removeHint") : t("scenes.step.removeOnlyHint")}
        >
          ×
        </button>
      </div>
      <div className="scene-step-fx">
        <FxStateRow
          labelKey="scenes.fx.chaser"
          state={step.chaser_state}
          options={chaserOptions}
          onChange={onChangeChaserState}
          compact
        />
        <FxStateRow
          labelKey="scenes.fx.movement"
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
  labelKey,
  state,
  options,
  onChange,
  compact,
}: {
  labelKey: keyof Translation;
  state: SceneFxState;
  options: { id: string; name: string }[];
  onChange: (state: SceneFxState) => void;
  compact?: boolean;
}) {
  const t = useT();
  const mode: "inherit" | "disabled" | "enabled" =
    state.type === "inherit" ? "inherit" : state.type === "disabled" ? "disabled" : "enabled";
  const enabledId = state.type === "enabled" ? state.id : "";
  return (
    <div className={`fx-state-row${compact ? " compact" : ""}`}>
      <span className="fx-state-label">{t(labelKey)}</span>
      <div className="fx-state-options">
        <label>
          <input
            type="radio"
            checked={mode === "inherit"}
            onChange={() => onChange({ type: "inherit" })}
          />
          {t("scenes.fx.inherit")}
        </label>
        <label>
          <input
            type="radio"
            checked={mode === "disabled"}
            onChange={() => onChange({ type: "disabled" })}
          />
          {t("scenes.fx.disable")}
        </label>
        <label>
          <input
            type="radio"
            checked={mode === "enabled"}
            onChange={() => onChange({ type: "enabled", id: enabledId || options[0]?.id || "" })}
            disabled={options.length === 0}
          />
          {t("scenes.fx.enable")}
        </label>
        {mode === "enabled" ? (
          <select
            value={enabledId}
            onChange={(e) => onChange({ type: "enabled", id: e.currentTarget.value })}
          >
            {options.length === 0 ? <option value="">{t("scenes.fx.noOptions")}</option> : null}
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
