import { useFrame } from "@react-three/fiber";
import { useEffect, useMemo, useRef } from "react";
import * as THREE from "three";
import type { FixtureKind, FixtureLightState } from "./useFixtureLightStates";

/// Per-fixture overrides delivered from the StageConfigPanel. Each
/// is `null` when the operator hasn't set one, in which case the
/// renderer falls back to the kind-based defaults.
export interface FixtureRenderOverrides {
  beamAngle: { minDeg: number; maxDeg: number } | null;
  prism: { threshold: number; facets: number; splayDeg: number } | null;
}

const DEFAULT_OVERRIDES: FixtureRenderOverrides = { beamAngle: null, prism: null };

/// Module-level cache of loaded gobo textures, keyed by URL. Many
/// fixtures of the same model share the same gobo wheel, so caching
/// here keeps memory bounded and lets pattern swaps be instant
/// once a wheel position has been seen.
const goboTextureCache = new Map<string, THREE.Texture>();

function getGoboTexture(url: string): THREE.Texture {
  let tex = goboTextureCache.get(url);
  if (!tex) {
    const placeholder = new THREE.Texture();
    placeholder.colorSpace = THREE.SRGBColorSpace;
    placeholder.wrapS = THREE.ClampToEdgeWrapping;
    placeholder.wrapT = THREE.ClampToEdgeWrapping;
    placeholder.minFilter = THREE.LinearFilter;
    placeholder.magFilter = THREE.LinearFilter;
    placeholder.flipY = false;
    tex = placeholder;
    goboTextureCache.set(url, tex);
    const img = new Image();
    img.onload = () => {
      // Many fixture libraries store gobos as dark-palette
      // grayscale GIFs (pattern in faint greys against near-black
      // background). When Three.js's SpotLight.map shader
      // multiplies the cone colour by the texture's RGB, those
      // dark pixels collapse the projection to ~5% brightness and
      // the pattern reads as a uniform circle. We re-process the
      // bitmap on a canvas: stretch the dynamic range so the
      // brightest pixels saturate to white and the darkest go to
      // black. Result: a clean stencil that projects with proper
      // contrast regardless of how the source was authored.
      placeholder.image = stretchContrast(img);
      placeholder.needsUpdate = true;
    };
    img.onerror = (err) => {
      console.warn("[Preview3D] gobo image decode failed", url.slice(0, 80), err);
    };
    img.src = url;
  }
  return tex;
}

/// Sample the loaded image, find its dynamic range, and produce a
/// new canvas where pixels are remapped so min→0 and max→1. Keeps
/// chromatic gobos (red, blue) recognisable; dark grayscale gobos
/// become white-on-black stencils suitable for the SpotLight.map
/// multiply formula.
function stretchContrast(img: HTMLImageElement): HTMLCanvasElement {
  const w = img.naturalWidth || 64;
  const h = img.naturalHeight || 64;
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) return canvas;
  ctx.drawImage(img, 0, 0, w, h);
  const data = ctx.getImageData(0, 0, w, h);
  const px = data.data;
  // First pass: find max luminance.
  let maxL = 0;
  for (let i = 0; i < px.length; i += 4) {
    const l = Math.max(px[i], px[i + 1], px[i + 2]);
    if (l > maxL) maxL = l;
  }
  // Avoid divide by zero on a fully-black image.
  if (maxL <= 0) return canvas;
  const scale = 255 / maxL;
  // Second pass: stretch each pixel proportionally so the brightest
  // texel hits 255. Preserves hue: a red gobo stays red, just
  // saturated. Alpha is left alone — gobos are usually opaque.
  for (let i = 0; i < px.length; i += 4) {
    px[i] = Math.min(255, Math.round(px[i] * scale));
    px[i + 1] = Math.min(255, Math.round(px[i + 1] * scale));
    px[i + 2] = Math.min(255, Math.round(px[i + 2] * scale));
  }
  ctx.putImageData(data, 0, 0);
  return canvas;
}

/// 1×1 white texture used as the "no gobo" fallback so the beam
/// shader can always sample a `goboMap` uniform without branching.
/// Sampling returns 1.0 → cone is uniformly bright (no shape
/// modulation) — same look as a fixture without a gobo wheel.
const DEFAULT_WHITE_TEX: THREE.Texture = (() => {
  const data = new Uint8Array([255, 255, 255, 255]);
  const t = new THREE.DataTexture(data, 1, 1, THREE.RGBAFormat);
  t.colorSpace = THREE.SRGBColorSpace;
  t.minFilter = THREE.LinearFilter;
  t.magFilter = THREE.LinearFilter;
  t.needsUpdate = true;
  return t;
})();

