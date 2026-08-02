import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { ask } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useRef, useState } from "react";
import "./App.css";
import { ChaserView } from "./components/ChaserView";
import { ConfigView } from "./components/ConfigView";
import { LoopsView } from "./components/LoopsView";
import { MovementView } from "./components/MovementView";
import { OverallBpmControl } from "./components/OverallBpmControl";
import { Preview3D } from "./components/Preview3D";
import { ScenesView } from "./components/ScenesView";
import { StageView } from "./components/StageView";
import { useT } from "./i18n";
import type { Translation } from "./i18n/translations";
import { isDocMode } from "./lib/docMode";
import {
  type PopoutView,
  closeAppCascade,
  openPopout,
  readPopoutView,
  toggleFullscreen,
} from "./lib/windowing";
import { useShowStore } from "./stores/show";

type Tab = PopoutView;

const TABS: { id: Tab; labelKey: keyof Translation }[] = [
  { id: "stage", labelKey: "app.tab.stage" },
  { id: "scenes", labelKey: "app.tab.scenes" },
  { id: "loops", labelKey: "app.tab.loops" },
  { id: "chaser", labelKey: "app.tab.chaser" },
  { id: "movement", labelKey: "app.tab.movement" },
  { id: "preview3d", labelKey: "app.tab.preview3d" },
  { id: "config", labelKey: "app.tab.config" },
];

function renderTab(tab: Tab) {
  switch (tab) {
    case "stage":
      return <StageView />;
    case "scenes":
      return <ScenesView />;
    case "loops":
      return <LoopsView />;
    case "chaser":
      return <ChaserView />;
    case "movement":
      return <MovementView />;
    case "preview3d":
      return <Preview3D />;
    case "config":
      return <ConfigView />;
  }
}

function App() {
  const t = useT();
  // When the URL carries ?popout=<view>, this window is a single-view child
  // window meant for a second monitor. The tabs nav and Open/Save controls
  // collapse into a minimal header pinned to that view.
  const popoutView = useMemo(readPopoutView, []);
  const [tab, setTab] = useState<Tab>(popoutView ?? "stage");
  const refresh = useShowStore((s) => s.refresh);
  const initListeners = useShowStore((s) => s.initListeners);
  const renameShow = useShowStore((s) => s.renameShow);
  const showPath = useShowStore((s) => s.showPath);
  const showName = useShowStore((s) => s.show?.name ?? t("app.show.untitled"));
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
    // Doc mode drives the store directly via `window.__DOC__.hydrate()`,
    // so the Tauri-backed refresh + event listener would only fight the
    // mocked state and add noise to captures.
    if (isDocMode()) return;
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
    if (isDocMode()) return;
    let unlisten: (() => void) | null = null;
    let confirming = false;
    (async () => {
      const win = getCurrentWebviewWindow();
      unlisten = await win.onCloseRequested(async (event) => {
        if (confirming) return;
        event.preventDefault();
        confirming = true;
        try {
          const ok = await ask(t("app.dialog.closeBody"), {
            title: t("app.dialog.closeTitle"),
            kind: "warning",
          });
          if (ok) await closeAppCascade();
        } finally {
          confirming = false;
        }
      });
    })();
    return () => {
      unlisten?.();
    };
  }, [popoutView, t]);

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
      setToast(t("app.toast.renameError", { err: e instanceof Error ? e.message : String(e) }));
    }
  };

  const popoutTab = popoutView ? TABS.find((tab) => tab.id === popoutView) : null;
  const popoutLabel = popoutTab ? t(popoutTab.labelKey) : null;

  return (
    <div className={`app-root${popoutView ? " app-root--popout" : ""}`}>
      <nav className="tabs">
        <div className="tabs-left">
          <span className="brand">{popoutLabel ? `DMX · ${popoutLabel}` : t("app.brand")}</span>
          {!popoutView &&
            TABS.map((meta) => {
              const label = t(meta.labelKey);
              return (
                <span key={meta.id} className={`tab-group${tab === meta.id ? " active" : ""}`}>
                  <button
                    type="button"
                    className={`tab${tab === meta.id ? " active" : ""}`}
                    data-doc-tab={meta.id}
                    onClick={() => setTab(meta.id)}
                  >
                    {label}
                  </button>
                  <button
                    type="button"
                    className="tab-popout-btn"
                    title={t("app.openInOtherWindow", { name: label })}
                    aria-label={t("app.openInOtherWindow", { name: label })}
                    onClick={(e) => {
                      e.stopPropagation();
                      openPopout(meta.id);
                    }}
                  >
                    ↗
                  </button>
                </span>
              );
            })}
        </div>
        <div className="tabs-globals" data-doc="globals">
          <OverallBpmControl />
          <button
            type="button"
            className={`global-btn blackout-btn${blackoutActive ? " active" : ""}`}
            data-doc="blackout"
            onClick={() => setBlackout(!blackoutActive)}
            title={t("app.global.blackoutTitle")}
          >
            {t("app.global.blackout")}
          </button>
          <button
            type="button"
            className="global-btn blind-btn"
            data-doc="blind"
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
            title={t("app.global.blindTitle")}
          >
            {t("app.global.blind")}
          </button>
        </div>
        <div className="tabs-right">
          <button
            type="button"
            className="fullscreen-btn"
            title={t("app.fullscreenHint")}
            aria-label={t("app.fullscreen")}
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
                    ? t("app.show.renameHint", { path: showPath })
                    : t("app.show.renameHintEmpty")
                }
                onClick={startRename}
              >
                {showName}
                {showPath ? "" : " *"}
              </button>
            ))}
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
