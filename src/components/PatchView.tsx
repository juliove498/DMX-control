import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import { useEffect, useMemo, useState } from "react";
import { fixtureImageSrc } from "../lib/fixtureImage";
import { useShowStore } from "../stores/show";

const STAGE_COLS = 6;
const STAGE_SPACING = 96;

function newFixtureId(existing: { id: string }[]): string {
  let n = 1;
  while (existing.some((f) => f.id === `fx-${n}`)) n += 1;
  return `fx-${n}`;
}

function nextFreeAddress(
  existing: FixtureInstance[],
  libraryById: Record<string, FixtureDefinition>,
  universe: number,
  channelCount: number,
): number {
  if (channelCount <= 0) return 1;
  const used = new Set<number>();
  for (const f of existing) {
    if (f.universe !== universe) continue;
    const def = libraryById[f.definition_id];
    const len = def?.modes[f.mode_index]?.channels.length ?? 0;
    for (let i = 0; i < len; i++) used.add(f.address + i);
  }
  for (let start = 1; start + channelCount - 1 <= 512; start++) {
    let ok = true;
    for (let i = 0; i < channelCount; i++) {
      if (used.has(start + i)) {
        ok = false;
        break;
      }
    }
    if (ok) return start;
  }
  return 1;
}

function nextStagePosition(existing: FixtureInstance[], offsetInBatch: number): [number, number] {
  const n = existing.length + offsetInBatch;
  return [(n % STAGE_COLS) * STAGE_SPACING, Math.floor(n / STAGE_COLS) * STAGE_SPACING];
}

function AddFixtureForm({
  library,
  libraryById,
  onAddBulk,
  defaultUniverse,
}: {
  library: FixtureDefinition[];
  libraryById: Record<string, FixtureDefinition>;
  onAddBulk: (fixtures: FixtureInstance[]) => Promise<void>;
  defaultUniverse: number;
}) {
  const show = useShowStore((s) => s.show);
  const [defId, setDefId] = useState<string>(library[0]?.id ?? "");
  const [modeIndex, setModeIndex] = useState(0);
  const [universe, setUniverse] = useState(defaultUniverse);
  const [address, setAddress] = useState(1);
  const [addressDirty, setAddressDirty] = useState(false);
  const [quantity, setQuantity] = useState(1);
  const [label, setLabel] = useState("");

  const def = library.find((d) => d.id === defId);
  const channelCount = def?.modes[modeIndex]?.channels.length ?? 0;
  const fixtures = show?.fixtures ?? [];

  // Auto-fill the start address with the first free range whenever the user
  // hasn't manually overridden it. Recomputes when fixture/mode/universe or the
  // patched set changes so adding one row pushes the next default forward.
  useEffect(() => {
    if (addressDirty) return;
    if (channelCount === 0) return;
    setAddress(nextFreeAddress(fixtures, libraryById, universe, channelCount));
  }, [addressDirty, fixtures, libraryById, universe, channelCount]);

  const submit = async () => {
    if (!def || !show || channelCount === 0) return;
    const safeQty = Math.max(1, Math.min(quantity, 64));
    const baseLabel = label.trim();
    const built: FixtureInstance[] = [];
    for (let i = 0; i < safeQty; i++) {
      const id = newFixtureId([...show.fixtures, ...built]);
      built.push({
        id,
        definition_id: def.id,
        mode_index: modeIndex,
        universe,
        address: address + i * channelCount,
        label:
          baseLabel === ""
            ? safeQty > 1
              ? `${def.name} ${i + 1}`
              : null
            : safeQty > 1
              ? `${baseLabel} ${i + 1}`
              : baseLabel,
        position: nextStagePosition(show.fixtures, i),
      });
    }
    await onAddBulk(built);
    setLabel("");
    // Drop the dirty flag so the next render picks the new "next free" slot
    // computed against the now-larger fixtures list.
    setAddressDirty(false);
  };

  return (
    <div className="add-fixture">
      <select value={defId} onChange={(e) => setDefId(e.currentTarget.value)}>
        {library.map((d) => (
          <option key={d.id} value={d.id}>
            {d.manufacturer} {d.name}
          </option>
        ))}
      </select>
      <select
        value={modeIndex}
        onChange={(e) => {
          setModeIndex(Number(e.currentTarget.value));
          setAddressDirty(false);
        }}
        disabled={!def || def.modes.length === 1}
      >
        {def?.modes.map((m, i) => (
          <option key={m.name} value={i}>
            {m.name} ({m.channels.length}ch)
          </option>
        ))}
      </select>
      <label>
        Qty
        <input
          type="number"
          min={1}
          max={64}
          value={quantity}
          onChange={(e) => setQuantity(Number(e.currentTarget.value))}
        />
      </label>
      <label>
        U
        <input
          type="number"
          min={0}
          value={universe}
          onChange={(e) => {
            setUniverse(Number(e.currentTarget.value));
            setAddressDirty(false);
          }}
        />
      </label>
      <label>
        Addr
        <input
          type="number"
          min={1}
          max={512}
          value={address}
          onChange={(e) => {
            setAddress(Number(e.currentTarget.value));
            setAddressDirty(true);
          }}
        />
      </label>
      <input
        placeholder="Label (opcional)"
        value={label}
        onChange={(e) => setLabel(e.currentTarget.value)}
      />
      <button type="button" onClick={submit} disabled={!def || channelCount === 0}>
        Add
      </button>
    </div>
  );
}

