import type { BlackoutFixture } from "@bindings/BlackoutFixture";
import type { BlindFixture } from "@bindings/BlindFixture";
import type { ChannelRole } from "@bindings/ChannelRole";
import type { GlobalsConfig } from "@bindings/GlobalsConfig";
import { useT } from "../i18n";
import { useShowStore } from "../stores/show";

const DEFAULT_GLOBALS: GlobalsConfig = {
  blackout: { active: false, fade_in_ms: 200, fade_out_ms: 800, fixtures: [] },
  blind: { fade_in_ms: 80, fade_out_ms: 1500, fixtures: [] },
};

/// Mirror of `ChannelRole::label()` — returns the snake-case role name,
/// or the inner string for `Other("foo")`. Used to identify each channel
/// when the user toggles "drive at 255 on blind".
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
  const cfg = show.globals ?? DEFAULT_GLOBALS;

  const libraryById: Record<string, (typeof library)[number]> = {};
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
  // Blackout: fixture/channel selection. Empty fixtures list = "auto"
  // (every patched fixture, intensity-or-RGB + strobe). Otherwise only
  // the listed fixtures' listed channels go to 0; per-fixture empty
  // `channels_to_zero` means "auto for this fixture".
  const isBlackoutAssigned = (id: string) => cfg.blackout.fixtures.some((f) => f.fixture_id === id);
  const setBlackoutFixtures = (next: BlackoutFixture[]) =>
    updateGlobals({ ...cfg, blackout: { ...cfg.blackout, fixtures: next } });
  const toggleBlackoutFixture = (id: string) => {
    if (isBlackoutAssigned(id)) {
      setBlackoutFixtures(cfg.blackout.fixtures.filter((f) => f.fixture_id !== id));
    } else {
      setBlackoutFixtures([...cfg.blackout.fixtures, { fixture_id: id, channels_to_zero: [] }]);
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
  const isAssigned = (id: string) => cfg.blind.fixtures.some((f) => f.fixture_id === id);
  const setBlindFixtures = (next: BlindFixture[]) =>
    updateGlobals({ ...cfg, blind: { ...cfg.blind, fixtures: next } });
  const toggleBlindFixture = (id: string) => {
    if (isAssigned(id)) {
      setBlindFixtures(cfg.blind.fixtures.filter((f) => f.fixture_id !== id));
    } else {
      setBlindFixtures([...cfg.blind.fixtures, { fixture_id: id, channels_at_full: [] }]);
    }
  };
  const toggleChannelAtFull = (fixtureId: string, role: string) => {
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

      <section className="config-section">
        <h3>{t("buttons.section.blackout")}</h3>
        <p className="hint">{t("buttons.blackoutIntro")}</p>
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
        <h4>
          {cfg.blackout.fixtures.length === 0
            ? t("buttons.assignedBlackoutAuto", { count: fixtures.length })
            : t("buttons.assignedBlackout", { count: cfg.blackout.fixtures.length })}
        </h4>
        {fixtures.length === 0 ? (
          <p className="empty">{t("buttons.emptyBlackout")}</p>
        ) : (
          <ul className="blind-fixture-list">
            {fixtures.map((f) => {
              const assigned = cfg.blackout.fixtures.find((b) => b.fixture_id === f.id);
              const def = libraryById[f.definition_id];
              const mode = def?.modes[f.mode_index];
              return (
                <li key={f.id} className={assigned ? "assigned" : ""}>
                  <label className="blind-fixture-head">
                    <input
                      type="checkbox"
                      checked={!!assigned}
                      onChange={() => toggleBlackoutFixture(f.id)}
                    />
                    <span className="blind-fixture-name">{f.label ?? f.id}</span>
                    <span className="blind-fixture-meta">
                      U{f.universe} · {f.address}
                    </span>
                  </label>
                  {assigned && mode ? (
                    <div className="blind-fixture-channels">
                      <span className="blind-channels-hint">
                        {assigned.channels_to_zero.length === 0
                          ? t("buttons.blackoutHintAuto")
                          : t("buttons.blackoutHintCustom")}
                      </span>
                      <div className="blind-channels-chips">
                        {mode.channels.map((ch, i) => {
                          const role = roleLabel(ch.role);
                          const active = assigned.channels_to_zero.includes(role);
                          return (
                            <label
                              key={`${f.id}-${i}-${role}`}
                              className={`blind-channel-chip${active ? " active" : ""}`}
                            >
                              <input
                                type="checkbox"
                                checked={active}
                                onChange={() => toggleBlackoutChannel(f.id, role)}
                              />
                              {role}
                            </label>
                          );
                        })}
                      </div>
                    </div>
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}
      </section>

      <section className="config-section">
        <h3>{t("buttons.section.blind")}</h3>
        <p className="hint">{t("buttons.blindIntro")}</p>
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
        <h4>{t("buttons.assignedBlind", { count: cfg.blind.fixtures.length })}</h4>
        {fixtures.length === 0 ? (
          <p className="empty">{t("buttons.emptyBlind")}</p>
        ) : (
          <ul className="blind-fixture-list">
            {fixtures.map((f) => {
              const assigned = cfg.blind.fixtures.find((b) => b.fixture_id === f.id);
              const def = libraryById[f.definition_id];
              const mode = def?.modes[f.mode_index];
              return (
                <li key={f.id} className={assigned ? "assigned" : ""}>
                  <label className="blind-fixture-head">
                    <input
                      type="checkbox"
                      checked={!!assigned}
                      onChange={() => toggleBlindFixture(f.id)}
                    />
                    <span className="blind-fixture-name">{f.label ?? f.id}</span>
                    <span className="blind-fixture-meta">
                      U{f.universe} · {f.address}
                    </span>
                  </label>
                  {assigned && mode ? (
                    <div className="blind-fixture-channels">
                      <span className="blind-channels-hint">
                        {assigned.channels_at_full.length === 0
                          ? t("buttons.blindHintAuto")
                          : t("buttons.blindHintCustom")}
                      </span>
                      <div className="blind-channels-chips">
                        {mode.channels.map((ch, i) => {
                          const role = roleLabel(ch.role);
                          const active = assigned.channels_at_full.includes(role);
                          return (
                            <label
                              key={`${f.id}-${i}-${role}`}
                              className={`blind-channel-chip${active ? " active" : ""}`}
                            >
                              <input
                                type="checkbox"
                                checked={active}
                                onChange={() => toggleChannelAtFull(f.id, role)}
                              />
                              {role}
                            </label>
                          );
                        })}
                      </div>
                    </div>
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </main>
  );
}
