import type { MirrorStatus } from "@bindings/MirrorStatus";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { useT } from "../i18n";

// Always-visible role indicator in the header: which console is LIVE (driving
// lights + listening to VDJ) and which is a passive MIRROR. Clicking it is
// the manual failover control — confirm-guarded, because flipping roles
// starts/stops DMX output on the spot.
export function MirrorChip() {
  const t = useT();
  const [status, setStatus] = useState<MirrorStatus | null>(null);

  useEffect(() => {
    let alive = true;
    invoke<MirrorStatus>("mirror_status")
      .then((s) => {
        if (alive) setStatus(s);
      })
      .catch(() => {
        // Doc mode / stubbed IPC: no chip.
      });
    const un = listen<MirrorStatus>("mirror:status", (e) => {
      if (alive) setStatus(e.payload);
    });
    return () => {
      alive = false;
      un.then((f) => f());
    };
  }, []);

  if (!status) return null;
  const isLive = status.role === "live";

  const onClick = async () => {
    const question = isLive ? t("mirror.confirmMirror") : t("mirror.confirmTake");
    const yes = await ask(question, { title: "DMX Control", kind: "warning" });
    if (!yes) return;
    try {
      await invoke(isLive ? "become_mirror" : "take_control");
    } catch {
      // The HealthBanner surfaces command failures globally.
    }
  };

  // Compact label: the header must fit a 13" screen, so the chip says only
  // the role (plus a mirror count); host names and states go in the tooltip.
  let cls = "mirror-chip";
  let label: string;
  let detail: string;
  if (isLive) {
    cls += " mirror-chip--live";
    label = `🔴 ${t("mirror.chip.live")}`;
    if (status.mirrors > 0) label += ` ·${status.mirrors}`;
    detail =
      status.mirrors > 0
        ? t("mirror.chip.mirrors", { n: String(status.mirrors) })
        : t("mirror.chip.live");
  } else if (status.live_lost) {
    cls += " mirror-chip--lost";
    label = `🪞 ${t("mirror.chip.lost")}`;
    detail = t("mirror.chip.noSignal");
  } else if (status.live_name) {
    cls += " mirror-chip--mirror";
    label = `🪞 ${t("mirror.chip.mirror")}`;
    detail = t("mirror.chip.mirrorOf", { name: status.live_name });
  } else {
    cls += " mirror-chip--mirror";
    label = `🪞 ${t("mirror.chip.mirror")}…`;
    detail = t("mirror.chip.mirrorAlone");
  }

  return (
    <button
      type="button"
      className={cls}
      onClick={onClick}
      title={`${detail} — ${t("mirror.chip.hint")}`}
    >
      {label}
    </button>
  );
}
