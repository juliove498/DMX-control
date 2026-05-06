import { useEffect } from "react";
import { FixedSizeList as List } from "react-window";
import { useT } from "../i18n";
import { useEngineStore } from "../stores/engine";

const ROW_HEIGHT = 32;
const VISIBLE_ROWS = 18;

export function DirectOutput() {
  const t = useT();
  const channels = useEngineStore((s) => s.channels);
  const master = useEngineStore((s) => s.master);
  const blackout = useEngineStore((s) => s.blackout);
  const stats = useEngineStore((s) => s.stats);
  const setChannel = useEngineStore((s) => s.setChannel);
  const setMaster = useEngineStore((s) => s.setMaster);
  const setBlackout = useEngineStore((s) => s.setBlackout);
  const clearAll = useEngineStore((s) => s.clearAll);
  const refresh = useEngineStore((s) => s.refresh);
  const initListeners = useEngineStore((s) => s.initListeners);

  useEffect(() => {
    refresh(0);
    let unlisten: (() => void) | null = null;
    initListeners().then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, [refresh, initListeners]);

  const Row = ({ index, style }: { index: number; style: React.CSSProperties }) => {
    const value = channels[index] ?? 0;
    return (
      <div className="row" style={style}>
        <span className="ch-label">{String(index + 1).padStart(3, "0")}</span>
        <input
          type="range"
          min={0}
          max={255}
          value={value}
          onChange={(e) => setChannel(0, index, Number(e.currentTarget.value))}
        />
        <span className="ch-value">{value}</span>
      </div>
    );
  };

  return (
    <main className="direct-output">
      <header className="toolbar">
        <div className="title">{t("direct.title")}</div>
        <div className="status">
          <span className={`fps ${stats && stats.fps > 13 ? "ok" : "warn"}`}>
            {stats ? t("direct.fpsValue", { fps: stats.fps.toFixed(1) }) : t("direct.fpsNone")}
          </span>
          {stats ? (
            <span className="meta">
              {t("direct.statsMeta", {
                frames: stats.frames_total,
                late: stats.late_frames,
                ms: (stats.max_frame_micros / 1000).toFixed(1),
              })}
            </span>
          ) : null}
        </div>
      </header>

      <section className="master">
        <label>
          {t("direct.master")}
          <input
            type="range"
            min={0}
            max={255}
            value={master}
            onChange={(e) => setMaster(Number(e.currentTarget.value))}
          />
          <span className="ch-value">{master}</span>
        </label>
        <button
          type="button"
          className={blackout ? "blackout active" : "blackout"}
          onClick={() => setBlackout(!blackout)}
        >
          {blackout ? t("direct.blackoutOn") : t("direct.blackout")}
        </button>
        <button type="button" onClick={() => clearAll()}>
          {t("direct.clearAll")}
        </button>
      </section>

      <section className="channels">
        <List
          height={ROW_HEIGHT * VISIBLE_ROWS}
          itemCount={channels.length}
          itemSize={ROW_HEIGHT}
          width="100%"
        >
          {Row}
        </List>
      </section>
    </main>
  );
}
