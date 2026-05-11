// Annotation primitives. Each annotation describes one shape (circle,
// rect, arrow, callout) anchored to either a CSS selector or an explicit
// point. The capture pipeline composites them into one SVG overlay
// injected into the page right before snapshot, so they render at
// native resolution alongside the UI.
//
// Layout has two passes:
//   1. Static annotations (rect/circle/arrow) are anchored to their
//      target — they can't move. They paint first and feed an
//      `occupied` list of their bounding boxes.
//   2. Callouts have flexible placement. For each one, we try the
//      author's preferred placement first, then right/bottom/top/left
//      in order, picking the first one whose bubble doesn't overlap
//      anything in `occupied` (or any target rect). The chosen
//      bubble's box is then pushed into `occupied` so the next
//      callout sees it and routes around it.
//
// The detector is intentionally local: it doesn't know about the
// underlying UI, only about other annotations. That keeps the rule
// simple — annotations don't sit on top of each other — without
// trying to be a general-purpose layout engine.

export type Point = { x: number; y: number };
export type AnnotationTarget = string | Point;

export type Annotation =
  | { type: "circle"; target: AnnotationTarget; number?: number; padding?: number; color?: string }
  | { type: "rect"; target: AnnotationTarget; number?: number; padding?: number; color?: string }
  | { type: "arrow"; from: AnnotationTarget; to: AnnotationTarget; number?: number; color?: string }
  | { type: "callout"; target: AnnotationTarget; text: string; number?: number; placement?: "top" | "bottom" | "left" | "right"; color?: string };

export const DEFAULT_COLOR = "#ff3b30"; // matches the app's red accent so annotations feel native

