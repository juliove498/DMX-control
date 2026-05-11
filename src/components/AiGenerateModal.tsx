import type { DraftScene } from "@bindings/DraftScene";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import { useEffect, useMemo, useState } from "react";
import { useT } from "../i18n";
import { useShowStore } from "../stores/show";

const plural = (n: number) => (n === 1 ? "" : "s");

/// Modal triggered from Scenes header (new) or from a scene's
/// "Mejorar con IA" button (iterate). Two-stage flow:
///   1. **Form**: prompt + step count + scope → "Generate".
///   2. **Preview**: shows draft + a "Refinar" textarea so the
///      operator can iterate without leaving the modal. Apply
///      creates a new scene; if the modal opened from an existing
///      scene, "Reemplazar original" overwrites in place.
///
/// `initialSeed` skips stage 1 and starts in preview with the
/// provided draft, so "Mejorar con IA" lands the operator straight
/// in the iteration loop.
export interface AiGenerateModalSeed {
  /// Original scene's id, present when iterating an existing scene
  /// (enables the "Reemplazar original" apply variant). Absent when
  /// iterating a draft that hasn't been applied yet.
  sceneId?: string;
  /// Display name for the modal title.
  sceneName: string;
  /// The seed itself, used both as the starting preview and as the
  /// payload sent to the LLM as `seed_scene`.
  draft: DraftScene;
}

