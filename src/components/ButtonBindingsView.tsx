import type { ButtonAction } from "@bindings/ButtonAction";
import type { ButtonActiveMode } from "@bindings/ButtonActiveMode";
import type { ButtonBindings } from "@bindings/ButtonBindings";
import type { ButtonIcon } from "@bindings/ButtonIcon";
import type { LaunchpadBinding } from "@bindings/LaunchpadBinding";
import type { StreamDeckBinding } from "@bindings/StreamDeckBinding";
import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { useShowStore } from "../stores/show";

// Launchpad MK2 grid layout. The hardware addresses pads with two-digit
// "yx" notes — row 1 (top) starts at 81, row 8 (bottom) at 11. We render
// rows top-to-bottom in the same orientation the operator sees on the
// device so muscle memory transfers from the physical surface to the
// editor and back.
const LP_NOTE_ROWS: number[][] = [
  [81, 82, 83, 84, 85, 86, 87, 88, 89],
  [71, 72, 73, 74, 75, 76, 77, 78, 79],
  [61, 62, 63, 64, 65, 66, 67, 68, 69],
  [51, 52, 53, 54, 55, 56, 57, 58, 59],
  [41, 42, 43, 44, 45, 46, 47, 48, 49],
  [31, 32, 33, 34, 35, 36, 37, 38, 39],
  [21, 22, 23, 24, 25, 26, 27, 28, 29],
  [11, 12, 13, 14, 15, 16, 17, 18, 19],
];
// Top-row CCs sit above the grid; we expose them in their own strip.
const LP_TOP_CCS: number[] = [104, 105, 106, 107, 108, 109, 110, 111];

const SD_KEY_ROWS: number[][] = [
  [0, 1, 2, 3, 4],
  [5, 6, 7, 8, 9],
  [10, 11, 12, 13, 14],
];

const EMPTY_BINDINGS: ButtonBindings = {
  custom_enabled: false,
  launchpad: [],
  streamdeck: [],
};

// Approximate the Launchpad MK2 palette index to a screen RGB so the
// preview grid hints at what the LED will look like on the hardware.
// Not a perfect match — the MK2 has 128 specific colours — but enough
// for the operator to tell red from blue from "dark" at a glance.
function lpPaletteToRgb(idx: number): string {
  if (idx === 0) return "#1c1c1c";
  // Hand-tuned tiers that match the bands we use in the factory layout.
  // Falls back to a hashed hue for unknown indices so the user still
  // sees *some* colour when they pick one outside our defaults.
  const swatches: Record<number, string> = {
    1: "#3a3a3a", 3: "#dddddd",
    5: "#ff3b3b", 7: "#7a0c0c",
    9: "#ff8a30", 11: "#7a3a0a",
    13: "#ffe640", 15: "#7a6e10",
    17: "#37d137", 19: "#0e5a0e",
    33: "#36c8c8", 37: "#56e8e8", 39: "#1a5454",
    41: "#3aa1ff", 43: "#103a66",
    45: "#3a5cff", 47: "#0e1a55",
    49: "#9a36c8", 50: "#b85ce8",
    53: "#ff39c2", 54: "#5a0e3e",
    55: "#ff5fb7", 56: "#7e3055",
    57: "#ffb6c1", 60: "#88e08c",
    61: "#cefac7", 73: "#92e63a",
    74: "#3e6e10", 78: "#3ec880",
    79: "#7adda0", 81: "#9a3ad1",
    82: "#502070", 84: "#ffae33", 85: "#7a4c12",
    95: "#ff7aaf", 96: "#5e2a3e",
    99: "#9ab1ff", 100: "#42548a",
    113: "#a86aff", 114: "#c891ff",
    115: "#ff7da6", 116: "#ffa9bf",
    117: "#3ea6c2",
  };
  if (swatches[idx]) return swatches[idx];
  const h = (idx * 13) % 360;
  return `hsl(${h}deg, 60%, 40%)`;
}

function rgbToCss(rgb: [number, number, number]): string {
  return `rgb(${rgb[0]}, ${rgb[1]}, ${rgb[2]})`;
}

// ---- Action helpers ----------------------------------------------------

