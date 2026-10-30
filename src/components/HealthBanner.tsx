import type { MirrorStatus } from "@bindings/MirrorStatus";
import type { PeerInfo } from "@bindings/PeerInfo";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { useT } from "../i18n";

// Failures used to die silently: persist errors went only to the log file and
// the store awaits invoke() without a single catch, so a rejected command was
// an unhandled promise rejection — the click just "did nothing". After losing
// an event night to that, every failure now gets pixels on screen. The same
// surface raises the two failover alarms: TWO consoles claiming "live" on one
// network (they'd fight over the same nodes), and a mirror losing its live
// console (offer to take control).

function fmtReason(r: unknown): string {
  if (typeof r === "string") return r;
  if (r && typeof r === "object") {
    const o = r as { message?: unknown };
    if (typeof o.message === "string") return o.message;
    try {
      return JSON.stringify(r);
    } catch {
      // fall through to String()
    }
  }
  return String(r);
}

export function HealthBanner() {
  const t = useT();
  const [configProblems, setConfigProblems] = useState<string[]>([]);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [opError, setOpError] = useState<string | null>(null);
  const [peers, setPeers] = useState<PeerInfo[]>([]);
  const [mirror, setMirror] = useState<MirrorStatus | null>(null);
  // Dismissing the two-live alarm silences THAT peer set only; if the set
  // changes (another console flips to live, or reconnects under a new name)
  // the strip comes back. At a gig this must not be missable.
  const [conflictDismissed, setConflictDismissed] = useState<string | null>(null);
  const [secondLaunch, setSecondLaunch] = useState(false);
  const [repairing, setRepairing] = useState(false);
  const [repairMsg, setRepairMsg] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    invoke<string[]>("config_health")
      .then((problems) => {
        if (alive && Array.isArray(problems) && problems.length > 0) {
          setConfigProblems(problems);
        }
      })
      .catch(() => {
        // Doc mode / stubbed IPC: no health data, no banner.
      });
    invoke<PeerInfo[]>("instance_peers")
      .then((p) => {
        if (alive && Array.isArray(p)) setPeers(p);
      })
      .catch(() => {
        // Same: fine without the radar.
      });
    invoke<MirrorStatus>("mirror_status")
      .then((s) => {
        if (alive) setMirror(s);
      })
      .catch(() => {
        // Same.
      });
    const unErr = listen<string>("save:error", (e) => {
      if (alive) setSaveError(fmtReason(e.payload));
    });
    const unOk = listen("save:ok", () => {
      if (alive) setSaveError(null);
    });
    const unPeers = listen<PeerInfo[]>("instances:peers", (e) => {
      if (alive) setPeers(Array.isArray(e.payload) ? e.payload : []);
    });
    const unMirror = listen<MirrorStatus>("mirror:status", (e) => {
      if (alive) setMirror(e.payload);
    });
    const unSecond = listen("instances:second-launch", () => {
      if (alive) setSecondLaunch(true);
    });
    const onRejection = (ev: PromiseRejectionEvent) => {
      setOpError(fmtReason(ev.reason));
    };
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      alive = false;
      unErr.then((f) => f());
      unOk.then((f) => f());
      unPeers.then((f) => f());
      unMirror.then((f) => f());
      unSecond.then((f) => f());
      window.removeEventListener("unhandledrejection", onRejection);
    };
  }, []);

  // The op toast is transient: it tells the operator "that click failed",
  // then gets out of the way. Persistent conditions have their own strips.
  useEffect(() => {
    if (!opError) return;
    const id = window.setTimeout(() => setOpError(null), 12000);
    return () => window.clearTimeout(id);
  }, [opError]);

  useEffect(() => {
    if (!secondLaunch) return;
    const id = window.setTimeout(() => setSecondLaunch(false), 8000);
    return () => window.clearTimeout(id);
  }, [secondLaunch]);

  // Root-owned files from an accidental sudo run are the recurring cause of
  // "nothing saves anymore"; the fix needs admin rights, so we hand it to the
  // OS's own password dialog and re-probe afterwards.
  const repair = async () => {
    setRepairing(true);
    try {
      const problems = await invoke<string[]>("repair_config_permissions");
      const left = Array.isArray(problems) ? problems : [];
      setConfigProblems(left);
      if (left.length === 0) {
        setSaveError(null);
        setRepairMsg(t("health.repairOk"));
      }
    } catch (e) {
      setOpError(fmtReason(e));
    } finally {
      setRepairing(false);
    }
  };

  useEffect(() => {
    if (!repairMsg) return;
    const id = window.setTimeout(() => setRepairMsg(null), 8000);
    return () => window.clearTimeout(id);
  }, [repairMsg]);

  const takeControl = async () => {
    const yes = await ask(t("mirror.confirmTake"), { title: "DMX Control", kind: "warning" });
    if (!yes) return;
    try {
      await invoke("take_control");
    } catch {
      // Surfaced by the unhandledrejection toast path via a fresh throw is
      // not guaranteed here; the mirror:status event will tell the truth.
    }
  };

  const livePeers = peers.filter((p) => p.role === "live");
  const conflictKey = livePeers
    .map((p) => p.name)
    .sort()
    .join("|");
  const showConflict =
    mirror?.role === "live" && livePeers.length > 0 && conflictDismissed !== conflictKey;
  const showLiveLost = mirror?.role === "mirror" && mirror.live_lost;

  if (
    configProblems.length === 0 &&
    !saveError &&
    !opError &&
    !showConflict &&
    !showLiveLost &&
    !secondLaunch &&
    !repairMsg
  ) {
    return null;
  }

  return (
    <div className="health-banner-stack">
      {configProblems.length > 0 && (
        <div className="health-banner health-banner--error">
          <span className="health-banner__title">⚠️ {t("health.configProblems")}</span>
          <span className="health-banner__detail">{configProblems.join(" · ")}</span>
          <button
            type="button"
            className="health-banner__action"
            onClick={repair}
            disabled={repairing}
          >
            {repairing ? t("health.repairing") : t("health.repair")}
          </button>
          <button
            type="button"
            className="health-banner__close"
            onClick={() => setConfigProblems([])}
            title={t("health.dismiss")}
          >
            ✕
          </button>
        </div>
      )}
      {repairMsg && (
        <div className="health-banner health-banner--ok">
          <span className="health-banner__detail">✅ {repairMsg}</span>
        </div>
      )}
      {saveError && (
        <div className="health-banner health-banner--error">
          <span className="health-banner__title">⚠️ {t("health.saveError")}</span>
          <span className="health-banner__detail">{saveError}</span>
        </div>
      )}
      {showConflict && (
        <div className="health-banner health-banner--error">
          <span className="health-banner__title">🚨 {t("health.bothLiveTitle")}</span>
          <span className="health-banner__detail">
            {t("health.bothLiveDetail", { names: livePeers.map((p) => p.name).join(", ") })}
          </span>
          <button
            type="button"
            className="health-banner__close"
            onClick={() => setConflictDismissed(conflictKey)}
            title={t("health.dismiss")}
          >
            ✕
          </button>
        </div>
      )}
      {showLiveLost && (
        <div className="health-banner health-banner--error">
          <span className="health-banner__title">🚨 {t("mirror.liveLostTitle")}</span>
          <span className="health-banner__detail">{t("mirror.liveLostDetail")}</span>
          <button type="button" className="health-banner__action" onClick={takeControl}>
            {t("mirror.takeControl")}
          </button>
        </div>
      )}
      {secondLaunch && (
        <div className="health-banner health-banner--warn">
          <span className="health-banner__detail">{t("health.secondLaunch")}</span>
        </div>
      )}
      {opError && (
        <div className="health-banner health-banner--warn">
          <span className="health-banner__title">{t("health.opFailed")}</span>
          <span className="health-banner__detail">{opError}</span>
          <button
            type="button"
            className="health-banner__close"
            onClick={() => setOpError(null)}
            title={t("health.dismiss")}
          >
            ✕
          </button>
        </div>
      )}
    </div>
  );
}
