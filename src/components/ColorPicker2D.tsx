import { useCallback, useMemo, useRef } from "react";

export interface ColorPicker2DProps {
  r: number;
  g: number;
  b: number;
  onChange: (r: number, g: number, b: number) => void;
}

function clamp(n: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, n));
}

function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const hp = (h / 60) % 6;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  let r1 = 0;
  let g1 = 0;
  let b1 = 0;
  if (hp < 1) [r1, g1, b1] = [c, x, 0];
  else if (hp < 2) [r1, g1, b1] = [x, c, 0];
  else if (hp < 3) [r1, g1, b1] = [0, c, x];
  else if (hp < 4) [r1, g1, b1] = [0, x, c];
  else if (hp < 5) [r1, g1, b1] = [x, 0, c];
  else [r1, g1, b1] = [c, 0, x];
  const m = l - c / 2;
  return [Math.round((r1 + m) * 255), Math.round((g1 + m) * 255), Math.round((b1 + m) * 255)];
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const rn = r / 255;
  const gn = g / 255;
  const bn = b / 255;
  const max = Math.max(rn, gn, bn);
  const min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  let h = 0;
  let s = 0;
  if (max !== min) {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    if (max === rn) h = ((gn - bn) / d + (gn < bn ? 6 : 0)) * 60;
    else if (max === gn) h = ((bn - rn) / d + 2) * 60;
    else h = ((rn - gn) / d + 4) * 60;
  }
  return [h, s, l];
}

export function ColorPicker2D({ r, g, b, onChange }: ColorPicker2DProps) {
  const ref = useRef<HTMLDivElement | null>(null);

  // The 2D picker fixes saturation at 100% and uses (hue, lightness) for the
  // axes. White lives at the top, the pure colour band in the middle, black
  // at the bottom — the gradient that DJs / lighting ops are used to.
  const cursor = useMemo(() => {
    const [h, _s, l] = rgbToHsl(r, g, b);
    return { x: (h / 360) * 100, y: (1 - l) * 100 };
  }, [r, g, b]);

  const update = useCallback(
    (clientX: number, clientY: number) => {
      const el = ref.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const fx = clamp((clientX - rect.left) / rect.width, 0, 1);
      const fy = clamp((clientY - rect.top) / rect.height, 0, 1);
      const hue = fx * 360;
      const lightness = 1 - fy;
      const [nr, ng, nb] = hslToRgb(hue, 1, lightness);
      onChange(nr, ng, nb);
    },
    [onChange],
  );

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    update(e.clientX, e.clientY);
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    // Only drag while the primary button is held.
    if ((e.buttons & 1) === 0) return;
    update(e.clientX, e.clientY);
  };

  return (
    <div
      ref={ref}
      className="picker-2d"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      aria-label={`Color picker — R${r} G${g} B${b}`}
    >
      <div className="hue-bg" />
      <div className="lightness-overlay" />
      <div className="cursor" style={{ left: `${cursor.x}%`, top: `${cursor.y}%` }} />
    </div>
  );
}
