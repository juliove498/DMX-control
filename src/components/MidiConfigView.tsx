import type { MidiStatus } from "@bindings/MidiStatus";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useT } from "../i18n";
import { useShowStore } from "../stores/show";

type Device = { name: string; has_input: boolean; has_output: boolean };

/// A short colour palette test for the Launchpad MK2: lights up the
/// bottom row (notes 11-18) with the eight palette colours that map
/// roughly to the standard rainbow + white. Sends NoteOn with channel 1
/// (status byte 0x90) and palette velocity. After 1.5 s, sends NoteOn
/// velocity 0 to clear them.
const LAUNCHPAD_MK2_TEST_NOTES = [11, 12, 13, 14, 15, 16, 17, 18];
const LAUNCHPAD_MK2_TEST_VELOCITIES = [
  5, // red
  9, // orange
  13, // yellow
  17, // green
  37, // cyan
  45, // blue
  53, // magenta
  3, // white
];

export function MidiConfigView() {
  const t = useT();
  const listMidiDevices = useShowStore((s) => s.listMidiDevices);
  const connectMidiDevice = useShowStore((s) => s.connectMidiDevice);
  const disconnectMidi = useShowStore((s) => s.disconnectMidi);
  const sendMidiRaw = useShowStore((s) => s.sendMidiRaw);

  const [devices, setDevices] = useState<Device[]>([]);
  const [status, setStatus] = useState<MidiStatus>({
    connected: null,
    has_output: false,
    last_event: null,
  });
  const [error, setError] = useState<string | null>(null);

  const refreshDevices = async () => {
    try {
      setDevices(await listMidiDevices());
    } catch (e) {
      setError(t("midi.errListing", { err: stringifyError(e) }));
    }
  };

  const refreshStatus = async () => {
    try {
      const s = await invoke<MidiStatus>("get_midi_status");
      setStatus(s);
    } catch {
      // ignore — status query is best effort
    }
  };

  useEffect(() => {
    listMidiDevices()
      .then(setDevices)
      .catch((e) => setError(t("midi.errListing", { err: stringifyError(e) })));
    invoke<MidiStatus>("get_midi_status")
      .then(setStatus)
      .catch(() => {});
  }, [listMidiDevices, t]);

  const onConnect = async (name: string) => {
    setError(null);
    try {
      await connectMidiDevice(name);
      await refreshStatus();
    } catch (e) {
      setError(t("midi.errConnect", { name, err: stringifyError(e) }));
    }
  };

  const onDisconnect = async () => {
    setError(null);
    try {
      await disconnectMidi();
      await refreshStatus();
    } catch (e) {
      setError(t("midi.errDisconnect", { err: stringifyError(e) }));
    }
  };

  const onTestPads = async () => {
    setError(null);
    try {
      for (let i = 0; i < LAUNCHPAD_MK2_TEST_NOTES.length; i++) {
        await sendMidiRaw([0x90, LAUNCHPAD_MK2_TEST_NOTES[i], LAUNCHPAD_MK2_TEST_VELOCITIES[i]]);
      }
      window.setTimeout(() => {
        Promise.all(LAUNCHPAD_MK2_TEST_NOTES.map((n) => sendMidiRaw([0x90, n, 0]))).catch(() => {});
      }, 1500);
    } catch (e) {
      setError(t("midi.errTest", { err: stringifyError(e) }));
    }
  };

  const isLaunchpad = status.connected?.toLowerCase().includes("launchpad") ?? false;

  return (
    <main className="page midi-config-view">
      <header className="page-head">
        <h2>{t("midi.title")}</h2>
        <div className="actions">
          <button type="button" onClick={refreshDevices}>
            {t("midi.refresh")}
          </button>
        </div>
      </header>

      <p className="hint">{t("midi.refreshHint")}</p>

      {error ? (
        <output className="lib-error" aria-live="polite">
          {error}
        </output>
      ) : null}

      <section className="config-section">
        <h3>{t("midi.detectedDevices", { count: devices.length })}</h3>
        {devices.length === 0 ? (
          <p className="empty">{t("midi.empty")}</p>
        ) : (
          <ul className="midi-device-list">
            {devices.map((d) => {
              const isConnected = status.connected === d.name;
              const ioLabel = [
                d.has_input ? t("midi.in") : "",
                d.has_input && d.has_output ? " · " : "",
                d.has_output ? t("midi.out") : "",
              ].join("");
              return (
                <li key={d.name} className={isConnected ? "connected" : ""}>
                  <span className="midi-device-name">{d.name}</span>
                  <span className="midi-device-meta">{ioLabel}</span>
                  {isConnected ? (
                    <button type="button" onClick={onDisconnect}>
                      {t("midi.disconnect")}
                    </button>
                  ) : (
                    <button type="button" onClick={() => onConnect(d.name)}>
                      {t("midi.connect")}
                    </button>
                  )}
                </li>
              );
            })}
          </ul>
        )}
      </section>

      {status.connected ? (
        <section className="config-section">
          <h3>{t("midi.statusSection")}</h3>
          <p>
            {t("midi.connectedToFmt", {
              name: status.connected,
              io: status.has_output ? t("midi.bothIO") : t("midi.onlyIn"),
              surface: isLaunchpad ? ` · ${t("midi.surfaceActive")}` : "",
            })}
          </p>
          <div className="midi-controls">
            <button type="button" onClick={onTestPads} disabled={!status.has_output}>
              {t("midi.testPads")}
            </button>
          </div>
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
