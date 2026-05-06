import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { LANGUAGES, useLanguage } from "../i18n";
import { useShowStore } from "../stores/show";

/// General preferences. Phase 1 is just the language picker — future
/// home for any other "applies to the whole app" toggles (theme, fps
/// readout, default startup view, etc.).
export function GeneralView() {
  const { lang, setLang, t } = useLanguage();
  const newShow = useShowStore((s) => s.newShow);
  const openShow = useShowStore((s) => s.openShow);
  const saveShow = useShowStore((s) => s.saveShow);
  const showPath = useShowStore((s) => s.showPath);
  const showName = useShowStore((s) => s.show?.name ?? t("app.show.untitled"));

  const [toast, setToast] = useState<string | null>(null);
  useEffect(() => {
    if (toast === null) return;
    const id = window.setTimeout(() => setToast(null), 3500);
    return () => window.clearTimeout(id);
  }, [toast]);

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
        if (typeof picked !== "string") return;
        saved = await saveShow(picked);
      }
      setToast(t("app.toast.savedAt", { path: saved }));
    } catch (e) {
      setToast(t("app.toast.saveError", { err: e instanceof Error ? e.message : String(e) }));
    }
  };

  return (
    <main className="page general-view">
      <header className="page-head">
        <h2>{t("general.title")}</h2>
      </header>

      <section className="card">
        <h3>{t("general.showFile")}</h3>
        <p className="hint">{t("general.showFileHint")}</p>
        <div className="row">
          <button type="button" onClick={() => newShow()}>
            {t("app.button.new")}
          </button>
          <button type="button" onClick={onOpen}>
            {t("app.button.open")}
          </button>
          <button type="button" className="primary" onClick={onSave}>
            {t("app.button.save")}
          </button>
        </div>
        {toast ? <output className="app-toast app-toast--inline">{toast}</output> : null}
      </section>

      <section className="card">
        <h3>{t("general.language")}</h3>
        <p className="hint">{t("general.languageHint")}</p>
        <div className="row">
          {LANGUAGES.map((l) => (
            <button
              key={l.code}
              type="button"
              className={lang === l.code ? "primary" : ""}
              onClick={() => setLang(l.code)}
            >
              {l.nativeName}
            </button>
          ))}
        </div>
      </section>
    </main>
  );
}
