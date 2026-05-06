import type { SyncProbe } from "@bindings/SyncProbe";
import type { SyncResult } from "@bindings/SyncResult";
import type { SyncSettings } from "@bindings/SyncSettings";
import type { SyncStatus } from "@bindings/SyncStatus";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { useT } from "../i18n";

/// Cross-machine config sync via private GitHub Gist. PAT lives in the
/// OS keychain; settings (gist id, labels) in
/// `~/Library/Application Support/dmx-control/sync.json` (or the
/// equivalent on Windows). Outputs are excluded from push by default
/// because hardware differs between Mac and Windows.
export function SyncView() {
  const t = useT();
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [probe, setProbe] = useState<SyncProbe | null>(null);
  const [tokenInput, setTokenInput] = useState("");
  const [gistIdInput, setGistIdInput] = useState("");
  const [machineLabel, setMachineLabel] = useState("");
  const [includeOutputs, setIncludeOutputs] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await invoke<SyncStatus>("sync_status");
      setStatus(s);
      setGistIdInput(s.settings.gist_id ?? "");
      setMachineLabel(s.settings.machine_label ?? "");
      setIncludeOutputs(s.settings.include_outputs ?? false);
    } catch (e) {
      setError(stringify(e));
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const saveToken = async () => {
    if (!tokenInput.trim()) return;
    setBusy("token");
    setError(null);
    try {
      await invoke("sync_set_token", { token: tokenInput.trim() });
      setTokenInput("");
      const login = await invoke<string>("sync_whoami");
      setInfo(t("sync.token.connectedAs", { user: login }));
      await refresh();
    } catch (e) {
      setError(stringify(e));
    } finally {
      setBusy(null);
    }
  };

  const clearToken = async () => {
    if (!confirm(t("sync.token.deleteConfirm"))) return;
    try {
      await invoke("sync_clear_token");
      await refresh();
    } catch (e) {
      setError(stringify(e));
    }
  };

  const saveSettings = async () => {
    setError(null);
    try {
      const next: SyncSettings = {
        ...(status?.settings ?? defaultSettings()),
        gist_id: gistIdInput.trim(),
        machine_label: machineLabel.trim(),
        include_outputs: includeOutputs,
      };
      const s = await invoke<SyncStatus>("sync_save_settings", { settings: next });
      setStatus(s);
    } catch (e) {
      setError(stringify(e));
    }
  };

  const onProbe = async () => {
    setBusy("probe");
    setError(null);
    try {
      const p = await invoke<SyncProbe>("sync_probe");
      setProbe(p);
    } catch (e) {
      setError(stringify(e));
    } finally {
      setBusy(null);
    }
  };

  const onPush = async (force: boolean) => {
    setBusy("push");
    setError(null);
    setInfo(null);
    try {
      const r = await invoke<SyncResult>("sync_push", { force });
      setInfo(
        t("sync.toast.pushOk", {
          id: r.gist_id.slice(0, 7),
          time: formatTime(r.remote_updated_at) ?? "—",
        }),
      );
      await refresh();
      setProbe(null);
    } catch (e) {
      setError(stringify(e));
    } finally {
      setBusy(null);
    }
  };

  const onPull = async () => {
    if (!confirm(t("sync.action.pullConfirm"))) return;
    setBusy("pull");
    setError(null);
    setInfo(null);
    try {
      const r = await invoke<SyncResult>("sync_pull");
      setInfo(
        t("sync.toast.pullOk", {
          machine: r.remote_machine ?? "?",
          time: formatTime(r.remote_updated_at) ?? "—",
        }),
      );
      await refresh();
      setProbe(null);
    } catch (e) {
      setError(stringify(e));
    } finally {
      setBusy(null);
    }
  };

  if (!status) {
    return (
      <main className="page sync-view">
        <header className="page-head">
          <h2>{t("sync.title")}</h2>
        </header>
        <p className="empty">{t("common.loading")}</p>
      </main>
    );
  }

  const remoteAhead = probe?.remote_ahead === true;

  return (
    <main className="page sync-view">
      <header className="page-head">
        <h2>{t("sync.title")}</h2>
        <p className="hint">{t("sync.intro")}</p>
      </header>

      {error && <div className="error">{error}</div>}
      {info && <div className="info-banner">{info}</div>}

      <section className="card">
        <h3>{t("sync.section.token")}</h3>
        {status.has_token ? (
          <div className="row">
            <span className="ok">{t("sync.token.saved")}</span>
            <button type="button" className="ghost" onClick={clearToken}>
              {t("sync.token.delete")}
            </button>
          </div>
        ) : (
          <div className="row">
            <input
              type="password"
              value={tokenInput}
              onChange={(e) => setTokenInput(e.target.value)}
              placeholder={t("sync.token.placeholder")}
              style={{ flex: 1, minWidth: 240 }}
            />
            <button
              type="button"
              className="primary"
              disabled={!tokenInput.trim() || busy === "token"}
              onClick={saveToken}
            >
              {busy === "token" ? t("sync.token.saving") : t("sync.token.save")}
            </button>
          </div>
        )}
        <p className="hint">{t("sync.token.help")}</p>
      </section>

      <section className="card">
        <h3>{t("sync.section.settings")}</h3>
        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          {t("sync.machineLabel")}
          <input
            value={machineLabel}
            onChange={(e) => setMachineLabel(e.target.value)}
            placeholder={t("sync.machineLabel.placeholder")}
          />
        </label>
        <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          {t("sync.gistId")}
          <input
            value={gistIdInput}
            onChange={(e) => setGistIdInput(e.target.value)}
            placeholder={t("sync.gistId.placeholder")}
          />
        </label>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={includeOutputs}
            onChange={(e) => setIncludeOutputs(e.target.checked)}
          />
          {t("sync.includeOutputs")}
        </label>
        <div className="row">
          <button type="button" onClick={saveSettings}>
            {t("sync.saveSettings")}
          </button>
        </div>
      </section>

      <section className="card">
        <h3>{t("sync.section.status")}</h3>
        <div className="row">
          <strong>{t("sync.status.lastPush")}</strong>
          <span>{formatTime(status.settings.last_pushed_at) ?? t("sync.status.never")}</span>
        </div>
        <div className="row">
          <strong>{t("sync.status.lastPull")}</strong>
          <span>{formatTime(status.settings.last_pulled_at) ?? t("sync.status.never")}</span>
        </div>
        <div className="row">
          <strong>{t("sync.status.lastRemote")}</strong>
          <span>
            {formatTime(status.settings.last_remote_updated) ?? "—"}
            {status.settings.last_remote_machine
              ? ` · ${t("sync.status.from", { machine: status.settings.last_remote_machine })}`
              : ""}
          </span>
        </div>
        <div className="row">
          <button type="button" onClick={onProbe} disabled={!status.has_token || busy === "probe"}>
            {busy === "probe" ? t("sync.probing") : t("sync.probe")}
          </button>
          {probe ? (
            <span className={remoteAhead ? "warn" : "ok"}>
              {probe.remote_updated_at == null
                ? t("sync.probe.empty")
                : remoteAhead
                  ? t("sync.probe.ahead", {
                      time: formatTime(probe.remote_updated_at) ?? "—",
                      machine: probe.remote_machine ?? "?",
                    })
                  : t("sync.probe.upToDate", {
                      time: formatTime(probe.remote_updated_at) ?? "—",
                      machine: probe.remote_machine ?? "?",
                    })}
            </span>
          ) : null}
        </div>
      </section>

      <section className="card">
        <h3>{t("sync.section.actions")}</h3>
        <div className="row">
          <button
            type="button"
            className="primary"
            onClick={() => onPush(false)}
            disabled={!status.has_token || busy === "push" || !machineLabel.trim()}
          >
            {busy === "push" ? t("sync.action.pushing") : t("sync.action.push")}
          </button>
          <button
            type="button"
            onClick={() => onPush(true)}
            disabled={!status.has_token || busy === "push"}
            title={t("sync.action.forcePushTitle")}
          >
            {t("sync.action.forcePush")}
          </button>
          <button
            type="button"
            onClick={onPull}
            disabled={!status.has_token || !status.settings.gist_id || busy === "pull"}
          >
            {busy === "pull" ? t("sync.action.pulling") : t("sync.action.pull")}
          </button>
        </div>
        <p className="hint">{t("sync.backupHint")}</p>
      </section>
    </main>
  );
}

function defaultSettings(): SyncSettings {
  return {
    gist_id: "",
    machine_label: "",
    include_outputs: false,
    last_remote_updated: null,
    last_pushed_at: null,
    last_pulled_at: null,
    last_remote_machine: null,
  };
}

function formatTime(iso: string | null | undefined): string | null {
  if (!iso) return null;
  try {
    const d = new Date(iso);
    return d.toLocaleString();
  } catch {
    return iso;
  }
}

function stringify(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}
