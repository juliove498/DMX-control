import type { D2xxDeviceInfo } from "@bindings/D2xxDeviceInfo";
import type { OutputBindingConfig } from "@bindings/OutputBindingConfig";
import type { OutputsConfig } from "@bindings/OutputsConfig";
import type { SerialPortInfo } from "@bindings/SerialPortInfo";
import { useState } from "react";
import { useT } from "../i18n";
import type { Translation } from "../i18n/translations";
import { useShowStore } from "../stores/show";

/// Best-effort OS sniff for UI hints. We can't query the Tauri backend
/// here without an extra IPC, but the renderer's userAgent is good
/// enough to decide whether to show the "run Zadig" tip.
function isWindows(): boolean {
  if (typeof navigator === "undefined") return false;
  return /Windows/i.test(navigator.userAgent);
}

type Kind = OutputBindingConfig["kind"];

// Mapping kind → translation key. The actual labels resolve at render
// time so a language flip repaints the rows immediately.
const KIND_KEYS: Record<Kind, keyof Translation> = {
  mock: "outputs.kind.mock",
  art_net: "outputs.kind.artNet",
  sacn: "outputs.kind.sacn",
  enttec_usb: "outputs.kind.enttec",
  open_dmx: "outputs.kind.openDmx",
  open_dmx_ftdi: "outputs.kind.openDmxFtdi",
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
  const t = useT();
  const universes = universesToString(binding.universes);
  const setUniverses = (v: string) => onChange({ ...binding, universes: parseUniverses(v) });
  return (
    <div className="binding-row">
      <div className="binding-head">
        <span className="binding-kind">{t(KIND_KEYS[binding.kind])}</span>
        <input
          className="binding-id"
          value={binding.id}
          onChange={(e) => onChange({ ...binding, id: e.currentTarget.value })}
        />
        <button type="button" className="danger" onClick={onRemove}>
          {t("outputs.remove")}
        </button>
      </div>
      <div className="binding-fields">
        {binding.kind === "art_net" ? (
          <label>
            {t("outputs.field.target")}
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
              {t("outputs.field.sourceName")}
              <input
                value={binding.source_name}
                onChange={(e) => onChange({ ...binding, source_name: e.currentTarget.value })}
              />
            </label>
            <label>
              {t("outputs.field.priority")}
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
            {t("outputs.field.serialPort")}
            <select
              value={binding.port}
              onChange={(e) => onChange({ ...binding, port: e.currentTarget.value })}
            >
              <option value="">{t("outputs.placeholder.select")}</option>
              {serialPorts.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name}
                  {p.looks_like_enttec ? ` ${t("outputs.tag.ftdi")}` : ""}
                  {p.product ? ` — ${p.product}` : ""}
                </option>
              ))}
              {binding.port && !serialPorts.some((p) => p.name === binding.port) ? (
                <option value={binding.port}>
                  {binding.port} {t("outputs.tag.offline")}
                </option>
              ) : null}
            </select>
          </label>
        ) : null}
        {binding.kind === "open_dmx_ftdi" ? (
          <>
            <label>
              {t("outputs.field.ftdiDevice")}
              <select
                value={binding.serial}
                onChange={(e) => onChange({ ...binding, serial: e.currentTarget.value })}
              >
                <option value="">{t("outputs.placeholder.select")}</option>
                {ftdiDevices.map((d) => (
                  <option key={d.serial_number} value={d.serial_number}>
                    {d.serial_number}
                    {d.description ? ` — ${d.description}` : ""}
                    {d.port_open ? ` ${t("outputs.tag.inUse")}` : ""}
                  </option>
                ))}
                {binding.serial && !ftdiDevices.some((d) => d.serial_number === binding.serial) ? (
                  <option value={binding.serial}>
                    {binding.serial} {t("outputs.tag.offline")}
                  </option>
                ) : null}
              </select>
            </label>
            {ftdiDevices.length === 0 && isWindows() ? (
              <p className="hint" style={{ fontSize: 11 }}>
                {t("outputs.zadigHint")}
              </p>
            ) : null}
            <label>
              <input
                type="checkbox"
                checked={binding.dtr_high}
                onChange={(e) => onChange({ ...binding, dtr_high: e.currentTarget.checked })}
              />
              {t("outputs.field.dtrHigh")}
            </label>
            <label>
              <input
                type="checkbox"
                checked={binding.rts_high}
                onChange={(e) => onChange({ ...binding, rts_high: e.currentTarget.checked })}
              />
              {t("outputs.field.rtsHigh")}
            </label>
          </>
        ) : null}
        <label>
          {t("outputs.field.universes")}
          <input value={universes} onChange={(e) => setUniverses(e.currentTarget.value)} />
        </label>
      </div>
    </div>
  );
}

export function OutputsView() {
  const t = useT();
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
    return <main className="page">{t("common.loading")}</main>;
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
        <h2>{t("outputs.title")}</h2>
        <div className="actions">
          <button
            type="button"
            onClick={() => {
              refreshSerialPorts();
              refreshFtdiDevices();
            }}
          >
            {t("outputs.rescan")}
          </button>
          <button type="button" onClick={() => addBinding("mock")}>
            {t("outputs.add.mock")}
          </button>
          <button type="button" onClick={() => addBinding("art_net")}>
            {t("outputs.add.artNet")}
          </button>
          <button type="button" onClick={() => addBinding("sacn")}>
            {t("outputs.add.sacn")}
          </button>
          <button type="button" onClick={() => addBinding("enttec_usb")}>
            {t("outputs.add.enttec")}
          </button>
          <button type="button" onClick={() => addBinding("open_dmx_ftdi")}>
            {t("outputs.add.openDmxFtdi")}
          </button>
          <button type="button" onClick={() => addBinding("open_dmx")}>
            {t("outputs.add.openDmx")}
          </button>
        </div>
      </header>

      <section className="bindings">
        {config.bindings.length === 0 ? (
          <p className="empty">{t("outputs.empty")}</p>
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
        <span className={dirty ? "dirty" : ""}>{dirty ? t("outputs.dirty") : t("outputs.ok")}</span>
        <div className="actions">
          <button type="button" disabled={!dirty} onClick={revert}>
            {t("outputs.discard")}
          </button>
          <button type="button" disabled={!dirty} onClick={apply}>
            {t("outputs.apply")}
          </button>
        </div>
      </footer>
    </main>
  );
}
