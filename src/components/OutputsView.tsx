import type { D2xxDeviceInfo } from "@bindings/D2xxDeviceInfo";
import type { OutputBindingConfig } from "@bindings/OutputBindingConfig";
import type { OutputsConfig } from "@bindings/OutputsConfig";
import type { SerialPortInfo } from "@bindings/SerialPortInfo";
import { useState } from "react";
import { useShowStore } from "../stores/show";

/// Best-effort OS sniff for UI hints. We can't query the Tauri backend
/// here without an extra IPC, but the renderer's userAgent is good
/// enough to decide whether to show the "run Zadig" tip.
function isWindows(): boolean {
  if (typeof navigator === "undefined") return false;
  return /Windows/i.test(navigator.userAgent);
}

type Kind = OutputBindingConfig["kind"];

const KIND_LABELS: Record<Kind, string> = {
  mock: "Mock (logging)",
  art_net: "Art-Net",
  sacn: "sACN (E1.31)",
  enttec_usb: "Enttec DMX USB Pro",
  open_dmx: "Open DMX (OS serial — fallback)",
  open_dmx_ftdi: "Open DMX / ElectroTAS (FTDI directo, recomendado)",
};

function newId(kind: Kind, existing: OutputBindingConfig[]): string {
  let n = 1;
  while (existing.some((b) => b.id === `${kind}-${n}`)) n += 1;
  return `${kind}-${n}`;
}

function defaultBinding(kind: Kind, existing: OutputBindingConfig[]): OutputBindingConfig {
  const id = newId(kind, existing);
  switch (kind) {
    case "mock":
      return { kind, id, universes: [0] };
    case "art_net":
      return { kind, id, target: "127.0.0.1:6454", universes: [0] };
    case "sacn":
      return { kind, id, source_name: "DMX Control", priority: 100, universes: [0] };
    case "enttec_usb":
      return { kind, id, port: "", universes: [0] };
    case "open_dmx":
      return { kind, id, port: "", universes: [0] };
    case "open_dmx_ftdi":
      return { kind, id, serial: "", universes: [0], dtr_high: false, rts_high: false };
  }
}

function parseUniverses(input: string): number[] {
  return input
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
    .map((s) => Number.parseInt(s, 10))
    .filter((n) => Number.isFinite(n) && n >= 0 && n <= 0x7fff);
}

function universesToString(us: number[]): string {
  return us.join(", ");
}

function BindingRow({
  binding,
  onChange,
  onRemove,
  serialPorts,
  ftdiDevices,
}: {
  binding: OutputBindingConfig;
  onChange: (next: OutputBindingConfig) => void;
  onRemove: () => void;
  serialPorts: SerialPortInfo[];
  ftdiDevices: D2xxDeviceInfo[];
}) {
  const universes = universesToString(binding.universes);
  const setUniverses = (v: string) => onChange({ ...binding, universes: parseUniverses(v) });
  return (
    <div className="binding-row">
      <div className="binding-head">
        <span className="binding-kind">{KIND_LABELS[binding.kind]}</span>
        <input
          className="binding-id"
          value={binding.id}
          onChange={(e) => onChange({ ...binding, id: e.currentTarget.value })}
        />
        <button type="button" className="danger" onClick={onRemove}>
          Remove
        </button>
      </div>
      <div className="binding-fields">
        {binding.kind === "art_net" ? (
          <label>
            Target IP:Port
            <input
              value={binding.target}
              onChange={(e) => onChange({ ...binding, target: e.currentTarget.value })}
              placeholder="127.0.0.1:6454"
            />
          </label>
        ) : null}
        {binding.kind === "sacn" ? (
          <>
            <label>
              Source name
              <input
                value={binding.source_name}
                onChange={(e) => onChange({ ...binding, source_name: e.currentTarget.value })}
              />
            </label>
            <label>
              Priority
              <input
                type="number"
                min={1}
                max={200}
                value={binding.priority}
                onChange={(e) => onChange({ ...binding, priority: Number(e.currentTarget.value) })}
              />
            </label>
          </>
        ) : null}
        {binding.kind === "enttec_usb" || binding.kind === "open_dmx" ? (
          <label>
            Serial port
            <select
              value={binding.port}
              onChange={(e) => onChange({ ...binding, port: e.currentTarget.value })}
            >
              <option value="">— select —</option>
              {serialPorts.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name}
                  {p.looks_like_enttec ? " (FTDI)" : ""}
                  {p.product ? ` — ${p.product}` : ""}
                </option>
              ))}
              {binding.port && !serialPorts.some((p) => p.name === binding.port) ? (
                <option value={binding.port}>{binding.port} (offline)</option>
              ) : null}
            </select>
          </label>
        ) : null}
        {binding.kind === "open_dmx_ftdi" ? (
          <>
            <label>
              FTDI device (by serial)
              <select
                value={binding.serial}
                onChange={(e) => onChange({ ...binding, serial: e.currentTarget.value })}
              >
                <option value="">— select —</option>
                {ftdiDevices.map((d) => (
                  <option key={d.serial_number} value={d.serial_number}>
                    {d.serial_number}
                    {d.description ? ` — ${d.description}` : ""}
                    {d.port_open ? " (in use)" : ""}
                  </option>
                ))}
                {binding.serial && !ftdiDevices.some((d) => d.serial_number === binding.serial) ? (
                  <option value={binding.serial}>{binding.serial} (offline)</option>
                ) : null}
              </select>
            </label>
            {ftdiDevices.length === 0 && isWindows() ? (
              <p className="hint" style={{ fontSize: 11 }}>
                En Windows, libusb solo ve dispositivos FTDI bindeados a WinUSB. Si tu DMX FTDI no
                aparece, corré <strong>Zadig</strong> una vez y elegí el driver WinUSB para esa
                interface (eso saca el COM port). Si preferís mantener el COM port, cambiá esta
                salida a "Serial" y elegí el FTDI desde la lista de puertos.
              </p>
            ) : null}
            <label>
              <input
                type="checkbox"
                checked={binding.dtr_high}
                onChange={(e) => onChange({ ...binding, dtr_high: e.currentTarget.checked })}
              />
              DTR high
            </label>
            <label>
              <input
                type="checkbox"
                checked={binding.rts_high}
                onChange={(e) => onChange({ ...binding, rts_high: e.currentTarget.checked })}
              />
              RTS high
            </label>
          </>
        ) : null}
        <label>
          Universes (comma-separated)
          <input value={universes} onChange={(e) => setUniverses(e.currentTarget.value)} />
        </label>
      </div>
    </div>
  );
}

