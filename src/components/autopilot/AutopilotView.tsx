import type { AutopilotConfig } from "@bindings/AutopilotConfig";
import type { AutopilotStatus } from "@bindings/AutopilotStatus";
import type { ButtonAction } from "@bindings/ButtonAction";
import type { EnergyBand } from "@bindings/EnergyBand";
import type { EnergySource } from "@bindings/EnergySource";
import type { HeadZone } from "@bindings/HeadZone";
import type { LookInfo } from "@bindings/LookInfo";
import type { PaletteStyle } from "@bindings/PaletteStyle";
import type { RigFixture } from "@bindings/RigFixture";
import type { VocabEntry } from "@bindings/VocabEntry";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../../i18n";
import { useShowStore } from "../../stores/show";
import { HeadZonePad } from "./HeadZonePad";

// Modo Mágico: the autopilot's control room. One big switch, a live view of
// the musical clock + energy + the look it's playing, and the four things
// you tune: who takes part, how it improvises, which of your own looks it
// may use, and where each moving head is allowed to aim in this venue.

const SAVE_DEBOUNCE_MS = 350;
const CADENCES = [4, 8, 16, 32] as const;
const PALETTES: { id: PaletteStyle; swatches: string[] }[] = [
  { id: "auto", swatches: ["#ff5a1f", "#1fa0ff", "#3bff6a"] },
  { id: "warm", swatches: ["#ff5a1f", "#ffb020", "#ff2d75"] },
  { id: "cold", swatches: ["#1fa0ff", "#6b5bff", "#22e3e3"] },
  { id: "rainbow", swatches: ["#ff3b3b", "#3bff6a", "#3b6bff"] },
  { id: "mono", swatches: ["#ff4fa3", "#ff9ecf", "#ffe0ee"] },
];
const BANDS: (EnergyBand | null)[] = [null, "low", "mid", "high"];
const DEFAULT_ZONE = { pan_min: 0.3, pan_max: 0.7, tilt_min: 0.35, tilt_max: 0.7 };

function actionKey(a: ButtonAction): string {
  return "id" in a ? `${a.type}:${a.id}` : a.type;
}

function stringify(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e)
    return String((e as { message: unknown }).message);
  return String(e);
}

