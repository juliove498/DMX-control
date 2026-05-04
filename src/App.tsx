import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  ask,
  open as openDialog,
  save as saveDialog,
} from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useRef, useState } from "react";
import "./App.css";
import { ChaserView } from "./components/ChaserView";
import { ConfigView } from "./components/ConfigView";
import { MovementView } from "./components/MovementView";
import { ScenesView } from "./components/ScenesView";
import { StageView } from "./components/StageView";
import {
  closeAppCascade,
  openPopout,
  type PopoutView,
  readPopoutView,
  toggleFullscreen,
} from "./lib/windowing";
import { useShowStore } from "./stores/show";

type Tab = PopoutView;

const TABS: { id: Tab; label: string }[] = [
  { id: "stage", label: "Stage" },
  { id: "scenes", label: "Scenes" },
  { id: "chaser", label: "Chaser" },
  { id: "movement", label: "Movement" },
  { id: "config", label: "Config" },
];

function renderTab(tab: Tab) {
  switch (tab) {
    case "stage":
      return <StageView />;
    case "scenes":
      return <ScenesView />;
    case "chaser":
      return <ChaserView />;
    case "movement":
      return <MovementView />;
    case "config":
      return <ConfigView />;
  }
}

function App() {
  // When the URL carries ?popout=<view>, this window is a single-view child
  // window meant for a second monitor. The tabs nav and Open/Save controls
  // collapse into a minimal header pinned to that view.
  const popoutView = useMemo(readPopoutView, []);
  const [tab, setTab] = useState<Tab>(popoutView ?? "stage");
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

  // F11 toggles fullscreen for whichever window has focus. The browser's own
  // F11 handler is suppressed inside Tauri's webview, so we drive it through
  // the Tauri window API instead.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F11") {
        e.preventDefault();
        toggleFullscreen();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Confirm before quitting from the main window — closing it normally
  // would only kill that window and leave any popouts as orphans. We
  // intercept close-requested, ask, and on confirm cascade-close every
  // popout and then destroy() the main (destroy bypasses our own
  // listener, so we don't loop). Popouts intentionally don't get the
  // confirm: closing one of those is the obvious "I'm done with this
  // monitor" gesture and shouldn't tear the show down.
  useEffect(() => {
    if (popoutView) return;
    let unlisten: (() => void) | null = null;
    let confirming = false;
    (async () => {
      const win = getCurrentWebviewWindow();
      unlisten = await win.onCloseRequested(async (event) => {
        if (confirming) return;
        event.preventDefault();
        confirming = true;
        try {
          const ok = await ask(
            "¿Cerrar DMX Control?\nSe cerrarán también todas las ventanas popout abiertas.",
            { title: "Cerrar aplicación", kind: "warning" },
          );
          if (ok) await closeAppCascade();
        } finally {
          confirming = false;
        }
      });
    })();
    return () => {
      unlisten?.();
    };
  }, [popoutView]);

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

  const popoutLabel = popoutView ? TABS.find((t) => t.id === popoutView)?.label : null;

  return (
    <div className={`app-root${popoutView ? " app-root--popout" : ""}`}>
      <nav className="tabs">
        <div className="tabs-left">
          <span className="brand">{popoutLabel ? `DMX · ${popoutLabel}` : "DMX Control"}</span>
          {!popoutView &&
            TABS.map((t) => (
              <span key={t.id} className={`tab-group${tab === t.id ? " active" : ""}`}>
                <button
                  type="button"
                  className={`tab${tab === t.id ? " active" : ""}`}
                  onClick={() => setTab(t.id)}
                >
                  {t.label}
                </button>
                <button
                  type="button"
                  className="tab-popout-btn"
                  title={`Abrir ${t.label} en otra ventana`}
                  aria-label={`Abrir ${t.label} en otra ventana`}
                  onClick={(e) => {
                    e.stopPropagation();
                    openPopout(t.id);
                  }}
                >
                  ↗
                </button>
              </span>
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
          <button
            type="button"
            className="fullscreen-btn"
            title="Pantalla completa (F11)"
            aria-label="Pantalla completa"
            onClick={() => toggleFullscreen()}
          >
            ⛶
          </button>
          {!popoutView &&
            (renaming ? (
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
            ))}
          {!popoutView && (
            <>
              <button type="button" onClick={() => newShow()}>
                New
              </button>
              <button type="button" onClick={onOpen}>
                Open…
              </button>
              <button type="button" onClick={onSave}>
                Save
              </button>
            </>
          )}
        </div>
      </nav>
      <div className="tab-body">{renderTab(tab)}</div>
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