/// Vertex shader for the crossed-plane volumetric beam. Each
/// plane is a vertical sheet (PlaneGeometry) passing through the
/// cone axis. Local (x, y) span the plane: x across, y along the
/// beam axis. The fragment shader uses these to compute depth +
/// radial position relative to the cone, sampling the gobo at the
/// correct cross-section pixel.
///
/// Includes the clipping_planes chunks so floor / back-wall clipping
/// works on this ShaderMaterial — Three.js doesn't auto-add them.
const BEAM_VERTEX = /* glsl */ `
#include <clipping_planes_pars_vertex>
varying vec2 vLocal;
void main() {
  vLocal = position.xy;
  vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
  gl_Position = projectionMatrix * mvPosition;
  #include <clipping_planes_vertex>
}
`;

/// Fragment shader: each crossed plane reads a 1D slice through
/// the gobo, oriented by the plane's azimuth. With 4 planes at
/// 0/45/90/135° the slices combine additively into a continuous
/// volumetric shaft that takes the gobo's shape — the "haz tiene
/// forma del gobo" effect, but read as a real beam from any
/// camera angle (no edge-on disappearing slices).
///
/// `uDir` rotates the radial coordinate to the plane's azimuth so
/// the same gobo image is sampled along a different chord per
/// plane. From the side, each plane shows a different cross-
/// section of the gobo extruded along the beam.
const BEAM_FRAGMENT = /* glsl */ `
#include <clipping_planes_pars_fragment>
uniform sampler2D uGoboMap;
uniform vec3 uColor;
uniform float uOpacity;
uniform float uLength;
uniform float uHalfLength;
uniform float uMaxRadius;
uniform vec2 uDir;
varying vec2 vLocal;

void main() {
  #include <clipping_planes_fragment>
  // Depth from apex (apex at +halfLength, base at -halfLength).
  float depth = uHalfLength - vLocal.y;
  if (depth <= 0.0) discard;
  float t = depth / uLength;
  if (t > 1.0) discard;
  float radiusAtDepth = t * uMaxRadius;
  if (radiusAtDepth <= 1e-4) discard;
  float normRadial = vLocal.x / radiusAtDepth;
  float absRadial = abs(normRadial);
  if (absRadial > 1.0) discard;
  // 2D gobo UV: rotate the 1D radial into the plane's azimuth
  // direction so different planes sample different chords through
  // the gobo. Together they read as a 2D extrusion.
  vec2 uv = (normRadial * uDir + 1.0) * 0.5;
  vec4 g = texture2D(uGoboMap, uv);
  float bright = max(g.r, max(g.g, g.b));
  // Base brightness so the haze always glows; gobo boosts where
  // the pattern lets light through.
  float lifted = 0.55 + bright * 1.6;
  // Axial fade: ramp at the lens, dissolve into haze at the tip.
  float axial = smoothstep(0.0, 0.05, t) * (1.0 - smoothstep(0.55, 1.0, t));
  // Radial fade — soft cone edge.
  float radialFade = 1.0 - smoothstep(0.85, 1.0, absRadial);
  float alpha = lifted * axial * radialFade * uOpacity;
  if (alpha <= 0.001) discard;
  vec3 finalColor = uColor * (0.85 + bright * 0.7);
  gl_FragColor = vec4(finalColor, alpha);
}
`;

function buildBeamPlaneMaterial(
  beamLength: number,
  maxRadius: number,
  azimuth: number,
): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    uniforms: {
      uGoboMap: { value: DEFAULT_WHITE_TEX },
      uColor: { value: new THREE.Color(1, 1, 1) },
      uOpacity: { value: 0 },
      uLength: { value: beamLength },
      uHalfLength: { value: beamLength / 2 },
      uMaxRadius: { value: Math.max(maxRadius, 1e-4) },
      // Direction vector for sampling the gobo: rotates the 1D
      // radial coordinate into a 2D UV chord. cos(θ) along U,
      // -sin(θ) along V (matches three.js Y-rotation convention
      // where local +X maps to world +X·cos − +Z·sin).
      uDir: { value: new THREE.Vector2(Math.cos(azimuth), -Math.sin(azimuth)) },
    },
    vertexShader: BEAM_VERTEX,
    fragmentShader: BEAM_FRAGMENT,
    transparent: true,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
    side: THREE.DoubleSide,
    toneMapped: false,
    clipping: true,
  });
}