/// Coarse "kind" of an action, used as a single dropdown value in the
/// editor. Each kind maps to one or more ButtonAction variants
/// depending on whether an id/index parameter is needed.
type ActionKind =
  | "none"
  | "toggle_chaser"
  | "toggle_movement"
  | "recall_scene"
  | "start_loop_group"
  | "blackout"
  | "blind"
  | "tap"
  | "toggle_overall_bpm"
  | "bump_bpm"
  | "stop_loop_group";

const ACTION_KIND_LABELS: Record<ActionKind, string> = {
  none: "Sin acción",
  toggle_chaser: "Toggle chaser",
  toggle_movement: "Toggle movimiento",
  recall_scene: "Llamar secuencia",
  start_loop_group: "Iniciar lista loop",
  blackout: "Blackout",
  blind: "Blind (momentáneo)",
  tap: "TAP tempo",
  toggle_overall_bpm: "Toggle BPM global",
  bump_bpm: "Subir/bajar BPM del chaser activo",
  stop_loop_group: "Detener lista loop",
};

function actionToKind(a: ButtonAction): ActionKind {
  switch (a.type) {
    case "none":
      return "none";
    case "toggle_chaser":
    case "toggle_chaser_by_index":
      return "toggle_chaser";
    case "toggle_movement":
    case "toggle_movement_by_index":
      return "toggle_movement";
    case "recall_scene":
    case "recall_scene_by_index":
      return "recall_scene";
    case "start_loop_group":
    case "start_loop_group_by_index":
      return "start_loop_group";
    case "blackout":
      return "blackout";
    case "blind":
      return "blind";
    case "tap":
      return "tap";
    case "toggle_overall_bpm":
      return "toggle_overall_bpm";
    case "bump_active_chaser_bpm":
      return "bump_bpm";
    case "stop_loop_group":
      return "stop_loop_group";
  }
}

function actionParamId(a: ButtonAction): string | null {
  if (
    a.type === "toggle_chaser" ||
    a.type === "toggle_movement" ||
    a.type === "recall_scene" ||
    a.type === "start_loop_group"
  ) {
    return a.id;
  }
  return null;
}

function actionParamIndex(a: ButtonAction): number | null {
  if (
    a.type === "toggle_chaser_by_index" ||
    a.type === "toggle_movement_by_index" ||
    a.type === "recall_scene_by_index" ||
    a.type === "start_loop_group_by_index"
  ) {
    return a.index;
  }
  return null;
}

function buildAction(
  kind: ActionKind,
  modeByIndex: boolean,
  id: string,
  index: number,
  delta: number,
): ButtonAction {
  switch (kind) {
    case "none":
      return { type: "none" };
    case "toggle_chaser":
      return modeByIndex
        ? { type: "toggle_chaser_by_index", index }
        : { type: "toggle_chaser", id };
    case "toggle_movement":
      return modeByIndex
        ? { type: "toggle_movement_by_index", index }
        : { type: "toggle_movement", id };
    case "recall_scene":
      return modeByIndex
        ? { type: "recall_scene_by_index", index }
        : { type: "recall_scene", id };
    case "start_loop_group":
      return modeByIndex
        ? { type: "start_loop_group_by_index", index }
        : { type: "start_loop_group", id };
    case "blackout":
      return { type: "blackout" };
    case "blind":
      return { type: "blind" };
    case "tap":
      return { type: "tap" };
    case "toggle_overall_bpm":
      return { type: "toggle_overall_bpm" };
    case "bump_bpm":
      return { type: "bump_active_chaser_bpm", delta };
    case "stop_loop_group":
      return { type: "stop_loop_group" };
  }
}