// Browser-side overlay installer. The function is shipped to the page
// via Playwright's `page.evaluate(fn, args)`, which serializes the
// function source — keep it self-contained and pass external state via
// `args`. Returns nothing; just appends the SVG to `document.body`.
export function installOverlayInPage(
  args: { annotations: Annotation[]; viewport: { width: number; height: number }; defaultColor: string },
): void {
  const { annotations, viewport, defaultColor } = args;
  const NS = "http://www.w3.org/2000/svg";

  type Rect = { x: number; y: number; width: number; height: number };

  function resolveTarget(target: unknown): Rect | null {
    if (typeof target === "string") {
      const el = document.querySelector(target);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { x: r.left, y: r.top, width: r.width, height: r.height };
    }
    const p = target as { x: number; y: number };
    return { x: p.x, y: p.y, width: 1, height: 1 };
  }

  function setAttrs(el: Element, attrs: Record<string, string | number>) {
    for (const k of Object.keys(attrs)) el.setAttribute(k, String(attrs[k]));
  }

  // Pixel-amount of overlap between two rects on each axis (0 if they
  // don't intersect on that axis). Sum-axis area would be more
  // accurate but the consumers only need a "is this real overlap or
  // just a one-pixel kiss" distinction, so width × height of the
  // intersection is enough.
  function overlapArea(a: Rect, b: Rect): number {
    const dx = Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x);
    const dy = Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
    return dx > 0 && dy > 0 ? dx * dy : 0;
  }

  // Treat overlaps under this threshold (in px²) as "close enough" so
  // a one- or two-pixel kiss between a callout and a neighbouring
  // badge doesn't force a worse placement. Tuned empirically against
  // the existing capture set.
  const OVERLAP_TOLERANCE = 16;

  // Keep the badge fully visible — viewport edges crop circles drawn at
  // negative or overflowing coordinates, which is the most common
  // cosmetic bug when an annotated element sits flush against the top
  // or right of the screen.
  const BADGE_R = 14;
  function clampBadge(x: number, y: number) {
    const r = BADGE_R + 2;
    return {
      x: Math.min(Math.max(x, r), viewport.width - r),
      y: Math.min(Math.max(y, r), viewport.height - r),
    };
  }

  function badge(x: number, y: number, n: number, color: string): { node: SVGGElement; rect: Rect } {
    const p = clampBadge(x, y);
    const g = document.createElementNS(NS, "g") as SVGGElement;
    const c = document.createElementNS(NS, "circle");
    setAttrs(c, { cx: p.x, cy: p.y, r: BADGE_R, fill: color, stroke: "white", "stroke-width": 2 });
    const t = document.createElementNS(NS, "text");
    setAttrs(t, { x: p.x, y: p.y + 5, "text-anchor": "middle", fill: "white", "font-size": 16, "font-family": "system-ui, -apple-system, sans-serif", "font-weight": 700 });
    t.textContent = String(n);
    g.appendChild(c); g.appendChild(t);
    return { node: g, rect: { x: p.x - BADGE_R, y: p.y - BADGE_R, width: BADGE_R * 2, height: BADGE_R * 2 } };
  }

  // Wipe any previous overlay so re-running on the same page is idempotent.
  const old = document.getElementById("__doc_overlay__");
  if (old) old.remove();

  const svg = document.createElementNS(NS, "svg");
  setAttrs(svg, { width: viewport.width, height: viewport.height, viewBox: `0 0 ${viewport.width} ${viewport.height}` });
  svg.id = "__doc_overlay__";
  svg.style.cssText = "position:fixed;top:0;left:0;width:100vw;height:100vh;pointer-events:none;z-index:2147483647;";

  const defs = document.createElementNS(NS, "defs");
  defs.innerHTML = `<marker id="dochead" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="${defaultColor}" /></marker>`;
  svg.appendChild(defs);

  // Tracks every painted shape's bounding box plus the targets we
  // explicitly want to keep visible. Callout placement reads this.
  const occupied: Rect[] = [];

  // -------- Pass 1: anchored annotations (rect/circle/arrow) --------
  // Note: rect and circle are *outlines* delimiting an area, not solid
  // obstacles. A callout sitting inside a panel-sized rect outline is
  // fine; what we don't want is a callout sitting on top of someone
  // else's badge or callout. So we only push the *badges* into
  // `occupied`, never the outline itself. This keeps the available
  // placement space large enough that the fallback chain has somewhere
  // useful to land.
  for (const a of annotations) {
    const color = a.color || defaultColor;
    if (a.type === "circle") {
      const r = resolveTarget(a.target);
      if (!r) continue;
      const pad = a.padding ?? 8;
      const cx = r.x + r.width / 2;
      const cy = r.y + r.height / 2;
      const radius = Math.max(r.width, r.height) / 2 + pad;
      const ring = document.createElementNS(NS, "circle");
      setAttrs(ring, { cx, cy, r: radius, fill: "none", stroke: color, "stroke-width": 3 });
      svg.appendChild(ring);
      if (a.number != null) {
        const b = badge(cx + radius * 0.7, cy - radius * 0.7, a.number, color);
        svg.appendChild(b.node);
        occupied.push(b.rect);
      }
    } else if (a.type === "rect") {
      const r = resolveTarget(a.target);
      if (!r) continue;
      const pad = a.padding ?? 6;
      const rect = document.createElementNS(NS, "rect");
      const rx = r.x - pad;
      const ry = r.y - pad;
      const rw = r.width + pad * 2;
      const rh = r.height + pad * 2;
      setAttrs(rect, { x: rx, y: ry, width: rw, height: rh, fill: "none", stroke: color, "stroke-width": 3, rx: 6 });
      svg.appendChild(rect);
      if (a.number != null) {
        const b = badge(r.x + r.width + pad + 6, r.y - pad - 6, a.number, color);
        svg.appendChild(b.node);
        occupied.push(b.rect);
      }
    } else if (a.type === "arrow") {
      const from = resolveTarget(a.from);
      const to = resolveTarget(a.to);
      if (!from || !to) continue;
      const x1 = from.x + from.width / 2;
      const y1 = from.y + from.height / 2;
      const x2 = to.x + to.width / 2;
      const y2 = to.y + to.height / 2;
      const line = document.createElementNS(NS, "line");
      setAttrs(line, { x1, y1, x2, y2, stroke: color, "stroke-width": 3, "marker-end": "url(#dochead)" });
      svg.appendChild(line);
      // The line itself is thin; only the endpoint badge actually
      // occupies space worth dodging.
      if (a.number != null) {
        const b = badge(x1, y1, a.number, color);
        svg.appendChild(b.node);
        occupied.push(b.rect);
      }
    }
  }

  // -------- Pass 2: callouts with collision-aware placement --------
  type Placement = "top" | "bottom" | "left" | "right";
  const PLACEMENTS: Placement[] = ["right", "bottom", "top", "left"];
  const OFFSET = 24;
  const FONT_SIZE = 14;
  const PAD_X = 10;
  const PAD_Y = 8;

  // Pre-seed occupied with every callout's target rect, so callout #1
  // doesn't accidentally land on top of callout #2's target. Without
  // this, processing order would matter: the first callout claims the
  // most flexible placement and later callouts get squeezed into bad
  // ones that obscure the very element they are pointing at.
  for (const a of annotations) {
    if (a.type !== "callout") continue;
    const r = resolveTarget(a.target);
    if (r) occupied.push({ x: r.x, y: r.y, width: r.width, height: r.height });
  }

  function bubblePosition(target: Rect, boxWidth: number, boxHeight: number, placement: Placement): Rect {
    let tx =
      placement === "right" ? target.x + target.width + OFFSET
      : placement === "left" ? target.x - OFFSET - boxWidth
      : target.x;
    let ty =
      placement === "top" ? target.y - OFFSET - boxHeight
      : placement === "bottom" ? target.y + target.height + OFFSET
      : target.y + target.height / 2 - boxHeight / 2;
    tx = Math.min(Math.max(tx, 4), viewport.width - boxWidth - 4);
    ty = Math.min(Math.max(ty, 4), viewport.height - boxHeight - 4);
    return { x: tx, y: ty, width: boxWidth, height: boxHeight };
  }

  for (const a of annotations) {
    if (a.type !== "callout") continue;
    const r = resolveTarget(a.target);
    if (!r) continue;
    const color = a.color || defaultColor;

    // Measure text by appending it offscreen, reading bbox, then we
    // reuse the same node when we paint.
    const txt = document.createElementNS(NS, "text") as SVGTextElement;
    setAttrs(txt, { x: -9999, y: -9999, fill: "white", "font-size": FONT_SIZE, "font-family": "system-ui, -apple-system, sans-serif", "font-weight": 600 });
    txt.textContent = a.text;
    svg.appendChild(txt);
    const bbox = txt.getBBox();
    const boxWidth = bbox.width + PAD_X * 2;
    const boxHeight = bbox.height + PAD_Y * 2;

    // Try placements in order: author's preferred first, then the
    // remaining ones in the standard fallback order. The target rect
    // is included in the no-overlap set so the bubble never sits on
    // top of the very thing it's labelling.
    const preferred = a.placement || "right";
    const order: Placement[] = [preferred, ...PLACEMENTS.filter((p) => p !== preferred)];
    let chosen = bubblePosition(r, boxWidth, boxHeight, preferred);
    let chosenPlacement: Placement = preferred;
    const targetRect: Rect = { x: r.x, y: r.y, width: r.width, height: r.height };
    // Score each candidate by total overlap and pick the lowest. If
    // the preferred placement is within tolerance, win immediately so
    // we don't churn through the rest of the order chasing a tie.
    let bestScore = Number.POSITIVE_INFINITY;
    for (const p of order) {
      const candidate = bubblePosition(r, boxWidth, boxHeight, p);
      const score =
        occupied.reduce((acc, o) => acc + overlapArea(candidate, o), 0) +
        overlapArea(candidate, targetRect) * 2; // double-weight target overlap — never sit on what you're labelling
      if (score <= OVERLAP_TOLERANCE) {
        chosen = candidate;
        chosenPlacement = p;
        bestScore = score;
        break;
      }
      if (score < bestScore) {
        chosen = candidate;
        chosenPlacement = p;
        bestScore = score;
      }
    }

    // Paint bubble (rect first, text on top, connector behind).
    const box = document.createElementNS(NS, "rect");
    setAttrs(box, { x: chosen.x, y: chosen.y, width: boxWidth, height: boxHeight, rx: 6, fill: color });
    svg.appendChild(box);

    txt.setAttribute("x", String(chosen.x + PAD_X));
    // Use the bbox y-offset to compensate for fonts whose baseline
    // sits below the top of the glyphs (descenders push bbox.y
    // negative). This keeps the visual top-left of the text aligned
    // with the bubble's top-left padding, regardless of the script.
    txt.setAttribute("y", String(chosen.y + PAD_Y - bbox.y));

    const targetCx = r.x + r.width / 2;
    const targetCy = r.y + r.height / 2;
    const cx =
      chosenPlacement === "left" ? chosen.x + boxWidth
      : chosenPlacement === "right" ? chosen.x
      : chosen.x + boxWidth / 2;
    const cy =
      chosenPlacement === "top" ? chosen.y + boxHeight
      : chosenPlacement === "bottom" ? chosen.y
      : chosen.y + boxHeight / 2;
    const link = document.createElementNS(NS, "line");
    setAttrs(link, { x1: cx, y1: cy, x2: targetCx, y2: targetCy, stroke: color, "stroke-width": 2, "stroke-dasharray": "4 3" });
    svg.insertBefore(link, box);

    // Reorder so text sits in front of the bubble.
    svg.appendChild(txt);

    // Badge on the corner of the bubble *opposite* the target so it
    // never sits on top of the caption.
    if (a.number != null) {
      const bx =
        chosenPlacement === "right" || chosenPlacement === "bottom"
          ? chosen.x + boxWidth + 6
          : chosen.x - 6;
      const by = chosenPlacement === "top" ? chosen.y + boxHeight + 6 : chosen.y - 6;
      const b = badge(bx, by, a.number, color);
      svg.appendChild(b.node);
      occupied.push(b.rect);
    }

    occupied.push(chosen);
    occupied.push(targetRect);
  }

  document.body.appendChild(svg);
}
