import { TransformControls } from "@react-three/drei";
import { useEffect, useMemo, useRef } from "react";
import * as THREE from "three";
import {
  type FixturePlacement,
  type StageConfig,
  type TrussSegment,
  defaultFixturePlacement,
} from "./stageConfig";

// Drag-and-drop rigging: a transform gizmo on the selected fixture or
// truss writes straight into the StageConfig, so the operator lays out a
// new venue by dragging instead of typing coordinates. Fixtures dropped
// onto a truss snap to it (and follow it afterwards).

export type RigSelection =
  | { kind: "fixture"; id: string }
  | { kind: "truss"; id: string }
  | { kind: "trussEnd"; id: string; end: "from" | "to" }
  | null;

export type EditTool = "translate" | "rotate";

interface Props {
  config: StageConfig;
  onChange: (next: StageConfig) => void;
  selection: RigSelection;
  onSelect: (sel: RigSelection) => void;
  tool: EditTool;
  /** Current world position of every fixture (after truss/free resolution). */
  positions: Record<string, [number, number, number]>;
}

const SNAP_XZ = 0.45;
const SNAP_Y = 0.7;

function trussById(config: StageConfig, id: string): TrussSegment | undefined {
  return config.trusses.find((t) => t.id === id);
}

/** Closest point on a truss segment (XZ) → t in [0,1] and distance. */
function projectOnTruss(t: TrussSegment, x: number, z: number): { tt: number; dist: number } {
  const dx = t.toX - t.fromX;
  const dz = t.toZ - t.fromZ;
  const len2 = dx * dx + dz * dz;
  const tt =
    len2 > 1e-9 ? Math.max(0, Math.min(1, ((x - t.fromX) * dx + (z - t.fromZ) * dz) / len2)) : 0;
  const px = t.fromX + dx * tt;
  const pz = t.fromZ + dz * tt;
  return { tt, dist: Math.hypot(px - x, pz - z) };
}

export function RigEditor({ config, onChange, selection, onSelect, tool, positions }: Props) {
  const proxy = useMemo(() => {
    const o = new THREE.Object3D();
    o.rotation.order = "YXZ";
    return o;
  }, []);
  const dragging = useRef(false);
  const configRef = useRef(config);
  configRef.current = config;

  // Park the gizmo on whatever got selected (unless mid-drag).
  useEffect(() => {
    if (dragging.current || !selection) return;
    if (selection.kind === "fixture") {
      const p = positions[selection.id];
      if (!p) return;
      proxy.position.set(p[0], p[1], p[2]);
      const pl = config.fixturePlacements[selection.id];
      proxy.rotation.set(pl?.aimPitch ?? 0, pl?.aimYaw ?? 0, 0);
    } else if (selection.kind === "truss") {
      const t = trussById(config, selection.id);
      if (!t) return;
      proxy.position.set((t.fromX + t.toX) / 2, t.height, (t.fromZ + t.toZ) / 2);
      proxy.rotation.set(0, 0, 0);
    } else {
      const t = trussById(config, selection.id);
      if (!t) return;
      const x = selection.end === "from" ? t.fromX : t.toX;
      const z = selection.end === "from" ? t.fromZ : t.toZ;
      proxy.position.set(x, t.height, z);
      proxy.rotation.set(0, 0, 0);
    }
  }, [selection, positions, config, proxy]);

  const applyChange = () => {
    const cfg = configRef.current;
    if (!selection) return;
    if (selection.kind === "fixture") {
      const cur = cfg.fixturePlacements[selection.id] ?? defaultFixturePlacement();
      let next: FixturePlacement;
      if (tool === "rotate") {
        next = { ...cur, aimYaw: proxy.rotation.y, aimPitch: proxy.rotation.x };
      } else {
        next = {
          ...cur,
          mode: "free",
          free: { x: proxy.position.x, y: Math.max(0, proxy.position.y), z: proxy.position.z },
        };
      }
      onChange({ ...cfg, fixturePlacements: { ...cfg.fixturePlacements, [selection.id]: next } });
      return;
    }
    const t = trussById(cfg, selection.id);
    if (!t) return;
    let patched: TrussSegment;
    if (selection.kind === "truss") {
      const mx = (t.fromX + t.toX) / 2;
      const mz = (t.fromZ + t.toZ) / 2;
      const dx = proxy.position.x - mx;
      const dz = proxy.position.z - mz;
      patched = {
        ...t,
        fromX: t.fromX + dx,
        toX: t.toX + dx,
        fromZ: t.fromZ + dz,
        toZ: t.toZ + dz,
        height: Math.max(0.5, proxy.position.y),
      };
    } else if (selection.end === "from") {
      patched = {
        ...t,
        fromX: proxy.position.x,
        fromZ: proxy.position.z,
        height: Math.max(0.5, proxy.position.y),
      };
    } else {
      patched = {
        ...t,
        toX: proxy.position.x,
        toZ: proxy.position.z,
        height: Math.max(0.5, proxy.position.y),
      };
    }
    onChange({ ...cfg, trusses: cfg.trusses.map((s) => (s.id === t.id ? patched : s)) });
  };

  // Drop onto a truss: fixtures released within reach attach to it.
  const onRelease = () => {
    dragging.current = false;
    const cfg = configRef.current;
    if (!selection || selection.kind !== "fixture" || tool !== "translate") return;
    let best: { truss: TrussSegment; tt: number; dist: number } | null = null;
    for (const truss of cfg.trusses) {
      if (Math.abs(truss.height - proxy.position.y) > SNAP_Y) continue;
      const { tt, dist } = projectOnTruss(truss, proxy.position.x, proxy.position.z);
      if (dist <= SNAP_XZ && (!best || dist < best.dist)) best = { truss, tt, dist };
    }
    if (!best) return;
    const cur = cfg.fixturePlacements[selection.id] ?? defaultFixturePlacement();
    onChange({
      ...cfg,
      fixturePlacements: {
        ...cfg.fixturePlacements,
        [selection.id]: { ...cur, mode: "truss", truss: { trussId: best.truss.id, t: best.tt } },
      },
    });
  };

  const selectedTruss =
    selection && selection.kind !== "fixture" ? trussById(config, selection.id) : undefined;
  const gizmoMode: EditTool = selection?.kind === "fixture" ? tool : "translate";

  return (
    <group>
      <primitive object={proxy} />
      {selection ? (
        <TransformControls
          object={proxy}
          mode={gizmoMode}
          size={0.75}
          onMouseDown={() => {
            dragging.current = true;
          }}
          onMouseUp={onRelease}
          onObjectChange={applyChange}
        />
      ) : null}
      {/* Endpoint handles for the selected truss. */}
      {selectedTruss
        ? (["from", "to"] as const).map((end) => {
            const x = end === "from" ? selectedTruss.fromX : selectedTruss.toX;
            const z = end === "from" ? selectedTruss.fromZ : selectedTruss.toZ;
            const active = selection?.kind === "trussEnd" && selection.end === end;
            return (
              // biome-ignore lint/a11y/useKeyWithClickEvents: three.js mesh picked by raycast, not a DOM control; Esc/keyboard handled at the view level
              <mesh
                key={end}
                position={[x, selectedTruss.height, z]}
                onClick={(e) => {
                  e.stopPropagation();
                  onSelect({ kind: "trussEnd", id: selectedTruss.id, end });
                }}
              >
                <sphereGeometry args={[0.16, 16, 16]} />
                <meshBasicMaterial color={active ? "#f472b6" : "#a78bfa"} toneMapped={false} />
              </mesh>
            );
          })
        : null}
    </group>
  );
}