export function PatchView() {
  const show = useShowStore((s) => s.show);
  const library = useShowStore((s) => s.library);
  const libraryDir = useShowStore((s) => s.libraryDir);
  const patch = useShowStore((s) => s.patch);
  const addFixtures = useShowStore((s) => s.addFixtures);
  const removeFixture = useShowStore((s) => s.removeFixture);
  const updateFixture = useShowStore((s) => s.updateFixture);

  const libraryById = useMemo(() => {
    const m: Record<string, FixtureDefinition> = {};
    for (const d of library) m[d.id] = d;
    return m;
  }, [library]);

  const imageUrlByDef = useMemo(() => {
    const m: Record<string, string> = {};
    for (const d of library) {
      const url = fixtureImageSrc(d.image, libraryDir);
      if (url) m[d.id] = url;
    }
    return m;
  }, [library, libraryDir]);

  if (!show) return <main className="page">Cargando…</main>;

  const conflictsByFixture = new Set<string>();
  for (const c of patch.conflicts) {
    conflictsByFixture.add(c.fixture_a);
    conflictsByFixture.add(c.fixture_b);
  }
  const problemsByFixture = new Set(patch.problems.map((p) => p.fixture));

  return (
    <main className="page patch-view">
      <header className="page-head">
        <h2>Patch ({show.fixtures.length} fixtures)</h2>
        <span className={patch.conflicts.length || patch.problems.length ? "warn" : "ok"}>
          {patch.conflicts.length === 0 && patch.problems.length === 0
            ? "✓ patch limpio"
            : `${patch.conflicts.length} conflictos · ${patch.problems.length} problemas`}
        </span>
      </header>

      <AddFixtureForm
        library={library}
        libraryById={libraryById}
        onAddBulk={addFixtures}
        defaultUniverse={show.outputs.bindings[0]?.universes[0] ?? 0}
      />

      <table className="fixtures">
        <thead>
          <tr>
            <th>ID</th>
            <th>Label</th>
            <th>Fixture</th>
            <th>Mode</th>
            <th>U</th>
            <th>Addr</th>
            <th>Range</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {show.fixtures.map((f) => {
            const def = libraryById[f.definition_id];
            const mode = def?.modes[f.mode_index];
            const len = mode?.channels.length ?? 0;
            const lastChannel = len > 0 ? f.address + len - 1 : f.address;
            const bad = conflictsByFixture.has(f.id) || problemsByFixture.has(f.id);
            return (
              <tr key={f.id} className={bad ? "bad" : ""}>
                <td>{f.id}</td>
                <td>
                  <input
                    value={f.label ?? ""}
                    onChange={(e) =>
                      updateFixture({
                        ...f,
                        label: e.currentTarget.value === "" ? null : e.currentTarget.value,
                      })
                    }
                  />
                </td>
                <td>
                  <div className="patch-fixture-cell">
                    {def && imageUrlByDef[def.id] ? (
                      <img className="patch-fixture-icon" src={imageUrlByDef[def.id]} alt="" />
                    ) : (
                      <span className="patch-fixture-icon placeholder" aria-hidden="true" />
                    )}
                    <span>{def ? `${def.manufacturer} ${def.name}` : `?? ${f.definition_id}`}</span>
                  </div>
                </td>
                <td>{mode?.name ?? "?"}</td>
                <td>
                  <input
                    type="number"
                    min={0}
                    value={f.universe}
                    onChange={(e) =>
                      updateFixture({ ...f, universe: Number(e.currentTarget.value) })
                    }
                  />
                </td>
                <td>
                  <input
                    type="number"
                    min={1}
                    max={512}
                    value={f.address}
                    onChange={(e) =>
                      updateFixture({ ...f, address: Number(e.currentTarget.value) })
                    }
                  />
                </td>
                <td>
                  {f.address}…{lastChannel}
                </td>
                <td>
                  <button type="button" className="danger" onClick={() => removeFixture(f.id)}>
                    Del
                  </button>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>

      {patch.conflicts.length || patch.problems.length ? (
        <section className="patch-issues">
          {patch.problems.map((p) => (
            <div key={`p-${p.fixture}`} className="issue">
              <strong>{p.fixture}:</strong> {p.message}
            </div>
          ))}
          {patch.conflicts.map((c) => (
            <div
              key={`${c.fixture_a}-${c.fixture_b}-${c.universe}-${c.overlap_start}`}
              className="issue"
            >
              <strong>
                {c.fixture_a} ↔ {c.fixture_b}
              </strong>{" "}
              overlap on universe {c.universe} channels {c.overlap_start}…
              {c.overlap_start + c.overlap_len - 1}
            </div>
          ))}
        </section>
      ) : null}
    </main>
  );
}
