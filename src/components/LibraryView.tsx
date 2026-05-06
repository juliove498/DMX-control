import type { ChannelDefinition } from "@bindings/ChannelDefinition";
import type { ChannelRole } from "@bindings/ChannelRole";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useMemo, useState } from "react";
import { useT } from "../i18n";
import { fixtureImageSrc } from "../lib/fixtureImage";
import { useShowStore } from "../stores/show";

export function LibraryView() {
  const t = useT();
  const library = useShowStore((s) => s.library);
  const libraryDir = useShowStore((s) => s.libraryDir);
  const reload = useShowStore((s) => s.reloadLibrary);
  const setFixtureImage = useShowStore((s) => s.setFixtureImage);
  const setChannelRangeImage = useShowStore((s) => s.setChannelRangeImage);
  const [filter, setFilter] = useState("");
  // Which fixture has its gobo / range editor expanded. Single-row
  // expansion (rather than per-row toggles) keeps the panel scroll
  // predictable when the operator hops between fixtures.
  const [openId, setOpenId] = useState<string | null>(null);

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
      setImageError(t("library.errorOpenDialog", { err: msg }));
      return;
    }
    if (typeof picked !== "string") return;
    try {
      await setFixtureImage(definitionId, picked);
    } catch (e) {
      let msg: string;
      if (e instanceof Error) msg = e.message;
      else if (typeof e === "object" && e !== null && "message" in e)
        msg = String((e as { message: unknown }).message);
      else msg = JSON.stringify(e);
      console.error("set_fixture_image failed", e);
      setImageError(t("library.errorSetImage", { err: msg }));
    }
  };

  const pickRangeImage = async (
    definitionId: string,
    modeIndex: number,
    channelIndex: number,
    rangeIndex: number,
  ) => {
    setImageError(null);
    let picked: unknown;
    try {
      picked = await openDialog({
        multiple: false,
        filters: [{ name: "Image", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
      });
    } catch (e) {
      setImageError(t("library.errorOpenDialog", { err: stringifyError(e) }));
      return;
    }
    if (typeof picked !== "string") return;
    try {
      await setChannelRangeImage(definitionId, modeIndex, channelIndex, rangeIndex, picked);
    } catch (e) {
      console.error("set_channel_range_image failed", e);
      setImageError(t("library.errorSetImage", { err: stringifyError(e) }));
    }
  };

  return (
    <main className="page library-view">
      <header className="page-head">
        <h2>{t("library.title", { count: library.length })}</h2>
        <div className="actions">
          <input
            placeholder={t("library.search")}
            value={filter}
            onChange={(e) => setFilter(e.currentTarget.value)}
          />
          <button type="button" onClick={() => reload()}>
            {t("library.reload")}
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
            aria-label={t("common.close")}
          >
            ×
          </button>
        </div>
      ) : null}
      <div className="lib-list">
        {filtered.map((d) => {
          const imageUrl = fixtureImageSrc(d.image, libraryDir);
          const open = openId === d.id;
          return (
            <div key={d.id} className={`lib-item${open ? " expanded" : ""}`}>
              <div className="lib-item-row">
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
                        {t("library.modeSummary", { name: m.name, count: m.channels.length })}
                      </span>
                    ))}
                  </div>
                </div>
                <div className="lib-actions">
                  <button type="button" onClick={() => pickImage(d.id)}>
                    {d.image ? t("library.changeImage") : t("library.uploadImage")}
                  </button>
                  <button
                    type="button"
                    onClick={() => setOpenId(open ? null : d.id)}
                    title={open ? t("library.gobosCollapse") : t("library.gobosToggle")}
                  >
                    {open ? "▾" : "▸"} {open ? t("library.gobosCollapse") : t("library.gobosToggle")}
                  </button>
                </div>
              </div>
              {open ? (
                <RangesEditor
                  def={d}
                  libraryDir={libraryDir}
                  onPick={(mi, ci, ri) => pickRangeImage(d.id, mi, ci, ri)}
                />
              ) : null}
            </div>
          );
        })}
      </div>
      <p className="hint">
        {t("library.filesPath", {
          path: "~/Library/Application Support/dmx-control/fixtures/",
        })}
      </p>
    </main>
  );
}

