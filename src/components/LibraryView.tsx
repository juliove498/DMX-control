import { convertFileSrc } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useMemo, useState } from "react";
import { useShowStore } from "../stores/show";

export function LibraryView() {
  const library = useShowStore((s) => s.library);
  const libraryDir = useShowStore((s) => s.libraryDir);
  const reload = useShowStore((s) => s.reloadLibrary);
  const setFixtureImage = useShowStore((s) => s.setFixtureImage);
  const [filter, setFilter] = useState("");

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return library;
    return library.filter((d) => `${d.manufacturer} ${d.name} ${d.id}`.toLowerCase().includes(q));
  }, [library, filter]);

  const [imageError, setImageError] = useState<string | null>(null);
  const pickImage = async (definitionId: string) => {
    setImageError(null);
    let picked: unknown;
    try {
      picked = await openDialog({
        multiple: false,
        filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("openDialog failed", e);
      setImageError(`No se pudo abrir el diálogo: ${msg}`);
      return;
    }
    if (typeof picked !== "string") return;
    try {
      await setFixtureImage(definitionId, picked);
    } catch (e) {
      // Tauri command errors come back as { kind, message } via our
      // CommandError serde tag. Surface whatever shape they take so the
      // user can see what failed without digging into the log.
      let msg: string;
      if (e instanceof Error) msg = e.message;
      else if (typeof e === "object" && e !== null && "message" in e)
        msg = String((e as { message: unknown }).message);
      else msg = JSON.stringify(e);
      console.error("set_fixture_image failed", e);
      setImageError(`No se pudo cambiar la imagen: ${msg}`);
    }
  };

  return (
    <main className="page library-view">
      <header className="page-head">
        <h2>Library ({library.length})</h2>
        <div className="actions">
          <input
            placeholder="Search…"
            value={filter}
            onChange={(e) => setFilter(e.currentTarget.value)}
          />
          <button type="button" onClick={() => reload()}>
            Reload
          </button>
        </div>
      </header>
      {imageError ? (
        <div className="lib-error" role="alert">
          {imageError}
          <button
            type="button"
            className="lib-error-dismiss"
            onClick={() => setImageError(null)}
            aria-label="Cerrar"
          >
            ×
          </button>
        </div>
      ) : null}
      <div className="lib-list">
        {filtered.map((d) => {
          const imageUrl =
            d.image && libraryDir ? convertFileSrc(`${libraryDir}/${d.image}`) : null;
          return (
            <div key={d.id} className="lib-item">
              <div className="lib-thumb">
                {imageUrl ? (
                  <img src={imageUrl} alt="" />
                ) : (
                  <div className="lib-thumb-placeholder" aria-hidden="true">
                    ?
                  </div>
                )}
              </div>
              <div className="lib-info">
                <div className="lib-name">
                  <strong>
                    {d.manufacturer} {d.name}
                  </strong>{" "}
                  <span className="lib-id">— {d.id}</span>
                </div>
                <div className="lib-modes">
                  {d.modes.map((m) => (
                    <span key={m.name} className="mode-pill">
                      {m.name} · {m.channels.length}ch
                    </span>
                  ))}
                </div>
              </div>
              <div className="lib-actions">
                <button type="button" onClick={() => pickImage(d.id)}>
                  {d.image ? "Cambiar imagen" : "Subir imagen"}
                </button>
              </div>
            </div>
          );
        })}
      </div>
      <p className="hint">
        Los archivos JSON viven en <code>~/Library/Application Support/dmx-control/fixtures/</code>.
      </p>
    </main>
  );
}
