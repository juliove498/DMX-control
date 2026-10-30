import type { Os2lStatus } from "@bindings/Os2lStatus";
import type { VdjConfig } from "@bindings/VdjConfig";
import type { VdjInstance } from "@bindings/VdjInstance";
import type { VdjScanEntry } from "@bindings/VdjScanEntry";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../i18n";

// Two DJs in the booth = two VirtualDJs on the router, both pushing
// beats and buttons. This card lists every instance the app can see
// (Bonjour, the OS2L connections we hold, a sweep of the subnet, the
// ones remembered in the show), lets the operator label them by IP
// ("Julio", "Rafa") and pick the one the rig follows — HTTP host and
// OS2L filter switch together.

const OS2L_STATUS_EVENT = "os2l:status";

interface Row {
  ip: string;
  hostname: string | null;
  label: string;
  http_port: number;
  known: boolean;
  http_ok: boolean | null;
  playing: boolean | null;
  os2l_connected: boolean;
  os2l_beating: boolean;
  bpm: number | null;
  local: boolean;
  selected: boolean;
}

function selectionKey(host: string): string {
  const h = host.trim();
  return h.toLowerCase() === "localhost" ? "127.0.0.1" : h;
}

function stringify(e: unknown): string {
  return e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e);
}

export function VdjInstancesCard({
  config,
  onConfig,
}: {
  config: VdjConfig;
  onConfig: (next: VdjConfig) => void;
}) {
  const t = useT();
  const [scan, setScan] = useState<VdjScanEntry[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [os2l, setOs2l] = useState<Os2lStatus | null>(null);
  const [labels, setLabels] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  // HTTP verdict per IP from the quick per-row probe (a full scan
  // replaces it). `playing` is null while the plugin does not answer.
  const [probes, setProbes] = useState<Record<string, { ok: boolean; playing: boolean | null }>>(
    {},
  );
  const probing = useRef(new Set<string>());

  useEffect(() => {
    let alive = true;
    invoke<Os2lStatus>("os2l_status")
      .then((s) => {
        if (alive) setOs2l(s);
      })
      .catch(() => {});
    const un = listen<Os2lStatus>(OS2L_STATUS_EVENT, (e) => {
      if (alive) setOs2l(e.payload);
    });
    return () => {
      alive = false;
      un.then((f) => f());
    };
  }, []);

  const selected = selectionKey(config.host);
  const instances = config.instances ?? [];

  const rows = useMemo(() => {
    const byIp = new Map<string, Row>();
    const slot = (ip: string): Row => {
      let r = byIp.get(ip);
      if (!r) {
        r = {
          ip,
          hostname: null,
          label: "",
          http_port: config.port,
          known: false,
          http_ok: null,
          playing: null,
          os2l_connected: false,
          os2l_beating: false,
          bpm: null,
          local: ip === "127.0.0.1",
          selected: ip === selected,
        };
        byIp.set(ip, r);
      }
      return r;
    };
    for (const i of instances) {
      const r = slot(i.ip);
      r.known = true;
      r.label = i.label;
      r.http_port = i.http_port;
      r.hostname = i.hostname ?? r.hostname;
    }
    slot(selected);
    for (const e of scan ?? []) {
      const r = slot(e.ip);
      r.hostname = e.hostname ?? r.hostname;
      r.http_ok = e.http_ok;
      r.playing = e.playing;
      r.os2l_connected = e.os2l_connected;
      r.os2l_beating = e.os2l_beating;
      r.bpm = e.bpm;
    }
    for (const p of os2l?.peers ?? []) {
      const r = slot(p.ip);
      r.os2l_connected = p.connected;
      r.os2l_beating = p.beating;
      r.bpm = p.bpm;
    }
    for (const [ip, p] of Object.entries(probes)) {
      const r = byIp.get(ip);
      if (r && r.http_ok === null) {
        r.http_ok = p.ok;
        r.playing = p.playing;
      }
    }
    return [...byIp.values()].sort((a, b) => {
      if (a.selected !== b.selected) return a.selected ? -1 : 1;
      if (a.local !== b.local) return a.local ? -1 : 1;
      return a.ip.localeCompare(b.ip);
    });
  }, [instances, scan, os2l, selected, config.port, probes]);

  // Rows without an HTTP verdict get probed right away, so "OS2L
  // connected" never sits next to a question mark for long.
  useEffect(() => {
    for (const r of rows) {
      if (r.http_ok !== null || probing.current.has(r.ip)) continue;
      probing.current.add(r.ip);
      invoke<boolean | null>("vdj_probe_http", { ip: r.ip, port: r.http_port })
        .then((res) => {
          setProbes((m) => ({ ...m, [r.ip]: { ok: res !== null, playing: res } }));
        })
        .catch(() => {
          setProbes((m) => ({ ...m, [r.ip]: { ok: false, playing: null } }));
        })
        .finally(() => {
          probing.current.delete(r.ip);
        });
    }
  }, [rows]);

  const runScan = useCallback(async () => {
    setScanning(true);
    setError(null);
    try {
      setScan(await invoke<VdjScanEntry[]>("vdj_scan"));
      setProbes({});
    } catch (e) {
      setError(stringify(e));
    } finally {
      setScanning(false);
    }
  }, []);

  const saveInstances = useCallback(
    async (next: VdjInstance[], auto: boolean) => {
      setError(null);
      try {
        const cfg = await invoke<VdjConfig>("vdj_set_instances", {
          instances: next,
          autoSwitch: auto,
        });
        onConfig(cfg);
      } catch (e) {
        setError(stringify(e));
      }
    },
    [onConfig],
  );

  const commitLabel = (row: Row) => {
    const label = (labels[row.ip] ?? row.label).trim();
    const known = instances.find((i) => i.ip === row.ip);
    if (known ? known.label === label : label === "") return;
    const next = known
      ? instances.map((i) => (i.ip === row.ip ? { ...i, label } : i))
      : [...instances, { ip: row.ip, label, http_port: row.http_port, hostname: row.hostname }];
    setLabels((m) => {
      const { [row.ip]: _drop, ...rest } = m;
      return rest;
    });
    void saveInstances(next, config.auto_switch);
  };

  const use = async (row: Row) => {
    setBusy(row.ip);
    setError(null);
    try {
      const cfg = await invoke<VdjConfig>("vdj_select_instance", {
        ip: row.ip,
        hostname: row.hostname,
      });
      onConfig(cfg);
    } catch (e) {
      setError(stringify(e));
    } finally {
      setBusy(null);
    }
  };

  const forget = (row: Row) => {
    void saveInstances(
      instances.filter((i) => i.ip !== row.ip),
      config.auto_switch,
    );
  };

  return (
    <div className="vdj-inst" data-doc="vdj-instances">
      <div className="vdj-inst__head">
        <div>
          <h4>{t("vdj.inst.title")}</h4>
          <span className="hint">{t("vdj.inst.intro")}</span>
        </div>
        <button type="button" className="vdj-inst__scan" onClick={runScan} disabled={scanning}>
          {scanning ? t("vdj.inst.scanning") : t("vdj.inst.scan")}
        </button>
      </div>
      {error ? (
        <output className="error" aria-live="polite">
          {error}
        </output>
      ) : null}
      <ul className="vdj-inst__list">
        {rows.map((row) => {
          const label = labels[row.ip] ?? row.label;
          return (
            <li
              key={row.ip}
              className={`vdj-inst__row${row.selected ? " selected" : ""}`}
              data-doc-selected={row.selected ? "true" : undefined}
            >
              <button
                type="button"
                className="vdj-inst__use"
                disabled={row.selected || busy !== null}
                onClick={() => use(row)}
                title={row.selected ? t("vdj.inst.inUse") : t("vdj.inst.useHint")}
              >
                {row.selected ? t("vdj.inst.inUse") : t("vdj.inst.use")}
              </button>
              <div className="vdj-inst__who">
                <input
                  className="vdj-inst__label"
                  placeholder={t("vdj.inst.labelPlaceholder")}
                  value={label}
                  onChange={(e) => setLabels((m) => ({ ...m, [row.ip]: e.currentTarget.value }))}
                  onBlur={() => commitLabel(row)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") commitLabel(row);
                  }}
                />
                <span className="vdj-inst__addr">
                  {row.ip}
                  {row.local ? ` · ${t("vdj.inst.thisMachine")}` : ""}
                  {row.hostname ? ` · ${row.hostname}` : ""}
                </span>
              </div>
              <span
                className={`vdj-inst__badge${row.http_ok ? " ok" : row.http_ok === false ? " off" : ""}`}
                title={`${row.ip}:${row.http_port}`}
              >
                {row.http_ok === null
                  ? t("vdj.inst.httpUnknown")
                  : row.http_ok
                    ? row.playing
                      ? t("vdj.inst.httpPlaying")
                      : t("vdj.inst.httpIdle")
                    : t("vdj.inst.httpDown")}
              </span>
              <span
                className={`vdj-inst__badge${row.os2l_beating ? " beat" : row.os2l_connected ? " ok" : ""}`}
              >
                {row.os2l_beating
                  ? t("vdj.inst.os2lBeating", { bpm: row.bpm != null ? row.bpm.toFixed(1) : "—" })
                  : row.os2l_connected
                    ? t("vdj.inst.os2lConnected")
                    : t("vdj.inst.os2lOff")}
              </span>
              {row.known && !row.selected ? (
                <button
                  type="button"
                  className="vdj-inst__forget"
                  onClick={() => forget(row)}
                  title={t("vdj.inst.forget")}
                >
                  ×
                </button>
              ) : (
                <span />
              )}
            </li>
          );
        })}
      </ul>
      {rows.length <= 1 && scan === null ? <p className="hint">{t("vdj.inst.empty")}</p> : null}
      <label className="vdj-inst__auto">
        <input
          type="checkbox"
          checked={config.auto_switch}
          onChange={(e) => void saveInstances(instances, e.currentTarget.checked)}
        />
        <span>
          <strong>{t("vdj.inst.auto")}</strong>
          <span className="hint"> {t("vdj.inst.autoHint")}</span>
        </span>
      </label>
    </div>
  );
}
