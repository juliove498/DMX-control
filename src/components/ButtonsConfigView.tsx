import type { BlackoutFixture } from "@bindings/BlackoutFixture";
import type { BlindFixture } from "@bindings/BlindFixture";
import type { ChannelDefinition } from "@bindings/ChannelDefinition";
import type { ChannelRole } from "@bindings/ChannelRole";
import type { FixtureDefinition } from "@bindings/FixtureDefinition";
import type { FixtureInstance } from "@bindings/FixtureInstance";
import type { GlobalsConfig } from "@bindings/GlobalsConfig";
import type { MasterFixture } from "@bindings/MasterFixture";
import { useT } from "../i18n";
import type { Translation } from "../i18n/translations";
import { useShowStore } from "../stores/show";

const DEFAULT_GLOBALS: GlobalsConfig = {
  blackout: { active: false, fade_in_ms: 200, fade_out_ms: 800, fixtures: [] },
  blind: { fade_in_ms: 80, fade_out_ms: 1500, fixtures: [] },
  master: { fixtures: [] },
};

/// Mirror of `ChannelRole::label()` — returns the snake-case role name,
/// or the inner string for `Other("foo")`. Used to identify each channel
/// when the user toggles "drive at 255 on blind" or "scale by master".
function roleLabel(role: ChannelRole | unknown): string {
  if (typeof role === "string") return role;
  if (role && typeof role === "object" && "other" in role) {
    const inner = (role as { other: unknown }).other;
    if (typeof inner === "string" && inner.length > 0) return inner;
  }
  return "other";
}

