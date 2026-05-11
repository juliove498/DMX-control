import { Canvas } from "@react-three/fiber";
import { Bloom, EffectComposer, ToneMapping } from "@react-three/postprocessing";
import { BlendFunction, ToneMappingMode } from "postprocessing";
import { useEffect, useMemo, useRef, useState } from "react";
import * as THREE from "three";
import { useShowStore } from "../stores/show";
import { Audience } from "./preview3d/Audience";
import { type CameraPreset, CameraRig, type PresetTransition } from "./preview3d/CameraRig";
import { Fixture3D } from "./preview3d/Fixture3D";
import { StageConfigPanel } from "./preview3d/StageConfigPanel";
import { StageGeometry } from "./preview3d/StageGeometry";
import { type StageConfig, loadStageConfig, saveStageConfig } from "./preview3d/stageConfig";
import {
  fixtureHasGoboWheel,
  fixtureIsSpotKind,
  useFixtureLightStates,
} from "./preview3d/useFixtureLightStates";

const P3D_PANEL_WIDTH_KEY = "preview3d.panelWidth";
const P3D_PANEL_MIN = 220;
const P3D_PANEL_MAX = 720;

function clampPanelWidth(w: number): number {
  if (Number.isNaN(w)) return 320;
  return Math.max(P3D_PANEL_MIN, Math.min(P3D_PANEL_MAX, w));
}

