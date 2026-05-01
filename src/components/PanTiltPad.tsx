import { useCallback, useRef } from "react";

export interface PanTiltPadProps {
  pan: number;
  tilt: number;
  onChange: (pan: number, tilt: number) => void;
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, n));
}

export function PanTiltPad({ pan, tilt, onChange }: PanTiltPadProps) {
  const ref = useRef<HTMLDivElement | null>(null);

  const update = useCallback(
    (clientX: number, clientY: number) => {
      const el = ref.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const fx = clamp((clientX - rect.left) / rect.width, 0, 1);
      const fy = clamp((clientY - rect.top) / rect.height, 0, 1);
      // Pan: X axis, 0 left → 255 right.
      // Tilt: Y axis inverted, top of pad = max (255), bottom = 0.
      const newPan = Math.round(fx * 255);
      const newTilt = Math.round((1 - fy) * 255);
      onChange(newPan, newTilt);
    },
    [onChange],
  );

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    update(e.clientX, e.clientY);
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if ((e.buttons & 1) === 0) return;
    update(e.clientX, e.clientY);
  };

  const cursorX = (pan / 255) * 100;
  const cursorY = (1 - tilt / 255) * 100;

  return (
    <div
      ref={ref}
      className="pan-tilt-pad"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      aria-label={`Pan-Tilt pad — Pan ${pan} Tilt ${tilt}`}
    >
      <div className="pad-grid" />
      <div className="pad-crosshair-h" />
      <div className="pad-crosshair-v" />
      <div className="pad-cursor" style={{ left: `${cursorX}%`, top: `${cursorY}%` }} />
    </div>
  );
}
