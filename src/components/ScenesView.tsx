import type { DraftScene } from "@bindings/DraftScene";
import type { LoopGroupActiveChange } from "@bindings/LoopGroupActiveChange";
import type { Scene } from "@bindings/Scene";
import type { SceneFxState } from "@bindings/SceneFxState";
import type { SceneLoopGroup } from "@bindings/SceneLoopGroup";
import type { SceneStep } from "@bindings/SceneStep";
import { ask } from "@tauri-apps/plugin-dialog";
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
  const activeLoopGroupQuery = useShowStore((s) => s.activeLoopGroup);
  const createLoopGroup = useShowStore((s) => s.createLoopGroup);
  const updateLoopGroup = useShowStore((s) => s.updateLoopGroup);
  const deleteLoopGroup = useShowStore((s) => s.deleteLoopGroup);
  const startLoopGroup = useShowStore((s) => s.startLoopGroup);
  const stopLoopGroup = useShowStore((s) => s.stopLoopGroup);

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
    };
    tick();
    const interval = window.setInterval(tick, 300);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [activeLoopGroupQuery]);

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

          <LoopGroupsPanel
            groups={show.scene_loop_groups ?? []}
            scenes={scenes}
            activeLoop={activeLoop}
            onCreate={async () => {
              try {
                await createLoopGroup();
              } catch (e) {
                setError(stringifyError(e));
              }
            }}
            onUpdate={async (g) => {
              try {
                await updateLoopGroup(g);
              } catch (e) {
                setError(stringifyError(e));
              }
            }}
            onDelete={async (id) => {
              try {
                await deleteLoopGroup(id);
              } catch (e) {
                setError(stringifyError(e));
              }
            }}
            onStart={async (id) => {
              try {
                await startLoopGroup(id);
              } catch (e) {
                setError(stringifyError(e));
              }
            }}
            onStop={async () => {
              try {
                await stopLoopGroup();
              } catch (e) {
                setError(stringifyError(e));
              }
            }}
          />
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

