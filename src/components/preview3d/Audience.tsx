import { useMemo, useRef } from "react";
import * as THREE from "three";
import type { AudienceConfig } from "./stageConfig";

/// Crowd of humanoid silhouettes scattered in the audience zone.
/// Two `<instancedMesh>` calls — one for capsule bodies, one for
/// spherical heads — keep the GPU cost flat regardless of crowd
/// size; the shader runs once per instance with the per-instance
/// matrix substituted in.
///
/// The silhouettes are rendered with a `MeshStandardMaterial` so
/// they actually receive the rig's lights. As fixtures sweep the
/// floor with colour, the crowd changes hue — that's the visual
/// payoff we're after, not realistic geometry.
export function Audience({ config }: { config: AudienceConfig }) {
  const bodyRef = useRef<THREE.InstancedMesh | null>(null);
  const headRef = useRef<THREE.InstancedMesh | null>(null);

  // Crowd geometry derived from the zone footprint × density.
  // Capped at 1500 so a tall building of density values never
  // saws through frame budget.
  const { count, positions, scales, rotations } = useMemo(() => {
    if (!config.enabled) {
      return { count: 0, positions: [], scales: [], rotations: [] };
    }
    const w = Math.abs(config.zone.x2 - config.zone.x1);
    const d = Math.abs(config.zone.z2 - config.zone.z1);
    const area = Math.max(0, w * d);
    const target = Math.min(1500, Math.floor(area * config.density));
    const positions: [number, number, number][] = [];
    const scales: number[] = [];
    const rotations: number[] = [];
    const x0 = Math.min(config.zone.x1, config.zone.x2);
    const z0 = Math.min(config.zone.z1, config.zone.z2);
    for (let i = 0; i < target; i++) {
      // Deterministic pseudo-random positions: same seed → same
      // crowd layout across reloads, so the operator's fixtures
      // always paint the same heads.
      const px = pseudoRandom(i, 1) * w + x0;
      const pz = pseudoRandom(i, 2) * d + z0;
      // ±10% height variation so the crowd doesn't look stamped.
      const scale = 0.9 + pseudoRandom(i, 3) * 0.2;
      const yaw = pseudoRandom(i, 4) * Math.PI * 2;
      positions.push([px, 0, pz]);
      scales.push(scale);
      rotations.push(yaw);
    }
    return { count: target, positions, scales, rotations };
  }, [
    config.enabled,
    config.zone.x1,
    config.zone.z1,
    config.zone.x2,
    config.zone.z2,
    config.density,
  ]);

  // Push per-instance matrices into both meshes whenever the crowd
  // layout regenerates. We use a single Matrix4 buffer reused per
  // instance to avoid 1500 throwaway allocations on each rebuild.
  useMemo(() => {
    if (count === 0) return;
    const m = new THREE.Matrix4();
    const e = new THREE.Euler();
    const q = new THREE.Quaternion();
    const v = new THREE.Vector3();
    const sBody = new THREE.Vector3();
    const sHead = new THREE.Vector3();
    const bodyHalf = (config.averageHeight * 0.55) / 2; // body cylinder runs from ~floor to shoulder
    const headOffset = config.averageHeight * 0.55 + config.averageHeight * 0.1;
    for (let i = 0; i < count; i++) {
      const [x, , z] = positions[i];
      const scale = scales[i];
      e.set(0, rotations[i], 0);
      q.setFromEuler(e);
      // Body
      v.set(x, bodyHalf * scale + 0.05, z);
      sBody.set(scale, scale, scale);
      m.compose(v, q, sBody);
      bodyRef.current?.setMatrixAt(i, m);
      // Head
      v.set(x, headOffset * scale, z);
      sHead.set(scale, scale, scale);
      m.compose(v, q, sHead);
      headRef.current?.setMatrixAt(i, m);
    }
    if (bodyRef.current) bodyRef.current.instanceMatrix.needsUpdate = true;
    if (headRef.current) headRef.current.instanceMatrix.needsUpdate = true;
  }, [count, positions, scales, rotations, config.averageHeight]);

  if (!config.enabled || count === 0) return null;

  // Body: tall capsule. Standard material so the rig's spotLights
  // catch them. Slight roughness; not metallic. Dark base so the
  // silhouette reads against the floor when unlit.
  const bodyHeight = config.averageHeight * 0.55;
  const headRadius = config.averageHeight * 0.06;
  return (
    <group>
      <instancedMesh
        ref={bodyRef}
        args={[undefined, undefined, count]}
        castShadow={false}
        receiveShadow
      >
        <capsuleGeometry args={[config.averageHeight * 0.09, bodyHeight, 4, 8]} />
        <meshStandardMaterial color="#0d1014" roughness={0.85} metalness={0} />
      </instancedMesh>
      <instancedMesh
        ref={headRef}
        args={[undefined, undefined, count]}
        castShadow={false}
        receiveShadow
      >
        <sphereGeometry args={[headRadius, 8, 8]} />
        <meshStandardMaterial color="#0d1014" roughness={0.7} metalness={0} />
      </instancedMesh>
    </group>
  );
}

/// Cheap deterministic 0..1 hash. Two-arg form lets us derive
/// independent pseudo-random streams for X / Z / scale / yaw of
/// the same crowd index without consuming a real RNG.
function pseudoRandom(i: number, seed: number): number {
  const x = Math.sin(i * 12.9898 + seed * 78.233) * 43758.5453;
  return x - Math.floor(x);
}
