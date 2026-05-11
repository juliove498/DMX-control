import type { BridgeStatus } from "@bindings/BridgeStatus";
import type { PairedDevice } from "@bindings/PairedDevice";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../i18n";
import type { Translation } from "../i18n/translations";

/// Remote-bridge config tab. Lets the operator turn the LAN bridge on/
/// off, surface the IP+port for manual entry on the phone, walk a paired
/// device through the PIN/QR flow, and revoke devices that should no
/// longer be allowed in.
///
/// We poll status once per second while open. The bridge already pushes
/// `clients_connected` updates as devices come and go, but pairing PIN
/// countdowns and the running flag still need a tick — and 1 Hz is
/// cheap.
export function RemoteBridgeView() {
  const t = useT();
  const [status, setStatus] = useState<BridgeStatus | null>(null);
  const [devices, setDevices] = useState<PairedDevice[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await invoke<BridgeStatus>("bridge_status");
      setStatus(s);
    } catch (e) {
      setError(stringifyError(e));
    }
  }, []);

  const refreshDevices = useCallback(async () => {
    try {
      const d = await invoke<PairedDevice[]>("bridge_list_devices");
      setDevices(d);
    } catch (e) {
      setError(stringifyError(e));
    }
  }, []);

  useEffect(() => {
    refreshStatus();
    refreshDevices();
    pollRef.current = setInterval(() => {
      refreshStatus();
    }, 1000);
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [refreshStatus, refreshDevices]);

  const start = async () => {
    setBusy(true);
    setError(null);
    try {
      const s = await invoke<BridgeStatus>("bridge_start");
      setStatus(s);
    } catch (e) {
      setError(stringifyError(e));
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    setBusy(true);
    setError(null);
    try {
      const s = await invoke<BridgeStatus>("bridge_stop");
      setStatus(s);
    } catch (e) {
      setError(stringifyError(e));
    } finally {
      setBusy(false);
    }
  };

  const beginPairing = async () => {
    try {
      const s = await invoke<BridgeStatus>("bridge_begin_pairing");
      setStatus(s);
    } catch (e) {
      setError(stringifyError(e));
    }
  };

  const cancelPairing = async () => {
    try {
      const s = await invoke<BridgeStatus>("bridge_cancel_pairing");
      setStatus(s);
    } catch (e) {
      setError(stringifyError(e));
    }
  };

  const revoke = async (id: string) => {
    if (!confirm(t("remote.devices.revokeConfirm"))) return;
    try {
      const s = await invoke<BridgeStatus>("bridge_revoke_device", { deviceId: id });
      setStatus(s);
      await refreshDevices();
    } catch (e) {
      setError(stringifyError(e));
    }
  };

  const running = status?.running ?? false;
  const hostIp = status?.host_ip ?? null;
  const port = status?.port ?? null;
  const pairing = status?.pairing ?? null;

  const connectionString = useMemo(() => {
    if (!hostIp || !port) return null;
    return `${hostIp}:${port}`;
  }, [hostIp, port]);

  return (
    <main className="page remote-bridge-view">
      <header className="page-head">
        <h2>{t("remote.title")}</h2>
        <p className="hint">{t("remote.intro")}</p>
      </header>

      {error && <div className="error">{error}</div>}

      <section className="card">
        <div className="row">
          <strong>{t("remote.status")}</strong>
          <span className={running ? "ok" : "muted"}>
            {running ? t("remote.statusRunning") : t("remote.statusStopped")}
          </span>
          {running ? (
            <button type="button" onClick={stop} disabled={busy} className="danger">
              {t("remote.stop")}
            </button>
          ) : (
            <button type="button" onClick={start} disabled={busy} className="primary">
              {t("remote.start")}
            </button>
          )}
        </div>
        {running && (
          <>
            <div className="row">
              <strong>{t("remote.connectManually")}</strong>
              <code>{connectionString ?? "…"}</code>
            </div>
            <div className="row">
              <strong>{t("remote.clientsConnected")}</strong>
              <span>{status?.clients_connected ?? 0}</span>
            </div>
          </>
        )}
      </section>

      {running && (
        <section className="card">
          <h3>{t("remote.pairing")}</h3>
          {pairing ? (
            <div className="pairing-active">
              <div className="pin-big" aria-label="PIN">
                {pairing.pin.split("").join(" ")}
              </div>
              <div
                className="qr"
                aria-label="QR code"
                style={{ width: 220, height: 220, lineHeight: 0 }}
                // SVG generated by qrcodegen on the backend — no JS QR
                // library shipped with the app. The SVG itself has no
                // intrinsic size (uses 100% / viewBox), so we pin it
                // here to keep the card from stretching to fill the
                // available width.
                // biome-ignore lint/security/noDangerouslySetInnerHtml: trusted SVG from our own backend
                dangerouslySetInnerHTML={{ __html: pairing.qr_svg }}
              />
              <p className="hint">
                {t("remote.pairing.expiresIn", { secs: String(pairing.remaining_secs) })}
              </p>
              <button type="button" onClick={cancelPairing}>
                {t("remote.pairing.cancel")}
              </button>
            </div>
          ) : (
            <button type="button" onClick={beginPairing} className="primary">
              {t("remote.pairing.start")}
            </button>
          )}
        </section>
      )}

      <section className="card">
        <h3>{t("remote.devices", { count: devices.length })}</h3>
        {devices.length === 0 ? (
          <p className="empty">{t("remote.devices.empty")}</p>
        ) : (
          <ul className="devices-list">
            {devices.map((d) => (
              <li key={d.id}>
                <div>
                  <strong>{d.name}</strong>
                  <small>
                    {" "}
                    {t("remote.devices.pairedAgo", {
                      ago: formatRelative(t, Number(d.created_at)),
                    })}
                    {d.last_seen != null
                      ? ` ${t("remote.devices.seenAgo", { ago: formatRelative(t, Number(d.last_seen)) })}`
                      : ""}
                  </small>
                </div>
                <button type="button" className="danger" onClick={() => revoke(d.id)}>
                  {t("remote.devices.revoke")}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}

function stringifyError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

function formatRelative(
  t: <K extends keyof Translation>(key: K, vars?: Record<string, string | number>) => string,
  unixSecs: number,
): string {
  const delta = Math.max(0, Math.floor(Date.now() / 1000) - unixSecs);
  if (delta < 60) return t("remote.time.secAgo", { n: delta });
  if (delta < 3600) return t("remote.time.minAgo", { n: Math.floor(delta / 60) });
  if (delta < 86400) return t("remote.time.hAgo", { n: Math.floor(delta / 3600) });
  return t("remote.time.dAgo", { n: Math.floor(delta / 86400) });
}