interface BeamPlane {
  /// Azimuth around the cone axis in radians.
  angle: number;
  /// Material instance owned by this plane (its own uDir uniform).
  mat: THREE.ShaderMaterial;
}

/// Vertex shader for an axis-perpendicular disc slice. Local
/// position (x, y) in CircleGeometry space goes from `(-r,-r)` to
/// `(r,r)` over the disc's bounds; we normalise by the disc's
/// radius to get [-1, 1] radial coords for sampling the gobo as
/// a real 2D image.
const DISC_VERTEX = /* glsl */ `
#include <clipping_planes_pars_vertex>
uniform float uDiscRadius;
varying vec2 vRadial;
void main() {
  vRadial = position.xy / max(uDiscRadius, 1e-4);
  vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
  gl_Position = projectionMatrix * mvPosition;
  #include <clipping_planes_vertex>
}
`;

/// Fragment shader for axis-perpendicular discs. Samples the gobo
/// using full 2D radial UV — preserves the gobo's actual pattern
/// shape, not a chord. Discs are only visible at their face-on
/// angles (edge-on they vanish), so we pair them with the crossed
/// planes which carry the continuous shaft from any angle.
const DISC_FRAGMENT = /* glsl */ `
#include <clipping_planes_pars_fragment>
uniform sampler2D uGoboMap;
uniform vec3 uColor;
uniform float uOpacity;
varying vec2 vRadial;

void main() {
  #include <clipping_planes_fragment>
  float radial = length(vRadial);
  if (radial > 1.0) discard;
  vec2 uv = (vRadial + 1.0) * 0.5;
  vec4 g = texture2D(uGoboMap, uv);
  float bright = max(g.r, max(g.g, g.b));
  float lifted = 0.4 + bright * 1.6;
  float radialFade = 1.0 - smoothstep(0.85, 1.0, radial);
  float alpha = lifted * radialFade * uOpacity;
  if (alpha <= 0.001) discard;
  vec3 finalColor = uColor * (0.85 + bright * 0.7);
  gl_FragColor = vec4(finalColor, alpha);
}
`;

interface BeamDisc {
  t: number;
  depth: number;
  radius: number;
  mat: THREE.ShaderMaterial;
  axial: number;
}

function buildDiscMaterial(radius: number): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    uniforms: {
      uGoboMap: { value: DEFAULT_WHITE_TEX },
      uColor: { value: new THREE.Color(1, 1, 1) },
      uOpacity: { value: 0 },
      uDiscRadius: { value: Math.max(radius, 1e-4) },
    },
    vertexShader: DISC_VERTEX,
    fragmentShader: DISC_FRAGMENT,
    transparent: true,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
    side: THREE.DoubleSide,
    toneMapped: false,
    clipping: true,
  });
}

/// Disc count per beam — denser packs better hide individual
/// slices. 24 discs over a typical beam length (~22m) gives ~1m
/// spacing which reads as a continuous extrusion when the camera
/// isn't perpendicular to the beam.
const BEAM_DISC_COUNT = 24;

function buildBeamDiscs(beamLength: number, maxRadius: number): BeamDisc[] {
  const arr: BeamDisc[] = [];
  for (let i = 0; i < BEAM_DISC_COUNT; i++) {
    const t = (i + 0.5) / BEAM_DISC_COUNT;
    const radius = Math.max(t * maxRadius, 1e-4);
    const axial =
      smoothstepEase(0, 0.05, t) * (1 - smoothstepEase(0.55, 1, t));
    arr.push({
      t,
      depth: t * beamLength,
      radius,
      mat: buildDiscMaterial(radius),
      axial,
    });
  }
  return arr;
}