export function AutopilotView() {
  const t = useT();
  const audioBpmStart = useShowStore((s) => s.audioBpmStart);
  const show = useShowStore((s) => s.show);
  const persisted = show?.autopilot ?? null;

  const [draft, setDraftState] = useState<AutopilotConfig | null>(null);
  const draftRef = useRef<AutopilotConfig | null>(null);
  const dirtyRef = useRef(false);
  const saveTimer = useRef<number | null>(null);
  const [status, setStatus] = useState<AutopilotStatus | null>(null);
  const [rig, setRig] = useState<RigFixture[]>([]);
  const [toast, setToast] = useState<string | null>(null);

  const setDraft = useCallback((next: AutopilotConfig | null) => {
    draftRef.current = next;
    setDraftState(next);
  }, []);

  // The show is the source of truth; local edits win only until saved.
  useEffect(() => {
    if (!persisted) return;
    if (!dirtyRef.current) setDraft(persisted);
  }, [persisted, setDraft]);

  // Re-read the rig whenever the show changes (patch edits, mirror replication).
  useEffect(() => {
    if (!show) return;
    let alive = true;
    invoke<RigFixture[]>("autopilot_rig")
      .then((r) => {
        if (alive && Array.isArray(r)) setRig(r);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [show]);

  useEffect(() => {
    let alive = true;
    invoke<AutopilotStatus>("autopilot_status")
      .then((s) => {
        if (alive) setStatus(s);
      })
      .catch(() => {});
    const un = listen<AutopilotStatus>("autopilot:status", (e) => {
      if (alive) setStatus(e.payload);
    });
    return () => {
      alive = false;
      un.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(() => setToast(null), 4000);
    return () => window.clearTimeout(id);
  }, [toast]);

  const persist = useCallback((cfg: AutopilotConfig) => {
    invoke<AutopilotStatus>("autopilot_set_config", { config: cfg })
      .then((s) => {
        dirtyRef.current = false;
        setStatus(s);
      })
      .catch((e) => setToast(stringify(e)));
  }, []);

  const toggleCalibrate = useCallback(async () => {
    try {
      setStatus(
        await invoke<AutopilotStatus>("autopilot_set_calibrate", {
          on: !(status?.calibrating ?? false),
        }),
      );
    } catch (e) {
      setToast(stringify(e));
    }
  }, [status]);

  const update = useCallback(
    (patch: Partial<AutopilotConfig>) => {
      const cur = draftRef.current;
      if (!cur) return;
      const next = { ...cur, ...patch };
      dirtyRef.current = true;
      setDraft(next);
      if (saveTimer.current) window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => {
        saveTimer.current = null;
        persist(next);
      }, SAVE_DEBOUNCE_MS);
    },
    [persist, setDraft],
  );

  const toggleEnabled = useCallback(async () => {
    const cur = draftRef.current;
    if (!cur) return;
    const enabled = !cur.enabled;
    setDraft({ ...cur, enabled });
    try {
      setStatus(await invoke<AutopilotStatus>("autopilot_set_enabled", { enabled }));
    } catch (e) {
      setToast(stringify(e));
    }
  }, [setDraft]);

  const nudge = useCallback(async (kind: "next" | "back" | "hold") => {
    try {
      setStatus(await invoke<AutopilotStatus>("autopilot_nudge", { kind }));
    } catch (e) {
      setToast(stringify(e));
    }
  }, []);

  const pointHead = useCallback((fixtureId: string, pan: number, tilt: number) => {
    invoke("autopilot_point_head", { fixtureId, pan, tilt }).catch((e) => setToast(stringify(e)));
  }, []);

  // ---- derived ----
  const participants = useMemo(() => new Set(draft?.fixture_ids ?? []), [draft?.fixture_ids]);
  const groups = useMemo(() => {
    const by: Record<string, RigFixture[]> = { par: [], mover: [], strobe: [], other: [] };
    for (const f of rig) by[f.kind].push(f);
    return by;
  }, [rig]);
  const movers = useMemo(
    () => rig.filter((f) => f.kind === "mover" && participants.has(f.id)),
    [rig, participants],
  );
  const vocabCandidates = useMemo(() => {
    if (!show) return [];
    const scenes = show.scenes.map((s) => ({
      key: `recall_scene:${s.id}`,
      name: s.name,
      kind: "scene" as const,
      action: { type: "recall_scene", id: s.id } as ButtonAction,
    }));
    const snaps = show.snapshots.map((s) => ({
      key: `toggle_snapshot:${s.id}`,
      name: s.name,
      kind: "snapshot" as const,
      action: { type: "toggle_snapshot", id: s.id } as ButtonAction,
    }));
    return [...scenes, ...snaps];
  }, [show]);
  const vocabByKey = useMemo(() => {
    const m = new Map<string, VocabEntry>();
    for (const v of draft?.vocabulary ?? []) m.set(actionKey(v.action), v);
    return m;
  }, [draft?.vocabulary]);

  const setParticipant = (id: string, on: boolean) => {
    const ids = new Set(participants);
    if (on) ids.add(id);
    else ids.delete(id);
    update({ fixture_ids: [...ids] });
  };
  const setGroup = (kind: string, on: boolean) => {
    const ids = new Set(participants);
    for (const f of groups[kind] ?? []) {
      if (on) ids.add(f.id);
      else ids.delete(f.id);
    }
    update({ fixture_ids: [...ids] });
  };
  const setVocab = (action: ButtonAction, band: EnergyBand | null, weight?: number) => {
    if (!draft) return;
    const key = actionKey(action);
    const rest = draft.vocabulary.filter((v) => actionKey(v.action) !== key);
    if (band === null) {
      update({ vocabulary: rest });
      return;
    }
    const prev = vocabByKey.get(key);
    update({
      vocabulary: [...rest, { action, energy: band, weight: weight ?? prev?.weight ?? 1 }],
    });
  };
  const zoneFor = (id: string): HeadZone =>
    draft?.head_zones.find((z) => z.fixture_id === id) ?? { fixture_id: id, ...DEFAULT_ZONE };
  const setZone = (zone: HeadZone) => {
    if (!draft) return;
    const rest = draft.head_zones.filter((z) => z.fixture_id !== zone.fixture_id);
    update({ head_zones: [...rest, zone] });
  };
  const copyZoneToAll = (zone: HeadZone) => {
    update({
      head_zones: movers.map((m) => ({ ...zone, fixture_id: m.id })),
    });
  };

  const enabled = draft?.enabled ?? false;
  const energyPct = Math.round((status?.energy ?? 0) * 100);
  const cadence = draft?.cadence_beats ?? 8;
  const changeProgress =
    status && cadence > 0 ? 1 - Math.min(1, status.next_change_in / cadence) : 0;
  const stateLine = !enabled
    ? t("magic.state.off")
    : status?.ducked_for && status.ducked_for > 0
      ? t("magic.state.ducked", { s: status.ducked_for.toFixed(0) })
      : participants.size === 0
        ? t("magic.state.noFixtures")
        : status?.calibrating
          ? t("magic.state.calibrating")
          : status?.silent
            ? t("magic.state.silent")
            : t("magic.state.on");
  const silent = enabled && (status?.silent ?? false);
  const fallbackNote = enabled && status?.signal === "fallback" ? t("magic.state.fallback") : null;

  const programLabel = (kind: "color" | "dim" | "mover", value: string) =>
    t(`magic.${kind}.${value}` as Parameters<typeof t>[0]);

  if (!draft) {
    return <main className="page magic-view" />;
  }

  return (
    <main className="page magic-view">
      {/* ---- Hero: the switch, the clock, the energy ---- */}
      <section className={`magic-hero${enabled ? " on" : ""}`}>
        <div className="magic-hero__switch">
          <button
            type="button"
            className={`magic-toggle${enabled ? " on" : ""}`}
            onClick={toggleEnabled}
            title={t("magic.toggleHint")}
          >
            <span className="magic-toggle__icon">{enabled ? "✨" : "○"}</span>
            <span className="magic-toggle__label">{t("magic.title")}</span>
          </button>
          <p className={`magic-hero__state${silent ? " silent" : ""}`}>{stateLine}</p>
          <p className="hint magic-hero__meta">
            {t("magic.participantsCount", {
              n: String(status?.participants ?? participants.size),
              m: String(status?.movers ?? movers.length),
            })}
          </p>
          {fallbackNote ? <p className="hint magic-hero__warn">{fallbackNote}</p> : null}
          {enabled && status?.signal_detail ? (
            <p className="hint magic-hero__signal">{status.signal_detail}</p>
          ) : null}
        </div>

        <div className="magic-hero__clock">
          <div className="magic-bpm">
            <strong>{status ? status.bpm.toFixed(1) : "—"}</strong>
            <span className="hint">
              BPM · {t(`magic.clock.${status?.clock_source ?? "free"}` as Parameters<typeof t>[0])}
            </span>
          </div>
          <div className="magic-dots" aria-hidden="true">
            {[1, 2, 3, 4].map((b) => (
              <span key={b} className={`magic-dot${status?.beat_in_bar === b ? " on" : ""}`} />
            ))}
          </div>
          <div className="magic-bars" aria-hidden="true">
            {[1, 2, 3, 4, 5, 6, 7, 8].map((b) => (
              <span
                key={b}
                className={`magic-bar${(status?.bar_in_phrase ?? 0) >= b ? " done" : ""}${
                  status?.bar_in_phrase === b ? " now" : ""
                }`}
              />
            ))}
          </div>
          <div className="magic-next">
            <div className="magic-next__track">
              <div className="magic-next__fill" style={{ width: `${changeProgress * 100}%` }} />
            </div>
            <span className="hint">
              {enabled && status
                ? t("magic.nextChange", { n: status.next_change_in.toFixed(0) })
                : t("magic.cadenceLabel", { n: String(cadence) })}
            </span>
          </div>
        </div>

        <div className="magic-hero__energy">
          <div className="magic-meter" title={`${energyPct}%`}>
            <div className="magic-meter__fill" style={{ height: `${energyPct}%` }} />
          </div>
          <div className="magic-energy__text">
            <strong>{energyPct}%</strong>
            <span className={`magic-band magic-band--${status?.energy_band ?? "mid"}`}>
              {t(`magic.band.${status?.energy_band ?? "mid"}` as Parameters<typeof t>[0])}
            </span>
            <span className="hint">
              {t(`magic.energyFrom.${status?.energy_from ?? "manual"}` as Parameters<typeof t>[0])}
            </span>
          </div>
        </div>
      </section>

      {/* ---- Now playing ---- */}
      <section className="card magic-now">
        <div className="magic-now__main">
          <h3>{t("magic.now.title")}</h3>
          {status?.look ? (
            <LookCard look={status.look} big programLabel={programLabel} t={t} />
          ) : (
            <p className="hint">{enabled ? t("magic.now.waiting") : t("magic.now.off")}</p>
          )}
          <div className="magic-nudge">
            <button type="button" onClick={() => nudge("back")} disabled={!enabled}>
              ◀ {t("magic.nudge.back")}
            </button>
            <button type="button" onClick={() => nudge("hold")} disabled={!enabled}>
              {t("magic.nudge.hold")}
            </button>
            <button
              type="button"
              className="primary"
              onClick={() => nudge("next")}
              disabled={!enabled}
            >
              {t("magic.nudge.next")} ▶
            </button>
          </div>
        </div>
        <div className="magic-now__history">
          <h3>{t("magic.now.history")}</h3>
          {status?.history?.length ? (
            <div className="magic-history">
              {status.history.map((h, i) => (
                <LookCard key={`${h.name}-${i}`} look={h} programLabel={programLabel} t={t} />
              ))}
            </div>
          ) : (
            <p className="hint">{t("magic.now.noHistory")}</p>
          )}
        </div>
      </section>

      <div className="magic-grid">
        {/* ---- Participants ---- */}
        <section className="card magic-card">
          <h3>{t("magic.who.title")}</h3>
          <p className="hint">{t("magic.who.hint")}</p>
          {rig.length === 0 ? <p className="hint">{t("magic.who.empty")}</p> : null}
          {(["par", "mover", "strobe", "other"] as const).map((kind) =>
            groups[kind].length === 0 ? null : (
              <div key={kind} className="magic-group">
                <div className="magic-group__head">
                  <span>
                    {t(`magic.kind.${kind}` as Parameters<typeof t>[0])}{" "}
                    <span className="hint">
                      {groups[kind].filter((f) => participants.has(f.id)).length}/
                      {groups[kind].length}
                    </span>
                  </span>
                  <span className="magic-group__tools">
                    <button type="button" onClick={() => setGroup(kind, true)}>
                      {t("magic.who.all")}
                    </button>
                    <button type="button" onClick={() => setGroup(kind, false)}>
                      {t("magic.who.none")}
                    </button>
                  </span>
                </div>
                <div className="magic-chips">
                  {groups[kind].map((f) => (
                    <button
                      key={f.id}
                      type="button"
                      className={`magic-chip${participants.has(f.id) ? " on" : ""}`}
                      onClick={() => setParticipant(f.id, !participants.has(f.id))}
                    >
                      {f.label}
                    </button>
                  ))}
                </div>
              </div>
            ),
          )}
        </section>

        {/* ---- How it improvises ---- */}
        <section className="card magic-card">
          <h3>{t("magic.how.title")}</h3>
          <div className="magic-field">
            <span className="magic-field__label">{t("magic.how.cadence")}</span>
            <div className="magic-seg">
              {CADENCES.map((c) => (
                <button
                  key={c}
                  type="button"
                  className={draft.cadence_beats === c ? "active" : ""}
                  onClick={() => update({ cadence_beats: c })}
                >
                  {c}
                </button>
              ))}
            </div>
            <span className="hint">{t("magic.how.cadenceHint")}</span>
          </div>
          <label className="magic-field">
            <span className="magic-field__label">
              {t("magic.how.variety")} <span className="hint">{t("magic.how.varietyScale")}</span>
            </span>
            <input
              type="range"
              min={0}
              max={1}
              step={0.05}
              value={draft.variety}
              onChange={(e) => update({ variety: Number(e.currentTarget.value) })}
            />
          </label>
          <div className="magic-field">
            <span className="magic-field__label">{t("magic.how.palette")}</span>
            <div className="magic-palettes">
              {PALETTES.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  className={`magic-palette${draft.palette === p.id ? " active" : ""}`}
                  onClick={() => update({ palette: p.id })}
                >
                  <span className="magic-palette__swatches">
                    {p.swatches.map((c) => (
                      <span key={c} style={{ background: c }} />
                    ))}
                  </span>
                  <span>{t(`magic.palette.${p.id}` as Parameters<typeof t>[0])}</span>
                </button>
              ))}
            </div>
          </div>
          <div className="magic-field">
            <span className="magic-field__label">{t("magic.how.energy")}</span>
            <div className="magic-seg">
              {(["manual", "os2l", "mic"] as EnergySource[]).map((s) => (
                <button
                  key={s}
                  type="button"
                  className={draft.energy_source === s ? "active" : ""}
                  onClick={() => {
                    update({ energy_source: s });
                    // Listening needs the audio input running.
                    if (s === "mic") audioBpmStart(null).catch(() => {});
                  }}
                >
                  {t(`magic.energyFrom.${s}` as Parameters<typeof t>[0])}
                </button>
              ))}
            </div>
            <input
              type="range"
              min={0}
              max={1}
              step={0.05}
              value={draft.manual_energy}
              onChange={(e) => update({ manual_energy: Number(e.currentTarget.value) })}
              title={t("magic.how.manualEnergy")}
            />
            <div className="magic-seg magic-presets">
              {(
                [
                  ["low", 0.25],
                  ["mid", 0.55],
                  ["high", 0.85],
                ] as const
              ).map(([k, v]) => (
                <button
                  key={k}
                  type="button"
                  className={Math.abs(draft.manual_energy - v) < 0.02 ? "active" : ""}
                  onClick={() => update({ energy_source: "manual", manual_energy: v })}
                >
                  {t(`magic.how.preset.${k}` as Parameters<typeof t>[0])}
                </button>
              ))}
            </div>
            {draft.energy_source !== "manual" ? (
              <label className="magic-gain" title={t("magic.how.gainHint")}>
                <span>{t("magic.how.gain")}</span>
                <input
                  type="range"
                  min={0.5}
                  max={2.5}
                  step={0.05}
                  value={draft.energy_gain}
                  onChange={(e) => update({ energy_gain: Number(e.currentTarget.value) })}
                />
                <strong>×{draft.energy_gain.toFixed(2)}</strong>
              </label>
            ) : null}
            <span className="hint">{t("magic.how.energyHint")}</span>
          </div>
          <div className="magic-toggles">
            <label className="magic-toggle-row">
              <input
                type="checkbox"
                checked={draft.allow_movement}
                onChange={(e) => update({ allow_movement: e.currentTarget.checked })}
              />
              {t("magic.how.movement")}
            </label>
            <label className="magic-toggle-row">
              <input
                type="checkbox"
                checked={draft.allow_strobe}
                onChange={(e) => update({ allow_strobe: e.currentTarget.checked })}
              />
              {t("magic.how.strobe")}
            </label>
            <label className="magic-toggle-row">
              <input
                type="checkbox"
                checked={draft.allow_drops}
                onChange={(e) => update({ allow_drops: e.currentTarget.checked })}
              />
              {t("magic.how.drops")}
            </label>
            <label className="magic-toggle-row" title={t("magic.how.stopOnSilenceHint")}>
              <input
                type="checkbox"
                checked={draft.stop_on_silence}
                onChange={(e) => update({ stop_on_silence: e.currentTarget.checked })}
              />
              {t("magic.how.stopOnSilence")}
            </label>
          </div>
          <label className="magic-field">
            <span className="magic-field__label">
              {t("magic.how.floor")} <span className="hint">{Math.round(draft.floor * 100)}%</span>
            </span>
            <input
              type="range"
              min={0}
              max={0.5}
              step={0.01}
              value={draft.floor}
              onChange={(e) => update({ floor: Number(e.currentTarget.value) })}
            />
          </label>
          <label className="magic-field">
            <span className="magic-field__label">
              {t("magic.how.maxLevel")}{" "}
              <span className="hint">{Math.round(draft.max_level * 100)}%</span>
            </span>
            <input
              type="range"
              min={0.2}
              max={1}
              step={0.01}
              value={draft.max_level}
              onChange={(e) => update({ max_level: Number(e.currentTarget.value) })}
            />
          </label>
          <label className="magic-field">
            <span className="magic-field__label">
              {t("magic.how.ducking")}{" "}
              <span className="hint">{draft.ducking_secs.toFixed(0)} s</span>
            </span>
            <input
              type="range"
              min={0}
              max={30}
              step={1}
              value={draft.ducking_secs}
              onChange={(e) => update({ ducking_secs: Number(e.currentTarget.value) })}
            />
          </label>
          <label className="magic-field" title={t("magic.how.latencyHint")}>
            <span className="magic-field__label">
              {t("magic.how.latency")}{" "}
              <span className="hint">
                {draft.beat_offset_ms > 0 ? "+" : ""}
                {Math.round(draft.beat_offset_ms)} ms
              </span>
            </span>
            <input
              type="range"
              min={-300}
              max={300}
              step={5}
              value={draft.beat_offset_ms}
              onChange={(e) => update({ beat_offset_ms: Number(e.currentTarget.value) })}
            />
            <span className="hint">{t("magic.how.latencyHint")}</span>
            <div className="magic-nudge">
              <button
                type="button"
                className={status?.calibrating ? "active" : ""}
                onClick={toggleCalibrate}
                disabled={!enabled}
              >
                {status?.calibrating ? t("magic.how.calibrateStop") : t("magic.how.calibrate")}
              </button>
              <span className="hint">{t("magic.how.calibrateHint")}</span>
            </div>
          </label>
        </section>

        {/* ---- Vocabulary ---- */}
        <section className="card magic-card">
          <h3>{t("magic.vocab.title")}</h3>
          <p className="hint">{t("magic.vocab.hint")}</p>
          <div className="magic-toggles">
            <label className="magic-toggle-row">
              <input
                type="checkbox"
                checked={draft.use_vocabulary}
                onChange={(e) => update({ use_vocabulary: e.currentTarget.checked })}
              />
              {t("magic.vocab.use")}
            </label>
            <label className="magic-toggle-row">
              <input
                type="checkbox"
                checked={draft.use_generator}
                onChange={(e) => update({ use_generator: e.currentTarget.checked })}
              />
              {t("magic.vocab.useGenerator")}
            </label>
          </div>
          {vocabCandidates.length === 0 ? (
            <p className="hint">{t("magic.vocab.empty")}</p>
          ) : (
            <ul className="magic-vocab">
              {vocabCandidates.map((c) => {
                const entry = vocabByKey.get(c.key);
                return (
                  <li key={c.key} className={entry ? "tagged" : ""}>
                    <span className="magic-vocab__name">
                      <span className="magic-vocab__kind">{c.kind === "scene" ? "🎬" : "📸"}</span>
                      {c.name}
                    </span>
                    <span className="magic-seg magic-seg--small">
                      {BANDS.map((b) => (
                        <button
                          key={b ?? "none"}
                          type="button"
                          className={(entry?.energy ?? null) === b ? "active" : ""}
                          onClick={() => setVocab(c.action, b)}
                        >
                          {b ? t(`magic.band.${b}` as Parameters<typeof t>[0]) : "—"}
                        </button>
                      ))}
                    </span>
                    {entry ? (
                      <input
                        type="range"
                        className="magic-vocab__weight"
                        min={0.2}
                        max={3}
                        step={0.1}
                        value={entry.weight}
                        onChange={(e) =>
                          setVocab(c.action, entry.energy, Number(e.currentTarget.value))
                        }
                        title={t("magic.vocab.weight")}
                      />
                    ) : null}
                  </li>
                );
              })}
            </ul>
          )}
        </section>

        {/* ---- Head zones ---- */}
        <section className="card magic-card magic-card--zones">
          <h3>{t("magic.zones.title")}</h3>
          <p className="hint">{t("magic.zones.hint")}</p>
          {movers.length === 0 ? (
            <p className="hint">{t("magic.zones.empty")}</p>
          ) : (
            <div className="magic-zones">
              {movers.map((m) => {
                const z = zoneFor(m.id);
                return (
                  <div key={m.id} className="magic-zone">
                    <div className="magic-zone__head">
                      <strong>{m.label}</strong>
                      <button
                        type="button"
                        onClick={() => copyZoneToAll(z)}
                        title={t("magic.zones.copyAllHint")}
                      >
                        {t("magic.zones.copyAll")}
                      </button>
                    </div>
                    <HeadZonePad
                      zone={z}
                      onChange={setZone}
                      onPoint={(p, tl) => pointHead(m.id, p, tl)}
                    />
                    <div className="magic-zone__test">
                      <span className="hint">{t("magic.zones.test")}</span>
                      <button type="button" onClick={() => pointHead(m.id, z.pan_min, z.tilt_min)}>
                        ↙
                      </button>
                      <button type="button" onClick={() => pointHead(m.id, z.pan_max, z.tilt_min)}>
                        ↘
                      </button>
                      <button
                        type="button"
                        onClick={() =>
                          pointHead(
                            m.id,
                            (z.pan_min + z.pan_max) / 2,
                            (z.tilt_min + z.tilt_max) / 2,
                          )
                        }
                      >
                        ·
                      </button>
                      <button type="button" onClick={() => pointHead(m.id, z.pan_min, z.tilt_max)}>
                        ↖
                      </button>
                      <button type="button" onClick={() => pointHead(m.id, z.pan_max, z.tilt_max)}>
                        ↗
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </section>
      </div>

      {toast ? <output className="app-toast">{toast}</output> : null}
    </main>
  );
}

function LookCard({
  look,
  big,
  programLabel,
  t,
}: {
  look: LookInfo;
  big?: boolean;
  programLabel: (kind: "color" | "dim" | "mover", value: string) => string;
  t: ReturnType<typeof useT>;
}) {
  return (
    <div className={`magic-look${big ? " magic-look--big" : ""}`}>
      <div className="magic-look__swatches">
        {Array.from(new Set(look.palette)).map((c) => (
          <span key={c} style={{ background: c }} />
        ))}
      </div>
      <div className="magic-look__text">
        <strong>{look.vocab_name ?? look.name}</strong>
        <span className="magic-look__chips">
          {look.vocab_name ? (
            <span className="magic-chip-mini vocab">{t("magic.now.vocabTag")}</span>
          ) : null}
          <span className="magic-chip-mini">{programLabel("color", look.color_mode)}</span>
          <span className="magic-chip-mini">{programLabel("dim", look.dim_program)}</span>
          <span className="magic-chip-mini">{programLabel("mover", look.mover_program)}</span>
          {look.strobe ? <span className="magic-chip-mini strobe">⚡</span> : null}
        </span>
      </div>
    </div>
  );
}
