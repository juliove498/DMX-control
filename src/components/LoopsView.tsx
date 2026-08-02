import type { LoopEntry } from "@bindings/LoopEntry";
import type { LoopGroupActiveChange } from "@bindings/LoopGroupActiveChange";
import type { Scene } from "@bindings/Scene";
import type { SceneLoopGroup } from "@bindings/SceneLoopGroup";
import type { Snapshot } from "@bindings/Snapshot";
import type { Subdivision } from "@bindings/Subdivision";
import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { useT } from "../i18n";
import type { Translation } from "../i18n/translations";
import { useShowStore } from "../stores/show";

/// Dedicated tab for managing loop sequences. Layout: wide cards
/// (one per loop) with a tall header row (Play/Stop + name + count +
/// delete), then the sequence of entries — scenes and snapshots — as
/// horizontal chips with reorder/remove controls. Editing controls
/// (hold time, BPM sync + subdivision, add entry) sit at the bottom of
/// each card. The live "Running: X [Stop]" footer over in Scenes keeps
/// the operator informed without forcing a tab switch during a show.
export function LoopsView() {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const scenes = useMemo(() => show?.scenes ?? [], [show?.scenes]);
  const snapshots = useMemo(() => show?.snapshots ?? [], [show?.snapshots]);
  const groups = useMemo(() => show?.scene_loop_groups ?? [], [show?.scene_loop_groups]);
  const bpmEnabled = show?.globals.overall_bpm_enabled ?? false;

  const createLoopGroup = useShowStore((s) => s.createLoopGroup);
  const updateLoopGroup = useShowStore((s) => s.updateLoopGroup);
  const deleteLoopGroup = useShowStore((s) => s.deleteLoopGroup);
  const startLoopGroup = useShowStore((s) => s.startLoopGroup);
  const stopLoopGroup = useShowStore((s) => s.stopLoopGroup);
  const activeLoopGroupQuery = useShowStore((s) => s.activeLoopGroup);

  const [activeLoop, setActiveLoop] = useState<LoopGroupActiveChange>({
    active_group_id: null,
    current_index: null,
    current_scene_id: null,
  });
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const tick = () => {
      activeLoopGroupQuery()
        .then((s) => {
          if (!cancelled) setActiveLoop(s);
        })
        .catch(() => {});
    };
    tick();
    const interval = window.setInterval(tick, 300);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [activeLoopGroupQuery]);

  const wrapErr = (op: () => unknown | Promise<unknown>) => async () => {
    try {
      await op();
    } catch (e) {
      setError(stringifyError(e));
    }
  };

  if (!show) return <main className="page">{t("common.loading")}</main>;

  return (
    <main className="page loops-view">
      <header className="page-head">
        <h2>{t("loops.title")}</h2>
        <span className="meta">{t("loops.meta", { count: groups.length })}</span>
        <div className="actions">
          <button
            type="button"
            className="primary loops-new-btn"
            onClick={wrapErr(() => createLoopGroup())}
          >
            {t("loops.new")}
          </button>
        </div>
      </header>
      <p className="hint loops-intro">{t("loops.intro")}</p>

      {error ? (
        <output className="error" aria-live="polite">
          {error}
        </output>
      ) : null}

      {groups.length === 0 ? (
        <div className="loops-empty">
          <p>{t("loops.empty")}</p>
        </div>
      ) : (
        <ul className="loops-list">
          {groups.map((g) => (
            <LoopGroupCard
              key={g.id}
              group={g}
              scenes={scenes}
              snapshots={snapshots}
              bpmEnabled={bpmEnabled}
              isActive={activeLoop.active_group_id === g.id}
              currentIndex={
                activeLoop.active_group_id === g.id ? (activeLoop.current_index ?? null) : null
              }
              onUpdate={async (next) => {
                try {
                  await updateLoopGroup(next);
                } catch (e) {
                  setError(stringifyError(e));
                }
              }}
              onDelete={wrapErr(() => deleteLoopGroup(g.id))}
              onStart={wrapErr(() => startLoopGroup(g.id))}
              onStop={wrapErr(() => stopLoopGroup())}
            />
          ))}
        </ul>
      )}
    </main>
  );
}

const SUBDIVISIONS: { value: Subdivision; labelKey: keyof Translation }[] = [
  { value: "quarter", labelKey: "loops.sub.quarter" },
  { value: "half", labelKey: "loops.sub.half" },
  { value: "one", labelKey: "loops.sub.one" },
  { value: "two", labelKey: "loops.sub.two" },
  { value: "four", labelKey: "loops.sub.four" },
  { value: "eight", labelKey: "loops.sub.eight" },
];

