import type { ButtonAction } from "@bindings/ButtonAction";
import type { ButtonMode } from "@bindings/ButtonMode";
import type { CueFollowerStatus } from "@bindings/CueFollowerStatus";
import type { Os2lConfig } from "@bindings/Os2lConfig";
import type { Os2lStatus } from "@bindings/Os2lStatus";
import type { VdjConfig } from "@bindings/VdjConfig";
import type { VdjStatus } from "@bindings/VdjStatus";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { useT } from "../i18n";
import {
  type ActionKind,
  ACTION_KINDS as OS2L_KINDS,
  actionId,
  buildAction,
  itemsForKind,
  kindOfAction,
} from "../lib/actions";
import { deepClone } from "../lib/compat";
import { useShowStore } from "../stores/show";
import { VdjInstancesCard } from "./VdjInstancesCard";

const VDJ_STATUS_EVENT = "vdj:status";
const OS2L_STATUS_EVENT = "os2l:status";

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

        <VdjInstancesCard config={config} onConfig={(c) => setConfig(c)} />

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

        <details className="vdj-net-details">
          <summary>{t("vdj.net.title")}</summary>
          <div className="vdj-field">
            <label htmlFor="vdj-dbpath">{t("vdj.net.dbPath")}</label>
            <input
              id="vdj-dbpath"
              type="text"
              placeholder="~/Documents/VirtualDJ/database.xml"
              value={config.vdj_database_path}
              onChange={(e) => setConfig({ ...config, vdj_database_path: e.currentTarget.value })}
            />
            <span className="hint">{t("vdj.net.dbPathHint")}</span>
          </div>
          <div className="vdj-field-row">
            <div className="vdj-field">
              <label htmlFor="vdj-mapfrom">{t("vdj.net.mapFrom")}</label>
              <input
                id="vdj-mapfrom"
                type="text"
                placeholder="C:\\Musica"
                value={config.music_path_map_from}
                onChange={(e) =>
                  setConfig({ ...config, music_path_map_from: e.currentTarget.value })
                }
              />
            </div>
            <div className="vdj-field">
              <label htmlFor="vdj-mapto">{t("vdj.net.mapTo")}</label>
              <input
                id="vdj-mapto"
                type="text"
                placeholder="/Volumes/Musica"
                value={config.music_path_map_to}
                onChange={(e) => setConfig({ ...config, music_path_map_to: e.currentTarget.value })}
              />
            </div>
          </div>
          <span className="hint">{t("vdj.net.mapHint")}</span>
        </details>

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

      <Os2lSection />
      <FollowerCard />
    </main>
  );
}

/// Kinds the OS2L mapping editor exposes. Each maps to/from a
/// `ButtonAction`; id-based kinds get a second select with the show's
/// items of that type.
type Os2lKind = ActionKind;

