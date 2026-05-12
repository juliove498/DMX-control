import { useState } from "react";
import { useT } from "../i18n";
import type { Translation } from "../i18n/translations";
import { AiConfigView } from "./AiConfigView";
import { ButtonBindingsView } from "./ButtonBindingsView";
import { ButtonsConfigView } from "./ButtonsConfigView";
import { DirectOutput } from "./DirectOutput";
import { GeneralView } from "./GeneralView";
import { LibraryView } from "./LibraryView";
import { MidiConfigView } from "./MidiConfigView";
import { OutputsView } from "./OutputsView";
import { PatchView } from "./PatchView";
import { RemoteBridgeView } from "./RemoteBridgeView";
import { StreamDeckConfigView } from "./StreamDeckConfigView";
import { SyncView } from "./SyncView";
import { VdjConfigView } from "./VdjConfigView";

type ConfigTab =
  | "library"
  | "outputs"
  | "patch"
  | "blackout-blind"
  | "bindings"
  | "midi"
  | "streamdeck"
  | "ai"
  | "remote"
  | "sync"
  | "vdj"
  | "general"
  | "direct";

// Tab metadata uses translation keys instead of literal strings; the
// labels resolve at render time so a language flip immediately repaints
// without remounting. Grouped by setup flow: hardware → surface → app
// preferences → debug. Order inside each group reflects the order a
// fresh user would touch them.
type TabMeta = { id: ConfigTab; labelKey: keyof Translation; hintKey: keyof Translation };
const GROUPS: Array<{ id: string; items: TabMeta[] }> = [
  {
    id: "hardware",
    items: [
      { id: "library", labelKey: "config.tabs.library", hintKey: "config.tabs.libraryHint" },
      { id: "outputs", labelKey: "config.tabs.outputs", hintKey: "config.tabs.outputsHint" },
      { id: "patch", labelKey: "config.tabs.patch", hintKey: "config.tabs.patchHint" },
    ],
  },
  {
    id: "surface",
    items: [
      {
        id: "blackout-blind",
        labelKey: "config.tabs.blackoutBlind",
        hintKey: "config.tabs.blackoutBlindHint",
      },
      {
        id: "bindings",
        labelKey: "config.tabs.bindings",
        hintKey: "config.tabs.bindingsHint",
      },
      { id: "midi", labelKey: "config.tabs.midi", hintKey: "config.tabs.midiHint" },
      {
        id: "streamdeck",
        labelKey: "config.tabs.streamdeck",
        hintKey: "config.tabs.streamdeckHint",
      },
      { id: "ai", labelKey: "config.tabs.ai", hintKey: "config.tabs.aiHint" },
      { id: "remote", labelKey: "config.tabs.remote", hintKey: "config.tabs.remoteHint" },
      { id: "sync", labelKey: "config.tabs.sync", hintKey: "config.tabs.syncHint" },
      { id: "vdj", labelKey: "config.tabs.vdj", hintKey: "config.tabs.vdjHint" },
    ],
  },
  {
    id: "app",
    items: [{ id: "general", labelKey: "config.tabs.general", hintKey: "config.tabs.generalHint" }],
  },
  {
    id: "debug",
    items: [{ id: "direct", labelKey: "config.tabs.direct", hintKey: "config.tabs.directHint" }],
  },
];

export function ConfigView() {
  const t = useT();
  // Default to Library: that's where a fresh setup starts.
  const [sub, setSub] = useState<ConfigTab>("library");

  return (
    <main className="page config-view">
      <nav className="config-tabs">
        {GROUPS.map((group) => (
          <div key={group.id} className="config-tabs-group">
            {group.items.map((tab) => (
              <button
                key={tab.id}
                type="button"
                data-doc-config-tab={tab.id}
                className={`config-tab-btn${sub === tab.id ? " active" : ""}`}
                onClick={() => setSub(tab.id)}
                title={t(tab.hintKey)}
              >
                {t(tab.labelKey)}
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
        {sub === "bindings" && <ButtonBindingsView />}
        {sub === "midi" && <MidiConfigView />}
        {sub === "streamdeck" && <StreamDeckConfigView />}
        {sub === "ai" && <AiConfigView />}
        {sub === "remote" && <RemoteBridgeView />}
        {sub === "sync" && <SyncView />}
        {sub === "vdj" && <VdjConfigView />}
        {sub === "general" && <GeneralView />}
        {sub === "direct" && <DirectOutput />}
      </div>
    </main>
  );
}
