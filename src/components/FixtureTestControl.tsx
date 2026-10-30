import type { FixtureTestMode } from "@bindings/FixtureTestMode";
import type { FixtureTestStatus } from "@bindings/FixtureTestStatus";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { useT } from "../i18n";

// Pre-show fixture check (grandMA Highlight / Next, Eos flash): while on,
// every patched fixture — or the current selection — flashes open white at
// a safe level, or lights up one at a time in patch order so the operator
// can match each physical light with its tile. The backend lays it over
// the show without touching scenes, chasers or the programmer, and it
// switches itself off after a while.

export function FixtureTestControl({
  selectedIds,
  onCurrent,
}: {
  selectedIds: string[];
  onCurrent: (id: string | null) => void;
}) {
  const t = useT();
  const [status, setStatus] = useState<FixtureTestStatus | null>(null);
  const [mode, setMode] = useState<FixtureTestMode>("all");
  const [level, setLevel] = useState(0.3);
  const enabled = status?.enabled ?? false;

  useEffect(() => {
    let alive = true;
    invoke<FixtureTestStatus>("fixture_test_status")
      .then((s) => {
        if (alive && s) {
          setStatus(s);
          setMode(s.mode);
          setLevel(s.level);
        }
      })
      .catch(() => {});
    const un = listen<FixtureTestStatus>("fixture_test:status", (e) => {
      if (alive) setStatus(e.payload);
    });
    return () => {
      alive = false;
      un.then((f) => f()).catch(() => {});
    };
  }, []);

  useEffect(() => {
    onCurrent(status?.enabled && status.mode === "step" ? status.current_id : null);
  }, [status, onCurrent]);

  const apply = useCallback(
    (next: { enabled: boolean; mode: FixtureTestMode; level: number }) => {
      invoke<FixtureTestStatus>("fixture_test_set", {
        enabled: next.enabled,
        mode: next.mode,
        level: next.level,
        ids: selectedIds,
      })
        .then(setStatus)
        .catch(() => {});
    },
    [selectedIds],
  );

  // A running check follows the selection.
  const idsKey = selectedIds.join(",");
  const prevKey = useRef(idsKey);
  useEffect(() => {
    if (prevKey.current === idsKey) return;
    prevKey.current = idsKey;
    if (enabled) apply({ enabled: true, mode, level });
  }, [idsKey, enabled, mode, level, apply]);

  const minutes = Math.max(1, Math.ceil((status?.remaining_secs ?? 0) / 60));
  return (
    <div className={`ftest${enabled ? " on" : ""}`}>
      <button
        type="button"
        className={`ftest__btn${enabled ? " active" : ""}`}
        onClick={() => apply({ enabled: !enabled, mode, level })}
        title={t("stage.test.hint")}
      >
        ⚡ {t("stage.test")}
      </button>
      {enabled ? (
        <>
          <span className="magic-seg ftest__seg">
            {(["all", "step"] as FixtureTestMode[]).map((m) => (
              <button
                key={m}
                type="button"
                className={mode === m ? "active" : ""}
                onClick={() => {
                  setMode(m);
                  apply({ enabled: true, mode: m, level });
                }}
              >
                {t(m === "all" ? "stage.test.all" : "stage.test.step")}
              </button>
            ))}
          </span>
          <label className="ftest__level">
            {Math.round(level * 100)}%
            <input
              type="range"
              min={0.05}
              max={1}
              step={0.05}
              value={level}
              onChange={(e) => {
                const v = Number(e.currentTarget.value);
                setLevel(v);
                apply({ enabled: true, mode, level: v });
              }}
            />
          </label>
          <span className="hint ftest__info">
            {status?.mode === "step" && status.current_label
              ? t("stage.test.now", { name: status.current_label })
              : t("stage.test.count", { n: String(status?.count ?? 0) })}
            {" · "}
            {t("stage.test.remaining", { m: String(minutes) })}
          </span>
        </>
      ) : null}
    </div>
  );
}