/// Sub-panel that lists every channel on every mode that has DMX
/// ranges defined (gobo wheels, colour wheels, macro selectors).
/// Each range shows its thumbnail (or a placeholder) and an upload
/// button; the click → file picker → patch flow is identical to
/// the fixture-thumbnail uploader, just one level deeper in the
/// definition tree.
function RangesEditor({
  def,
  libraryDir,
  onPick,
}: {
  def: FixtureDefinition;
  libraryDir: string | null;
  onPick: (modeIndex: number, channelIndex: number, rangeIndex: number) => void;
}) {
  const t = useT();
  // Which channels carry ranges? Only those need a UI section. We
  // walk every mode independently because the channel layout often
  // differs between modes (a 12ch mode may not have the macro
  // channel that the 16ch mode does).
  //
  // Rust's serde drops the `ranges` field entirely from the IPC
  // payload when it's empty (`skip_serializing_if = "Vec::is_empty"`).
  // ts-rs still types it as `Array<ChannelRange>` so the compiler
  // doesn't warn, but at runtime channels without ranges arrive as
  // `{ ...other fields, /* no ranges */ }`. Defensive `?? []`
  // everywhere we touch it — without it, "Editar gobos" on any
  // fixture with at least one continuous channel would blow up.
  const hasAnyRanges = def.modes.some((m) =>
    m.channels.some((c) => (c.ranges ?? []).length > 0),
  );
  if (!hasAnyRanges) {
    return (
      <div className="lib-ranges">
        <p className="empty">{t("library.gobosEmpty")}</p>
      </div>
    );
  }

  return (
    <div className="lib-ranges">
      {def.modes.map((mode, modeIndex) => {
        const channelsWithRanges = mode.channels
          .map((channel, channelIndex) => ({ channel, channelIndex }))
          .filter(({ channel }) => (channel.ranges ?? []).length > 0);
        if (channelsWithRanges.length === 0) return null;
        return (
          <details key={`${mode.name}-${modeIndex}`} className="lib-mode" open={modeIndex === 0}>
            <summary>
              {t("library.gobosModeFmt", { name: mode.name, count: mode.channels.length })}
            </summary>
            {channelsWithRanges.map(({ channel, channelIndex }) => (
              <ChannelRangeList
                key={`${channelIndex}-${channelLabel(channel)}`}
                channel={channel}
                modeIndex={modeIndex}
                channelIndex={channelIndex}
                libraryDir={libraryDir}
                onPick={onPick}
              />
            ))}
          </details>
        );
      })}
    </div>
  );
}

function ChannelRangeList({
  channel,
  modeIndex,
  channelIndex,
  libraryDir,
  onPick,
}: {
  channel: ChannelDefinition;
  modeIndex: number;
  channelIndex: number;
  libraryDir: string | null;
  onPick: (modeIndex: number, channelIndex: number, rangeIndex: number) => void;
}) {
  const t = useT();
  return (
    <div className="lib-channel">
      <h5>
        {t("library.gobosChannelFmt", {
          name: channelLabel(channel),
          role: roleString(channel.role),
        })}
      </h5>
      <div className="lib-range-grid">
        {(channel.ranges ?? []).map((range, rangeIndex) => {
          const url = fixtureImageSrc(range.image ?? range.image_path, libraryDir);
          return (
            <div
              key={`${rangeIndex}-${range.label}-${range.from}-${range.to}`}
              className="lib-range-cell"
            >
              <div className="lib-range-thumb">
                {url ? (
                  <img src={url} alt="" />
                ) : (
                  <div className="lib-thumb-placeholder" aria-hidden="true">
                    ?
                  </div>
                )}
              </div>
              <div className="lib-range-label" title={range.label}>
                {t("library.gobosRangeFmt", {
                  label: range.label,
                  from: range.from,
                  to: range.to,
                })}
              </div>
              <button
                type="button"
                onClick={() => onPick(modeIndex, channelIndex, rangeIndex)}
                title={url ? t("library.gobosReplace") : t("library.gobosUpload")}
              >
                {url ? t("library.gobosReplace") : t("library.gobosUpload")}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function channelLabel(c: ChannelDefinition): string {
  return c.name ?? roleString(c.role);
}

function roleString(role: ChannelRole | unknown): string {
  if (typeof role === "string") return role;
  if (role && typeof role === "object" && "other" in role) {
    const inner = (role as { other: unknown }).other;
    if (typeof inner === "string" && inner.length > 0) return inner;
  }
  return "other";
}

function stringifyError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "object" && e !== null && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return JSON.stringify(e);
}
