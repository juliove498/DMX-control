import { useState } from "react";
import { ButtonsConfigView } from "./ButtonsConfigView";
import { DirectOutput } from "./DirectOutput";
import { LibraryView } from "./LibraryView";
import { MidiConfigView } from "./MidiConfigView";
import { OutputsView } from "./OutputsView";
import { PatchView } from "./PatchView";

type ConfigTab = "direct" | "patch" | "outputs" | "library" | "buttons" | "midi";

const SUB_TABS: Array<{ id: ConfigTab; label: string; hint: string }> = [
  { id: "direct", label: "Direct", hint: "Per-channel raw faders for universe 0" },
  { id: "patch", label: "Patch", hint: "Assign fixtures to DMX addresses" },
  { id: "outputs", label: "Outputs", hint: "Universes and DMX drivers" },
  { id: "library", label: "Library", hint: "Fixture definitions" },
  { id: "buttons", label: "Buttons", hint: "Blackout & Blind fades + blind fixtures" },
  { id: "midi", label: "MIDI", hint: "Control surface (Launchpad, etc.)" },
];

export function ConfigView() {
  const [sub, setSub] = useState<ConfigTab>("patch");

  return (
    <main className="page config-view">
      <aside className="config-sidebar">
        <h3>Config</h3>
        <nav className="config-nav">
          {SUB_TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className={`config-nav-btn${sub === t.id ? " active" : ""}`}
              onClick={() => setSub(t.id)}
              title={t.hint}
            >
              <span className="config-nav-label">{t.label}</span>
              <span className="config-nav-hint">{t.hint}</span>
            </button>
          ))}
        </nav>
      </aside>
      <div className="config-content">
        {sub === "direct" && <DirectOutput />}
        {sub === "patch" && <PatchView />}
        {sub === "outputs" && <OutputsView />}
        {sub === "library" && <LibraryView />}
        {sub === "buttons" && <ButtonsConfigView />}
        {sub === "midi" && <MidiConfigView />}
      </div>
    </main>
  );
}
