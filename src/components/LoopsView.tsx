import type { LoopGroupActiveChange } from "@bindings/LoopGroupActiveChange";
import type { Scene } from "@bindings/Scene";
import type { SceneLoopGroup } from "@bindings/SceneLoopGroup";
import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { useT } from "../i18n";
import { useShowStore } from "../stores/show";

/// Dedicated tab for managing scene loop groups. Layout: wide cards
/// (one per loop) with a tall header row (Play/Stop + name + count +
/// delete), then the sequence of scenes as horizontal chips with
/// reorder/remove controls. Editing controls (hold time, add scene)
/// sit at the bottom of each card. The live "Running: X [Stop]"
/// footer over in Scenes keeps the operator informed without forcing
/// a tab switch during a show.
export function LoopsView() {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const scenes = useMemo(() => show?.scenes ?? [], [show?.scenes]);
  const groups = useMemo(() => show?.scene_loop_groups ?? [], [show?.scene_loop_groups]);

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

function LoopGroupCard({
  group,
  scenes,
  isActive,
  currentIndex,
  onUpdate,
  onDelete,
  onStart,
  onStop,
}: {
  group: SceneLoopGroup;
  scenes: Scene[];
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

  const sceneById = useMemo(() => {
    const map: Record<string, Scene> = {};
    for (const s of scenes) map[s.id] = s;
    return map;
  }, [scenes]);

  const liveCount = group.scene_ids.filter((id) => !!sceneById[id]).length;
  const unassignedScenes = scenes.filter((s) => !group.scene_ids.includes(s.id));

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

  const moveScene = (index: number, dir: -1 | 1) => {
    const target = index + dir;
    if (target < 0 || target >= group.scene_ids.length) return;
    const next = [...group.scene_ids];
    [next[index], next[target]] = [next[target], next[index]];
    onUpdate({ ...group, scene_ids: next });
  };
  const removeAt = (index: number) => {
    const next = group.scene_ids.filter((_, i) => i !== index);
    onUpdate({ ...group, scene_ids: next });
  };
  const appendScene = (id: string) => {
    if (!id) return;
    onUpdate({ ...group, scene_ids: [...group.scene_ids, id] });
  };

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
        {group.scene_ids.length === 0 ? (
          <p className="loops-empty-scenes muted">{t("loops.emptyScenes")}</p>
        ) : (
          <ol className="loops-chips">
            {assignSlotKeys(group.scene_ids).map(({ key, sid, i }) => {
              const scene = sceneById[sid];
              const label = scene?.name ?? t("loops.missingScene");
              const live = isActive && currentIndex !== null && currentIndex === i;
              return (
                <li
                  key={key}
                  className={`loops-chip${live ? " live" : ""}${scene ? "" : " missing"}`}
                >
                  <span className="loops-chip-idx">{i + 1}</span>
                  <span className="loops-chip-name">{label}</span>
                  <div className="loops-chip-actions">
                    <button
                      type="button"
                      onClick={() => moveScene(i, -1)}
                      disabled={i === 0}
                      title={t("loops.moveUp")}
                      aria-label={t("loops.moveUp")}
                    >
                      ←
                    </button>
                    <button
                      type="button"
                      onClick={() => moveScene(i, 1)}
                      disabled={i === group.scene_ids.length - 1}
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
            title={t("loops.holdHint")}
          />
        </label>
        <span className="hint loops-hold-hint">{t("loops.holdHint")}</span>
        <select
          className="loops-add-select"
          value=""
          onChange={(e) => {
            appendScene(e.currentTarget.value);
            e.currentTarget.value = "";
          }}
        >
          <option value="">{t("loops.addPlaceholder")}</option>
          {unassignedScenes.map((s) => (
            <option key={s.id} value={s.id}>
              {s.name}
            </option>
          ))}
          {scenes
            .filter((s) => group.scene_ids.includes(s.id))
            .map((s) => (
              <option key={`dup-${s.id}`} value={s.id}>
                {t("loops.repeatOption", { name: s.name })}
              </option>
            ))}
        </select>
      </div>
    </li>
  );
}

/// Build stable React keys for a list that may contain duplicate scene
/// ids. The same scene id repeats once for each occurrence, so we
/// append `#N` where N is how many times that id has been seen so far.
/// Avoids using the bare array index as the key (which would break
/// reorders) while still letting the operator add the same scene
/// twice to a playlist.
function assignSlotKeys(scene_ids: string[]): { key: string; sid: string; i: number }[] {
  const seen: Record<string, number> = {};
  return scene_ids.map((sid, i) => {
    const count = (seen[sid] ?? 0) + 1;
    seen[sid] = count;
    return { key: `${sid}#${count}`, sid, i };
  });
}

function stringifyError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return JSON.stringify(e);
}
