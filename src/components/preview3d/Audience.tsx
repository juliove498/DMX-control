import { useFrame } from "@react-three/fiber";
import { useEffect, useMemo, useRef } from "react";
import * as THREE from "three";
import type { AudienceConfig } from "./stageConfig";

/// Crowd of dancing humanoid silhouettes scattered in the audience
/// zone. Six `<instancedMesh>` calls — torso, head, two arms, two
/// legs — keep the GPU cost flat regardless of crowd size; the
/// shader runs once per instance with the per-instance matrix
/// substituted in.
///
/// Arms animate per-frame on a per-instance phase so the crowd
/// reads as "dancing" instead of "frozen mannequins". Torso, head,
/// and legs hold static matrices with a per-instance yaw + scale
/// for variety; updating those each frame would 4× the matrix
/// compose cost without adding much visual payoff.
///
/// The silhouettes are rendered with a `MeshStandardMaterial` so
/// they actually receive the rig's lights. As fixtures sweep the
/// floor with colour, the crowd changes hue — that's the visual
/// payoff that makes the preview feel like a real venue.
export function Audience({ config }: { config: AudienceConfig }) {
  // One ref per limb. We could fold the static limbs into a single
  // group with sub-meshes, but instancedMesh demands per-mesh
  // matrices, and rotating arms independently means each arm needs
  // its own buffer.
  const torsoRef = useRef<THREE.InstancedMesh | null>(null);
  const headRef = useRef<THREE.InstancedMesh | null>(null);
  const armLRef = useRef<THREE.InstancedMesh | null>(null);
  const armRRef = useRef<THREE.InstancedMesh | null>(null);
  const legLRef = useRef<THREE.InstancedMesh | null>(null);
  const legRRef = useRef<THREE.InstancedMesh | null>(null);

  // Crowd geometry derived from the zone footprint × density.
  // Capped at 1500 so a tall building of density values never
  // saws through frame budget.
  const { count, positions, scales, rotations, phases } = useMemo(() => {
    if (!config.enabled) {
      return { count: 0, positions: [], scales: [], rotations: [], phases: [] };
    }
    const w = Math.abs(config.zone.x2 - config.zone.x1);
    const d = Math.abs(config.zone.z2 - config.zone.z1);
    const area = Math.max(0, w * d);
    const target = Math.min(1500, Math.floor(area * config.density));
    const positions: [number, number, number][] = [];
    const scales: number[] = [];
    const rotations: number[] = [];
    const phases: number[] = [];
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
      // Per-instance arm-swing phase. Different phases mean adjacent
      // people don't move in lockstep — the crowd dances, not
      // marches.
      const phase = pseudoRandom(i, 5) * Math.PI * 2;
      positions.push([px, 0, pz]);
      scales.push(scale);
      rotations.push(yaw);
      phases.push(phase);
    }
    return { count: target, positions, scales, rotations, phases };
  }, [
    config.enabled,
    config.zone.x1,
    config.zone.z1,
    config.zone.x2,
    config.zone.z2,
    config.density,
  ]);

  // Anatomic proportions in metres. averageHeight is overall body
  // height; we hand off ~55% to the torso, the rest to legs (waist
  // down) plus a small head on top.
  const torsoHeight = config.averageHeight * 0.45;
  const torsoRadius = config.averageHeight * 0.11;
  const headRadius = config.averageHeight * 0.075;
  const legHeight = config.averageHeight * 0.45;
  const legRadius = config.averageHeight * 0.06;
  const armLength = config.averageHeight * 0.36;
  const armRadius = config.averageHeight * 0.045;
  const shoulderWidth = config.averageHeight * 0.18;
  const hipWidth = config.averageHeight * 0.11;
  // Y offsets where each piece's centre should sit when the person
  // stands at the origin with their feet on the floor.
  const torsoCY = legHeight + torsoHeight / 2;
  const headCY = legHeight + torsoHeight + headRadius;
  const legCY = legHeight / 2;
  const shoulderCY = legHeight + torsoHeight - torsoRadius * 0.5;
  // Arms hang slightly forward from the torso so the dancing swing
  // reads as motion rather than a stiff windmill.
  const armRestZ = config.averageHeight * 0.02;

  // Per-frame: rotate the arms with a sin/cos around their shoulder
  // pivot so the crowd reads as moving. Phase varies per person so
  // adjacent dancers swing out of sync. Declared above the static
  // mount effect so that effect can call it for the frame-0 pose.
  const poseArms = useMemo(() => {
    const m = new THREE.Matrix4();
    const eRoot = new THREE.Euler();
    const eArm = new THREE.Euler();
    const qRoot = new THREE.Quaternion();
    const qArm = new THREE.Quaternion();
    const qFinal = new THREE.Quaternion();
    const root = new THREE.Vector3();
    const localPos = new THREE.Vector3();
    const armPivot = new THREE.Vector3();
    const sVec = new THREE.Vector3();
    return (time: number) => {
      if (count === 0) return;
      for (let i = 0; i < count; i++) {
        const [px, , pz] = positions[i];
        const sc = scales[i];
        const yaw = rotations[i];
        const phase = phases[i];
        eRoot.set(0, yaw, 0);
        qRoot.setFromEuler(eRoot);
        sVec.set(sc, sc, sc);

        for (const sign of [-1, 1] as const) {
          // Arm swing around the local X axis (forward/back) plus a
          // mirror offset on Z for left/right asymmetry. Range
          // ~±0.6 rad (~35°) is enough to read as dancing without
          // looking like jumping jacks.
          const swing = Math.sin(time * 1.6 + phase + (sign > 0 ? Math.PI : 0)) * 0.55;
          // Rotate -90° around local X so the arm hangs down by
          // default, then add the swing.
          eArm.set(-Math.PI / 2 + swing, 0, sign * 0.08);
          qArm.setFromEuler(eArm);
          // Final orientation = root yaw composed with the local
          // arm rotation.
          qFinal.multiplyQuaternions(qRoot, qArm);

          // Shoulder pivot in world space: rotate the local
          // shoulder offset by the root yaw and add to the person
          // position.
          localPos.set((sign * shoulderWidth) / 2, 0, armRestZ);
          localPos.applyQuaternion(qRoot);
          armPivot.set(
            px + localPos.x,
            shoulderCY * sc,
            pz + localPos.z,
          );
          // The arm capsule's centre is *armLength/2* below its
          // pivot when hanging. With our combined rotation, that
          // offset is along the rotated -Y axis — apply qFinal to
          // the local (0, -armLength/2, 0) vector to get the
          // displacement.
          localPos.set(0, -armLength / 2, 0);
          localPos.applyQuaternion(qFinal);
          root.set(armPivot.x + localPos.x, armPivot.y + localPos.y, armPivot.z + localPos.z);

          m.compose(root, qFinal, sVec);
          const ref = sign < 0 ? armLRef.current : armRRef.current;
          ref?.setMatrixAt(i, m);
        }
      }
      if (armLRef.current) armLRef.current.instanceMatrix.needsUpdate = true;
      if (armRRef.current) armRRef.current.instanceMatrix.needsUpdate = true;
    };
  }, [
    count,
    positions,
    scales,
    rotations,
    phases,
    armLength,
    armRestZ,
    shoulderCY,
    shoulderWidth,
  ]);

  // Set up the static limb matrices (torso, head, legs) on mount and
  // whenever the crowd layout changes. `useEffect` runs after refs
  // are populated — using `useMemo` for this side effect is what
  // produced the "everyone stacks at the origin" bug on first
  // render: the matrices were composed but never written, so the
  // GPU drew every instance at identity. Wiggling averageHeight
  // happened to retrigger and "fix" it because that dep flipped.
  useEffect(() => {
    if (count === 0) return;
    const m = new THREE.Matrix4();
    const e = new THREE.Euler();
    const v = new THREE.Vector3();
    const s = new THREE.Vector3();
    const yawQ = new THREE.Quaternion();
    const localPos = new THREE.Vector3();

    for (let i = 0; i < count; i++) {
      const [px, , pz] = positions[i];
      const sc = scales[i];
      const yaw = rotations[i];
      e.set(0, yaw, 0);
      yawQ.setFromEuler(e);
      s.set(sc, sc, sc);

      // Torso
      v.set(px, torsoCY * sc, pz);
      m.compose(v, yawQ, s);
      torsoRef.current?.setMatrixAt(i, m);

      // Head
      v.set(px, headCY * sc, pz);
      m.compose(v, yawQ, s);
      headRef.current?.setMatrixAt(i, m);

      // Legs — slight outward splay (±hipWidth/2) in the local
      // frame, rotated by the person's yaw. We compose the world
      // position by yaw-rotating the local hip offset and adding it
      // to the root.
      for (const sign of [-1, 1] as const) {
        localPos.set((sign * hipWidth) / 2, legCY * sc, 0);
        localPos.applyQuaternion(yawQ);
        v.set(px + localPos.x, legCY * sc, pz + localPos.z);
        m.compose(v, yawQ, s);
        const ref = sign < 0 ? legLRef.current : legRRef.current;
        ref?.setMatrixAt(i, m);
      }
    }
    if (torsoRef.current) torsoRef.current.instanceMatrix.needsUpdate = true;
    if (headRef.current) headRef.current.instanceMatrix.needsUpdate = true;
    if (legLRef.current) legLRef.current.instanceMatrix.needsUpdate = true;
    if (legRRef.current) legRRef.current.instanceMatrix.needsUpdate = true;
    // Initial arm pose — needed so the crowd has hands at frame 0
    // even if useFrame hasn't ticked yet.
    poseArms(0);
  }, [count, positions, scales, rotations, headCY, hipWidth, legCY, torsoCY, poseArms]);

  useFrame(({ clock }) => {
    poseArms(clock.elapsedTime);
  });

  if (!config.enabled || count === 0) return null;

  return (
    <group>
      <instancedMesh ref={torsoRef} args={[undefined, undefined, count]} receiveShadow>
        <capsuleGeometry args={[torsoRadius, torsoHeight, 4, 8]} />
        <meshStandardMaterial color={config.color} roughness={0.85} metalness={0} />
      </instancedMesh>
      <instancedMesh ref={headRef} args={[undefined, undefined, count]} receiveShadow>
        <sphereGeometry args={[headRadius, 8, 8]} />
        <meshStandardMaterial color={config.color} roughness={0.7} metalness={0} />
      </instancedMesh>
      <instancedMesh ref={armLRef} args={[undefined, undefined, count]} receiveShadow>
        <capsuleGeometry args={[armRadius, armLength, 3, 6]} />
        <meshStandardMaterial color={config.color} roughness={0.85} metalness={0} />
      </instancedMesh>
      <instancedMesh ref={armRRef} args={[undefined, undefined, count]} receiveShadow>
        <capsuleGeometry args={[armRadius, armLength, 3, 6]} />
        <meshStandardMaterial color={config.color} roughness={0.85} metalness={0} />
      </instancedMesh>
      <instancedMesh ref={legLRef} args={[undefined, undefined, count]} receiveShadow>
        <capsuleGeometry args={[legRadius, legHeight, 3, 6]} />
        <meshStandardMaterial color={config.color} roughness={0.85} metalness={0} />
      </instancedMesh>
      <instancedMesh ref={legRRef} args={[undefined, undefined, count]} receiveShadow>
        <capsuleGeometry args={[legRadius, legHeight, 3, 6]} />
        <meshStandardMaterial color={config.color} roughness={0.85} metalness={0} />
      </instancedMesh>
    </group>
  );
}

/// Cheap deterministic 0..1 hash. Two-arg form lets us derive
/// independent pseudo-random streams for X / Z / scale / yaw / arm
/// phase of the same crowd index without consuming a real RNG.
function pseudoRandom(i: number, seed: number): number {
  const x = Math.sin(i * 12.9898 + seed * 78.233) * 43758.5453;
  return x - Math.floor(x);
}