export function OutputsView() {
  const show = useShowStore((s) => s.show);
  const serialPorts = useShowStore((s) => s.serialPorts);
  const ftdiDevices = useShowStore((s) => s.ftdiDevices);
  const setOutputs = useShowStore((s) => s.setOutputs);
  const refreshSerialPorts = useShowStore((s) => s.refreshSerialPorts);
  const refreshFtdiDevices = useShowStore((s) => s.refreshFtdiDevices);

  const [draft, setDraft] = useState<OutputsConfig | null>(null);

  const config = draft ?? show?.outputs ?? null;
  const dirty = !!draft;

  if (!config) {
    return <main className="page">Cargando…</main>;
  }

  const update = (next: OutputsConfig) => setDraft(next);

  const addBinding = (kind: Kind) => {
    update({
      ...config,
      bindings: [...config.bindings, defaultBinding(kind, config.bindings)],
    });
  };

  const apply = async () => {
    if (!draft) return;
    await setOutputs(draft);
    setDraft(null);
  };

  const revert = () => setDraft(null);

  return (
    <main className="page outputs-view">
      <header className="page-head">
        <h2>Outputs</h2>
        <div className="actions">
          <button
            type="button"
            onClick={() => {
              refreshSerialPorts();
              refreshFtdiDevices();
            }}
          >
            Re-scan ports
          </button>
          <button type="button" onClick={() => addBinding("mock")}>
            + Mock
          </button>
          <button type="button" onClick={() => addBinding("art_net")}>
            + Art-Net
          </button>
          <button type="button" onClick={() => addBinding("sacn")}>
            + sACN
          </button>
          <button type="button" onClick={() => addBinding("enttec_usb")}>
            + Enttec USB
          </button>
          <button type="button" onClick={() => addBinding("open_dmx_ftdi")}>
            + Open DMX (FTDI directo)
          </button>
          <button type="button" onClick={() => addBinding("open_dmx")}>
            + Open DMX (OS serial)
          </button>
        </div>
      </header>

      <section className="bindings">
        {config.bindings.length === 0 ? (
          <p className="empty">Sin outputs. Agregá uno arriba.</p>
        ) : (
          config.bindings.map((b, i) => (
            <BindingRow
              key={b.id}
              binding={b}
              serialPorts={serialPorts}
              ftdiDevices={ftdiDevices}
              onChange={(next) =>
                update({ ...config, bindings: config.bindings.map((x, j) => (j === i ? next : x)) })
              }
              onRemove={() =>
                update({ ...config, bindings: config.bindings.filter((_, j) => j !== i) })
              }
            />
          ))
        )}
      </section>

      <footer className="page-foot">
        <span className={dirty ? "dirty" : ""}>{dirty ? "Cambios sin aplicar" : "OK"}</span>
        <div className="actions">
          <button type="button" disabled={!dirty} onClick={revert}>
            Descartar
          </button>
          <button type="button" disabled={!dirty} onClick={apply}>
            Aplicar
          </button>
        </div>
      </footer>
    </main>
  );
}