function summariseAction(
  a: ButtonAction,
  refs: {
    chasers: { id: string; name: string }[];
    movements: { id: string; name: string }[];
    scenes: { id: string; name: string }[];
    loops: { id: string; name: string }[];
  },
): string {
  const nameFor = (
    list: { id: string; name: string }[],
    id: string,
  ): string => list.find((x) => x.id === id)?.name ?? "?";
  switch (a.type) {
    case "none":
      return "—";
    case "toggle_chaser":
      return `Chaser → ${nameFor(refs.chasers, a.id)}`;
    case "toggle_chaser_by_index":
      return `Chaser #${a.index + 1}`;
    case "toggle_movement":
      return `Mov → ${nameFor(refs.movements, a.id)}`;
    case "toggle_movement_by_index":
      return `Mov #${a.index + 1}`;
    case "recall_scene":
      return `Sec → ${nameFor(refs.scenes, a.id)}`;
    case "recall_scene_by_index":
      return `Sec #${a.index + 1}`;
    case "start_loop_group":
      return `Loop → ${nameFor(refs.loops, a.id)}`;
    case "start_loop_group_by_index":
      return `Loop #${a.index + 1}`;
    case "blackout":
      return "Blackout";
    case "blind":
      return "Blind";
    case "tap":
      return "TAP";
    case "toggle_overall_bpm":
      return "BPM global";
    case "bump_active_chaser_bpm":
      return `BPM ${a.delta >= 0 ? "+" : ""}${a.delta}`;
    case "stop_loop_group":
      return "Detener loop";
  }
}

const ICON_OPTIONS: { value: ButtonIcon; label: string }[] = [
  { value: "none", label: "Sin icono" },
  { value: "play", label: "▶ Play" },
  { value: "stage", label: "▮▮ Escenas" },
  { value: "orbit", label: "◯ Movimiento" },
  { value: "bolt", label: "⚡ Rayo" },
  { value: "eye", label: "👁 Ojo" },
  { value: "tap", label: "● Tap" },
  { value: "metronome", label: "⏱ Metrónomo" },
  { value: "loop", label: "↻ Loop" },
];

// ---- Top-level view ---------------------------------------------------

