import type { VdjConfig } from "@bindings/VdjConfig";
import type { VdjStatus } from "@bindings/VdjStatus";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { useT } from "../i18n";

const VDJ_STATUS_EVENT = "vdj:status";

/// Manual Start/Stop UX: the operator edits host/port/token/etc., hits
/// Save, and the backend restarts the poller with the new config (if
/// `enabled` is on). The "Enabled" checkbox is the on/off switch — no
/// separate Start button — because the poller's running state IS the
/// enabled flag's persistence state. That keeps the UI honest with
/// what the show file actually stores.
export function VdjConfigView() {
  const t = useT();
  const [config, setConfig] = useState<VdjConfig | null>(null);
  const [status, setStatus] = useState<VdjStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [c, s] = await Promise.all([
        invoke<VdjConfig>("vdj_get_config"),
        invoke<VdjStatus>("vdj_get_status"),
      ]);
      setConfig(c);
      setStatus(s);
    } catch (e) {
      setError(stringify(e));
    }
  }, []);

  useEffect(() => {
    refresh();
    let unlisten: (() => void) | null = null;
    // Subscribe to the backend's status updates so the UI tracks
    // last_bpm / last_error in real time without polling.
    listen<VdjStatus>(VDJ_STATUS_EVENT, (ev) => {
      setStatus(ev.payload);
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, [refresh]);

  const onSave = async () => {
    if (!config) return;
    setSaving(true);
    setError(null);
    try {
      const s = await invoke<VdjStatus>("vdj_set_config", { config });
      setStatus(s);
    } catch (e) {
      setError(t("vdj.errSave", { err: stringify(e) }));
    } finally {
      setSaving(false);
    }
  };

  if (!config) {
    return <main className="page vdj-config-view" />;
  }

  return (
    <main className="page vdj-config-view">
      <header className="page-head">
        <h2>{t("vdj.title")}</h2>
      </header>
      <p className="hint">{t("vdj.intro")}</p>

      {error ? (
        <output className="lib-error" aria-live="polite">
          {error}
        </output>
      ) : null}

      <section className="config-section">
        <div className="form-row">
          <label htmlFor="vdj-host">{t("vdj.host")}</label>
          <input
            id="vdj-host"
            type="text"
            value={config.host}
            onChange={(e) => setConfig({ ...config, host: e.currentTarget.value })}
          />
          <span className="hint">{t("vdj.hostHint")}</span>
        </div>

        <div className="form-row">
          <label htmlFor="vdj-port">{t("vdj.port")}</label>
          <input
            id="vdj-port"
            type="number"
            min={1}
            max={65535}
            value={config.port}
            onChange={(e) =>
              setConfig({ ...config, port: clampInt(Number(e.currentTarget.value), 1, 65535) })
            }
          />
          <span className="hint">{t("vdj.portHint")}</span>
        </div>

        <div className="form-row">
          <label htmlFor="vdj-bearer">{t("vdj.bearer")}</label>
          <input
            id="vdj-bearer"
            type="password"
            value={config.bearer ?? ""}
            onChange={(e) =>
              setConfig({
                ...config,
                bearer: e.currentTarget.value.length > 0 ? e.currentTarget.value : null,
              })
            }
          />
          <span className="hint">{t("vdj.bearerHint")}</span>
        </div>

        <div className="form-row">
          <label htmlFor="vdj-interval">{t("vdj.interval")}</label>
          <input
            id="vdj-interval"
            type="number"
            min={50}
            max={5000}
            step={50}
            value={config.interval_ms}
            onChange={(e) =>
              setConfig({
                ...config,
                interval_ms: clampInt(Number(e.currentTarget.value), 50, 5000),
              })
            }
          />
          <span className="hint">{t("vdj.intervalHint")}</span>
        </div>

        <div className="form-row">
          <label htmlFor="vdj-enabled">
            <input
              id="vdj-enabled"
              type="checkbox"
              checked={config.enabled}
              onChange={(e) => setConfig({ ...config, enabled: e.currentTarget.checked })}
            />
            {t("vdj.enabled")}
          </label>
          <span className="hint">{t("vdj.enabledHint")}</span>
        </div>

        <div className="form-row">
          <button type="button" onClick={onSave} disabled={saving}>
            {t("vdj.save")}
          </button>
        </div>
      </section>

      <section className="config-section">
        <h3>{t("vdj.statusSection")}</h3>
        <ul className="vdj-status">
          <li>
            <span>{status?.running ? t("vdj.status.running") : t("vdj.status.stopped")}</span>
          </li>
          <li>
            <span>{t("vdj.status.lastBpm")}: </span>
            <strong>{status?.last_bpm != null ? status.last_bpm.toFixed(2) : "—"}</strong>
          </li>
          <li>
            <span>{t("vdj.status.lastUpdate")}: </span>
            <strong>
              {status?.last_success_at_secs != null
                ? t("vdj.status.secondsAgo", {
                    secs: String(Math.max(0, secondsSince(Number(status.last_success_at_secs)))),
                  })
                : t("vdj.status.never")}
            </strong>
          </li>
          {status?.last_error ? (
            <li className="vdj-status-error">
              <span>{t("vdj.status.error")}: </span>
              <code>{status.last_error}</code>
            </li>
          ) : null}
        </ul>
      </section>
    </main>
  );
}

function clampInt(v: number, lo: number, hi: number): number {
  if (!Number.isFinite(v)) return lo;
  return Math.max(lo, Math.min(hi, Math.round(v)));
}

function secondsSince(unixSecs: number): number {
  return Math.floor(Date.now() / 1000) - unixSecs;
}

function stringify(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return JSON.stringify(e);
}
