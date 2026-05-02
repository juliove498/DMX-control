import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import "./App.css";
import { ChaserView } from "./components/ChaserView";
import { ConfigView } from "./components/ConfigView";
import { MovementView } from "./components/MovementView";
import { ScenesView } from "./components/ScenesView";
import { StageView } from "./components/StageView";
import { useShowStore } from "./stores/show";

type Tab = "stage" | "scenes" | "chaser" | "movement" | "config";

const TABS: { id: Tab; label: string }[] = [
  { id: "stage", label: "Stage" },
  { id: "scenes", label: "Scenes" },
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
  const renameShow = useShowStore((s) => s.renameShow);
  const showPath = useShowStore((s) => s.showPath);
  const showName = useShowStore((s) => s.show?.name ?? "Untitled");
  const blackoutActive = useShowStore((s) => s.show?.globals?.blackout.active ?? false);
  const setBlackout = useShowStore((s) => s.setBlackout);
  const setBlind = useShowStore((s) => s.setBlind);

  const [renaming, setRenaming] = useState(false);
  const [renameDraft, setRenameDraft] = useState("");
  const renameInputRef = useRef<HTMLInputElement | null>(null);
  // Focus the rename input on entry. Using a ref + effect instead of
  // the JSX `autoFocus` attribute (biome a11y rule: autoFocus can
  // hijack focus from screen-reader users) keeps the same UX while
  // staying lint-clean.
  useEffect(() => {
    if (renaming) renameInputRef.current?.focus();
  }, [renaming]);
  // Transient toast for "saved to /path"; cleared by a timer so the
  // message disappears on its own without ceremony.
  const [toast, setToast] = useState<string | null>(null);
  useEffect(() => {
    if (toast === null) return;
    const id = window.setTimeout(() => setToast(null), 3500);
    return () => window.clearTimeout(id);
  }, [toast]);

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
    try {
      let saved: string;
      if (showPath) {
        saved = await saveShow();
      } else {
        const picked = await saveDialog({
          filters: [{ name: "DMX Show", extensions: ["json"] }],
          defaultPath: `${showName || "show"}.json`,
        });
        if (typeof picked !== "string") return; // user cancelled
        saved = await saveShow(picked);
      }
      setToast(`Guardado en ${saved}`);
    } catch (e) {
      setToast(`Error al guardar: ${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const startRename = () => {
    setRenameDraft(showName);
    setRenaming(true);
  };
  const commitRename = async () => {
    setRenaming(false);
    const next = renameDraft.trim();
    if (next === "" || next === showName) return;
    try {
      await renameShow(next);
    } catch (e) {
      setToast(`No se pudo renombrar: ${e instanceof Error ? e.message : String(e)}`);
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
          {renaming ? (
            <input
              ref={renameInputRef}
              className="show-name-input"
              value={renameDraft}
              onChange={(e) => setRenameDraft(e.currentTarget.value)}
              onBlur={commitRename}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitRename();
                else if (e.key === "Escape") setRenaming(false);
              }}
            />
          ) : (
            <button
              type="button"
              className="show-name selectable"
              title={
                showPath
                  ? `${showPath} · click para renombrar`
                  : "(sin guardar) · click para renombrar"
              }
              onClick={startRename}
            >
              {showName}
              {showPath ? "" : " *"}
            </button>
          )}
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
        {tab === "scenes" && <ScenesView />}
        {tab === "chaser" && <ChaserView />}
        {tab === "movement" && <MovementView />}
        {tab === "config" && <ConfigView />}
      </div>
      {toast ? (
        // <output> is the semantic element for transient status text;
        // matches what biome's a11y rule wants instead of `role="status"`
        // on a generic <div>.
        <output className="app-toast">{toast}</output>
      ) : null}
    </div>
  );
}

export default App;