export function ButtonsConfigView() {
  const t = useT();
  const show = useShowStore((s) => s.show);
  const library = useShowStore((s) => s.library);
  const updateGlobals = useShowStore((s) => s.updateGlobals);

  if (!show) return <main className="page">{t("common.loading")}</main>;
  const fixtures = show.fixtures;
  // Older shows saved before MasterConfig existed deserialise without
  // `master`. The Rust side fills it via `#[serde(default)]` but the
  // TS-side ShowFileV1 binding still types the field as required, so
  // we splice the default here defensively.
  const cfg: GlobalsConfig = {
    ...DEFAULT_GLOBALS,
    ...(show.globals ?? {}),
    master: show.globals?.master ?? DEFAULT_GLOBALS.master,
  };

  const libraryById: Record<string, FixtureDefinition> = {};
  for (const d of library) libraryById[d.id] = d;

  const setBlackoutFade = (key: "fade_in_ms" | "fade_out_ms", value: number) =>
    updateGlobals({
      ...cfg,
      blackout: { ...cfg.blackout, [key]: Math.max(0, value) },
    });
  const setBlindFade = (key: "fade_in_ms" | "fade_out_ms", value: number) =>
    updateGlobals({
      ...cfg,
      blind: { ...cfg.blind, [key]: Math.max(0, value) },
    });
  // ---- Master ------------------------------------------------------------
  const setMasterFixtures = (next: MasterFixture[]) =>
    updateGlobals({ ...cfg, master: { ...cfg.master, fixtures: next } });
  const isMasterAssigned = (id: string) =>
    cfg.master.fixtures.some((f) => f.fixture_id === id);
  const toggleMasterFixture = (id: string) => {
    if (isMasterAssigned(id)) {
      setMasterFixtures(cfg.master.fixtures.filter((f) => f.fixture_id !== id));
    } else {
      setMasterFixtures([
        ...cfg.master.fixtures,
        { fixture_id: id, channels_to_scale: [] },
      ]);
    }
  };
  const toggleMasterChannel = (fixtureId: string, role: string) => {
    setMasterFixtures(
      cfg.master.fixtures.map((f) => {
        if (f.fixture_id !== fixtureId) return f;
        const next = f.channels_to_scale.includes(role)
          ? f.channels_to_scale.filter((r) => r !== role)
          : [...f.channels_to_scale, role];
        return { ...f, channels_to_scale: next };
      }),
    );
  };
  // ---- Blackout ----------------------------------------------------------
  const isBlackoutAssigned = (id: string) =>
    cfg.blackout.fixtures.some((f) => f.fixture_id === id);
  const setBlackoutFixtures = (next: BlackoutFixture[]) =>
    updateGlobals({ ...cfg, blackout: { ...cfg.blackout, fixtures: next } });
  const toggleBlackoutFixture = (id: string) => {
    if (isBlackoutAssigned(id)) {
      setBlackoutFixtures(cfg.blackout.fixtures.filter((f) => f.fixture_id !== id));
    } else {
      setBlackoutFixtures([
        ...cfg.blackout.fixtures,
        { fixture_id: id, channels_to_zero: [] },
      ]);
    }
  };
  const toggleBlackoutChannel = (fixtureId: string, role: string) => {
    setBlackoutFixtures(
      cfg.blackout.fixtures.map((f) => {
        if (f.fixture_id !== fixtureId) return f;
        const next = f.channels_to_zero.includes(role)
          ? f.channels_to_zero.filter((r) => r !== role)
          : [...f.channels_to_zero, role];
        return { ...f, channels_to_zero: next };
      }),
    );
  };
  // ---- Blind -------------------------------------------------------------
  const isBlindAssigned = (id: string) => cfg.blind.fixtures.some((f) => f.fixture_id === id);
  const setBlindFixtures = (next: BlindFixture[]) =>
    updateGlobals({ ...cfg, blind: { ...cfg.blind, fixtures: next } });
  const toggleBlindFixture = (id: string) => {
    if (isBlindAssigned(id)) {
      setBlindFixtures(cfg.blind.fixtures.filter((f) => f.fixture_id !== id));
    } else {
      setBlindFixtures([
        ...cfg.blind.fixtures,
        { fixture_id: id, channels_at_full: [] },
      ]);
    }
  };
  const toggleBlindChannel = (fixtureId: string, role: string) => {
    setBlindFixtures(
      cfg.blind.fixtures.map((f) => {
        if (f.fixture_id !== fixtureId) return f;
        const next = f.channels_at_full.includes(role)
          ? f.channels_at_full.filter((r) => r !== role)
          : [...f.channels_at_full, role];
        return { ...f, channels_at_full: next };
      }),
    );
  };

  return (
    <main className="page buttons-config-view">
      <header className="page-head">
        <h2>{t("buttons.title")}</h2>
        <span className="meta">{t("buttons.subhead")}</span>
      </header>

      {/* ---- Master ---- */}
      <FixtureGroupSection
        sectionTitleKey="buttons.section.master"
        introKey="buttons.masterIntro"
        countAutoKey="buttons.assignedMasterAuto"
        countCustomKey="buttons.assignedMaster"
        emptyKey="buttons.emptyMaster"
        autoHintKey="buttons.masterHintAuto"
        customHintKey="buttons.masterHintCustom"
        fixtures={fixtures}
        libraryById={libraryById}
        assignmentForId={(id) => {
          const a = cfg.master.fixtures.find((f) => f.fixture_id === id);
          return a ? { selectedRoles: a.channels_to_scale } : null;
        }}
        onToggleAssignment={toggleMasterFixture}
        onToggleChannel={toggleMasterChannel}
      />

      {/* ---- Blackout ---- */}
      <FixtureGroupSection
        sectionTitleKey="buttons.section.blackout"
        introKey="buttons.blackoutIntro"
        countAutoKey="buttons.assignedBlackoutAuto"
        countCustomKey="buttons.assignedBlackout"
        emptyKey="buttons.emptyBlackout"
        autoHintKey="buttons.blackoutHintAuto"
        customHintKey="buttons.blackoutHintCustom"
        fixtures={fixtures}
        libraryById={libraryById}
        assignmentForId={(id) => {
          const a = cfg.blackout.fixtures.find((f) => f.fixture_id === id);
          return a ? { selectedRoles: a.channels_to_zero } : null;
        }}
        onToggleAssignment={toggleBlackoutFixture}
        onToggleChannel={toggleBlackoutChannel}
        fadeControls={
          <div className="config-grid">
            <label>
              {t("buttons.fadeIn")}
              <input
                type="number"
                min={0}
                max={10000}
                step={50}
                value={cfg.blackout.fade_in_ms}
                onChange={(e) => setBlackoutFade("fade_in_ms", Number(e.currentTarget.value))}
              />
            </label>
            <label>
              {t("buttons.fadeOut")}
              <input
                type="number"
                min={0}
                max={10000}
                step={50}
                value={cfg.blackout.fade_out_ms}
                onChange={(e) => setBlackoutFade("fade_out_ms", Number(e.currentTarget.value))}
              />
            </label>
          </div>
        }
      />

      {/* ---- Blind ---- */}
      <FixtureGroupSection
        sectionTitleKey="buttons.section.blind"
        introKey="buttons.blindIntro"
        // Blind has no "auto mode" key — the count format is the same
        // both ways, just shows the size of the assignment list.
        countAutoKey="buttons.assignedBlind"
        countCustomKey="buttons.assignedBlind"
        emptyKey="buttons.emptyBlind"
        autoHintKey="buttons.blindHintAuto"
        customHintKey="buttons.blindHintCustom"
        fixtures={fixtures}
        libraryById={libraryById}
        assignmentForId={(id) => {
          const a = cfg.blind.fixtures.find((f) => f.fixture_id === id);
          return a ? { selectedRoles: a.channels_at_full } : null;
        }}
        onToggleAssignment={toggleBlindFixture}
        onToggleChannel={toggleBlindChannel}
        fadeControls={
          <div className="config-grid">
            <label>
              {t("buttons.fadeIn")}
              <input
                type="number"
                min={0}
                max={5000}
                step={20}
                value={cfg.blind.fade_in_ms}
                onChange={(e) => setBlindFade("fade_in_ms", Number(e.currentTarget.value))}
              />
            </label>
            <label>
              {t("buttons.fadeOut")}
              <input
                type="number"
                min={0}
                max={10000}
                step={50}
                value={cfg.blind.fade_out_ms}
                onChange={(e) => setBlindFade("fade_out_ms", Number(e.currentTarget.value))}
              />
            </label>
          </div>
        }
      />
    </main>
  );
}