/// Root view for the 3D scene preview. Owns the persisted
/// `StageConfig`, hosts the Three.js Canvas, and orchestrates the
/// camera + sidebar. The actual rendering happens inside the
/// `<Canvas>` so React Three Fiber can manage the WebGL context;
/// everything outside the canvas is plain DOM.
export function Preview3D() {
  const show = useShowStore((s) => s.show);
  const showPath = useShowStore((s) => s.showPath);
  const library = useShowStore((s) => s.library);

  const [config, setConfig] = useState<StageConfig>(() => loadStageConfig(showPath));
  // Reload config when the loaded show changes — different shows can
  // have different rigs.
  useEffect(() => {
    setConfig(loadStageConfig(showPath));
  }, [showPath]);
  // Persist on every change. Cheap; localStorage is sync but the
  // payload is sub-KB.
  useEffect(() => {
    saveStageConfig(showPath, config);
  }, [showPath, config]);

  const [showPanel, setShowPanel] = useState(true);
  const [cameraReq, setCameraReq] = useState<PresetTransition>({
    preset: "front",
    stamp: 0,
  });
  const requestCamera = (preset: CameraPreset) =>
    setCameraReq((r) => ({ preset, stamp: r.stamp + 1 }));

  // Resizable sidebar width — persisted across sessions like the
  // Stage right-panel. Operator drags the splitter on the panel's
  // left edge to make room for fixture lists in big rigs.
  const [panelWidth, setPanelWidth] = useState<number>(() => {
    try {
      const saved = localStorage.getItem(P3D_PANEL_WIDTH_KEY);
      return clampPanelWidth(saved ? Number(saved) : 320);
    } catch {
      return 320;
    }
  });
  const [splitterActive, setSplitterActive] = useState(false);
  useEffect(() => {
    try {
      localStorage.setItem(P3D_PANEL_WIDTH_KEY, String(panelWidth));
    } catch {
      /* localStorage unavailable — width still works in-session. */
    }
  }, [panelWidth]);
  const onSplitterDown = (e: React.PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = panelWidth;
    setSplitterActive(true);
    const onMove = (ev: PointerEvent) => {
      const dx = startX - ev.clientX;
      setPanelWidth(clampPanelWidth(startW + dx));
    };
    const onUp = () => {
      setSplitterActive(false);
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };
    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  };

  const fixtures = show?.fixtures ?? [];
  const lightStates = useFixtureLightStates(fixtures, library);

  // Stable plane instances reused by every beam material's
  // `clippingPlanes`. Without this every fixture would create its
  // own plane and the WebGL renderer rebuilds the clip uniforms on
  // every change. We update the planes' constants in place when
  // the stage geometry config changes.
  const floorPlaneRef = useRef(new THREE.Plane(new THREE.Vector3(0, 1, 0), 0));
  const backWallPlaneRef = useRef(new THREE.Plane(new THREE.Vector3(0, 0, -1), 0));
  const clippingPlanes = useMemo(() => {
    // Floor plane stays at y=0; nothing to update.
    floorPlaneRef.current.set(new THREE.Vector3(0, 1, 0), 0);
    if (config.backWall.enabled) {
      // Plane normal pointing toward the audience (-Z), constant
      // such that points with z > depth/2 (behind the wall) are
      // clipped.
      backWallPlaneRef.current.set(new THREE.Vector3(0, 0, -1), config.floor.depth / 2);
      return [floorPlaneRef.current, backWallPlaneRef.current];
    }
    return [floorPlaneRef.current];
  }, [config.backWall.enabled, config.floor.depth]);

  // Resolve where each fixture sits in 3D + how it's pre-aimed.
  // Three sources of position, in priority order:
  //   1. Truss attachment → interpolate along the linked truss.
  //   2. Free override → use the explicit XYZ from the panel.
  //   3. Fall back to the 2D canvas position projected through
  //      pixelsPerMeter at the global rig height.
  const placements = useMemo(() => {
    const trussById: Record<string, (typeof config.trusses)[number]> = {};
    for (const t of config.trusses) trussById[t.id] = t;
    return fixtures.map((f) => {
      const override = config.fixturePlacements[f.id];
      let position: [number, number, number];
      if (override?.mode === "truss" && override.truss.trussId) {
        const truss = trussById[override.truss.trussId];
        if (truss) {
          const t = Math.max(0, Math.min(1, override.truss.t));
          position = [
            truss.fromX + (truss.toX - truss.fromX) * t,
            truss.height,
            truss.fromZ + (truss.toZ - truss.fromZ) * t,
          ];
        } else {
          // Truss got deleted — fall back to free coords gracefully.
          position = [override.free.x, override.free.y, override.free.z];
        }
      } else if (override?.mode === "free") {
        position = [override.free.x, override.free.y, override.free.z];
      } else {
        const px = f.position?.[0] ?? 0;
        const py = f.position?.[1] ?? 0;
        const x = (px - 600) / config.pixelsPerMeter;
        const z = (py - 300) / config.pixelsPerMeter;
        position = [x, config.rigHeight, z];
      }
      // Render tunables (brightness / beam angle / prism) are
      // per-fixture-DEFINITION, not per instance. Pull from the
      // definition-keyed map; missing entries get the safe defaults
      // (brightness 1, beamAngle/prism null = kind-based defaults).
      const defOverride = config.definitionOverrides[f.definition_id];
      return {
        id: f.id,
        position,
        aimYaw: override?.aimYaw ?? 0,
        aimPitch: override?.aimPitch ?? 0,
        // Pre-computed once per fixture: spot-class fixtures
        // (zoom/gobo/iris/prism) get castShadow=true at mount so
        // the shader compiles with the SpotLight.map path
        // available — a runtime toggle of castShadow doesn't
        // always force the recompile that's needed. Wash/par
        // fixtures stay castShadow=false to avoid hitting the
        // GPU's 16-texture-unit cap on big rigs.
        hasGobo: fixtureHasGoboWheel(f, library),
        isSpot: fixtureIsSpotKind(f, library),
        overrides: {
          brightness: defOverride?.brightness ?? 1,
          beamAngle: defOverride?.beamAngle ?? null,
          prism: defOverride?.prism ?? null,
        },
      };
    });
  }, [
    fixtures,
    library,
    config.pixelsPerMeter,
    config.rigHeight,
    config.trusses,
    config.fixturePlacements,
    config.definitionOverrides,
  ]);

  return (
    <main className="page p3d-page">
      <header className="page-head">
        <h2>Preview 3D</h2>
        <span className="meta">
          {fixtures.length} fixtures · live DMX 30fps · cone+bloom volumetric
        </span>
        <div className="actions">
          <CameraButtons request={cameraReq.preset} onPick={requestCamera} />
          <button
            type="button"
            className="p3d-toggle-panel"
            onClick={() => setShowPanel((v) => !v)}
            title={showPanel ? "Ocultar panel de escenario" : "Mostrar panel de escenario"}
          >
            {showPanel ? "▶ Ocultar config" : "◀ Config"}
          </button>
        </div>
      </header>
      <div
        className={`p3d-layout${showPanel ? "" : " no-panel"}${splitterActive ? " splitter-active" : ""}`}
        style={showPanel ? { gridTemplateColumns: `1fr 6px ${panelWidth}px` } : undefined}
      >
        <div className="p3d-canvas-wrap">
          <Canvas
            // shadows must be enabled at the renderer level for
            // SpotLight.map projections (gobos) to even compile in
            // the fragment shader — Three.js gates the spotLightMap
            // sampling behind `#ifdef USE_SHADOWMAP`. We use
            // "basic" filtering because we don't actually render
            // shadows on geometry (no mesh has receiveShadow); we
            // just need the shader path active so the gobo texture
            // gets projected through the light frustum.
            shadows="basic"
            dpr={[1, 2]}
            gl={{
              antialias: true,
              powerPreference: "high-performance",
              toneMapping: THREE.ACESFilmicToneMapping,
              toneMappingExposure: 1.1,
            }}
            onCreated={({ gl }) => {
              gl.localClippingEnabled = true;
            }}
          >
            {/* Background + ambient driven by `ambientLevel`. At 0
                we're a black void with only the rig lighting things;
                at 1 the room reads as a lit space (rehearsal hall /
                house-lights-on / load-in). Hemi + ambient scale
                together so colour temperature stays neutral. */}
            <color attach="background" args={[ambientBackground(config.ambientLevel)]} />
            <hemisphereLight args={["#9aa8c0", "#1a2030", 0.25 + config.ambientLevel * 1.4]} />
            <ambientLight intensity={0.05 + config.ambientLevel * 0.6} />

            {/* Fog density derived from atmosphere slider. Fog and
                bloom together sell the "club" feeling — bloom alone
                makes lights pop, fog makes the room feel like it
                has air in it.
                Exponential (FogExp2) instead of linear: linear fog
                ramps clear → black inside a fixed near/far band, and
                anything past `far` snaps to full fog colour. The
                snap was visible on zoom-out — the whole stage went
                black the moment the camera retreated past 30 m.
                Exp curve is smooth all the way through and never
                reaches "fully fogged", so you can pull back as far
                as you want and the venue just gets gradually
                hazier instead of disappearing. Density tuned so
                atmosphere=0.5 yields ~12 % fog at 5 m, ~27 % at
                12 m, ~50 % at 25 m — visible but breathable. */}
            <fogExp2 attach="fog" args={["#0a0e15", config.atmosphere * 0.028]} />

            <CameraRig
              request={cameraReq}
              dims={{ width: config.floor.width, depth: config.floor.depth }}
            />

            <StageGeometry config={config} />
            <Audience config={config.audience} />

            {placements.map((p) => (
              <Fixture3D
                key={p.id}
                position={p.position}
                state={lightStates[p.id]}
                rigHeight={config.rigHeight}
                atmosphere={config.atmosphere}
                clippingPlanes={clippingPlanes}
                aimYaw={p.aimYaw}
                aimPitch={p.aimPitch}
                overrides={p.overrides}
                hasGobo={p.hasGobo}
                isSpot={p.isSpot}
              />
            ))}

            <EffectComposer multisampling={2}>
              {/* Bloom is what makes the cones look like real
                  beams. Threshold low so emissives glow even when
                  the colour isn't saturated. */}
              <Bloom
                intensity={1.4}
                luminanceThreshold={0.18}
                luminanceSmoothing={0.85}
                mipmapBlur
                blendFunction={BlendFunction.SCREEN}
              />
              {/* ACES tone mapping inside the composer keeps colours
                  smooth at high intensity instead of clipping to
                  white. */}
              <ToneMapping mode={ToneMappingMode.ACES_FILMIC} />
            </EffectComposer>
          </Canvas>
        </div>
        {showPanel ? (
          <>
            <div className="p3d-splitter" onPointerDown={onSplitterDown} aria-hidden="true" />
            <StageConfigPanel
              config={config}
              onChange={setConfig}
              fixtures={fixtures}
              library={library}
            />
          </>
        ) : null}
      </div>
    </main>
  );
}