export function ButtonBindingsView() {
  const show = useShowStore((s) => s.show);
  const getButtonBindings = useShowStore((s) => s.getButtonBindings);
  const updateButtonBindings = useShowStore((s) => s.updateButtonBindings);
  const getDefaultButtonBindings = useShowStore((s) => s.getDefaultButtonBindings);

  const [bindings, setBindings] = useState<ButtonBindings>(EMPTY_BINDINGS);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<"lp" | "sd">("lp");
  const [editing, setEditing] = useState<
    | { kind: "lp"; note: number; is_cc: boolean }
    | { kind: "sd"; key: number }
    | null
  >(null);

  // Load current bindings on mount. Subsequent edits go through
  // commit() below, which both updates local state AND persists.
  // We deliberately don't subscribe to show?.button_bindings here:
  // commit() already sets state locally, and a re-fetch on every show
  // refresh would flicker the modal while the user is editing.
  useEffect(() => {
    let cancelled = false;
    getButtonBindings()
      .then((b) => {
        if (!cancelled) setBindings(b);
      })
      .catch((e) => setError(stringifyError(e)));
    return () => {
      cancelled = true;
    };
  }, [getButtonBindings]);

  if (!show) {
    return <main className="page">Cargando…</main>;
  }

  const refs = {
    chasers: show.chasers.map((c) => ({ id: c.id, name: c.name })),
    movements: show.movements.map((m) => ({ id: m.id, name: m.name })),
    scenes: show.scenes.map((s) => ({ id: s.id, name: s.name })),
    loops: (show.scene_loop_groups ?? []).map((g) => ({
      id: g.id,
      name: g.name,
    })),
  };

  const commit = async (next: ButtonBindings) => {
    setError(null);
    setBindings(next);
    try {
      await updateButtonBindings(next);
    } catch (e) {
      setError(stringifyError(e));
    }
  };

  const loadDefaults = async () => {
    // Only warn if there's something to lose — a fresh show with no
    // customisations yet should just absorb the defaults silently.
    // Use Tauri's native `ask` (plugin-dialog) instead of window.confirm
    // because the Tauri webview returns true without prompting for the
    // browser-level dialog APIs.
    const hasCustom =
      bindings.launchpad.length > 0 || bindings.streamdeck.length > 0;
    if (hasCustom) {
      const ok = await ask(
        `Esto va a reemplazar todas tus asignaciones actuales por el layout de fábrica:\n• ${bindings.launchpad.length} botones del Launchpad\n• ${bindings.streamdeck.length} botones del Stream Deck\n\n¿Continuar?`,
        { title: "Cargar layout de fábrica", kind: "warning" },
      );
      if (!ok) return;
    }
    try {
      const defaults = await getDefaultButtonBindings();
      await commit({ ...defaults, custom_enabled: true });
    } catch (e) {
      setError(stringifyError(e));
    }
  };

  const toggleCustom = async (enabled: boolean) => {
    await commit({ ...bindings, custom_enabled: enabled });
  };

  const upsertLp = (binding: LaunchpadBinding) => {
    const next = bindings.launchpad.filter(
      (b) => !(b.note === binding.note && b.is_cc === binding.is_cc),
    );
    next.push(binding);
    commit({ ...bindings, launchpad: next });
  };
  const removeLp = (note: number, is_cc: boolean) => {
    commit({
      ...bindings,
      launchpad: bindings.launchpad.filter(
        (b) => !(b.note === note && b.is_cc === is_cc),
      ),
    });
  };
  const upsertSd = (binding: StreamDeckBinding) => {
    const next = bindings.streamdeck.filter((b) => b.key !== binding.key);
    next.push(binding);
    commit({ ...bindings, streamdeck: next });
  };
  const removeSd = (key: number) => {
    commit({
      ...bindings,
      streamdeck: bindings.streamdeck.filter((b) => b.key !== key),
    });
  };

  const lpForNote = (note: number, is_cc: boolean) =>
    bindings.launchpad.find((b) => b.note === note && b.is_cc === is_cc) ?? null;
  const sdForKey = (key: number) =>
    bindings.streamdeck.find((b) => b.key === key) ?? null;

  return (
    <main className="page bindings-view">
      <header className="page-head">
        <h2>Botones del Launchpad y Stream Deck</h2>
        <div className="actions">
          <label className="bindings-custom-toggle">
            <input
              type="checkbox"
              checked={bindings.custom_enabled}
              onChange={(e) => toggleCustom(e.currentTarget.checked)}
            />
            Layout personalizado
          </label>
          <button type="button" onClick={loadDefaults}>
            Cargar layout de fábrica
          </button>
        </div>
      </header>

      <p className="hint">
        {bindings.custom_enabled
          ? "Modo personalizado activado: cada botón usa la lista de abajo. Si no hay asignación, el botón queda apagado."
          : "Layout de fábrica en uso. Activá Layout personalizado para reasignar botones, colores e iconos."}
      </p>

      {error ? (
        <output className="lib-error" aria-live="polite">
          {error}
        </output>
      ) : null}

      <nav className="bindings-tabs">
        <button
          type="button"
          className={`bindings-tab${activeTab === "lp" ? " active" : ""}`}
          onClick={() => setActiveTab("lp")}
        >
          Launchpad
        </button>
        <button
          type="button"
          className={`bindings-tab${activeTab === "sd" ? " active" : ""}`}
          onClick={() => setActiveTab("sd")}
        >
          Stream Deck
        </button>
      </nav>

      {activeTab === "lp" ? (
        <LaunchpadGridEditor
          bindings={bindings.launchpad}
          refs={refs}
          customEnabled={bindings.custom_enabled}
          onPick={(note, is_cc) => setEditing({ kind: "lp", note, is_cc })}
        />
      ) : (
        <StreamDeckGridEditor
          bindings={bindings.streamdeck}
          refs={refs}
          customEnabled={bindings.custom_enabled}
          onPick={(key) => setEditing({ kind: "sd", key })}
        />
      )}

      {editing?.kind === "lp" ? (
        <LpBindingModal
          note={editing.note}
          is_cc={editing.is_cc}
          binding={lpForNote(editing.note, editing.is_cc)}
          refs={refs}
          onClose={() => setEditing(null)}
          onSave={(b) => {
            upsertLp(b);
            setEditing(null);
          }}
          onRemove={() => {
            removeLp(editing.note, editing.is_cc);
            setEditing(null);
          }}
        />
      ) : null}

      {editing?.kind === "sd" ? (
        <SdBindingModal
          k={editing.key}
          binding={sdForKey(editing.key)}
          refs={refs}
          onClose={() => setEditing(null)}
          onSave={(b) => {
            upsertSd(b);
            setEditing(null);
          }}
          onRemove={() => {
            removeSd(editing.key);
            setEditing(null);
          }}
        />
      ) : null}
    </main>
  );
}

