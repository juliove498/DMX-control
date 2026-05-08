import { useEffect, useRef, useState } from "react";
import { useT } from "../i18n";
import { useShowStore } from "../stores/show";

/// Header BPM control: shows the current overall BPM, lets the user
/// toggle the override on/off, edit the value numerically, and tap-tempo
/// it via a dedicated TAP button.
///
/// The numeric input is local-state-driven so the user can type without
/// the value snapping mid-edit; we only commit on blur or Enter.
export function OverallBpmControl() {
  const t = useT();
  const enabled = useShowStore((s) => s.show?.globals?.overall_bpm_enabled ?? false);
  const persistedBpm = useShowStore((s) => s.show?.globals?.overall_bpm ?? 120);
  const setOverallBpm = useShowStore((s) => s.setOverallBpm);
  const setOverallBpmEnabled = useShowStore((s) => s.setOverallBpmEnabled);
  const tapOverallBpm = useShowStore((s) => s.tapOverallBpm);

  const [draft, setDraft] = useState(String(Math.round(persistedBpm)));
  const draftDirty = useRef(false);
  const tapFlash = useRef<HTMLButtonElement | null>(null);

  // Re-sync the draft to persisted BPM only when the user isn't
  // actively editing — otherwise typing while a TAP rolls in would
  // wipe the in-progress digit.
  useEffect(() => {
    if (!draftDirty.current) {
      setDraft(String(Math.round(persistedBpm)));
    }
  }, [persistedBpm]);

  const commitDraft = async () => {
    draftDirty.current = false;
    const parsed = Number(draft);
    if (Number.isFinite(parsed) && parsed >= 20 && parsed <= 300) {
      await setOverallBpm(parsed);
    } else {
      // Bad input — snap back to the persisted value.
      setDraft(String(Math.round(persistedBpm)));
    }
  };

  const onTap = async () => {
    // Visual flash on tap registration. CSS-driven: just toggle a
    // class for ~120 ms and the operator gets feedback even before
    // the round-trip to Rust completes.
    const el = tapFlash.current;
    if (el) {
      el.classList.add("flash");
      window.setTimeout(() => el.classList.remove("flash"), 120);
    }
    await tapOverallBpm();
  };

  return (
    <div className={`overall-bpm${enabled ? " active" : ""}`}>
      <span className="overall-bpm-label">{t("app.global.bpm.label")}</span>
      <input
        className="overall-bpm-input"
        type="number"
        min={20}
        max={300}
        step={1}
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
            setDraft(String(Math.round(persistedBpm)));
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
        title={t("app.global.bpm.tapHint")}
      >
        {t("app.global.bpm.tap")}
      </button>
    </div>
  );
}