/// OS2L server: config + live status + mapping table. Lives on the VDJ
/// page because both talk to the same app, but it's an independent
/// feature with its own apply button (restarts the TCP server).
function Os2lSection() {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const [config, setConfig] = useState<Os2lConfig | null>(null);
  const [status, setStatus] = useState<Os2lStatus | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [testName, setTestName] = useState("");

  // Config lives inside the show file; seed the editable draft once
  // the show has loaded (and only once, so edits aren't clobbered).
  useEffect(() => {
    if (show && config === null) {
      setConfig(deepClone(show.os2l));
    }
  }, [show, config]);

  useEffect(() => {
    invoke<Os2lStatus>("os2l_status")
      .then(setStatus)
      .catch(() => {});
    let unlisten: (() => void) | null = null;
    listen<Os2lStatus>(OS2L_STATUS_EVENT, (ev) => setStatus(ev.payload)).then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const apply = async () => {
    if (!config) return;
    setSaving(true);
    setError(null);
    try {
      setStatus(await invoke<Os2lStatus>("os2l_apply_config", { config }));
    } catch (e) {
      setError(t("vdj.os2l.errSave", { err: stringify(e) }));
    } finally {
      setSaving(false);
    }
  };

  const fireTest = async (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    try {
      setStatus(await invoke<Os2lStatus>("os2l_test_button", { name: trimmed }));
    } catch (e) {
      setError(stringify(e));
    }
  };

  if (!config) return null;

  const itemsFor = (kind: Os2lKind) => itemsForKind(show, kind);

  const updateMapping = (i: number, name: string, action: ButtonAction, mode?: ButtonMode) => {
    const mappings = config.mappings.map((m, j) =>
      j === i ? { name, action, mode: mode ?? m.mode ?? "toggle" } : m,
    );
    setConfig({ ...config, mappings });
  };

  const connected = status?.running && status.peer != null;

  return (
    <>
      <header className="page-head vdj-os2l-head">
        <h2>{t("vdj.os2l.title")}</h2>
      </header>
      <p className="hint vdj-intro">{t("vdj.os2l.intro")}</p>

      <section className="card vdj-card">
        <h3>{t("vdj.os2l.section.server")}</h3>

        <label className="checkbox-row vdj-toggle">
          <input
            type="checkbox"
            checked={config.enabled}
            onChange={(e) => setConfig({ ...config, enabled: e.currentTarget.checked })}
          />
          <span>
            <strong>{t("vdj.os2l.enabled")}</strong>
            <span className="hint vdj-toggle-hint">{t("vdj.os2l.enabledHint")}</span>
          </span>
        </label>

        <div className="vdj-field">
          <label htmlFor="os2l-port">{t("vdj.os2l.port")}</label>
          <input
            id="os2l-port"
            type="number"
            min={1}
            max={65535}
            value={config.port}
            onChange={(e) =>
              setConfig({ ...config, port: clampInt(Number(e.currentTarget.value), 1, 65535) })
            }
          />
          <span className="hint">{t("vdj.os2l.portHint")}</span>
        </div>

        <label className="checkbox-row vdj-toggle">
          <input
            type="checkbox"
            checked={config.quantize_to_beat}
            onChange={(e) => setConfig({ ...config, quantize_to_beat: e.currentTarget.checked })}
          />
          <span>
            <strong>{t("vdj.os2l.quantize")}</strong>
            <span className="hint vdj-toggle-hint">{t("vdj.os2l.quantizeHint")}</span>
          </span>
        </label>

        <div className="vdj-field vdj-field-indent">
          <label htmlFor="os2l-latency">{t("vdj.os2l.latencyOffset")}</label>
          <input
            id="os2l-latency"
            type="number"
            min={0}
            max={500}
            step={5}
            value={config.latency_offset_ms}
            onChange={(e) =>
              setConfig({
                ...config,
                latency_offset_ms: clampInt(Number(e.currentTarget.value), 0, 500),
              })
            }
            disabled={!config.quantize_to_beat}
          />
          <span className="hint">{t("vdj.os2l.latencyOffsetHint")}</span>
        </div>

        <div className="vdj-status-row">
          <span className={`vdj-status-pill${status?.running ? " running" : ""}`}>
            <span className="vdj-status-dot" aria-hidden="true" />
            {connected
              ? t("vdj.os2l.status.connected", { peer: status?.peer ?? "" })
              : status?.running
                ? t("vdj.os2l.status.listening")
                : t("vdj.os2l.status.stopped")}
          </span>
        </div>

        <div className="row vdj-status-line">
          <strong>{t("vdj.os2l.status.lastBeat")}</strong>
          <span className="vdj-bpm-value">
            {status?.last_beat_bpm != null ? status.last_beat_bpm.toFixed(2) : "—"}
          </span>
          <span>
            {status?.last_beat_at_secs != null
              ? t("vdj.status.secondsAgo", {
                  secs: String(Math.max(0, secondsSince(Number(status.last_beat_at_secs)))),
                })
              : ""}
          </span>
        </div>

        {status?.last_error ? (
          <div className="vdj-status-error">
            <strong>{t("vdj.status.error")}</strong>
            <code>{status.last_error}</code>
          </div>
        ) : null}
      </section>

      <section className="card vdj-card">
        <h3>{t("vdj.os2l.section.buttons")}</h3>
        <p className="hint">{t("vdj.os2l.conventionsHint")}</p>

        <div className="os2l-recent">
          <div className="os2l-recent-head">
            <strong>{t("vdj.os2l.recent")}</strong>
            {status != null && status.recent_buttons.length > 0 ? (
              <button
                type="button"
                className="os2l-recent-clear"
                onClick={() => {
                  invoke<Os2lStatus>("os2l_clear_recent")
                    .then(setStatus)
                    .catch(() => {});
                }}
              >
                {t("vdj.os2l.clearRecent")}
              </button>
            ) : null}
          </div>
          {status == null || status.recent_buttons.length === 0 ? (
            <p className="hint">{t("vdj.os2l.recentEmpty")}</p>
          ) : (
            <ul className="os2l-recent-list">
              {status.recent_buttons.map((b, i) => (
                <li key={`${b.name}-${String(b.at_secs)}-${i}`} className="os2l-recent-item">
                  <code>{b.name}</code>
                  <span className={`os2l-resolved${b.resolved ? "" : " miss"}`}>
                    {b.resolved ? `→ ${b.resolved}` : t("vdj.os2l.unresolved")}
                  </span>
                  {b.beat_offset_ms != null ? (
                    <span className="os2l-beat-offset" title={t("vdj.os2l.beatOffsetTitle")}>
                      +{b.beat_offset_ms} ms
                    </span>
                  ) : null}
                  {b.quantized ? (
                    <span className="os2l-quantized-chip" title={t("vdj.os2l.quantizedTitle")}>
                      ⏱
                    </span>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="os2l-test-row">
          <input
            placeholder={t("vdj.os2l.testPlaceholder")}
            value={testName}
            onChange={(e) => setTestName(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") fireTest(testName);
            }}
          />
          <button type="button" onClick={() => fireTest(testName)}>
            {t("vdj.os2l.testFire")}
          </button>
        </div>
      </section>

      <section className="card vdj-card">
        <h3>{t("vdj.os2l.section.mappings")}</h3>
        <p className="hint">{t("vdj.os2l.mappingsHint")}</p>

        {config.mappings.length > 0 ? (
          <div className="os2l-mappings">
            <div className="os2l-map-head" aria-hidden="true">
              <span>{t("vdj.os2l.mapName")}</span>
              <span>{t("vdj.os2l.mapAction")}</span>
              <span>{t("vdj.os2l.mapTarget")}</span>
              <span />
            </div>
            {config.mappings.map((m, i) => {
              const kind = kindOfAction(m.action) ?? "scene";
              const items = itemsFor(kind);
              const id = actionId(m.action);
              return (
                // biome-ignore lint/suspicious/noArrayIndexKey: rows are positional and edited in place
                <div key={i} className="os2l-map-row">
                  <input
                    value={m.name}
                    placeholder="drop-1"
                    onChange={(e) => updateMapping(i, e.currentTarget.value, m.action)}
                  />
                  <select
                    value={kind}
                    onChange={(e) => {
                      const k = e.currentTarget.value as Os2lKind;
                      updateMapping(i, m.name, buildAction(k, itemsFor(k)[0]?.id ?? null));
                    }}
                  >
                    {OS2L_KINDS.map((k) => (
                      <option key={k} value={k}>
                        {t(`vdj.os2l.kind.${k}`)}
                      </option>
                    ))}
                  </select>
                  <select
                    value={m.mode ?? "toggle"}
                    title={t("vdj.os2l.modeHint")}
                    onChange={(e) =>
                      updateMapping(i, m.name, m.action, e.currentTarget.value as ButtonMode)
                    }
                  >
                    {(["toggle", "flash", "swop"] as ButtonMode[]).map((md) => (
                      <option key={md} value={md}>
                        {t(`vdj.os2l.mode.${md}`)}
                      </option>
                    ))}
                  </select>
                  {items.length > 0 ? (
                    <select
                      value={id ?? ""}
                      onChange={(e) =>
                        updateMapping(i, m.name, buildAction(kind, e.currentTarget.value))
                      }
                    >
                      {items.map((it) => (
                        <option key={it.id} value={it.id}>
                          {it.name}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <span className="os2l-map-notarget">—</span>
                  )}
                  <button
                    type="button"
                    className="os2l-map-remove"
                    title={t("vdj.os2l.removeMapping")}
                    onClick={() =>
                      setConfig({
                        ...config,
                        mappings: config.mappings.filter((_, j) => j !== i),
                      })
                    }
                  >
                    ✕
                  </button>
                </div>
              );
            })}
          </div>
        ) : null}

        <button
          type="button"
          className="os2l-map-add"
          onClick={() =>
            setConfig({
              ...config,
              mappings: [
                ...config.mappings,
                {
                  name: "",
                  action: buildAction("scene", itemsFor("scene")[0]?.id ?? null),
                  mode: "toggle",
                },
              ],
            })
          }
        >
          {t("vdj.os2l.addMapping")}
        </button>
      </section>

      {error ? (
        <output className="error vdj-error" aria-live="polite">
          {error}
        </output>
      ) : null}

      <div className="vdj-save-bar">
        <button type="button" className="primary vdj-save-btn" onClick={apply} disabled={saving}>
          {t("vdj.os2l.apply")}
        </button>
      </div>
    </>
  );
}

function clampInt(v: number, lo: number, hi: number): number {
  if (!Number.isFinite(v)) return lo;
  return Math.max(lo, Math.min(hi, Math.round(v)));
}

/// Live-firing follower control. Cue AUTHORING lives in the Cues tab
/// (own transport, zoomable waveform); this card only arms/disarms the
/// follower that fires cues from VirtualDJ's playhead at show time.
function FollowerCard() {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const [status, setStatus] = useState<CueFollowerStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<CueFollowerStatus>("cue_follower_status")
      .then(setStatus)
      .catch(() => {});
    let unlisten: (() => void) | null = null;
    listen<CueFollowerStatus>("cues:status", (ev) => setStatus(ev.payload)).then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const enabled = show?.vdj.cue_follower_enabled ?? false;
  const toggle = async (on: boolean) => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await invoke<CueFollowerStatus>("cue_follower_set_enabled", { enabled: on }));
    } catch (e) {
      setError(stringify(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <header className="page-head vdj-os2l-head">
        <h2>{t("vdj.cues.title")}</h2>
      </header>
      <section className="card vdj-card">
        <label className="checkbox-row vdj-toggle">
          <input
            type="checkbox"
            checked={enabled}
            disabled={busy}
            onChange={(e) => toggle(e.currentTarget.checked)}
          />
          <span>
            <strong>{t("vdj.cues.enabled")}</strong>
          </span>
        </label>
        <p className="hint">
          {status?.playing && status.title
            ? t("vdj.cues.tracking", { title: status.title })
            : t("vdj.cues.waiting")}
        </p>
        <p className="hint">{t("vdj.cues.editHint")}</p>
        {error ? (
          <output className="error vdj-error" aria-live="polite">
            {error}
          </output>
        ) : null}
      </section>
    </>
  );
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