// ---- Launchpad grid ----------------------------------------------------

function LaunchpadGridEditor({
  bindings,
  refs,
  customEnabled,
  onPick,
}: {
  bindings: LaunchpadBinding[];
  refs: BindingRefs;
  customEnabled: boolean;
  onPick: (note: number, is_cc: boolean) => void;
}) {
  const byNote = useMemo(() => {
    const map = new Map<string, LaunchpadBinding>();
    for (const b of bindings) map.set(`${b.is_cc ? "cc" : "n"}-${b.note}`, b);
    return map;
  }, [bindings]);

  return (
    <section className="bindings-section">
      <h3>Launchpad MK2 ({bindings.length} {bindings.length === 1 ? "botón" : "botones"})</h3>
      {!customEnabled ? (
        <p className="hint warn">
          Las asignaciones de abajo se guardan, pero solo se aplican cuando activás <em>Layout
          personalizado</em>. Mientras tanto el Launchpad usa el layout de fábrica.
        </p>
      ) : null}
      <div className="lp-grid-wrap">
        {/* Top row of round CC buttons — same shape as the MK2 hardware. */}
        <div className="lp-top-row">
          {LP_TOP_CCS.map((cc) => (
            <LpCell
              key={`cc-${cc}`}
              note={cc}
              is_cc
              shape="round"
              binding={byNote.get(`cc-${cc}`) ?? null}
              refs={refs}
              onPick={() => onPick(cc, true)}
            />
          ))}
        </div>
        <div className="lp-grid">
          {LP_NOTE_ROWS.map((row) => (
            <div key={`row-${row[0]}`} className="lp-grid-row">
              {row.map((n, ci) => {
                // Last column on the MK2 is the "scene" column —
                // round buttons that physically read different from
                // the 8×8 square pad grid.
                const shape: "square" | "round" =
                  ci === row.length - 1 ? "round" : "square";
                return (
                  <LpCell
                    key={`n-${n}`}
                    note={n}
                    is_cc={false}
                    shape={shape}
                    binding={byNote.get(`n-${n}`) ?? null}
                    refs={refs}
                    onPick={() => onPick(n, false)}
                  />
                );
              })}
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function LpCell({
  note,
  is_cc,
  shape,
  binding,
  refs,
  onPick,
}: {
  note: number;
  is_cc: boolean;
  shape: "square" | "round";
  binding: LaunchpadBinding | null;
  refs: BindingRefs;
  onPick: () => void;
}) {
  const dim = binding ? lpPaletteToRgb(binding.color_dim) : "#171717";
  const bright = binding ? lpPaletteToRgb(binding.color_bright) : "#171717";
  const label = binding?.label || "";
  const summary = binding ? summariseAction(binding.action, refs) : "";
  // Round buttons (top row + right column on the MK2) get a radial
  // gradient so the dim/bright colours form a soft halo rather than
  // the diagonal split used for the square grid pads. Reads more like
  // an LED dot than a tile.
  const background =
    shape === "round"
      ? `radial-gradient(circle at 35% 30%, ${bright}, ${dim} 75%)`
      : `linear-gradient(135deg, ${dim} 0%, ${dim} 55%, ${bright} 100%)`;
  return (
    <button
      type="button"
      className={`lp-cell lp-cell--${shape}${binding ? " bound" : ""}`}
      style={{ background }}
      onClick={onPick}
      title={
        binding
          ? `${is_cc ? "CC" : "Note"} ${note}\n${label}\n${summary}`
          : `${is_cc ? "CC" : "Note"} ${note} (libre)`
      }
    >
      <span className="lp-cell-num">{note}</span>
      {binding ? <span className="lp-cell-label">{label || summary}</span> : null}
    </button>
  );
}

// ---- Stream Deck grid --------------------------------------------------

function StreamDeckGridEditor({
  bindings,
  refs,
  customEnabled,
  onPick,
}: {
  bindings: StreamDeckBinding[];
  refs: BindingRefs;
  customEnabled: boolean;
  onPick: (key: number) => void;
}) {
  const byKey = useMemo(() => {
    const map = new Map<number, StreamDeckBinding>();
    for (const b of bindings) map.set(b.key, b);
    return map;
  }, [bindings]);

  return (
    <section className="bindings-section">
      <h3>Stream Deck MK2 ({bindings.length} {bindings.length === 1 ? "botón" : "botones"})</h3>
      {!customEnabled ? (
        <p className="hint warn">
          Activá <em>Layout personalizado</em> para que el Stream Deck use estas asignaciones.
        </p>
      ) : null}
      <div className="sd-grid">
        {SD_KEY_ROWS.map((row) => (
          <div key={`row-${row[0]}`} className="sd-grid-row">
            {row.map((k) => (
              <SdCell
                key={k}
                k={k}
                binding={byKey.get(k) ?? null}
                refs={refs}
                onPick={() => onPick(k)}
              />
            ))}
          </div>
        ))}
      </div>
    </section>
  );
}

function SdCell({
  k,
  binding,
  refs,
  onPick,
}: {
  k: number;
  binding: StreamDeckBinding | null;
  refs: BindingRefs;
  onPick: () => void;
}) {
  const off = binding ? rgbToCss(binding.color_off) : "#1a1a1a";
  const on = binding ? rgbToCss(binding.color_on) : "#1a1a1a";
  const label = binding?.label || "";
  const summary = binding ? summariseAction(binding.action, refs) : "";
  return (
    <button
      type="button"
      className={`sd-cell${binding ? " bound" : ""}`}
      style={{
        background: `linear-gradient(160deg, ${off} 0%, ${off} 55%, ${on} 100%)`,
      }}
      onClick={onPick}
      title={
        binding ? `Key ${k}\n${label}\n${summary}` : `Key ${k} (libre)`
      }
    >
      <span className="sd-cell-num">{k}</span>
      <span className="sd-cell-label">{label || summary}</span>
    </button>
  );
}

// ---- Edit modals -------------------------------------------------------

type BindingRefs = {
  chasers: { id: string; name: string }[];
  movements: { id: string; name: string }[];
  scenes: { id: string; name: string }[];
  loops: { id: string; name: string }[];
};

function LpBindingModal({
  note,
  is_cc,
  binding,
  refs,
  onClose,
  onSave,
  onRemove,
}: {
  note: number;
  is_cc: boolean;
  binding: LaunchpadBinding | null;
  refs: BindingRefs;
  onClose: () => void;
  onSave: (b: LaunchpadBinding) => void;
  onRemove: () => void;
}) {
  const seed: LaunchpadBinding =
    binding ?? {
      note,
      is_cc,
      action: { type: "none" },
      label: "",
      color_dim: 1,
      color_bright: 3,
      active_mode: "auto",
    };
  const [draft, setDraft] = useState(seed);
  return (
    <div className="bindings-modal-backdrop" onClick={onClose} onKeyDown={(e) => { if (e.key === "Escape") onClose(); }} role="presentation">
      <div className="bindings-modal" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()} role="presentation">
        <h3>
          Launchpad {is_cc ? "CC" : "Note"} {note}
        </h3>
        <label>
          Etiqueta
          <input
            type="text"
            value={draft.label}
            onChange={(e) => setDraft({ ...draft, label: e.currentTarget.value })}
          />
        </label>
        <ActionEditor
          action={draft.action}
          onChange={(action) => setDraft({ ...draft, action })}
          refs={refs}
        />
        <div className="bindings-modal-grid">
          <label>
            Color dim (idle)
            <input
              type="number"
              min={0}
              max={127}
              value={draft.color_dim}
              onChange={(e) =>
                setDraft({ ...draft, color_dim: clamp(Number(e.currentTarget.value), 0, 127) })
              }
            />
            <span
              className="lp-color-swatch"
              style={{ background: lpPaletteToRgb(draft.color_dim) }}
            />
          </label>
          <label>
            Color bright (activo)
            <input
              type="number"
              min={0}
              max={127}
              value={draft.color_bright}
              onChange={(e) =>
                setDraft({ ...draft, color_bright: clamp(Number(e.currentTarget.value), 0, 127) })
              }
            />
            <span
              className="lp-color-swatch"
              style={{ background: lpPaletteToRgb(draft.color_bright) }}
            />
          </label>
        </div>
        <p className="hint">
          Los índices de paleta van de 0 (apagado) a 127 — ver §11 del manual del Launchpad MK2.
        </p>
        <ActiveModeEditor
          mode={draft.active_mode}
          onChange={(active_mode) => setDraft({ ...draft, active_mode })}
        />
        <div className="bindings-modal-actions">
          {binding ? (
            <button type="button" className="danger" onClick={onRemove}>
              Eliminar asignación
            </button>
          ) : null}
          <span style={{ flex: 1 }} />
          <button type="button" onClick={onClose}>
            Cancelar
          </button>
          <button type="button" onClick={() => onSave(draft)}>
            Guardar
          </button>
        </div>
      </div>
    </div>
  );
}

function SdBindingModal({
  k,
  binding,
  refs,
  onClose,
  onSave,
  onRemove,
}: {
  k: number;
  binding: StreamDeckBinding | null;
  refs: BindingRefs;
  onClose: () => void;
  onSave: (b: StreamDeckBinding) => void;
  onRemove: () => void;
}) {
  const seed: StreamDeckBinding =
    binding ?? {
      key: k,
      action: { type: "none" },
      label: "",
      color_off: [40, 40, 40],
      color_on: [200, 200, 200],
      icon: "play",
      active_mode: "auto",
    };
  const [draft, setDraft] = useState(seed);
  return (
    <div className="bindings-modal-backdrop" onClick={onClose} onKeyDown={(e) => { if (e.key === "Escape") onClose(); }} role="presentation">
      <div className="bindings-modal" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()} role="presentation">
        <h3>Stream Deck — Tecla {k}</h3>
        <label>
          Etiqueta
          <input
            type="text"
            value={draft.label}
            onChange={(e) => setDraft({ ...draft, label: e.currentTarget.value })}
          />
        </label>
        <label>
          Icono
          <select
            value={draft.icon}
            onChange={(e) =>
              setDraft({ ...draft, icon: e.currentTarget.value as ButtonIcon })
            }
          >
            {ICON_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        <ActionEditor
          action={draft.action}
          onChange={(action) => setDraft({ ...draft, action })}
          refs={refs}
        />
        <div className="bindings-modal-grid">
          <RgbInput
            label="Color idle"
            value={draft.color_off}
            onChange={(v) => setDraft({ ...draft, color_off: v })}
          />
          <RgbInput
            label="Color activo"
            value={draft.color_on}
            onChange={(v) => setDraft({ ...draft, color_on: v })}
          />
        </div>
        <ActiveModeEditor
          mode={draft.active_mode}
          onChange={(active_mode) => setDraft({ ...draft, active_mode })}
        />
        <div className="bindings-modal-actions">
          {binding ? (
            <button type="button" className="danger" onClick={onRemove}>
              Eliminar asignación
            </button>
          ) : null}
          <span style={{ flex: 1 }} />
          <button type="button" onClick={onClose}>
            Cancelar
          </button>
          <button type="button" onClick={() => onSave(draft)}>
            Guardar
          </button>
        </div>
      </div>
    </div>
  );
}

function RgbInput({
  label,
  value,
  onChange,
}: {
  label: string;
  value: [number, number, number];
  onChange: (v: [number, number, number]) => void;
}) {
  const hex = `#${value.map((c) => clamp(c, 0, 255).toString(16).padStart(2, "0")).join("")}`;
  return (
    <label className="rgb-input">
      {label}
      <input
        type="color"
        value={hex}
        onChange={(e) => {
          const v = e.currentTarget.value;
          const r = Number.parseInt(v.slice(1, 3), 16);
          const g = Number.parseInt(v.slice(3, 5), 16);
          const b = Number.parseInt(v.slice(5, 7), 16);
          onChange([r, g, b]);
        }}
      />
      <code>{`${value[0]}, ${value[1]}, ${value[2]}`}</code>
    </label>
  );
}

function ActionEditor({
  action,
  onChange,
  refs,
}: {
  action: ButtonAction;
  onChange: (a: ButtonAction) => void;
  refs: BindingRefs;
}) {
  const kind = actionToKind(action);
  // Track id/index/delta as local state so switching between kinds
  // doesn't reset values the user has already typed.
  const [byIndex, setByIndex] = useState(actionParamIndex(action) !== null);
  const [id, setId] = useState(actionParamId(action) ?? "");
  const [index, setIndex] = useState(actionParamIndex(action) ?? 0);
  const [delta, setDelta] = useState(
    action.type === "bump_active_chaser_bpm" ? action.delta : 1,
  );

  useEffect(() => {
    setByIndex(actionParamIndex(action) !== null);
    setId(actionParamId(action) ?? "");
    setIndex(actionParamIndex(action) ?? 0);
    if (action.type === "bump_active_chaser_bpm") setDelta(action.delta);
  }, [action]);

  const emit = (
    nextKind: ActionKind,
    nextByIndex: boolean,
    nextId: string,
    nextIndex: number,
    nextDelta: number,
  ) => {
    onChange(buildAction(nextKind, nextByIndex, nextId, nextIndex, nextDelta));
  };

  const list =
    kind === "toggle_chaser"
      ? refs.chasers
      : kind === "toggle_movement"
        ? refs.movements
        : kind === "recall_scene"
          ? refs.scenes
          : kind === "start_loop_group"
            ? refs.loops
            : [];

  const needsTarget =
    kind === "toggle_chaser" ||
    kind === "toggle_movement" ||
    kind === "recall_scene" ||
    kind === "start_loop_group";

  return (
    <div className="action-editor">
      <label>
        Acción
        <select
          value={kind}
          onChange={(e) =>
            emit(e.currentTarget.value as ActionKind, byIndex, id, index, delta)
          }
        >
          {Object.entries(ACTION_KIND_LABELS).map(([v, l]) => (
            <option key={v} value={v}>
              {l}
            </option>
          ))}
        </select>
      </label>
      {needsTarget ? (
        <div className="action-target">
          <label>
            <input
              type="radio"
              checked={!byIndex}
              onChange={() => emit(kind, false, id, index, delta)}
            />
            Por nombre
          </label>
          <label>
            <input
              type="radio"
              checked={byIndex}
              onChange={() => emit(kind, true, id, index, delta)}
            />
            Por posición (#1, #2…)
          </label>
          {byIndex ? (
            <label>
              Posición
              <input
                type="number"
                min={1}
                max={64}
                value={index + 1}
                onChange={(e) => {
                  const nextIndex = clamp(Number(e.currentTarget.value) - 1, 0, 63);
                  setIndex(nextIndex);
                  emit(kind, true, id, nextIndex, delta);
                }}
              />
            </label>
          ) : (
            <label>
              Destino
              <select
                value={id}
                onChange={(e) => {
                  const v = e.currentTarget.value;
                  setId(v);
                  emit(kind, false, v, index, delta);
                }}
              >
                <option value="">— elegir —</option>
                {list.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.name}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>
      ) : null}
      {kind === "bump_bpm" ? (
        <label>
          Delta BPM
          <input
            type="number"
            min={-50}
            max={50}
            step={0.5}
            value={delta}
            onChange={(e) => {
              const v = Number(e.currentTarget.value);
              setDelta(v);
              emit(kind, byIndex, id, index, v);
            }}
          />
        </label>
      ) : null}
    </div>
  );
}

function ActiveModeEditor({
  mode,
  onChange,
}: {
  mode: ButtonActiveMode;
  onChange: (m: ButtonActiveMode) => void;
}) {
  return (
    <label>
      Estado activo
      <select
        value={mode}
        onChange={(e) => onChange(e.currentTarget.value as ButtonActiveMode)}
      >
        <option value="auto">Automático (según la acción)</option>
        <option value="always_idle">Siempre apagado/idle</option>
        <option value="always_active">Siempre encendido/activo</option>
      </select>
    </label>
  );
}

function clamp(n: number, min: number, max: number) {
  if (Number.isNaN(n)) return min;
  return Math.min(max, Math.max(min, Math.round(n)));
}

function stringifyError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return JSON.stringify(e);
}
