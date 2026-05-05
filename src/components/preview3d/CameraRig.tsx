import { OrbitControls, PerspectiveCamera } from "@react-three/drei";
import { useFrame, useThree } from "@react-three/fiber";
import { useEffect, useMemo, useRef } from "react";
import * as THREE from "three";

export type CameraPreset = "front" | "side" | "top" | "back" | "drum";

export interface PresetTransition {
  preset: CameraPreset;
  /// Each request bumps this so the rig knows to re-trigger the
  /// move animation even when the user picks the same preset twice
  /// in a row (useful after they've dragged the orbit camera).
  stamp: number;
}

interface RigDimensions {
  /// Floor width — used to scale orbit distance so a small stage
  /// doesn't have the camera 30m away.
  width: number;
  depth: number;
}

/// Camera rig with smooth tween-in to named presets. The orbit
/// controls take over once the tween settles, so the operator can
/// nudge the angle freely; pressing a preset button re-tweens to
/// that vantage from wherever they are.
export function CameraRig({
  request,
  dims,
}: {
  request: PresetTransition;
  dims: RigDimensions;
}) {
  const { camera } = useThree();
  const controlsRef = useRef<React.ComponentRef<typeof OrbitControls> | null>(null);

  const presets = useMemo(() => buildPresets(dims), [dims.width, dims.depth]);
  const targetCamPos = useRef(new THREE.Vector3());
  const targetLook = useRef(new THREE.Vector3());
  const tweening = useRef(0); // remaining tween time in seconds

  useEffect(() => {
    const p = presets[request.preset];
    targetCamPos.current.copy(p.position);
    targetLook.current.copy(p.lookAt);
    tweening.current = 0.8;
    // Auto-update controls once we land.
  }, [request.stamp, request.preset, presets]);

  useFrame((_, delta) => {
    if (tweening.current <= 0) return;
    const remaining = Math.max(0, tweening.current - delta);
    const t = 1 - remaining / 0.8;
    // Ease-out cubic for a confident landing.
    const eased = 1 - (1 - t) * (1 - t) * (1 - t);
    camera.position.lerp(targetCamPos.current, eased * 0.35 + 0.05);
    if (controlsRef.current) {
      const c = controlsRef.current as unknown as { target: THREE.Vector3; update: () => void };
      c.target.lerp(targetLook.current, eased * 0.35 + 0.05);
      c.update();
    }
    tweening.current = remaining;
    if (remaining <= 0) {
      camera.position.copy(targetCamPos.current);
      if (controlsRef.current) {
        const c = controlsRef.current as unknown as { target: THREE.Vector3; update: () => void };
        c.target.copy(targetLook.current);
        c.update();
      }
    }
  });

  return (
    <>
      <PerspectiveCamera
        makeDefault
        fov={45}
        near={0.1}
        far={200}
        position={[presets.front.position.x, presets.front.position.y, presets.front.position.z]}
      />
      <OrbitControls
        ref={controlsRef}
        target={[presets.front.lookAt.x, presets.front.lookAt.y, presets.front.lookAt.z]}
        enableDamping
        dampingFactor={0.08}
        maxPolarAngle={Math.PI * 0.49}
        minDistance={2}
        maxDistance={Math.max(dims.width, dims.depth) * 4}
      />
    </>
  );
}

function buildPresets(dims: RigDimensions): Record<
  CameraPreset,
  { position: THREE.Vector3; lookAt: THREE.Vector3 }
> {
  const w = dims.width;
  const d = dims.depth;
  const lookAtCentre = new THREE.Vector3(0, 1.5, 0);
  return {
    // FOH (front of house) — audience perspective, slightly above
    // human eye-line so the trusses read.
    front: {
      position: new THREE.Vector3(0, 2.8, -d * 1.2 - 2),
      lookAt: lookAtCentre,
    },
    // Stage right — 90° rotated; useful to read pan sweeps.
    side: {
      position: new THREE.Vector3(w * 0.9 + 2, 2.5, 0),
      lookAt: lookAtCentre,
    },
    // Top-down (god view) — hard cap on polar angle elsewhere
    // prevents the orbit from going past horizontal, but we set the
    // preset itself to nearly-vertical for a plan view.
    top: {
      position: new THREE.Vector3(0.001, Math.max(w, d) * 1.1, 0),
      lookAt: new THREE.Vector3(0, 0, 0),
    },
    // Behind the band looking out — useful for back-truss fixtures.
    back: {
      position: new THREE.Vector3(0, 2.8, d * 0.9 + 2),
      lookAt: lookAtCentre,
    },
    // "Drum riser" — low and central. Mimics a band's onstage
    // perspective.
    drum: {
      position: new THREE.Vector3(0, 1.6, 0.5),
      lookAt: new THREE.Vector3(0, 1.6, -d),
    },
  };
}
