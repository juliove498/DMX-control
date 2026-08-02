import { useEffect, useRef, useState } from "react";
import { useT } from "../i18n";
import { useShowStore } from "../stores/show";

/// Header BPM control: shows the current overall BPM, lets the user
/// toggle the override on/off, edit the value numerically, and tap-tempo
/// it via a dedicated TAP button.
///
/// The numeric input is local-state-driven so the user can type without
/// the value snapping mid-edit; we only commit on blur or Enter.
///
/// Pattern recording: a "REC" button toggles a recording mode. While
/// recording, the TAP button captures pattern hits instead of advancing
/// the BPM measurement. Clicking REC again quantises and commits the
/// pattern. An "✕" button next to REC clears the active pattern.
export function OverallBpmControl() {
  const t = useT();
  const enabled = useShowStore((s) => s.show?.globals?.overall_bpm_enabled ?? false);
  const persistedBpm = useShowStore((s) => s.show?.globals?.overall_bpm ?? 120);
  const tempoPattern = useShowStore((s) => s.show?.globals?.tempo_pattern ?? null);
  const setOverallBpm = useShowStore((s) => s.setOverallBpm);
  const setOverallBpmEnabled = useShowStore((s) => s.setOverallBpmEnabled);
  const tapOverallBpm = useShowStore((s) => s.tapOverallBpm);
  const startPatternRecording = useShowStore((s) => s.startPatternRecording);
  const tapPatternRecord = useShowStore((s) => s.tapPatternRecord);
  const stopPatternRecording = useShowStore((s) => s.stopPatternRecording);
  const clearTempoPattern = useShowStore((s) => s.clearTempoPattern);

  const [draft, setDraft] = useState(formatBpm(persistedBpm));
  const draftDirty = useRef(false);
  const tapFlash = useRef<HTMLButtonElement | null>(null);
  // Local mirror of recording state. The Rust runtime owns the source
  // of truth, but the store doesn't re-render on its changes (it's
  // in-memory only, not persisted) — so we hold the flag here and flip
  // it ourselves around the start/stop calls.
  const [recording, setRecording] = useState(false);

  // Re-sync the draft to persisted BPM only when the user isn't
  // actively editing — otherwise typing while a TAP rolls in would
  // wipe the in-progress digit.
  useEffect(() => {
    if (!draftDirty.current) {
      setDraft(formatBpm(persistedBpm));
    }
  }, [persistedBpm]);

  const commitDraft = async () => {
    draftDirty.current = false;
    const parsed = Number(draft);
    if (Number.isFinite(parsed) && parsed >= 20 && parsed <= 300) {
      await setOverallBpm(parsed);
    } else {
      // Bad input — snap back to the persisted value.
      setDraft(formatBpm(persistedBpm));
    }
  };

  const flashTap = () => {
    // Visual flash on tap registration. CSS-driven: just toggle a
    // class for ~120 ms and the operator gets feedback even before
    // the round-trip to Rust completes.
    const el = tapFlash.current;
    if (el) {
      el.classList.add("flash");
      window.setTimeout(() => el.classList.remove("flash"), 120);
    }
  };

  const onTap = async () => {
    flashTap();
    // Recording mode: the same TAP button captures pattern hits
    // instead of advancing the BPM measurement, so the operator
    // doesn't have to learn a second physical control.
    if (recording) {
      await tapPatternRecord();
    } else {
      await tapOverallBpm();
    }
  };

  const onRecord = async () => {
    if (recording) {
      await stopPatternRecording();
      setRecording(false);
    } else {
      await startPatternRecording();
      setRecording(true);
    }
  };

  const onClearPattern = async () => {
    if (recording) {
      setRecording(false);
    }
    await clearTempoPattern();
  };

  return (
    <div
      className={`overall-bpm${enabled ? " active" : ""}${
        recording ? " recording" : ""
      }${tempoPattern ? " has-pattern" : ""}`}
    >
      <span className="overall-bpm-label">{t("app.global.bpm.label")}</span>
      <input
        className="overall-bpm-input"
        type="number"
        min={20}
        max={300}
        // 0.01 BPM = ~1 beat of drift over ~10 minutes at 120 BPM. Two
        // decimals is what VDJ ships and what we want to preserve
        // exactly through the pipeline — rounding to integers
        // reintroduces the desync the operator is trying to fix.
        step={0.01}
        value={draft}
        onChange={(e) => {
          draftDirty.current = true;
          setDraft(e.currentTarget.value);
        }}
        onBlur={commitDraft}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            (e.target as HTMLInputElement).blur();
          } else if (e.key === "Escape") {
            draftDirty.current = false;
            setDraft(formatBpm(persistedBpm));
            (e.target as HTMLInputElement).blur();
          }
        }}
        title={t("app.global.bpm.editHint")}
      />
      <button
        type="button"
        className={`global-btn bpm-toggle-btn${enabled ? " active" : ""}`}
        onClick={() => setOverallBpmEnabled(!enabled)}
        title={t("app.global.bpm.toggleHint")}
      >
        {enabled ? t("app.global.bpm.on") : t("app.global.bpm.off")}
      </button>
      <button
        ref={tapFlash}
        type="button"
        className="global-btn bpm-tap-btn"
        onClick={onTap}
        title={recording ? t("app.global.bpm.recordingHint") : t("app.global.bpm.tapHint")}
      >
        {t("app.global.bpm.tap")}
      </button>
      <button
        type="button"
        className={`global-btn bpm-rec-btn${recording ? " active" : ""}`}
        onClick={onRecord}
        title={
          recording ? t("app.global.bpm.recordingHint") : t("app.global.bpm.recordPatternHint")
        }
      >
        {recording ? t("app.global.bpm.recordingPattern") : t("app.global.bpm.recordPattern")}
      </button>
      {tempoPattern && (
        <>
          <span
            className="bpm-pattern-info"
            title={t("app.global.bpm.patternActive", {
              count: String(tempoPattern.hits.length),
              bars: String(tempoPattern.bars),
            })}
          >
            {t("app.global.bpm.patternActive", {
              count: String(tempoPattern.hits.length),
              bars: String(tempoPattern.bars),
            })}
          </span>
          <button
            type="button"
            className="global-btn bpm-clear-btn"
            onClick={onClearPattern}
            title={t("app.global.bpm.clearPatternHint")}
          >
            {t("app.global.bpm.clearPattern")}
          </button>
        </>
      )}
    </div>
  );
}

/// Render a BPM at the precision we care about (2 decimals), but drop
/// trailing zeros so 120 doesn't show as "120.00". Integer BPMs read
/// naturally; fractional ones show their actual value (120.55).
function formatBpm(bpm: number): string {
  const fixed = bpm.toFixed(2);
  // Strip trailing zeros and an orphan decimal point: 120.00 → 120,
  // 120.50 → 120.5, 120.55 → 120.55.
  return fixed.replace(/\.?0+$/, "");
}