/// Generic "assignment list of fixtures with per-fixture channel roles"
/// — used by all three globals (Master, Blackout, Blind). The wrapper
/// owns the section heading, intro hint, optional fade controls, and
/// the collapsible per-fixture rows. Each row is a `<details>` so the
/// long page that the operator was complaining about now defaults to
/// just three folded summaries with chip lists revealed only when
/// the operator opens an individual fixture.
function FixtureGroupSection({
  sectionTitleKey,
  introKey,
  countAutoKey,
  countCustomKey,
  emptyKey,
  autoHintKey,
  customHintKey,
  fixtures,
  libraryById,
  assignmentForId,
  onToggleAssignment,
  onToggleChannel,
  fadeControls,
}: {
  sectionTitleKey: keyof Translation;
  introKey: keyof Translation;
  countAutoKey: keyof Translation;
  countCustomKey: keyof Translation;
  emptyKey: keyof Translation;
  autoHintKey: keyof Translation;
  customHintKey: keyof Translation;
  fixtures: FixtureInstance[];
  libraryById: Record<string, FixtureDefinition>;
  /** Returns the per-fixture assignment (with the role list) when the
   *  fixture is assigned to this global; `null` otherwise. */
  assignmentForId: (id: string) => { selectedRoles: string[] } | null;
  /** Add or remove the fixture from this global's assignment list. */
  onToggleAssignment: (id: string) => void;
  /** Toggle one role chip on a fixture's per-fixture role list. */
  onToggleChannel: (fixtureId: string, role: string) => void;
  /** Optional fade-time grid rendered below the intro (Blackout / Blind). */
  fadeControls?: React.ReactNode;
}) {
  const t = useT();
  const assignedCount = fixtures.reduce(
    (acc, f) => (assignmentForId(f.id) ? acc + 1 : acc),
    0,
  );
  const isAuto = assignedCount === 0;
  return (
    <section className="config-section">
      <h3>{t(sectionTitleKey)}</h3>
      <p className="hint">{t(introKey)}</p>
      {fadeControls}
      <h4>
        {isAuto
          ? t(countAutoKey, { count: fixtures.length })
          : t(countCustomKey, { count: assignedCount })}
      </h4>
      {fixtures.length === 0 ? (
        <p className="empty">{t(emptyKey)}</p>
      ) : (
        <ul className="blind-fixture-list">
          {fixtures.map((f) => (
            <FixtureRow
              key={f.id}
              fixture={f}
              def={libraryById[f.definition_id]}
              assignment={assignmentForId(f.id)}
              autoHintKey={autoHintKey}
              customHintKey={customHintKey}
              onToggleAssignment={() => onToggleAssignment(f.id)}
              onToggleChannel={(role) => onToggleChannel(f.id, role)}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

/// Single fixture row inside any of the three sections. `<details>` so
/// the heavy channel-chip grid only renders when the user actually
/// opens it. The `<summary>` has the assignment checkbox and a small
/// status badge ("auto" / "N canales"); clicking the checkbox toggles
/// assignment without expanding, clicking the rest of the summary
/// expands.
function FixtureRow({
  fixture,
  def,
  assignment,
  autoHintKey,
  customHintKey,
  onToggleAssignment,
  onToggleChannel,
}: {
  fixture: FixtureInstance;
  def: FixtureDefinition | undefined;
  assignment: { selectedRoles: string[] } | null;
  autoHintKey: keyof Translation;
  customHintKey: keyof Translation;
  onToggleAssignment: () => void;
  onToggleChannel: (role: string) => void;
}) {
  const t = useT();
  const mode = def?.modes[fixture.mode_index];
  const isAssigned = !!assignment;
  const customCount = assignment?.selectedRoles.length ?? 0;
  return (
    <li className={isAssigned ? "assigned" : ""}>
      <details>
        <summary className="blind-fixture-head">
          <input
            type="checkbox"
            checked={isAssigned}
            onChange={onToggleAssignment}
            // Stop the click from bubbling to the <summary>, which
            // would also toggle the open state. The expansion gesture
            // has its own affordance (click anywhere else on the
            // summary) — checkbox should ONLY change assignment.
            onClick={(e) => e.stopPropagation()}
          />
          <span className="blind-fixture-name">{fixture.label ?? fixture.id}</span>
          <span className="blind-fixture-meta">
            U{fixture.universe} · {fixture.address}
          </span>
          {isAssigned ? (
            <span className="blind-fixture-badge">
              {customCount === 0 ? "auto" : `${customCount}ch`}
            </span>
          ) : null}
        </summary>
        {isAssigned && mode ? (
          <div className="blind-fixture-channels">
            <span className="blind-channels-hint">
              {customCount === 0 ? t(autoHintKey) : t(customHintKey)}
            </span>
            <div className="blind-channels-chips">
              {mode.channels.map((ch: ChannelDefinition, i: number) => {
                const role = roleLabel(ch.role);
                const active = assignment?.selectedRoles.includes(role) ?? false;
                return (
                  <label
                    key={`${fixture.id}-${i}-${role}`}
                    className={`blind-channel-chip${active ? " active" : ""}`}
                  >
                    <input
                      type="checkbox"
                      checked={active}
                      onChange={() => onToggleChannel(role)}
                    />
                    {role}
                  </label>
                );
              })}
            </div>
          </div>
        ) : null}
      </details>
    </li>
  );
}