function LoopGroupCard({
  group,
  scenes,
  snapshots,
  bpmEnabled,
  isActive,
  currentIndex,
  onUpdate,
  onDelete,
  onStart,
  onStop,
}: {
  group: SceneLoopGroup;
  scenes: Scene[];
  snapshots: Snapshot[];
  bpmEnabled: boolean;
  isActive: boolean;
  currentIndex: number | null;
  onUpdate: (group: SceneLoopGroup) => void | Promise<void>;
  onDelete: () => void | Promise<void>;
  onStart: () => void | Promise<void>;
  onStop: () => void | Promise<void>;
}) {
  const t = useT();
  const [name, setName] = useState(group.name);
  const [hold, setHold] = useState(group.hold_ms_override);
  useEffect(() => setName(group.name), [group.name]);
  useEffect(() => setHold(group.hold_ms_override), [group.hold_ms_override]);

  const nameById = useMemo(() => {
    const map: Record<string, { name: string; kind: "scene" | "snapshot" }> = {};
    for (const s of scenes) map[s.id] = { name: s.name, kind: "scene" };
    for (const s of snapshots) map[s.id] = { name: s.name, kind: "snapshot" };
    return map;
  }, [scenes, snapshots]);

  const liveCount = group.entries.filter((e) => !!nameById[e.id]).length;
  const bpmSyncActive = group.sync_to_bpm && bpmEnabled;

  const commitName = () => {
    const next = name.trim();
    if (next === "" || next === group.name) {
      setName(group.name);
      return;
    }
    onUpdate({ ...group, name: next });
  };

  const commitHold = () => {
    const next = Math.max(0, Math.floor(hold));
    if (next === group.hold_ms_override) return;
    onUpdate({ ...group, hold_ms_override: next });
  };

  const moveEntry = (index: number, dir: -1 | 1) => {
    const target = index + dir;
    if (target < 0 || target >= group.entries.length) return;
    const next = [...group.entries];
    [next[index], next[target]] = [next[target], next[index]];
    onUpdate({ ...group, entries: next });
  };
  const removeAt = (index: number) => {
    const next = group.entries.filter((_, i) => i !== index);
    onUpdate({ ...group, entries: next });
  };
  const appendEntry = (raw: string) => {
    // Option values encode kind + id as "scene:<id>" / "snapshot:<id>".
    if (!raw) return;
    const sep = raw.indexOf(":");
    if (sep < 0) return;
    const kind = raw.slice(0, sep);
    const id = raw.slice(sep + 1);
    const entry: LoopEntry = kind === "snapshot" ? { type: "snapshot", id } : { type: "scene", id };
    onUpdate({ ...group, entries: [...group.entries, entry] });
  };

  const inList = (id: string) => group.entries.some((e) => e.id === id);

  return (
    <li className={`card loops-card${isActive ? " active" : ""}`}>
      <div className="loops-card-head">
        <button
          type="button"
          className={`loops-go${isActive ? " stop" : ""}`}
          title={isActive ? t("loops.stopHint") : t("loops.startHint")}
          onClick={() => (isActive ? onStop() : onStart())}
          disabled={!isActive && liveCount === 0}
        >
          {isActive ? "■" : "▶"}
        </button>
        <input
          className="loops-name"
          value={name}
          onChange={(e) => setName(e.currentTarget.value)}
          onBlur={commitName}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitName();
            else if (e.key === "Escape") setName(group.name);
          }}
        />
        <span className="loops-count muted">
          {t("loops.sceneCount", { count: liveCount, plural: liveCount === 1 ? "" : "s" })}
        </span>
        <button
          type="button"
          className="danger loops-del"
          title={t("loops.deleteTitle")}
          onClick={async () => {
            const ok = await ask(t("loops.deleteBody", { name: group.name }), {
              title: t("loops.deleteTitle"),
              kind: "warning",
            });
            if (ok) onDelete();
          }}
        >
          ×
        </button>
      </div>

      <div className="loops-card-body">
        {group.entries.length === 0 ? (
          <p className="loops-empty-scenes muted">{t("loops.emptyScenes")}</p>
        ) : (
          <ol className="loops-chips">
            {assignSlotKeys(group.entries).map(({ key, entry, i }) => {
              const target = nameById[entry.id];
              const label = target?.name ?? t("loops.missingScene");
              const isSnapshot = entry.type === "snapshot";
              const live = isActive && currentIndex !== null && currentIndex === i;
              return (
                <li
                  key={key}
                  className={`loops-chip${live ? " live" : ""}${target ? "" : " missing"}${
                    isSnapshot ? " snapshot" : ""
                  }`}
                  title={isSnapshot ? t("loops.snapshotChipHint") : undefined}
                >
                  <span className="loops-chip-idx">{i + 1}</span>
                  {isSnapshot ? (
                    <span className="loops-chip-kind" aria-hidden="true">
                      ●
                    </span>
                  ) : null}
                  <span className="loops-chip-name">{label}</span>
                  <div className="loops-chip-actions">
                    <button
                      type="button"
                      onClick={() => moveEntry(i, -1)}
                      disabled={i === 0}
                      title={t("loops.moveUp")}
                      aria-label={t("loops.moveUp")}
                    >
                      ←
                    </button>
                    <button
                      type="button"
                      onClick={() => moveEntry(i, 1)}
                      disabled={i === group.entries.length - 1}
                      title={t("loops.moveDown")}
                      aria-label={t("loops.moveDown")}
                    >
                      →
                    </button>
                    <button
                      type="button"
                      className="danger"
                      onClick={() => removeAt(i)}
                      title={t("loops.removeFromList")}
                      aria-label={t("loops.removeFromList")}
                    >
                      ×
                    </button>
                  </div>
                </li>
              );
            })}
          </ol>
        )}
      </div>

      <div className="loops-card-foot">
        <label className="loops-hold-field">
          <span className="hint">{t("loops.hold")}</span>
          <input
            type="number"
            min={0}
            max={120000}
            step={100}
            value={hold}
            onChange={(e) => setHold(Number(e.currentTarget.value))}
            onBlur={commitHold}
            disabled={bpmSyncActive}
            title={bpmSyncActive ? t("loops.holdDisabledHint") : t("loops.holdHint")}
          />
        </label>
        <label className="loops-sync-field" title={t("loops.syncBpmHint")}>
          <input
            type="checkbox"
            checked={group.sync_to_bpm}
            onChange={(e) => onUpdate({ ...group, sync_to_bpm: e.currentTarget.checked })}
          />
          <span className="hint">{t("loops.syncBpm")}</span>
        </label>
        {group.sync_to_bpm ? (
          <select
            className="loops-subdivision-select"
            value={group.subdivision}
            onChange={(e) =>
              onUpdate({ ...group, subdivision: e.currentTarget.value as Subdivision })
            }
            title={t("loops.subdivisionHint")}
          >
            {SUBDIVISIONS.map((s) => (
              <option key={s.value} value={s.value}>
                {t(s.labelKey)}
              </option>
            ))}
          </select>
        ) : null}
        {group.sync_to_bpm && !bpmEnabled ? (
          <span className="hint loops-sync-warn">{t("loops.syncBpmOff")}</span>
        ) : null}
        <select
          className="loops-add-select"
          value=""
          onChange={(e) => {
            appendEntry(e.currentTarget.value);
            e.currentTarget.value = "";
          }}
        >
          <option value="">{t("loops.addPlaceholder")}</option>
          <optgroup label={t("loops.addSceneGroup")}>
            {scenes.map((s) => (
              <option key={`sc-${s.id}`} value={`scene:${s.id}`}>
                {inList(s.id) ? t("loops.repeatOption", { name: s.name }) : s.name}
              </option>
            ))}
          </optgroup>
          <optgroup label={t("loops.addSnapshotGroup")}>
            {snapshots.map((s) => (
              <option key={`sn-${s.id}`} value={`snapshot:${s.id}`}>
                {inList(s.id) ? t("loops.repeatOption", { name: s.name }) : s.name}
              </option>
            ))}
          </optgroup>
        </select>
      </div>
    </li>
  );
}

/// Build stable React keys for a list that may contain duplicate entry
/// ids. The same id repeats once for each occurrence, so we append
/// `#N` where N is how many times that id has been seen so far.
/// Avoids using the bare array index as the key (which would break
/// reorders) while still letting the operator add the same cue twice.
function assignSlotKeys(entries: LoopEntry[]): { key: string; entry: LoopEntry; i: number }[] {
  const seen: Record<string, number> = {};
  return entries.map((entry, i) => {
    const count = (seen[entry.id] ?? 0) + 1;
    seen[entry.id] = count;
    return { key: `${entry.type}:${entry.id}#${count}`, entry, i };
  });
}

function stringifyError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return JSON.stringify(e);
}
