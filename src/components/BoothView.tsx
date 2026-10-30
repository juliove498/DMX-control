import type { AutopilotStatus } from "@bindings/AutopilotStatus";
import type { ButtonAction } from "@bindings/ButtonAction";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import { useT } from "../i18n";
import { useShowStore } from "../stores/show";
import { FixtureTestControl } from "./FixtureTestControl";
import { useFixtureLightStates } from "./preview3d/useFixtureLightStates";

// One-screen live view for a booth where one person does everything: the
// rig at a glance (only the fixtures brought to this event, like Stage),
// the snapshot / scene / chaser / movement / loop pads with their live
// state, and Modo Mágico's switch with the energy presets. The global
// header keeps BPM, blackout and blind; the status bar keeps outputs and
// VirtualDJ. Nothing here is a second source of truth: every pad goes
// through the same commands the other views use.

const EMPTY: FixtureInstance[] = [];
const PRESETS: { key: "low" | "mid" | "high"; level: number }[] = [
  { key: "low", level: 0.25 },
  { key: "mid", level: 0.55 },
  { key: "high", level: 0.85 },
];

function fire(action: ButtonAction) {
  invoke("preview_action", { action }).catch(() => {});
}

export function BoothView() {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const library = useShowStore((s) => s.library);
  const toggleChaser = useShowStore((s) => s.toggleChaser);
  const toggleMovement = useShowStore((s) => s.toggleMovement);
  const activateSnapshot = useShowStore((s) => s.activateSnapshot);
  const deactivateSnapshot = useShowStore((s) => s.deactivateSnapshot);
  const startLoopGroup = useShowStore((s) => s.startLoopGroup);
  const stopLoopGroup = useShowStore((s) => s.stopLoopGroup);
  const activeSceneId = useShowStore((s) => s.activeSceneId);
  const activeSnapshotId = useShowStore((s) => s.activeSnapshotId);
  const activeLoopGroup = useShowStore((s) => s.activeLoopGroup);

  // Same per-machine switch as Stage: fixtures left home stay hidden.
  const [showAll, setShowAll] = useState<boolean>(() => {
    try {
      return localStorage.getItem("stage.showAllFixtures") === "1";
    } catch {
      return false;
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem("stage.showAllFixtures", showAll ? "1" : "0");
    } catch {
      /* per-session only */
    }
  }, [showAll]);
  const fixtures = useMemo(
    () => (show ? show.fixtures.filter((f) => showAll || f.in_use !== false) : EMPTY),
    [show, showAll],
  );
  const awayCount = useMemo(
    () => (show?.fixtures ?? []).filter((f) => f.in_use === false).length,
    [show],
  );
  const light = useFixtureLightStates(fixtures, library, 10);

  // Live states: fetched once, then kept fresh by the same events the
  // dedicated views listen to.
  const [sceneId, setSceneId] = useState<string | null>(null);
  const [snapshotId, setSnapshotId] = useState<string | null>(null);
  const [loopId, setLoopId] = useState<string | null>(null);
  const [magic, setMagic] = useState<AutopilotStatus | null>(null);
  useEffect(() => {
    let alive = true;
    activeSceneId()
      .then((id) => {
        if (alive) setSceneId(id);
      })
      .catch(() => {});
    activeSnapshotId()
      .then((id) => {
        if (alive) setSnapshotId(id);
      })
      .catch(() => {});
    activeLoopGroup()
      .then((g) => {
        if (alive) setLoopId(g?.active_group_id ?? null);
      })
      .catch(() => {});
    invoke<AutopilotStatus>("autopilot_status")
      .then((s) => {
        if (alive && s) setMagic(s);
      })
      .catch(() => {});
    const uns = [
      listen<{ active_scene_id: string | null }>("scene:active_changed", (e) => {
        if (alive) setSceneId(e.payload.active_scene_id);
      }),
      listen<{ active_snapshot_id: string | null }>("snapshot:active_changed", (e) => {
        if (alive) setSnapshotId(e.payload.active_snapshot_id ?? null);
      }),
      listen<{ active_group_id: string | null }>("loop_group:active_changed", (e) => {
        if (alive) setLoopId(e.payload.active_group_id ?? null);
      }),
      listen<AutopilotStatus>("autopilot:status", (e) => {
        if (alive) setMagic(e.payload);
      }),
    ];
    return () => {
      alive = false;
      for (const u of uns) u.then((f) => f()).catch(() => {});
    };
  }, [activeSceneId, activeSnapshotId, activeLoopGroup]);

  // Mini-map: fit the patch positions into the card.
  const box = useMemo(() => {
    if (fixtures.length === 0) return { x0: 0, y0: 0, w: 1, h: 1 };
    let x0 = Number.POSITIVE_INFINITY;
    let y0 = Number.POSITIVE_INFINITY;
    let x1 = Number.NEGATIVE_INFINITY;
    let y1 = Number.NEGATIVE_INFINITY;
    for (const f of fixtures) {
      const [x, y] = f.position;
      x0 = Math.min(x0, x);
      y0 = Math.min(y0, y);
      x1 = Math.max(x1, x);
      y1 = Math.max(y1, y);
    }
    return { x0, y0, w: Math.max(1, x1 - x0), h: Math.max(1, y1 - y0) };
  }, [fixtures]);

  if (!show) return <main className="page booth-view" />;

  const magicOn = magic?.enabled ?? false;
  const energy = magic?.energy ?? 0;
  const toggleMagic = () => {
    invoke<AutopilotStatus>("autopilot_set_enabled", { enabled: !magicOn })
      .then((s) => {
        if (s) setMagic(s);
      })
      .catch(() => {});
  };

  return (
    <main className="page booth-view">
      <section className={`booth-magic${magicOn ? " on" : ""}`}>
        <button
          type="button"
          className={`magic-toggle booth-magic__toggle${magicOn ? " on" : ""}`}
          onClick={toggleMagic}
          title={t("magic.toggleHint")}
        >
          <span className="magic-toggle__icon">{magicOn ? "✨" : "○"}</span>
          <span className="magic-toggle__label">{t("booth.magic")}</span>
        </button>
        <div className="booth-magic__info">
          <strong>{magic?.look?.name ?? "—"}</strong>
          <span className="hint">
            {magic ? `${Math.round(energy * 100)}% · ${magic.bpm.toFixed(1)} BPM` : ""}
          </span>
        </div>
        <div className="magic-seg booth-magic__presets">
          {PRESETS.map((p) => (
            <button
              key={p.key}
              type="button"
              className={Math.abs(energy - p.level) < 0.02 ? "active" : ""}
              onClick={() => fire({ type: "autopilot_energy", level: p.level })}
            >
              {t(`magic.how.preset.${p.key}` as Parameters<typeof t>[0])}
            </button>
          ))}
        </div>
      </section>

      <section className="card booth-rig">
        <header className="booth-card__head">
          <h3>{t("booth.rig")}</h3>
          <span className="hint">
            {t("stage.test.count", { n: String(fixtures.length) })}
            {awayCount > 0 && !showAll ? t("stage.metaAway", { n: String(awayCount) }) : ""}
          </span>
          <div className="booth-card__actions">
            {awayCount > 0 ? (
              <button
                type="button"
                className={`stage-pill-btn${showAll ? " active" : ""}`}
                onClick={() => setShowAll((v) => !v)}
              >
                {showAll ? t("booth.onlyInUse") : t("booth.showAll")}
              </button>
            ) : null}
            <FixtureTestControl selectedIds={[]} onCurrent={() => {}} />
          </div>
        </header>
        <div className="booth-map">
          {fixtures.map((f) => {
            const st = light[f.id];
            const lit = st ? Math.max(st.color.r, st.color.g, st.color.b) : 0;
            const bg = st
              ? `rgb(${Math.round(st.color.r * 255)}, ${Math.round(st.color.g * 255)}, ${Math.round(st.color.b * 255)})`
              : "transparent";
            const left = ((f.position[0] - box.x0) / box.w) * 100;
            const top = ((f.position[1] - box.y0) / box.h) * 100;
            const label = f.label ?? f.id;
            return (
              <div
                key={f.id}
                className={`booth-dot${f.in_use === false ? " away" : ""}`}
                style={{ left: `${left}%`, top: `${top}%` }}
                title={label}
              >
                <span
                  className="booth-dot__lamp"
                  style={{
                    background: bg,
                    boxShadow: lit > 0.05 ? `0 0 ${8 + lit * 14}px ${bg}` : "none",
                  }}
                />
                <span className="booth-dot__label">{label}</span>
              </div>
            );
          })}
        </div>
      </section>

      <section className="booth-pads">
        <PadGroup
          title={t("booth.snapshots")}
          empty={t("booth.empty")}
          kind="snapshot"
          items={show.snapshots.map((s) => ({
            id: s.id,
            name: s.name,
            active: snapshotId === s.id,
          }))}
          onPress={(id, active) => {
            if (active) deactivateSnapshot().catch(() => {});
            else activateSnapshot(id).catch(() => {});
          }}
        />
        <PadGroup
          title={t("booth.scenes")}
          empty={t("booth.empty")}
          kind="scene"
          items={show.scenes.map((s) => ({ id: s.id, name: s.name, active: sceneId === s.id }))}
          onPress={(id) => fire({ type: "recall_scene", id })}
        />
        <PadGroup
          title={t("booth.chasers")}
          empty={t("booth.empty")}
          kind="chaser"
          items={show.chasers.map((c) => ({ id: c.id, name: c.name, active: c.enabled }))}
          onPress={(id, active) => toggleChaser(id, !active).catch(() => {})}
        />
        <PadGroup
          title={t("booth.movements")}
          empty={t("booth.empty")}
          kind="movement"
          items={show.movements.map((m) => ({ id: m.id, name: m.name, active: m.enabled }))}
          onPress={(id, active) => toggleMovement(id, !active).catch(() => {})}
        />
        <PadGroup
          title={t("booth.loops")}
          empty={t("booth.empty")}
          kind="loop"
          items={show.scene_loop_groups.map((g) => ({
            id: g.id,
            name: g.name,
            active: loopId === g.id,
          }))}
          onPress={(id, active) => {
            if (active) stopLoopGroup().catch(() => {});
            else startLoopGroup(id).catch(() => {});
          }}
        />
      </section>
    </main>
  );
}

function PadGroup({
  title,
  empty,
  kind,
  items,
  onPress,
}: {
  title: string;
  empty: string;
  kind: "snapshot" | "scene" | "chaser" | "movement" | "loop";
  items: { id: string; name: string; active: boolean }[];
  onPress: (id: string, active: boolean) => void;
}) {
  return (
    <div className={`booth-group booth-group--${kind}`}>
      <h4>{title}</h4>
      {items.length === 0 ? (
        <p className="hint">{empty}</p>
      ) : (
        <div className="booth-group__pads">
          {items.map((it) => (
            <button
              key={it.id}
              type="button"
              className={`booth-pad${it.active ? " active" : ""}`}
              onClick={() => onPress(it.id, it.active)}
            >
              {it.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
