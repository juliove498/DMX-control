import type { VdjConfig } from "@bindings/VdjConfig";
import type { VdjStatus } from "@bindings/VdjStatus";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { useT } from "../i18n";

const VDJ_STATUS_EVENT = "vdj:status";

/// VirtualDJ tempo-bridge configuration. Layout: 3 cards (Connection,
/// Tempo behaviour, Live status), each grouped by purpose so the
/// operator can scan the page top-to-bottom: "where does it talk?",
/// "what does it do?", "is it working right now?". The Save button at
/// the bottom commits the form and the backend restarts the poller
/// with the new config if `enabled` is on.
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
    // Subscribe to the backend's status updates so the live card
    // tracks last_bpm / last_error in real time without polling.
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
      <p className="hint vdj-intro">{t("vdj.intro")}</p>

      {error ? (
        <output className="error vdj-error" aria-live="polite">
          {error}
        </output>
      ) : null}

      <section className="card vdj-card">
        <h3>{t("vdj.section.connection")}</h3>

        <div className="vdj-field">
          <label htmlFor="vdj-host">{t("vdj.host")}</label>
          <input
            id="vdj-host"
            type="text"
            value={config.host}
            onChange={(e) => setConfig({ ...config, host: e.currentTarget.value })}
          />
          <span className="hint">{t("vdj.hostHint")}</span>
        </div>

        <div className="vdj-field-row">
          <div className="vdj-field">
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

          <div className="vdj-field">
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
        </div>

        <div className="vdj-field">
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

        <label className="checkbox-row vdj-toggle">
          <input
            type="checkbox"
            checked={config.enabled}
            onChange={(e) => setConfig({ ...config, enabled: e.currentTarget.checked })}
          />
          <span>
            <strong>{t("vdj.enabled")}</strong>
            <span className="hint vdj-toggle-hint">{t("vdj.enabledHint")}</span>
          </span>
        </label>
      </section>

      <section className="card vdj-card">
        <h3>{t("vdj.section.behavior")}</h3>

        <label className="checkbox-row vdj-toggle">
          <input
            type="checkbox"
            checked={config.halve_above_enabled}
            onChange={(e) => setConfig({ ...config, halve_above_enabled: e.currentTarget.checked })}
          />
          <span>
            <strong>{t("vdj.halve.enabled")}</strong>
            <span className="hint vdj-toggle-hint">{t("vdj.halve.enabledHint")}</span>
          </span>
        </label>

        <div className="vdj-field vdj-field-indent">
          <label htmlFor="vdj-halve-threshold">{t("vdj.halve.threshold")}</label>
          <input
            id="vdj-halve-threshold"
            type="number"
            min={20}
            max={300}
            step={1}
            value={config.halve_above_threshold}
            onChange={(e) =>
              setConfig({
                ...config,
                halve_above_threshold: clampInt(Number(e.currentTarget.value), 20, 300),
              })
            }
            disabled={!config.halve_above_enabled}
          />
          <span className="hint">{t("vdj.halve.thresholdHint")}</span>
        </div>
      </section>

      <section className="card vdj-card vdj-status-card">
        <h3>{t("vdj.statusSection")}</h3>

        <div className="vdj-status-row">
          <span className={`vdj-status-pill${status?.running ? " running" : ""}`}>
            <span className="vdj-status-dot" aria-hidden="true" />
            {status?.running ? t("vdj.status.running") : t("vdj.status.stopped")}
          </span>
        </div>

        <div className="row vdj-status-line">
          <strong>{t("vdj.status.lastBpm")}</strong>
          <span className="vdj-bpm-value">
            {status?.last_bpm != null ? status.last_bpm.toFixed(2) : "—"}
          </span>
        </div>

        <div className="row vdj-status-line">
          <strong>{t("vdj.status.lastUpdate")}</strong>
          <span>
            {status?.last_success_at_secs != null
              ? t("vdj.status.secondsAgo", {
                  secs: String(Math.max(0, secondsSince(Number(status.last_success_at_secs)))),
                })
              : t("vdj.status.never")}
          </span>
        </div>

        {status?.last_error ? (
          <div className="vdj-status-error">
            <strong>{t("vdj.status.error")}</strong>
            <code>{status.last_error}</code>
          </div>
        ) : null}
      </section>

      <div className="vdj-save-bar">
        <button type="button" className="primary vdj-save-btn" onClick={onSave} disabled={saving}>
          {t("vdj.save")}
        </button>
      </div>
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
