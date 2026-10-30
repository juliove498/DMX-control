import type { ChannelDefinition } from "@bindings/ChannelDefinition";
import type { ChannelRole } from "@bindings/ChannelRole";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useMemo, useState } from "react";
import { useT } from "../i18n";
import { deepClone } from "../lib/compat";
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
  // Which fixture has a sub-panel expanded, and which panel: the gobo
  // thumbnail grid or the full definition editor. Single-row expansion
  // (rather than per-row toggles) keeps the panel scroll predictable
  // when the operator hops between fixtures.
  const [openPanel, setOpenPanel] = useState<{ id: string; kind: "gobos" | "edit" } | null>(null);

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
          const open = openPanel?.id === d.id ? openPanel.kind : null;
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
                    onClick={() =>
                      setOpenPanel(open === "gobos" ? null : { id: d.id, kind: "gobos" })
                    }
                    title={open === "gobos" ? t("library.gobosCollapse") : t("library.gobosToggle")}
                  >
                    {open === "gobos" ? "▾" : "▸"}{" "}
                    {open === "gobos" ? t("library.gobosCollapse") : t("library.gobosToggle")}
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      setOpenPanel(open === "edit" ? null : { id: d.id, kind: "edit" })
                    }
                  >
                    {open === "edit" ? "▾" : "▸"} {t("library.edit")}
                  </button>
                </div>
              </div>
              {open === "gobos" ? (
                <RangesEditor
                  def={d}
                  libraryDir={libraryDir}
                  onPick={(mi, ci, ri) => pickRangeImage(d.id, mi, ci, ri)}
                />
              ) : null}
              {open === "edit" ? (
                <DefinitionEditor key={d.id} def={d} onClose={() => setOpenPanel(null)} />
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
  const hasAnyRanges = def.modes.some((m) => m.channels.some((c) => (c.ranges ?? []).length > 0));
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

/// Every non-`other` role the backend understands, in the order the
/// role dropdown lists them. Mirrors `ChannelRole` in fixture.rs —
/// keep in sync when adding roles there.
const STANDARD_ROLES = [
  "intensity",
  "red",
  "green",
  "blue",
  "white",
  "amber",
  "uv",
  "cyan",
  "magenta",
  "yellow",
  "pan",
  "pan_fine",
  "tilt",
  "tilt_fine",
  "color_wheel",
  "gobo",
  "strobe",
  "zoom",
  "focus",
  "iris",
  "shutter",
] as const;

/// Sentinel value for the role <select> meaning "custom role" — the
/// actual string then lives in a free-text input next to it.
const ROLE_OTHER = "__other";

/// Deep-copy a definition and materialise the fields serde may have
/// dropped from the IPC payload (`ranges` when empty, `name` /
/// `description` when null) so the form can bind to them directly.
function normalizeDef(def: FixtureDefinition): FixtureDefinition {
  const d = deepClone(def);
  for (const m of d.modes) {
    for (const c of m.channels) {
      c.ranges = c.ranges ?? [];
      c.name = c.name ?? null;
      c.description = c.description ?? null;
    }
  }
  return d;
}

function clampDmx(v: string): number {
  const n = Math.round(Number(v));
  if (!Number.isFinite(n)) return 0;
  return Math.max(0, Math.min(255, n));
}

/// In-place editor for one fixture definition: a friendly form for the
/// common edits (names, roles, defaults, range labels/limits) plus a
/// raw-JSON tab for anything the form doesn't cover (adding channels,
/// modes, pan/tilt ranges…). Both tabs save through the same backend
/// command, which validates and rewrites the library JSON atomically.
function DefinitionEditor({ def, onClose }: { def: FixtureDefinition; onClose: () => void }) {
  const t = useT();
  const updateFixtureDefinition = useShowStore((s) => s.updateFixtureDefinition);
  const [draft, setDraft] = useState<FixtureDefinition>(() => normalizeDef(def));
  const [tab, setTab] = useState<"form" | "json">("form");
  const [jsonText, setJsonText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  /// All form edits funnel through here: clone, mutate, set. Cloning
  /// per keystroke is fine — the biggest definitions are a few hundred
  /// KB of base64 image and clone in well under a millisecond.
  const patchDraft = (fn: (d: FixtureDefinition) => void) => {
    setDraft((prev) => {
      const next = deepClone(prev);
      fn(next);
      return next;
    });
  };

  const switchTab = (next: "form" | "json") => {
    if (next === tab) return;
    setError(null);
    if (next === "json") {
      setJsonText(JSON.stringify(draft, null, 2));
      setTab("json");
      return;
    }
    // Leaving the raw tab: parse it back into the draft so the form
    // shows the manual edits. A broken buffer keeps you on the raw tab
    // with the error visible instead of silently discarding your text.
    try {
      setDraft(normalizeDef(JSON.parse(jsonText) as FixtureDefinition));
      setTab("form");
    } catch (e) {
      setError(t("library.editSaveError", { err: stringifyError(e) }));
    }
  };

  const save = async () => {
    setError(null);
    setSaving(true);
    try {
      const body = tab === "json" ? jsonText : JSON.stringify(draft, null, 2);
      await updateFixtureDefinition(def.id, body);
      onClose();
    } catch (e) {
      setError(t("library.editSaveError", { err: stringifyError(e) }));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="lib-editor">
      <div className="lib-editor-tabs">
        <button
          type="button"
          className={tab === "form" ? "active" : ""}
          onClick={() => switchTab("form")}
        >
          {t("library.editTabForm")}
        </button>
        <button
          type="button"
          className={tab === "json" ? "active" : ""}
          onClick={() => switchTab("json")}
        >
          {t("library.editTabJson")}
        </button>
        <span className="lib-editor-id">{t("library.editIdNote", { id: def.id })}</span>
      </div>
      {error ? (
        <div className="lib-error" role="alert">
          {error}
        </div>
      ) : null}
      {tab === "form" ? (
        <div className="lib-editor-form">
          <div className="lib-editor-head">
            <label>
              {t("library.editManufacturer")}
              <input
                value={draft.manufacturer}
                onChange={(e) => {
                  const v = e.currentTarget.value;
                  patchDraft((d) => {
                    d.manufacturer = v;
                  });
                }}
              />
            </label>
            <label>
              {t("library.editName")}
              <input
                value={draft.name}
                onChange={(e) => {
                  const v = e.currentTarget.value;
                  patchDraft((d) => {
                    d.name = v;
                  });
                }}
              />
            </label>
          </div>
          {draft.modes.map((mode, mi) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: modes are positional and edited in place
            <div key={mi} className="lib-editor-mode">
              <label className="lib-editor-modename">
                {t("library.editModeName")}
                <input
                  value={mode.name}
                  onChange={(e) => {
                    const v = e.currentTarget.value;
                    patchDraft((d) => {
                      d.modes[mi].name = v;
                    });
                  }}
                />
              </label>
              <div className="lib-editor-chan-head" aria-hidden="true">
                <span />
                <span>{t("library.editColName")}</span>
                <span>{t("library.editColRole")}</span>
                <span>{t("library.editColDefault")}</span>
                <span>{t("library.editColDescription")}</span>
              </div>
              {mode.channels.map((ch, ci) => (
                <ChannelEditorRow
                  // biome-ignore lint/suspicious/noArrayIndexKey: channels are positional and edited in place
                  key={ci}
                  channel={ch}
                  modeIndex={mi}
                  channelIndex={ci}
                  patchDraft={patchDraft}
                />
              ))}
            </div>
          ))}
        </div>
      ) : (
        <div className="lib-editor-json">
          <p className="hint">{t("library.editJsonHint")}</p>
          <textarea
            value={jsonText}
            onChange={(e) => setJsonText(e.currentTarget.value)}
            spellCheck={false}
            rows={22}
          />
        </div>
      )}
      <div className="lib-editor-actions">
        <button type="button" onClick={onClose}>
          {t("common.cancel")}
        </button>
        <button type="button" className="primary" disabled={saving} onClick={save}>
          {t("common.save")}
        </button>
      </div>
    </div>
  );
}

function ChannelEditorRow({
  channel,
  modeIndex,
  channelIndex,
  patchDraft,
}: {
  channel: ChannelDefinition;
  modeIndex: number;
  channelIndex: number;
  patchDraft: (fn: (d: FixtureDefinition) => void) => void;
}) {
  const t = useT();
  const patchChannel = (fn: (c: ChannelDefinition) => void) =>
    patchDraft((d) => {
      fn(d.modes[modeIndex].channels[channelIndex]);
    });
  const roleValue = typeof channel.role === "string" ? channel.role : ROLE_OTHER;
  const ranges = channel.ranges ?? [];

  return (
    <div className="lib-editor-ch">
      <div className="lib-editor-ch-main">
        <span className="chnum">{channelIndex + 1}</span>
        <input
          value={channel.name ?? ""}
          placeholder={roleString(channel.role)}
          onChange={(e) => {
            const v = e.currentTarget.value;
            patchChannel((c) => {
              c.name = v.length > 0 ? v : null;
            });
          }}
        />
        <div className="lib-editor-role">
          <select
            value={roleValue}
            onChange={(e) => {
              const v = e.currentTarget.value;
              patchChannel((c) => {
                c.role = v === ROLE_OTHER ? { other: "" } : (v as ChannelRole);
              });
            }}
          >
            {STANDARD_ROLES.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
            <option value={ROLE_OTHER}>{t("library.editRoleCustom")}</option>
          </select>
          {typeof channel.role !== "string" ? (
            <input
              value={channel.role.other}
              placeholder="custom_role"
              onChange={(e) => {
                const v = e.currentTarget.value;
                patchChannel((c) => {
                  c.role = { other: v };
                });
              }}
            />
          ) : null}
        </div>
        <input
          type="number"
          min={0}
          max={255}
          value={channel.default}
          onChange={(e) => {
            const v = clampDmx(e.currentTarget.value);
            patchChannel((c) => {
              c.default = v;
            });
          }}
        />
        <input
          value={channel.description ?? ""}
          onChange={(e) => {
            const v = e.currentTarget.value;
            patchChannel((c) => {
              c.description = v.length > 0 ? v : null;
            });
          }}
        />
      </div>
      <div className="lib-editor-ranges">
        {ranges.length > 0 ? <span className="ranges-label">{t("library.editRanges")}</span> : null}
        {ranges.map((range, ri) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: ranges are positional and edited in place
          <div key={ri} className="lib-editor-range">
            <input
              type="number"
              min={0}
              max={255}
              value={range.from}
              onChange={(e) => {
                const v = clampDmx(e.currentTarget.value);
                patchChannel((c) => {
                  c.ranges[ri].from = v;
                });
              }}
            />
            <span aria-hidden="true">–</span>
            <input
              type="number"
              min={0}
              max={255}
              value={range.to}
              onChange={(e) => {
                const v = clampDmx(e.currentTarget.value);
                patchChannel((c) => {
                  c.ranges[ri].to = v;
                });
              }}
            />
            <input
              className="range-label"
              value={range.label}
              onChange={(e) => {
                const v = e.currentTarget.value;
                patchChannel((c) => {
                  c.ranges[ri].label = v;
                });
              }}
            />
            <button
              type="button"
              className="range-remove"
              title={t("library.editRemoveRange")}
              onClick={() =>
                patchChannel((c) => {
                  c.ranges.splice(ri, 1);
                })
              }
            >
              ✕
            </button>
          </div>
        ))}
        <button
          type="button"
          className="lib-editor-addrange"
          onClick={() =>
            patchChannel((c) => {
              const last = c.ranges[c.ranges.length - 1];
              const from = last ? Math.min(255, last.to + 1) : 0;
              c.ranges.push({
                from,
                to: Math.min(255, from + 49),
                label: "",
                image: null,
                image_path: null,
              });
            })
          }
        >
          {t("library.editAddRange")}
        </button>
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
