import type { AudioBpmStatus } from "@bindings/AudioBpmStatus";
import type { AudioInputInfo } from "@bindings/AudioInputInfo";
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
  const audioBpmDevices = useShowStore((s) => s.audioBpmDevices);
  const audioBpmStart = useShowStore((s) => s.audioBpmStart);
  const audioBpmStop = useShowStore((s) => s.audioBpmStop);
  const audioBpmStatus = useShowStore((s) => s.audioBpmStatus);
  const audioBpmSetAuto = useShowStore((s) => s.audioBpmSetAuto);

  // Mic BPM counter: popover open state, device list, live status.
  const [micOpen, setMicOpen] = useState(false);
  const [micDevices, setMicDevices] = useState<AudioInputInfo[]>([]);
  const [micDevice, setMicDevice] = useState<string>("");
  const [mic, setMic] = useState<AudioBpmStatus | null>(null);

  // Poll the listener while the popover is open OR the counter runs,
  // so the 🎤 chip keeps showing the live BPM with the panel closed.
  useEffect(() => {
    if (!micOpen && !mic?.running) return;
    let cancelled = false;
    const tick = () => {
      audioBpmStatus()
        .then((s) => {
          if (!cancelled) setMic(s);
        })
        .catch(() => {});
    };
    tick();
    const interval = window.setInterval(tick, 250);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [micOpen, mic?.running, audioBpmStatus]);

  const openMic = async () => {
    setMicOpen(true);
    try {
      setMicDevices(await audioBpmDevices());
    } catch {
      setMicDevices([]);
    }
  };

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
      <button
        type="button"
        className={`global-btn bpm-mic-btn${mic?.running ? " active" : ""}${
          mic?.beat ? " beat" : ""
        }`}
        onClick={() => (micOpen ? setMicOpen(false) : openMic())}
        title={t("app.global.bpm.micHint")}
      >
        {mic?.running && mic.bpm != null ? `🎤 ${mic.bpm.toFixed(1)}` : "🎤"}
      </button>
      {micOpen ? (
        <div className="bpm-mic-panel">
          <div className="bpm-mic-head">
            <strong>{t("app.global.bpm.micTitle")}</strong>
            <button type="button" className="ghost bpm-mic-close" onClick={() => setMicOpen(false)}>
              ✕
            </button>
          </div>
          <label className="bpm-mic-device">
            {t("app.global.bpm.micDevice")}
            <select
              value={micDevice}
              onChange={(e) => setMicDevice(e.currentTarget.value)}
              disabled={mic?.running ?? false}
            >
              <option value="">{t("app.global.bpm.micDefault")}</option>
              {micDevices.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.name}
                  {d.is_default ? " ✓" : ""}
                </option>
              ))}
            </select>
          </label>
          <div className="bpm-mic-row">
            <button
              type="button"
              className={mic?.running ? "danger" : "primary"}
              onClick={async () => {
                if (mic?.running) {
                  await audioBpmStop();
                  setMic((s) => (s ? { ...s, running: false, bpm: null } : s));
                } else {
                  await audioBpmStart(micDevice || null);
                  setMic((s) => (s ? { ...s, running: true } : s));
                }
              }}
            >
              {mic?.running ? t("app.global.bpm.micStop") : t("app.global.bpm.micStart")}
            </button>
            <div className="bpm-mic-level" aria-hidden="true">
              <div
                className="bpm-mic-level-fill"
                style={{ width: `${Math.round((mic?.level ?? 0) * 100)}%` }}
              />
            </div>
            <span className={`bpm-mic-beat${mic?.beat ? " on" : ""}`} aria-hidden="true" />
          </div>
          <div className="bpm-mic-row">
            <span className="bpm-mic-readout">
              {mic?.running
                ? mic.bpm != null
                  ? mic.bpm.toFixed(1)
                  : t("app.global.bpm.micWaiting")
                : "—"}
            </span>
            <div
              className="bpm-mic-conf"
              title={t("app.global.bpm.micConfidence")}
              aria-hidden="true"
            >
              <div
                className="bpm-mic-conf-fill"
                style={{ width: `${Math.round((mic?.confidence ?? 0) * 100)}%` }}
              />
            </div>
          </div>
          <div className="bpm-mic-row">
            <button
              type="button"
              disabled={!(mic?.running && mic.bpm != null)}
              onClick={() => {
                if (mic?.bpm != null) setOverallBpm(mic.bpm);
              }}
              title={t("app.global.bpm.micApplyHint")}
            >
              {t("app.global.bpm.micApply")}
            </button>
            <label className="bpm-mic-auto" title={t("app.global.bpm.micAutoHint")}>
              <input
                type="checkbox"
                checked={mic?.auto_apply ?? false}
                onChange={async (e) => {
                  const v = e.currentTarget.checked;
                  await audioBpmSetAuto(v);
                  setMic((s) => (s ? { ...s, auto_apply: v } : s));
                }}
              />
              {t("app.global.bpm.micAuto")}
            </label>
          </div>
          {mic?.error ? <p className="bpm-mic-error">{mic.error}</p> : null}
        </div>
      ) : null}
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