function smoothstepEase(edge0: number, edge1: number, x: number): number {
  const t = Math.max(0, Math.min(1, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

/// Four crossed planes around the cone axis at 0/45/90/135°.
/// Each plane is a thin vertical sheet through the cone; together
/// they read as a continuous beam from any camera angle without
/// the discrete-disc artefact of stacked cross-sections. Rendering
/// cost: 4 quads per beam — vastly cheaper than the disc stack
/// while looking better.
function buildBeamPlanes(beamLength: number, maxRadius: number): BeamPlane[] {
  const angles = [0, Math.PI / 4, Math.PI / 2, (3 * Math.PI) / 4];
  return angles.map((angle) => ({
    angle,
    mat: buildBeamPlaneMaterial(beamLength, maxRadius, angle),
  }));
}

/// Per-label cache of procedural fallback textures. We only build
/// these once per distinct gobo label, so a wheel with 16 entries
/// allocates 16 small canvases, total ~1MB.
const proceduralGoboCache = new Map<string, THREE.Texture>();

function getProceduralGoboTexture(label: string): THREE.Texture {
  let tex = proceduralGoboCache.get(label);
  if (tex) return tex;
  const size = 256;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    tex = new THREE.Texture();
    proceduralGoboCache.set(label, tex);
    return tex;
  }
  // Black ground = blocks light; white shapes = let light through.
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, size, size);
  // Outer ring so the wheel position visually reads as a circle
  // (most real gobos have a circular footprint).
  ctx.strokeStyle = "#fff";
  ctx.lineWidth = 8;
  ctx.beginPath();
  ctx.arc(size / 2, size / 2, size / 2 - 18, 0, Math.PI * 2);
  ctx.stroke();
  // Decorative radial spokes — gives a "gobo-like" look that's
  // clearly different from a plain cone, and rotates on its own
  // visually as pan/tilt sweep (because the texture is fixed in
  // light-local space).
  ctx.save();
  ctx.translate(size / 2, size / 2);
  ctx.lineWidth = 4;
  for (let i = 0; i < 8; i++) {
    ctx.rotate(Math.PI / 4);
    ctx.beginPath();
    ctx.moveTo(28, 0);
    ctx.lineTo(size / 2 - 28, 0);
    ctx.stroke();
  }
  ctx.restore();
  // Label text centred. Truncated if too long to keep readable
  // even when projected from far.
  ctx.fillStyle = "#fff";
  ctx.font = "bold 26px sans-serif";
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const txt = label.length > 16 ? `${label.slice(0, 14)}…` : label;
  ctx.fillText(txt, size / 2, size / 2);

  tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  tex.wrapS = THREE.ClampToEdgeWrapping;
  tex.wrapT = THREE.ClampToEdgeWrapping;
  tex.minFilter = THREE.LinearFilter;
  tex.magFilter = THREE.LinearFilter;
  proceduralGoboCache.set(label, tex);
  return tex;
}

/// One fixture rendered in the 3D scene. Two physical models share
/// the same component:
///
///   - **Spot** (moving heads with zoom/gobo/prism/iris): tight beam,
///     narrow cone, high contrast core. Renders the 7-facet prism
///     pattern as N secondary cones at small angular splay when
///     `prismActive` is true.
///   - **Wash** (RGB pars / RGBW washes / dimmer-only fresnels):
///     wide soft cone, ambient-bath look. Always one cone, no
///     prism multiplexing.
///
/// Floor + back-wall clipping is delivered via material
/// `clippingPlanes`. Without that, the additive cone meshes
/// visibly cross the floor — bad enough that the user noticed it
/// the first second they opened the preview.
export function Fixture3D({
  position,
  state,
  rigHeight,
  atmosphere,
  clippingPlanes,
  aimYaw,
  aimPitch,
  overrides = DEFAULT_OVERRIDES,
  // hasGobo is computed in Preview3D for diagnostics but the
  // actual castShadow decision now uses isSpot — see prop docs.
  hasGobo: _hasGobo,
  isSpot,
}: {
  position: [number, number, number];
  state: FixtureLightState | undefined;
  rigHeight: number;
  atmosphere: number;
  /// Active clipping planes (floor, optional back wall) — passed in
  /// from Preview3D so all fixtures share the same plane instances
  /// and the clipping cost is paid once at render-state level.
  clippingPlanes: THREE.Plane[];
  /// Base aim BEFORE the live pan/tilt are applied. Yaw rotates
  /// around world Y, pitch around the local X after the yaw. This
  /// lets a fixture be "rigged facing forward" (pitch ≈ -π/2)
  /// without the 0,0 DMX values pointing the beam at the floor.
  aimYaw: number;
  aimPitch: number;
  /// Per-fixture render overrides (beam angle range, prism behaviour).
  /// `null` fields fall back to kind-based defaults.
  overrides?: FixtureRenderOverrides;
  /// True when this fixture's mode declares a gobo wheel with at
  /// least one projectable range. Used for diagnostics; the actual
  /// `castShadow` decision uses `isSpot` so the shader compiles
  /// with the SpotLight.map path available even before the first
  /// gobo image arrives over the wire.
  hasGobo: boolean;
  /// True for spot-class fixtures (have zoom/gobo/prism/iris).
  /// Drives `castShadow` so the shader unlocks SpotLight.map
  /// projection. Wash fixtures stay false so big rigs don't blow
  /// past the GPU's 16 texture-unit cap.
  isSpot: boolean;
}) {
  const yokeRef = useRef<THREE.Group | null>(null);
  const lightRef = useRef<THREE.SpotLight | null>(null);
  const bodyMatRef = useRef<THREE.MeshStandardMaterial | null>(null);
  const haloMatRef = useRef<THREE.MeshBasicMaterial | null>(null);

  // Cone shader materials — separate instances for the main cone
  // and the prism splat cones because they have different radii
  // (prism splats are narrower). Built once per (length, radius)
  // and updated in place each frame for color / opacity / gobo.
  const kind: FixtureKind = state?.kind ?? "wash";
  const beamLength = kind === "spot" ? 22 : 9;
  const maxRadius = useMemo(() => {
    const maxDeg = overrides.beamAngle?.maxDeg ?? (kind === "spot" ? 8 : 55);
    return beamLength * Math.tan(maxDeg * THREE.MathUtils.DEG2RAD);
  }, [kind, beamLength, overrides.beamAngle?.maxDeg]);
  // Crossed planes through the cone axis — 4 vertical sheets at
  // azimuths 0/45/90/135°. Combined additively they read as a
  // continuous volumetric shaft with the gobo's shape extruded
  // along the beam, not discrete floating discs.
  const mainBeamPlanes = useMemo(
    () => buildBeamPlanes(beamLength, maxRadius),
    [beamLength, maxRadius],
  );
  const prismBeamPlanes = useMemo(
    () => buildBeamPlanes(beamLength, maxRadius * 0.55),
    [beamLength, maxRadius],
  );
  // Discs perpendicular to the beam axis carry the gobo's full 2D
  // pattern (planes can only show 1D chords). Together they read
  // as a volumetric beam shaft with the gobo extruded.
  const mainBeamDiscs = useMemo(
    () => buildBeamDiscs(beamLength, maxRadius),
    [beamLength, maxRadius],
  );
  const prismBeamDiscs = useMemo(
    () => buildBeamDiscs(beamLength, maxRadius * 0.55),
    [beamLength, maxRadius],
  );
  // Apply clipping planes so the beam doesn't punch the floor or
  // back wall when the fixture is aimed at them.
  useEffect(() => {
    for (const p of mainBeamPlanes) p.mat.clippingPlanes = clippingPlanes;
    for (const p of prismBeamPlanes) p.mat.clippingPlanes = clippingPlanes;
    for (const d of mainBeamDiscs) d.mat.clippingPlanes = clippingPlanes;
    for (const d of prismBeamDiscs) d.mat.clippingPlanes = clippingPlanes;
  }, [
    mainBeamPlanes,
    prismBeamPlanes,
    mainBeamDiscs,
    prismBeamDiscs,
    clippingPlanes,
  ]);

  // Prism behaviour. Hoisted above useFrame so the closure has
  // access without a TDZ error. The renderer block uses these
  // again — same values, single source of truth.
  const prismThreshold = overrides.prism?.threshold ?? 8;
  const prismFacets = overrides.prism?.facets ?? 7;
  const prismSplayDeg = overrides.prism?.splayDeg ?? 6;
  const prismActive = (state?.prismValue ?? 0) > prismThreshold;

  // The SpotLight needs a target whose `matrixWorld` actually
  // updates with the scene graph. The default `light.target` is an
  // unparented Object3D — its matrixWorld stays identity, which
  // makes the shadow camera always look at world origin and the
  // gobo always project at floor centre instead of where the
  // fixture is actually aimed. We create our own target, parent
  // it under the tilt group below, and rebind it via effect.
  const targetObj = useMemo(() => {
    const t = new THREE.Object3D();
    t.name = "fx-spot-target";
    return t;
  }, []);
  useEffect(() => {
    if (lightRef.current) lightRef.current.target = targetObj;
  }, [targetObj]);

  // Per-frame: write current colour/intensity/strobe/zoom into the
  // beam mesh + spotLight + lens emissive. Smooth pan/tilt with an
  // exponential follow.
  useFrame((_, delta) => {
    const s = state;
    const yoke = yokeRef.current;
    if (!yoke) return;
    if (!s) {
      // No state yet (snapshot still loading) — leave defaults.
      return;
    }
    const strobeFactor =
      s.strobe > 0.02
        ? 0.5 + 0.5 * Math.sin(performance.now() * 0.001 * (4 + s.strobe * 50))
        : 1;
    const lit = s.intensity * strobeFactor;

    // Beam shader uniforms — one main cone + optional prism cones,
    // both sharing the per-fixture color / opacity / gobo state.
    // The shader samples the gobo radially so the BEAM ITSELF
    // takes the gobo's shape (not just the floor projection).
    const goboTex = s.goboImage
      ? getGoboTexture(s.goboImage)
      : s.goboLabel
        ? getProceduralGoboTexture(s.goboLabel)
        : DEFAULT_WHITE_TEX;
    // baseOpacity drives both the planes (continuous shaft) and
    // the discs (gobo 2D shape). Planes get most of the budget
    // since they're visible from any angle; discs add the gobo
    // pattern detail at oblique views.
    const baseOpacity = lit * (0.55 + atmosphere * 1.6);
    const prismOn = (s.prismValue ?? 0) > prismThreshold;
    const mainScale = prismOn ? 0.55 : 1;
    for (const p of mainBeamPlanes) {
      p.mat.uniforms.uColor.value.setRGB(s.color.r, s.color.g, s.color.b);
      p.mat.uniforms.uOpacity.value = baseOpacity * mainScale * 0.7;
      p.mat.uniforms.uGoboMap.value = goboTex;
    }
    for (const p of prismBeamPlanes) {
      p.mat.uniforms.uColor.value.setRGB(s.color.r, s.color.g, s.color.b);
      p.mat.uniforms.uOpacity.value = baseOpacity * 0.5;
      p.mat.uniforms.uGoboMap.value = goboTex;
    }
    // Discs: per-disc axial fade pre-baked at construction.
    for (const d of mainBeamDiscs) {
      d.mat.uniforms.uColor.value.setRGB(s.color.r, s.color.g, s.color.b);
      d.mat.uniforms.uOpacity.value = baseOpacity * mainScale * d.axial * 0.45;
      d.mat.uniforms.uGoboMap.value = goboTex;
    }
    for (const d of prismBeamDiscs) {
      d.mat.uniforms.uColor.value.setRGB(s.color.r, s.color.g, s.color.b);
      d.mat.uniforms.uOpacity.value = baseOpacity * d.axial * 0.3;
      d.mat.uniforms.uGoboMap.value = goboTex;
    }

    if (lightRef.current) {
      lightRef.current.color.setRGB(
        Math.max(s.color.r, 0.01),
        Math.max(s.color.g, 0.01),
        Math.max(s.color.b, 0.01),
      );
      const baseLum = s.kind === "spot" ? 22 : 12;
      lightRef.current.intensity = lit * baseLum;
      lightRef.current.angle = beamHalfAngleRad(s.kind, s.zoom, overrides.beamAngle);
      // Spots: tight penumbra so the gobo / beam edges read sharp.
      // Washes: penumbra near 1 means the entire cone is the falloff
      // zone, so the floor projection is a smooth radial gradient
      // with no visible edge — exactly the "bañando el ambiente"
      // look the operator wanted from RGB pars.
      lightRef.current.penumbra = s.kind === "spot" ? 0.3 : 0.95;
      // Gobo projection. Three.js samples the spotLight.map
      // through the light's `shadow.matrix`. With castShadow=true
      // the shadow pass updates that matrix automatically, but we
      // also update it manually so the transition is one frame
      // tighter (no half-frame lag between fixture move and
      // projection follow). Falls back to a procedural texture
      // (label + radial spokes) when the active range has no
      // image — common with imported defs that ship labels but
      // not bitmaps.
      const wantsProjection = s.goboImage !== null || s.goboLabel !== null;
      // Three.js disables `light.map` when castShadow is false. The
      // `hasGobo` mount-time prop should already have set this true
      // for fixtures whose def declares a gobo wheel — but if the
      // role/name detection was off (or the def was edited after
      // mount), we override at runtime here so any fixture with an
      // active live gobo gets the shader path enabled.
      if (wantsProjection && !lightRef.current.castShadow) {
        lightRef.current.castShadow = true;
      }
      if (s.goboImage) {
        const tex = getGoboTexture(s.goboImage);
        if (lightRef.current.map !== tex) lightRef.current.map = tex;
        lightRef.current.shadow.updateMatrices(lightRef.current);
      } else if (s.goboLabel) {
        const tex = getProceduralGoboTexture(s.goboLabel);
        if (lightRef.current.map !== tex) lightRef.current.map = tex;
        lightRef.current.shadow.updateMatrices(lightRef.current);
      } else if (lightRef.current.map) {
        lightRef.current.map = null;
      }
    }
    if (bodyMatRef.current) {
      bodyMatRef.current.emissive.setRGB(
        s.color.r * 0.9,
        s.color.g * 0.9,
        s.color.b * 0.9,
      );
      bodyMatRef.current.emissiveIntensity = lit;
    }
    if (haloMatRef.current) {
      haloMatRef.current.color.setRGB(s.color.r, s.color.g, s.color.b);
      haloMatRef.current.opacity = Math.min(
        1,
        lit * (s.kind === "wash" ? 0.85 : 0.5),
      );
    }

    // Smooth pan/tilt: exponential follow ~250ms time constant.
    const k = 1 - Math.exp(-delta * 8);
    yoke.rotation.y = THREE.MathUtils.lerp(yoke.rotation.y, s.pan, k);
    const tiltChild = yoke.children[0];
    if (tiltChild) {
      tiltChild.rotation.x = THREE.MathUtils.lerp(tiltChild.rotation.x, s.tilt, k);
    }
  });

  return (
    <group position={position}>
      {/* Body / casing — sits at rig height. The yoke (pan/tilt
          driver) hangs just below. */}
      <mesh position={[0, 0, 0]} castShadow={false}>
        <boxGeometry args={[0.35, 0.25, 0.35]} />
        <meshStandardMaterial color="#1a1a1a" metalness={0.5} roughness={0.6} />
      </mesh>

      {/* Base aim (yaw + pitch) — applied as a fixed transform
          BEFORE the live pan/tilt yoke. With aim=(0,0) the lens
          points -Y (straight down) and pan/tilt sweep relative to
          that. With aim pitch=-π/2 the fixture aims horizontally
          forward (a "Sharpy on a deck" rig). */}
      <group rotation={[0, aimYaw, 0]}>
        <group rotation={[aimPitch, 0, 0]}>
          <group ref={yokeRef} position={[0, -0.18, 0]}>
            {/* Tilt child — beam, light, lens all rotate around X here. */}
            <group>
          {/* Lens face — the bright disc you see when looking up at
              a fixture. Small + emissive, drives the bloom hot
              core. */}
          <mesh rotation={[Math.PI, 0, 0]}>
            <circleGeometry args={[0.12, 32]} />
            <meshStandardMaterial
              ref={bodyMatRef}
              color="#0e0e10"
              emissive={new THREE.Color(0x000000)}
              emissiveIntensity={0}
              toneMapped={false}
            />
          </mesh>
          {/* Halo — wider washes should have a much fuller halo so
              the soft-floor-glow look reads even when the cone is
              dim through bloom. */}
          <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.001, 0]}>
            <circleGeometry args={[kind === "wash" ? 0.5 : 0.32, 32]} />
            <meshBasicMaterial
              ref={haloMatRef}
              color={new THREE.Color(0xffffff)}
              transparent
              opacity={0}
              blending={THREE.AdditiveBlending}
              depthWrite={false}
              toneMapped={false}
              clippingPlanes={clippingPlanes}
            />
          </mesh>

          {/* === Beam cones === */}
          {/* Spots get a real beam cone (additive volumetric look,
              prism splat when active). Washes (RGB pars / RGBW
              wash heads / dimmer-only fresnels) DON'T render a
              cone — their visual is a glow at the lens plus the
              SpotLight bathing the surfaces below with very soft
              edges. That matches what an LED par actually looks
              like in a smokeless room. */}
          {kind === "spot"
            ? mainBeamPlanes.map((p, i) => (
                <mesh
                  key={`plane-${i}`}
                  position={[0, -beamLength / 2, 0]}
                  rotation={[0, p.angle, 0]}
                >
                  <planeGeometry args={[2 * maxRadius, beamLength]} />
                  <primitive object={p.mat} attach="material" />
                </mesh>
              ))
            : null}
          {/* Disc cross-sections paired with the planes — these
              carry the gobo's actual 2D pattern (planes only see
              chords). They mostly show when the camera isn't
              perpendicular to the beam axis. */}
          {kind === "spot"
            ? mainBeamDiscs.map((d, i) => (
                <mesh
                  key={`disc-${i}`}
                  position={[0, -d.depth, 0]}
                  rotation={[-Math.PI / 2, 0, 0]}
                >
                  <circleGeometry args={[d.radius, 48]} />
                  <primitive object={d.mat} attach="material" />
                </mesh>
              ))
            : null}
          {kind === "spot" && prismActive
            ? Array.from({ length: prismFacets }).flatMap((_, i, arr) => {
                if (i === 0) return [];
                const angle = ((i - 1) / (arr.length - 1)) * Math.PI * 2;
                const splay = prismSplayDeg * THREE.MathUtils.DEG2RAD;
                const rx = Math.sin(angle) * splay;
                const rz = Math.cos(angle) * splay;
                return [
                  <group key={`prism-${i}`} rotation={[rx, 0, rz]}>
                    {prismBeamPlanes.map((p, j) => (
                      <mesh
                        key={`prism-plane-${j}`}
                        position={[0, -beamLength / 2, 0]}
                        rotation={[0, p.angle, 0]}
                      >
                        <planeGeometry args={[2 * maxRadius * 0.55, beamLength]} />
                        <primitive object={p.mat} attach="material" />
                      </mesh>
                    ))}
                    {prismBeamDiscs.map((d, j) => (
                      <mesh
                        key={`prism-disc-${j}`}
                        position={[0, -d.depth, 0]}
                        rotation={[-Math.PI / 2, 0, 0]}
                      >
                        <circleGeometry args={[d.radius, 32]} />
                        <primitive object={d.mat} attach="material" />
                      </mesh>
                    ))}
                  </group>,
                ];
              })
            : null}

          {/* Real spotLight illuminating geometry. castShadow is
              enabled ONLY for fixtures that need gobo projection —
              each shadow-casting spotLight reserves a fragment
              texture unit, and the GPU caps that at 16. Wash /
              par fixtures keep castShadow=false so the shader
              compiles cleanly even with 30+ lights in the scene. */}
          <spotLight
            ref={lightRef}
            position={[0, 0, 0]}
            angle={0.3}
            penumbra={0.4}
            intensity={0}
            distance={beamLength * 1.2}
            decay={1.4}
            castShadow={isSpot}
            shadow-mapSize-width={256}
            shadow-mapSize-height={256}
          />
          {/* Target sits in the same local space as the light, 1m
              "below" along the tilt-group's -Y. Because it's a
              real scene graph child, its matrixWorld actually
              updates when pan/tilt rotate, so the shadow camera
              looks where the fixture is aimed instead of always
              at world origin. */}
          <primitive object={targetObj} position={[0, -1, 0]} />
        </group>
          </group>
        </group>
      </group>

      <mesh position={[0, 0.18, 0]}>
        <sphereGeometry args={[0.05, 12, 12]} />
        <meshBasicMaterial color="#3a3a3a" toneMapped={false} />
      </mesh>


      {rigHeight > 0 ? (
        <mesh position={[0, rigHeight / 2, 0]}>
          <cylinderGeometry args={[0.005, 0.005, rigHeight, 6]} />
          <meshBasicMaterial color="#222" />
        </mesh>
      ) : null}
    </group>
  );
}

function beamHalfAngleRad(
  kind: FixtureKind,
  zoom: number,
  override: { minDeg: number; maxDeg: number } | null,
): number {
  // Override path: operator-tuned beam range. Most useful for
  // matching specific fixtures — a Sharpy is 0-3°, a Mac Aura is
  // 8-25°, a Mac Quantum is 7-50°. Operators dial the real numbers
  // off the spec sheet and the cone snaps to match.
  if (override) {
    return THREE.MathUtils.lerp(
      override.minDeg * THREE.MathUtils.DEG2RAD,
      override.maxDeg * THREE.MathUtils.DEG2RAD,
      zoom,
    );
  }
  // Defaults by kind. Spots = small-format beam-class (2-8°);
  // washes = wide ambient bath (25-55°).
  if (kind === "spot") {
    return THREE.MathUtils.lerp(
      2 * THREE.MathUtils.DEG2RAD,
      8 * THREE.MathUtils.DEG2RAD,
      zoom,
    );
  }
  return THREE.MathUtils.lerp(
    25 * THREE.MathUtils.DEG2RAD,
    55 * THREE.MathUtils.DEG2RAD,
    zoom,
  );
}

