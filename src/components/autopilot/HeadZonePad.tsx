import type { HeadZone } from "@bindings/HeadZone";
import { useRef } from "react";

// 2-D pan/tilt pad for one moving head: the rectangle is the window the
// autopilot may aim inside (fractions of the fixture's physical range).
// Drag the body to move it, the corners to resize it.

interface Props {
  zone: HeadZone;
  onChange: (zone: HeadZone) => void;
  /** Called with a normalised point when the operator wants to aim the real head there. */
  onPoint: (pan: number, tilt: number) => void;
}

type Drag =
  | { kind: "move"; x0: number; y0: number; z: HeadZone }
  | { kind: "corner"; cx: "min" | "max"; cy: "min" | "max" };

const MIN_SIZE = 0.05;

function clamp01(v: number): number {
  return Math.max(0, Math.min(1, v));
}

export function HeadZonePad({ zone, onChange, onPoint }: Props) {
  const ref = useRef<HTMLDivElement | null>(null);
  const drag = useRef<Drag | null>(null);

  const norm = (e: React.PointerEvent) => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return { x: 0, y: 0 };
    return {
      x: clamp01((e.clientX - r.left) / r.width),
      // Screen y grows downward; tilt "up" is 1 at the top of the pad.
      y: clamp01(1 - (e.clientY - r.top) / r.height),
    };
  };

  const onDown = (e: React.PointerEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    const corner = target.dataset.corner;
    e.currentTarget.setPointerCapture(e.pointerId);
    if (corner) {
      const [cx, cy] = corner.split("-") as ["min" | "max", "min" | "max"];
      drag.current = { kind: "corner", cx, cy };
      return;
    }
    const { x, y } = norm(e);
    const inside =
      x >= zone.pan_min && x <= zone.pan_max && y >= zone.tilt_min && y <= zone.tilt_max;
    if (inside) {
      drag.current = { kind: "move", x0: x, y0: y, z: { ...zone } };
    } else if (e.altKey || e.metaKey) {
      onPoint(x, y);
    }
  };

  const onMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const { x, y } = norm(e);
    if (d.kind === "move") {
      const w = d.z.pan_max - d.z.pan_min;
      const h = d.z.tilt_max - d.z.tilt_min;
      const px = clamp01(Math.min(1 - w, Math.max(0, d.z.pan_min + (x - d.x0))));
      const ty = clamp01(Math.min(1 - h, Math.max(0, d.z.tilt_min + (y - d.y0))));
      onChange({ ...zone, pan_min: px, pan_max: px + w, tilt_min: ty, tilt_max: ty + h });
    } else {
      const next = { ...zone };
      if (d.cx === "min") next.pan_min = Math.min(x, zone.pan_max - MIN_SIZE);
      else next.pan_max = Math.max(x, zone.pan_min + MIN_SIZE);
      if (d.cy === "min") next.tilt_min = Math.min(y, zone.tilt_max - MIN_SIZE);
      else next.tilt_max = Math.max(y, zone.tilt_min + MIN_SIZE);
      onChange(next);
    }
  };

  const onUp = () => {
    drag.current = null;
  };

  const left = `${zone.pan_min * 100}%`;
  const width = `${(zone.pan_max - zone.pan_min) * 100}%`;
  const top = `${(1 - zone.tilt_max) * 100}%`;
  const height = `${(zone.tilt_max - zone.tilt_min) * 100}%`;

  return (
    <div
      ref={ref}
      className="zone-pad"
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={onUp}
    >
      <div className="zone-pad__grid" aria-hidden="true" />
      <div className="zone-pad__rect" style={{ left, width, top, height }}>
        <span className="zone-pad__corner" data-corner="min-max" style={{ left: 0, top: 0 }} />
        <span className="zone-pad__corner" data-corner="max-max" style={{ right: 0, top: 0 }} />
        <span className="zone-pad__corner" data-corner="min-min" style={{ left: 0, bottom: 0 }} />
        <span className="zone-pad__corner" data-corner="max-min" style={{ right: 0, bottom: 0 }} />
      </div>
      <span className="zone-pad__axis zone-pad__axis--x">pan →</span>
      <span className="zone-pad__axis zone-pad__axis--y">tilt ↑</span>
    </div>
  );
}