/// Background colour for the WebGL clear, modulated by ambient
/// level. Pitch-black void at 0 → muted grey-blue venue feel at
/// 1. Linear interpolation between two hand-picked end stops.
function ambientBackground(level: number): string {
  const a = Math.max(0, Math.min(1, level));
  const c0 = [0x05, 0x08, 0x0d];
  const c1 = [0x18, 0x1f, 0x2a];
  const r = Math.round(c0[0] + (c1[0] - c0[0]) * a);
  const g = Math.round(c0[1] + (c1[1] - c0[1]) * a);
  const b = Math.round(c0[2] + (c1[2] - c0[2]) * a);
  return `rgb(${r},${g},${b})`;
}

function CameraButtons({
  request,
  onPick,
}: {
  request: CameraPreset;
  onPick: (p: CameraPreset) => void;
}) {
  const items: { id: CameraPreset; label: string; hint: string }[] = [
    { id: "front", label: "Front", hint: "FOH — perspectiva del público" },
    { id: "side", label: "Side", hint: "Stage right (90°)" },
    { id: "top", label: "Top", hint: "Vista en planta" },
    { id: "back", label: "Back", hint: "Detrás de la banda" },
    { id: "drum", label: "Drum", hint: "Drum riser — perspectiva del músico" },
  ];
  return (
    <div className="p3d-cam">
      {items.map((it) => (
        <button
          key={it.id}
          type="button"
          className={`p3d-cam-btn${request === it.id ? " active" : ""}`}
          onClick={() => onPick(it.id)}
          title={it.hint}
        >
          {it.label}
        </button>
      ))}
    </div>
  );
}
