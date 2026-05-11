import type { StreamDeckStatus } from "@bindings/StreamDeckStatus";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useT } from "../i18n";
import { useShowStore } from "../stores/show";

type Device = { serial: string; kind: string; key_count: number };

export function StreamDeckConfigView() {
  const t = useT();
  const listStreamDeckDevices = useShowStore((s) => s.listStreamDeckDevices);
  const connectStreamDeckDevice = useShowStore((s) => s.connectStreamDeckDevice);
  const disconnectStreamDeck = useShowStore((s) => s.disconnectStreamDeck);

  const [devices, setDevices] = useState<Device[]>([]);
  const [status, setStatus] = useState<StreamDeckStatus>({
    connected: null,
    kind: null,
    key_count: null,
  });
  const [error, setError] = useState<string | null>(null);

  const refreshDevices = async () => {
    try {
      setDevices(await listStreamDeckDevices());
    } catch (e) {
      setError(t("streamdeck.errListing", { err: stringifyError(e) }));
    }
  };

  const refreshStatus = async () => {
    try {
      setStatus(await invoke<StreamDeckStatus>("get_streamdeck_status"));
    } catch {
      // best effort
    }
  };

  useEffect(() => {
    listStreamDeckDevices()
      .then(setDevices)
      .catch((e) => setError(t("streamdeck.errListing", { err: stringifyError(e) })));
    invoke<StreamDeckStatus>("get_streamdeck_status")
      .then(setStatus)
      .catch(() => {});
  }, [listStreamDeckDevices, t]);

  const onConnect = async (serial: string) => {
    setError(null);
    try {
      await connectStreamDeckDevice(serial);
      await refreshStatus();
    } catch (e) {
      setError(t("streamdeck.errConnect", { name: serial, err: stringifyError(e) }));
    }
  };

  const onDisconnect = async () => {
    setError(null);
    try {
      await disconnectStreamDeck();
      await refreshStatus();
    } catch (e) {
      setError(t("streamdeck.errDisconnect", { err: stringifyError(e) }));
    }
  };

  return (
    <main className="page midi-config-view">
      <header className="page-head">
        <h2>{t("streamdeck.title")}</h2>
        <div className="actions">
          <button type="button" onClick={refreshDevices}>
            {t("streamdeck.refresh")}
          </button>
        </div>
      </header>

      <p className="hint" data-doc="streamdeck-intro">{t("streamdeck.intro")}</p>

      {error ? (
        <output className="lib-error" aria-live="polite">
          {error}
        </output>
      ) : null}

      <section className="config-section" data-doc="streamdeck-devices">
        <h3>{t("streamdeck.detectedDevices", { count: devices.length })}</h3>
        {devices.length === 0 ? (
          <p className="empty">{t("streamdeck.empty")}</p>
        ) : (
          <ul className="midi-device-list">
            {devices.map((d) => {
              const isConnected = status.connected === d.serial;
              return (
                <li key={d.serial} className={isConnected ? "connected" : ""}>
                  <span className="midi-device-name">{d.kind}</span>
                  <span className="midi-device-meta">
                    {t("streamdeck.deviceMeta", {
                      keys: String(d.key_count),
                      serial: d.serial,
                    })}
                  </span>
                  {isConnected ? (
                    <button type="button" onClick={onDisconnect}>
                      {t("streamdeck.disconnect")}
                    </button>
                  ) : (
                    <button type="button" onClick={() => onConnect(d.serial)}>
                      {t("streamdeck.connect")}
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {status.connected ? (
        <section className="config-section" data-doc="streamdeck-status">
          <h3>{t("streamdeck.statusSection")}</h3>
          <p>
            {t("streamdeck.connectedToFmt", {
              kind: status.kind ?? "?",
              keys: String(status.key_count ?? 0),
            })}
          </p>
        </section>
      ) : null}
    </main>
  );
}

function stringifyError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return JSON.stringify(e);
}
