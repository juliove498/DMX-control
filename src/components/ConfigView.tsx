import { useState } from "react";
import { AiConfigView } from "./AiConfigView";
import { ButtonsConfigView } from "./ButtonsConfigView";
import { DirectOutput } from "./DirectOutput";
import { LibraryView } from "./LibraryView";
import { MidiConfigView } from "./MidiConfigView";
import { OutputsView } from "./OutputsView";
import { PatchView } from "./PatchView";

type ConfigTab =
  | "library"
  | "outputs"
  | "patch"
  | "blackout-blind"
  | "midi"
  | "ai"
  | "direct";

// Grouped by setup flow: hardware → control surface → debug.
// Order inside each group reflects the order a fresh user would touch them
// (define fixtures → wire universes → patch addresses; then surface; debug last).
const GROUPS: Array<{ id: string; items: Array<{ id: ConfigTab; label: string; hint: string }> }> =
  [
    {
      id: "hardware",
      items: [
        { id: "library", label: "Library", hint: "Definiciones de fixtures" },
        { id: "outputs", label: "Outputs", hint: "Universos y drivers DMX" },
        { id: "patch", label: "Patch", hint: "Asignar fixtures a direcciones DMX" },
      ],
    },
    {
      id: "surface",
      items: [
        {
          id: "blackout-blind",
          label: "Blackout & Blind",
          hint: "Fades y fixtures afectados por los botones globales",
        },
        { id: "midi", label: "MIDI", hint: "Superficie de control (Launchpad, etc.)" },
        {
          id: "ai",
          label: "IA",
          hint: "Provider, modelo y API key para generación de escenas con LLM",
        },
      ],
    },
    {
      id: "debug",
      items: [
        { id: "direct", label: "Direct", hint: "Faders crudos por canal — herramienta de debug" },
      ],
    },
  ];

export function ConfigView() {
  // Default to Library: that's where a fresh setup starts. Existing users can
  // jump anywhere with a single click; not worth persisting last-used.
  const [sub, setSub] = useState<ConfigTab>("library");

  return (
    <main className="page config-view">
      <nav className="config-tabs">
        {GROUPS.map((group) => (
          <div key={group.id} className="config-tabs-group">
            {group.items.map((t) => (
              <button
                key={t.id}
                type="button"
                className={`config-tab-btn${sub === t.id ? " active" : ""}`}
                onClick={() => setSub(t.id)}
                title={t.hint}
              >
                {t.label}
              </button>
            ))}
          </div>
        ))}
      </nav>
      <div className="config-content">
        {sub === "library" && <LibraryView />}
        {sub === "outputs" && <OutputsView />}
        {sub === "patch" && <PatchView />}
        {sub === "blackout-blind" && <ButtonsConfigView />}
        {sub === "midi" && <MidiConfigView />}
        {sub === "ai" && <AiConfigView />}
        {sub === "direct" && <DirectOutput />}
      </div>
    </main>
  );
}
