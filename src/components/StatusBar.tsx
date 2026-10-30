import type { Os2lStatus } from "@bindings/Os2lStatus";
import type { OutputHealth } from "@bindings/OutputHealth";
import type { VdjStatus } from "@bindings/VdjStatus";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { useT } from "../i18n";
import { useShowStore } from "../stores/show";

// Bottom status bar: one chip per configured DMX output (is it opening,
// sending frames and — for Art-Net — is the node answering?) on the left,
// VirtualDJ (HTTP poller + OS2L socket) on the right. Colour carries the
// state: green live, amber waiting, red failing, grey off. Details go in
// the tooltip; a click opens the matching Config section. It has its own
// strip so the header keeps its density on small screens.

type Dot = "ok" | "warn" | "err" | "off";
export type StatusSection = "outputs" | "vdj";

function useEventState<T>(command: string, event: string, refreshKey: unknown): T | null {
  const [value, setValue] = useState<T | null>(null);
  // `refreshKey` (the loaded show) re-fetches after a show swap, when the
  // outputs table is rebuilt.
  // biome-ignore lint/correctness/useExhaustiveDependencies: refreshKey is the deliberate re-fetch trigger.
  useEffect(() => {
    let alive = true;
    invoke<T>(command)
      .then((v) => {
        if (alive && v) setValue(v);
      })
      .catch(() => {});
    const un = listen<T>(event, (e) => {
      if (alive) setValue(e.payload);
    });
    return () => {
      alive = false;
      un.then((f) => f()).catch(() => {});
    };
  }, [command, event, refreshKey]);
  return value;
}

const KIND_ICON: Record<string, string> = {
  art_net: "🌐",
  sacn: "🌐",
  enttec_usb: "🔌",
  open_dmx: "🔌",
  open_dmx_ftdi: "🔌",
  mock: "🧪",
};

function shortLabel(o: OutputHealth): string {
  switch (o.kind) {
    case "art_net": {
      const ip = o.label.replace(/^Art-Net\s*/, "");
      const octets = ip.split(".");
      return octets.length === 4 ? `Art-Net .${octets[3]}` : "Art-Net";
    }
    case "sacn":
      return "sACN";
    case "enttec_usb":
      return "Enttec";
    case "open_dmx":
      return "OpenDMX";
    case "open_dmx_ftdi":
      return "FTDI";
    default:
      return o.label;
  }
}

export function StatusBar({ onOpen }: { onOpen?: (section: StatusSection) => void }) {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const suspended = useShowStore((s) => s.outputSuspended);
  const outputs = useEventState<OutputHealth[]>("output_health", "output:health", show);
  const vdj = useEventState<VdjStatus>("vdj_get_status", "vdj:status", show);
  const os2l = useEventState<Os2lStatus>("os2l_status", "os2l:status", show);
  // A 1 s clock so "fresh" judgements age without new events.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);

  if (outputs === null && vdj === null && os2l === null) return null;

  const outputChips: React.ReactNode[] = [];
  for (const o of outputs ?? []) {
    let dot: Dot;
    let detail: string;
    if (suspended) {
      dot = "off";
      detail = t("status.out.suspended");
    } else if (o.state === "error") {
      dot = "err";
      detail = o.last_error ?? t("status.out.error");
    } else if (o.linked === false) {
      dot = "warn";
      detail = o.kind === "art_net" ? t("status.out.noNode") : t("status.out.portClosed");
    } else if (o.state === "ok") {
      dot = "ok";
      detail = t("status.out.ok");
    } else {
      dot = "warn";
      detail = t("status.out.idle");
    }
    const uni = o.universes.length > 0 ? ` · U${o.universes.join(",")}` : "";
    outputChips.push(
      <button
        key={o.id}
        type="button"
        className={`link-chip link-chip--${dot}`}
        title={`${o.label}${uni}\n${detail}\n${t("status.out.hint")}`}
        onClick={() => onOpen?.("outputs")}
      >
        <span className="link-chip__dot" />
        <span className="link-chip__icon">{KIND_ICON[o.kind] ?? "⬢"}</span>
        <span className="link-chip__label">{shortLabel(o)}</span>
      </button>,
    );
  }

  let vdjChip: React.ReactNode = null;
  if (vdj !== null || os2l !== null) {
    const pollRunning = vdj?.running ?? false;
    const pollFresh =
      vdj?.last_success_at_secs != null && now / 1000 - Number(vdj.last_success_at_secs) < 4;
    const os2lRunning = os2l?.running ?? false;
    const lines: string[] = [];
    // Two DJs in the booth: say which VirtualDJ the rig follows.
    const host = show?.vdj?.host ?? "";
    const sourceKey = host.trim().toLowerCase() === "localhost" ? "127.0.0.1" : host.trim();
    const source = (show?.vdj?.instances ?? []).find((i) => i.ip === sourceKey);
    const sourceLabel = source?.label?.trim() || null;
    if (sourceLabel) lines.push(t("status.vdj.source", { label: sourceLabel, ip: sourceKey }));
    let dot: Dot;
    if (!pollRunning && !os2lRunning) {
      dot = "off";
      lines.push(t("status.vdj.off"));
    } else {
      if (pollRunning) {
        if (vdj?.last_error) lines.push(`HTTP: ${vdj.last_error}`);
        else if (pollFresh)
          lines.push(
            t("status.vdj.poll", { bpm: vdj?.last_bpm != null ? vdj.last_bpm.toFixed(1) : "—" }),
          );
      }
      if (os2lRunning) {
        if (os2l?.last_error) lines.push(`OS2L: ${os2l.last_error}`);
        else if (os2l?.peer) lines.push(t("status.vdj.os2lPeer", { peer: os2l.peer }));
        else lines.push(t("status.vdj.os2lListening", { port: String(os2l?.port ?? "") }));
      } else if (os2l?.last_error) {
        lines.push(`OS2L: ${os2l.last_error}`);
      }
      const hasError = Boolean(vdj?.last_error) || Boolean(os2l?.last_error);
      const live = pollFresh || Boolean(os2l?.peer);
      dot = hasError ? "err" : live ? "ok" : "warn";
      if (dot === "warn") lines.push(t("status.vdj.waiting"));
    }
    vdjChip = (
      <button
        key="vdj"
        type="button"
        className={`link-chip link-chip--${dot}`}
        title={`${lines.join("\n")}\n${t("status.vdj.hint")}`}
        onClick={() => onOpen?.("vdj")}
      >
        <span className="link-chip__dot" />
        <span className="link-chip__icon">🎧</span>
        <span className="link-chip__label">{sourceLabel ? `VDJ · ${sourceLabel}` : "VDJ"}</span>
      </button>
    );
  }

  if (outputChips.length === 0 && !vdjChip) return null;
  return (
    <footer className="status-bar">
      <div className="status-bar__group">
        {outputChips.length > 0 ? (
          outputChips
        ) : (
          <span className="hint">{t("status.out.none")}</span>
        )}
      </div>
      <div className="status-bar__group status-bar__group--right">{vdjChip}</div>
    </footer>
  );
}
