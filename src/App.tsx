import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import "./App.css";
import { ChaserView } from "./components/ChaserView";
import { ConfigView } from "./components/ConfigView";
import { MovementView } from "./components/MovementView";
import { StageView } from "./components/StageView";
import { useShowStore } from "./stores/show";

type Tab = "stage" | "chaser" | "movement" | "config";

const TABS: { id: Tab; label: string }[] = [
  { id: "stage", label: "Stage" },
  { id: "chaser", label: "Chaser" },
  { id: "movement", label: "Movement" },
  { id: "config", label: "Config" },
];

function App() {
  const [tab, setTab] = useState<Tab>("stage");
  const refresh = useShowStore((s) => s.refresh);
  const initListeners = useShowStore((s) => s.initListeners);
  const newShow = useShowStore((s) => s.newShow);
  const openShow = useShowStore((s) => s.openShow);
  const saveShow = useShowStore((s) => s.saveShow);
  const showPath = useShowStore((s) => s.showPath);
  const showName = useShowStore((s) => s.show?.name ?? "Untitled");
  const blackoutActive = useShowStore((s) => s.show?.globals?.blackout.active ?? false);
  const setBlackout = useShowStore((s) => s.setBlackout);
  const setBlind = useShowStore((s) => s.setBlind);

  useEffect(() => {
    refresh();
    let unlisten: (() => void) | null = null;
    initListeners().then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, [refresh, initListeners]);

  // Safety net: if the user holds the Blind button and switches windows, the
  // pointer-up event may never reach us. Listening on the document keeps the
  // press-state honest no matter where the release happens.
  useEffect(() => {
    const release = () => setBlind(false);
    document.addEventListener("pointerup", release);
    document.addEventListener("pointercancel", release);
    window.addEventListener("blur", release);
    return () => {
      document.removeEventListener("pointerup", release);
      document.removeEventListener("pointercancel", release);
      window.removeEventListener("blur", release);
    };
  }, [setBlind]);

  const onOpen = async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "DMX Show", extensions: ["json"] }],
    });
    if (typeof picked === "string") {
      await openShow(picked);
    }
  };

  const onSave = async () => {
    if (showPath) {
      await saveShow();
      return;
    }
    const picked = await saveDialog({
      filters: [{ name: "DMX Show", extensions: ["json"] }],
      defaultPath: `${showName || "show"}.json`,
    });
    if (typeof picked === "string") {
      await saveShow(picked);
    }
  };

  return (
    <div className="app-root">
      <nav className="tabs">
        <div className="tabs-left">
          <span className="brand">DMX Control</span>
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className={`tab${tab === t.id ? " active" : ""}`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>
        <div className="tabs-globals">
          <button
            type="button"
            className={`global-btn blackout-btn${blackoutActive ? " active" : ""}`}
            onClick={() => setBlackout(!blackoutActive)}
            title="Blackout (toggle, fades configurables)"
          >
            BLACKOUT
          </button>
          <button
            type="button"
            className="global-btn blind-btn"
            onPointerDown={(e) => {
              e.preventDefault();
              e.currentTarget.setPointerCapture(e.pointerId);
              setBlind(true);
            }}
            onPointerUp={(e) => {
              try {
                e.currentTarget.releasePointerCapture(e.pointerId);
              } catch {}
              setBlind(false);
            }}
            onPointerLeave={() => setBlind(false)}
            onPointerCancel={() => setBlind(false)}
            title="Blind / blinder (mantené presionado, halógeno con fade in/out)"
          >
            BLIND
          </button>
        </div>
        <div className="tabs-right">
          <span className="show-name selectable" title={showPath ?? "(unsaved)"}>
            {showName}
            {showPath ? "" : " *"}
          </span>
          <button type="button" onClick={() => newShow()}>
            New
          </button>
          <button type="button" onClick={onOpen}>
            Open…
          </button>
          <button type="button" onClick={onSave}>
            Save
          </button>
        </div>
      </nav>
      <div className="tab-body">
        {tab === "stage" && <StageView />}
        {tab === "chaser" && <ChaserView />}
        {tab === "movement" && <MovementView />}
        {tab === "config" && <ConfigView />}
      </div>
    </div>
  );
}

export default App;