/// Panel that lives under the scene list and lets the operator
/// curate "loop groups" — ordered playlists of scenes that cycle on
/// their own. Each group has a name, an ordered list of scenes, and
/// an optional dwell-override that forces every scene in the group
/// to hold for the same duration regardless of its own steps.
///
/// The panel intentionally stays compact: a one-line summary per
/// group, with the editor inlined inside a `<details>` so a show
/// with a dozen groups doesn't dominate the sidebar.
function LoopGroupsPanel({
  groups,
  scenes,
  activeLoop,
  onCreate,
  onUpdate,
  onDelete,
  onStart,
  onStop,
}: {
  groups: SceneLoopGroup[];
  scenes: Scene[];
  activeLoop: LoopGroupActiveChange;
  onCreate: () => void | Promise<void>;
  onUpdate: (group: SceneLoopGroup) => void | Promise<void>;
  onDelete: (id: string) => void | Promise<void>;
  onStart: (id: string) => void | Promise<void>;
  onStop: () => void | Promise<void>;
}) {
  return (
    <section className="scenes-loop-panel" data-doc="loop-groups">
      <header className="scenes-loop-head">
        <h4>Listas en loop</h4>
        <button type="button" className="scenes-loop-new" onClick={() => onCreate()}>
          + Nueva
        </button>
      </header>
      <p className="hint scenes-loop-hint">
        Reproducí secuencias en cadena: secuencia 1 → 2 → 3 → 1…
      </p>
      {groups.length === 0 ? (
        <p className="empty">Todavía no hay listas. Creá una y arrastrá secuencias adentro.</p>
      ) : (
        <ul className="scenes-loop-list">
          {groups.map((g) => (
            <LoopGroupCard
              key={g.id}
              group={g}
              scenes={scenes}
              isActive={activeLoop.active_group_id === g.id}
              currentIndex={
                activeLoop.active_group_id === g.id ? (activeLoop.current_index ?? null) : null
              }
              onUpdate={onUpdate}
              onDelete={() => onDelete(g.id)}
              onStart={() => onStart(g.id)}
              onStop={() => onStop()}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function LoopGroupCard({
  group,
  scenes,
  isActive,
  currentIndex,
  onUpdate,
  onDelete,
  onStart,
  onStop,
}: {
  group: SceneLoopGroup;
  scenes: Scene[];
  isActive: boolean;
  currentIndex: number | null;
  onUpdate: (group: SceneLoopGroup) => void | Promise<void>;
  onDelete: () => void | Promise<void>;
  onStart: () => void | Promise<void>;
  onStop: () => void | Promise<void>;
}) {
  const [name, setName] = useState(group.name);
  const [hold, setHold] = useState(group.hold_ms_override);
  useEffect(() => setName(group.name), [group.name]);
  useEffect(() => setHold(group.hold_ms_override), [group.hold_ms_override]);

  const sceneById = useMemo(() => {
    const map: Record<string, Scene> = {};
    for (const s of scenes) map[s.id] = s;
    return map;
  }, [scenes]);

  const liveCount = group.scene_ids.filter((id) => !!sceneById[id]).length;
  const unassignedScenes = scenes.filter((s) => !group.scene_ids.includes(s.id));

  const commitName = () => {
    const next = name.trim();
    if (next === "" || next === group.name) {
      setName(group.name);
      return;
    }
    onUpdate({ ...group, name: next });
  };

  const commitHold = () => {
    const next = Math.max(0, Math.floor(hold));
    if (next === group.hold_ms_override) return;
    onUpdate({ ...group, hold_ms_override: next });
  };

  const moveScene = (index: number, dir: -1 | 1) => {
    const target = index + dir;
    if (target < 0 || target >= group.scene_ids.length) return;
    const next = [...group.scene_ids];
    [next[index], next[target]] = [next[target], next[index]];
    onUpdate({ ...group, scene_ids: next });
  };
  const removeAt = (index: number) => {
    const next = group.scene_ids.filter((_, i) => i !== index);
    onUpdate({ ...group, scene_ids: next });
  };
  const appendScene = (id: string) => {
    if (!id) return;
    onUpdate({ ...group, scene_ids: [...group.scene_ids, id] });
  };

  return (
    <li className={`scenes-loop-card${isActive ? " active" : ""}`}>
      <div className="scenes-loop-card-row">
        <button
          type="button"
          className="scenes-loop-go"
          title={isActive ? "Detener loop" : "Iniciar loop"}
          onClick={() => (isActive ? onStop() : onStart())}
          disabled={!isActive && liveCount === 0}
        >
          {isActive ? "■" : "▶"}
        </button>
        <input
          className="scenes-loop-name"
          value={name}
          onChange={(e) => setName(e.currentTarget.value)}
          onBlur={commitName}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitName();
            else if (e.key === "Escape") setName(group.name);
          }}
        />
        <span className="scenes-loop-count">
          {liveCount} {liveCount === 1 ? "secuencia" : "secuencias"}
        </span>
        <button
          type="button"
          className="danger scenes-loop-del"
          title="Eliminar lista"
          onClick={async () => {
            const ok = await ask(`¿Eliminar la lista "${group.name}"?`, {
              title: "Eliminar lista en loop",
              kind: "warning",
            });
            if (ok) onDelete();
          }}
        >
          ×
        </button>
      </div>
      <details className="scenes-loop-details">
        <summary>Editar</summary>
        <div className="scenes-loop-edit">
          <label className="scenes-loop-hold">
            Tiempo por secuencia (ms)
            <input
              type="number"
              min={0}
              max={120000}
              step={100}
              value={hold}
              onChange={(e) => setHold(Number(e.currentTarget.value))}
              onBlur={commitHold}
              title="0 = usar el ciclo natural de cada secuencia (fade + hold de cada paso)"
            />
            <span className="hint scenes-loop-hold-hint">
              0 = usa el ciclo natural de cada secuencia.
            </span>
          </label>
          {group.scene_ids.length === 0 ? (
            <p className="empty">Sin secuencias todavía.</p>
          ) : (
            <ol className="scenes-loop-items">
              {assignSlotKeys(group.scene_ids).map(({ key, sid, i }) => {
                const scene = sceneById[sid];
                const label = scene?.name ?? "⚠ secuencia eliminada";
                const live = isActive && currentIndex !== null && currentIndex === i;
                return (
                  <li
                    key={key}
                    className={`scenes-loop-item${live ? " live" : ""}${scene ? "" : " missing"}`}
                  >
                    <span className="scenes-loop-item-idx">{i + 1}.</span>
                    <span className="scenes-loop-item-name">{label}</span>
                    <button
                      type="button"
                      onClick={() => moveScene(i, -1)}
                      disabled={i === 0}
                      title="Mover arriba"
                    >
                      ↑
                    </button>
                    <button
                      type="button"
                      onClick={() => moveScene(i, 1)}
                      disabled={i === group.scene_ids.length - 1}
                      title="Mover abajo"
                    >
                      ↓
                    </button>
                    <button
                      type="button"
                      className="danger"
                      onClick={() => removeAt(i)}
                      title="Quitar de la lista"
                    >
                      ×
                    </button>
                  </li>
                );
              })}
            </ol>
          )}
          <div className="scenes-loop-add-row">
            <select
              value=""
              onChange={(e) => {
                appendScene(e.currentTarget.value);
                e.currentTarget.value = "";
              }}
            >
              <option value="">+ Agregar secuencia…</option>
              {unassignedScenes.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
              {scenes
                .filter((s) => group.scene_ids.includes(s.id))
                .map((s) => (
                  <option key={`dup-${s.id}`} value={s.id}>
                    {s.name} (repetir)
                  </option>
                ))}
            </select>
          </div>
        </div>
      </details>
    </li>
  );
}

/// Build stable React keys for a list that may contain duplicate scene
/// ids. The same scene id repeats once for each occurrence, so we
/// append `#N` where N is how many times that id has been seen so far.
/// Avoids using the bare array index as the key (which would break
/// reorders) while still letting the operator add the same scene
/// twice to a playlist.
function assignSlotKeys(scene_ids: string[]): { key: string; sid: string; i: number }[] {
  const seen: Record<string, number> = {};
  return scene_ids.map((sid, i) => {
    const count = (seen[sid] ?? 0) + 1;
    seen[sid] = count;
    return { key: `${sid}#${count}`, sid, i };
  });
}

function stringifyError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return JSON.stringify(e);
}