export function AiGenerateModal({
  onClose,
  initialSeed,
}: {
  onClose: () => void;
  initialSeed?: AiGenerateModalSeed;
}) {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const aiGenerateSceneDraft = useShowStore((s) => s.aiGenerateSceneDraft);
  const aiApplyDraftScene = useShowStore((s) => s.aiApplyDraftScene);
  const aiReplaceScene = useShowStore((s) => s.aiReplaceScene);
  const getAiConfig = useShowStore((s) => s.getAiConfig);

  const [prompt, setPrompt] = useState("");
  const [refinePrompt, setRefinePrompt] = useState("");
  const [stepCount, setStepCount] = useState(initialSeed?.draft.steps.length ?? 4);
  const [scope, setScope] = useState<"all" | "selected">("all");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [draft, setDraft] = useState<DraftScene | null>(initialSeed?.draft ?? null);
  /// The seed sent to the LLM. Cleared when the operator hits
  /// "Re-generar desde cero" so the next call starts fresh.
  const [seedForLlm, setSeedForLlm] = useState<DraftScene | null>(initialSeed?.draft ?? null);
  /// The original scene id stays sticky across refinements — the
  /// "Reemplazar original" button shows whenever this is set, so the
  /// operator can iterate as many times as they want and still
  /// commit back into the same scene at the end.
  const [originalSceneId] = useState<string | undefined>(initialSeed?.sceneId);
  const [originalSceneName] = useState<string | undefined>(initialSeed?.sceneName);
  const [generating, setGenerating] = useState(false);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [providerLabel, setProviderLabel] = useState<string>("?");

  useEffect(() => {
    getAiConfig()
      .then((c) => {
        setProviderLabel(
          c.provider === "anthropic"
            ? `Anthropic · ${c.anthropic.model || "default"}`
            : c.provider === "openai"
              ? `OpenAI · ${c.openai.model || "default"}`
              : "—",
        );
      })
      .catch(() => setProviderLabel("?"));
  }, [getAiConfig]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const fixtures = show?.fixtures ?? [];
  const libraryById = useMemo(() => {
    const m: Record<string, FixtureDefinition> = {};
    for (const d of show?.library ?? []) m[d.id] = d;
    return m;
  }, [show?.library]);

  const onGenerate = async () => {
    if (!prompt.trim()) {
      setError(t("aiGen.errPromptEmpty"));
      return;
    }
    setError(null);
    setDraft(null);
    setGenerating(true);
    try {
      const ids = scope === "selected" ? Array.from(selectedIds) : null;
      if (scope === "selected" && (!ids || ids.length === 0)) {
        setError(t("aiGen.errScopeEmpty"));
        return;
      }
      const result = await aiGenerateSceneDraft(prompt.trim(), stepCount, ids, null);
      setDraft(result);
      setSeedForLlm(result);
    } catch (e) {
      setError(stringifyError(e));
    } finally {
      setGenerating(false);
    }
  };

  const onRefine = async () => {
    if (!seedForLlm) return;
    if (!refinePrompt.trim()) {
      setError(t("aiGen.errRefineEmpty"));
      return;
    }
    setError(null);
    setGenerating(true);
    try {
      const result = await aiGenerateSceneDraft(refinePrompt.trim(), stepCount, null, seedForLlm);
      setDraft(result);
      setSeedForLlm(result);
      setRefinePrompt("");
    } catch (e) {
      setError(stringifyError(e));
    } finally {
      setGenerating(false);
    }
  };

  const onResetFromScratch = () => {
    setDraft(null);
    setSeedForLlm(null);
    setRefinePrompt("");
  };

  const onApplyAsNew = async () => {
    if (!draft) return;
    setApplying(true);
    setError(null);
    try {
      await aiApplyDraftScene(draft);
      onClose();
    } catch (e) {
      setError(stringifyError(e));
    } finally {
      setApplying(false);
    }
  };

  const onReplaceOriginal = async () => {
    if (!draft || !originalSceneId) return;
    setApplying(true);
    setError(null);
    try {
      await aiReplaceScene(originalSceneId, draft);
      onClose();
    } catch (e) {
      setError(stringifyError(e));
    } finally {
      setApplying(false);
    }
  };

  return (
    <div
      className="ai-modal-backdrop"
      onPointerDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <section className="ai-modal" aria-label={t("aiGen.aria")}>
        <header className="ai-modal-head">
          <h3>
            {originalSceneName
              ? t("aiGen.titleIterating", { name: originalSceneName })
              : t("aiGen.title")}
          </h3>
          <span className="ai-modal-meta">{providerLabel}</span>
          <button
            type="button"
            className="ai-modal-close"
            onClick={onClose}
            aria-label={t("common.close")}
          >
            ×
          </button>
        </header>

        {!draft ? (
          <PromptForm
            prompt={prompt}
            onPrompt={setPrompt}
            stepCount={stepCount}
            onStepCount={setStepCount}
            scope={scope}
            onScope={setScope}
            fixtures={fixtures}
            libraryById={libraryById}
            selectedIds={selectedIds}
            onToggleFixture={(id) =>
              setSelectedIds((prev) => {
                const next = new Set(prev);
                if (next.has(id)) next.delete(id);
                else next.add(id);
                return next;
              })
            }
            generating={generating}
            onGenerate={onGenerate}
          />
        ) : (
          <DraftPreview
            draft={draft}
            fixtures={fixtures}
            libraryById={libraryById}
            applying={applying}
            generating={generating}
            refinePrompt={refinePrompt}
            onRefinePrompt={setRefinePrompt}
            onRefine={onRefine}
            onResetFromScratch={onResetFromScratch}
            onApplyAsNew={onApplyAsNew}
            onReplaceOriginal={originalSceneId ? onReplaceOriginal : undefined}
            originalSceneName={originalSceneName}
          />
        )}

        {error ? (
          <output className="ai-modal-err" aria-live="polite">
            {error}
          </output>
        ) : null}
      </section>
    </div>
  );
}

function PromptForm({
  prompt,
  onPrompt,
  stepCount,
  onStepCount,
  scope,
  onScope,
  fixtures,
  libraryById,
  selectedIds,
  onToggleFixture,
  generating,
  onGenerate,
}: {
  prompt: string;
  onPrompt: (v: string) => void;
  stepCount: number;
  onStepCount: (n: number) => void;
  scope: "all" | "selected";
  onScope: (s: "all" | "selected") => void;
  fixtures: FixtureInstance[];
  libraryById: Record<string, FixtureDefinition>;
  selectedIds: Set<string>;
  onToggleFixture: (id: string) => void;
  generating: boolean;
  onGenerate: () => void;
}) {
  const t = useT();
  return (
    <div className="ai-modal-body">
      <label className="ai-modal-field">
        <span>{t("aiGen.field.prompt")}</span>
        <textarea
          rows={4}
          value={prompt}
          onChange={(e) => onPrompt(e.currentTarget.value)}
          placeholder={t("aiGen.field.promptPlaceholder")}
          disabled={generating}
        />
      </label>
      <div className="ai-modal-row">
        <label className="ai-modal-field-inline">
          <span>{t("aiGen.field.stepCount")}</span>
          <input
            type="number"
            min={1}
            max={16}
            value={stepCount}
            onChange={(e) =>
              onStepCount(Math.max(1, Math.min(16, Number(e.currentTarget.value) || 1)))
            }
            disabled={generating}
          />
        </label>
        <label className="ai-modal-field-inline">
          <span>{t("aiGen.field.scope")}</span>
          <select
            value={scope}
            onChange={(e) => onScope(e.currentTarget.value as "all" | "selected")}
            disabled={generating}
          >
            <option value="all">{t("aiGen.scope.all", { count: fixtures.length })}</option>
            <option value="selected">{t("aiGen.scope.selected")}</option>
          </select>
        </label>
      </div>

      {scope === "selected" ? (
        <div className="ai-modal-fixtures">
          <p className="hint">{t("aiGen.scope.fixturesHint")}</p>
          <ul className="ai-fixture-list">
            {fixtures.map((f) => {
              const def = libraryById[f.definition_id];
              const checked = selectedIds.has(f.id);
              return (
                <li key={f.id}>
                  <label>
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => onToggleFixture(f.id)}
                      disabled={generating}
                    />
                    <span className="ai-fix-label">{f.label ?? def?.name ?? f.id}</span>
                    <span className="ai-fix-meta">
                      U{f.universe}·{f.address} · {def?.name ?? "?"}
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
        </div>
      ) : null}

      <div className="ai-modal-actions">
        <button
          type="button"
          className="ai-primary"
          onClick={onGenerate}
          disabled={generating || fixtures.length === 0}
          title={fixtures.length === 0 ? t("aiGen.disabledHint") : t("aiGen.activeHint")}
        >
          {generating ? t("aiGen.generating") : t("aiGen.generate")}
        </button>
      </div>
    </div>
  );
}

function DraftPreview({
  draft,
  fixtures,
  libraryById,
  applying,
  generating,
  refinePrompt,
  onRefinePrompt,
  onRefine,
  onResetFromScratch,
  onApplyAsNew,
  onReplaceOriginal,
  originalSceneName,
}: {
  draft: DraftScene;
  fixtures: FixtureInstance[];
  libraryById: Record<string, FixtureDefinition>;
  applying: boolean;
  generating: boolean;
  refinePrompt: string;
  onRefinePrompt: (v: string) => void;
  onRefine: () => void;
  onResetFromScratch: () => void;
  onApplyAsNew: () => void;
  onReplaceOriginal?: () => void;
  originalSceneName?: string;
}) {
  const t = useT();
  const fixtureLabel = (id: string): string => {
    const f = fixtures.find((x) => x.id === id);
    if (!f) return id;
    return f.label ?? libraryById[f.definition_id]?.name ?? id;
  };
  const busy = applying || generating;

  return (
    <div className="ai-modal-body">
      <div className="ai-draft-head">
        <strong>{draft.name}</strong>
        <span className="hint">
          {t("aiGen.preview.stepCount", {
            count: draft.steps.length,
            plural: plural(draft.steps.length),
          })}
        </span>
      </div>
      <ul className="ai-draft-steps">
        {draft.steps.map((step, i) => (
          // Read-only preview of a frozen draft; rows hold no state
          // and the list is regenerated whole on every prompt iteration,
          // so the position is a fine stable identity here.
          // biome-ignore lint/suspicious/noArrayIndexKey: draft preview is read-only.
          <li key={i} className="ai-draft-step">
            <header>
              <span className="ai-step-num">{i + 1}</span>
              <strong>{step.name ?? t("aiGen.preview.stepPlaceholder", { n: i + 1 })}</strong>
              <span className="hint">
                {t("aiGen.preview.stepMeta", {
                  fade: step.fade_in_ms,
                  hold: step.hold_ms,
                  count: step.fixtures.length,
                  plural: plural(step.fixtures.length),
                })}
              </span>
            </header>
            <ul className="ai-draft-fixtures">
              {step.fixtures.map((fx, j) => (
                // Same reasoning as above — preview, not editable.
                // biome-ignore lint/suspicious/noArrayIndexKey: draft preview is read-only.
                <li key={j}>
                  <span className="ai-fix-label">{fixtureLabel(fx.fixture_id)}</span>
                  <span className="ai-fix-values">
                    {fx.values.map((v) => `ch${v.channel_offset}=${v.value}`).join(", ")}
                  </span>
                </li>
              ))}
            </ul>
          </li>
        ))}
      </ul>

      <div className="ai-refine">
        <label className="ai-modal-field">
          <span>{t("aiGen.refine")}</span>
          <textarea
            rows={2}
            value={refinePrompt}
            onChange={(e) => onRefinePrompt(e.currentTarget.value)}
            placeholder={t("aiGen.refinePlaceholder")}
            disabled={busy}
          />
        </label>
        <button
          type="button"
          onClick={onRefine}
          disabled={busy || !refinePrompt.trim()}
          title={t("aiGen.refineHint")}
        >
          {generating ? t("aiGen.refining") : t("aiGen.refineLabel")}
        </button>
      </div>

      <div className="ai-modal-actions">
        <button type="button" onClick={onResetFromScratch} disabled={busy}>
          {t("aiGen.resetFromScratch")}
        </button>
        {onReplaceOriginal ? (
          <button
            type="button"
            onClick={onReplaceOriginal}
            disabled={busy}
            title={t("aiGen.replaceOriginalHint", {
              name: originalSceneName ?? "—",
            })}
          >
            {applying ? t("aiGen.replacing") : t("aiGen.replaceOriginal")}
          </button>
        ) : null}
        <button type="button" className="ai-primary" onClick={onApplyAsNew} disabled={busy}>
          {applying ? t("aiGen.applying") : t("aiGen.applyNew")}
        </button>
      </div>
    </div>
  );
}

function stringifyError(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message?: unknown }).message ?? e);
  }
  return JSON.stringify(e);
}
