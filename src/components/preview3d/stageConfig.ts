/// Stage geometry config — the operator's mental picture of the
/// physical room. Lives in localStorage keyed by show path so each
/// venue / setup the operator works with keeps its own dimensions
/// without polluting the show file (which gets shared / synced).

/// Per-fixture override telling the 3D preview where to draw a
/// fixture in stage space and which way it's pointing before
/// pan/tilt are applied. Without this the preview falls back to
/// projecting the fixture's 2D canvas position into world space at
/// the global rig height — fine for a quick look, useless for a
/// real rig where the operator has 6 fixtures spread along the
/// front truss and 4 more on a back ladder.
///
/// Two attachment modes:
///   - **free**: explicit world-space position. Use when the
///     fixture is floor-mounted or on a piece of structure we
///     don't model as a truss.
///   - **truss**: anchored to a truss segment. Position is computed
///     from the segment endpoints + a `t ∈ [0,1]` slider, so when
///     the operator nudges the truss the fixture moves with it.
export interface FixturePlacement {
  /// Mode picker. "free" or "truss".
  mode: "free" | "truss";
  /// World-space position. Only used in "free" mode; in "truss"
  /// mode the X,Y,Z come from the linked truss.
  free: { x: number; y: number; z: number };
  /// Truss attachment. `t` runs from 0 (segment start) to 1 (end).
  /// Y comes from the truss height.
  truss: { trussId: string | null; t: number };
  /// Base aim rotations applied BEFORE pan/tilt. Yaw is around the
  /// world Y axis (compass heading); pitch is around the local X
  /// after yaw. Both in radians.
  /// Defaults (0,0) = lens pointing straight down, lens facing
  /// audience direction. Pitch = -π/2 makes the fixture point
  /// forward instead of down (e.g. a fixture on the floor aimed at
  /// the band).
  aimYaw: number;
  aimPitch: number;
  /// Legacy: beam-angle / prism used to be set per-instance, but
  /// these belong to the fixture type, not the unit on stage. Moved
  /// to [DefinitionRenderOverrides] keyed by definition id (read
  /// via `config.definitionOverrides[f.definition_id]`). The fields
  /// stay on the type so old saved configs still parse — the render
  /// path stopped reading them.
  beamAngle?: { minDeg: number; maxDeg: number } | null;
  prism?: { threshold: number; facets: number; splayDeg: number } | null;
}

/// Per-fixture-DEFINITION render tunables for the 3D preview. These
/// describe the physical characteristics of a fixture model (a
/// Sharpy beam vs. a Mac Aura wash), so they're shared by every
/// instance of the same definition. Stored per show because the
/// brightness multiplier depends on the operator's current monitor
/// (a dim screen wants higher; a bright laptop screen wants lower).
export interface DefinitionRenderOverrides {
  /// Multiplier applied to the rendered intensity (beam + spotlight
  /// + lens halo together). Default 1.0. Useful range is roughly
  /// 0.2 (very dim) to 1.8 (boost). Per-fixture-TYPE because all
  /// instances of the same model should glow with the same scale.
  brightness: number;
  /// Beam half-angle range in degrees. `null` = kind-based defaults
  /// (spots 2-8°, washes 25-55°). The zoom DMX channel lerps min↔
  /// max as 0..1.
  beamAngle: { minDeg: number; maxDeg: number } | null;
  /// Prism behaviour. `null` = defaults (threshold 8 / facets 7 /
  /// splay 6°).
  prism: { threshold: number; facets: number; splayDeg: number } | null;
}

export function defaultDefinitionRenderOverrides(): DefinitionRenderOverrides {
  return { brightness: 1, beamAngle: null, prism: null };
}

export function defaultFixturePlacement(): FixturePlacement {
  return {
    mode: "free",
    free: { x: 0, y: 5, z: 0 },
    truss: { trussId: null, t: 0.5 },
    aimYaw: 0,
    aimPitch: 0,
  };
}

export interface TrussSegment {
  id: string;
  name?: string;
  /// Endpoints in stage coordinates (meters). Origin is centre of
  /// the floor; +X is stage-right, +Z is upstage (away from
  /// audience), Y is up.
  fromX: number;
  fromZ: number;
  toX: number;
  toZ: number;
  /// Truss height above the floor. Front truss usually 4-6m for
  /// clubs, 6-9m for theatres.
  height: number;
  /// Cross-section thickness used for the visual rendering only —
  /// real trusses are 30-50cm; we keep it slim for visual clarity.
  diameter: number;
}

export interface StageConfig {
  floor: {
    /// Stage left-right span (along X axis).
    width: number;
    /// Stage front-back span (along Z axis).
    depth: number;
    /// Grid cell size in meters. 1m is the universal default but
    /// some operators think in 0.5m or feet.
    cellSize: number;
    /// Hex colour for the grid lines.
    gridColor: string;
    /// Hex colour for the floor surface itself. Default near-black
    /// reads dramatic but bounces almost nothing — bump it (greys,
    /// warm browns) to actually see RGB par output land on the
    /// floor. Bigger albedo = more visible wash.
    color: string;
  };
  /// Optional back wall (cyc, scrim) — just a flat plane standing
  /// upstage, for fixtures pointing back to render against.
  backWall: {
    enabled: boolean;
    height: number;
    /// Hex colour for the wall surface. Same bouncing logic as the
    /// floor: a darker wall eats wash output, a lighter wall reads
    /// the colour the rig is throwing at it.
    color: string;
  };
  trusses: TrussSegment[];
  /// Default rig height for fixtures that don't have a per-fixture
  /// override yet. Operator can override globally; a future
  /// iteration will move this per-fixture.
  rigHeight: number;
  /// Atmosphere / haze density (0..1). Drives both fog density and
  /// beam visibility — without haze, beams look invisible because
  /// real beams need particles to scatter on.
  atmosphere: number;
  /// Room ambient light (0..1). 0 = pitch black venue (most
  /// dramatic; default), 1 = house lights on / rehearsal-room
  /// bright. Operators rarely have total blackout when designing,
  /// so this slider lets them match their actual room.
  ambientLevel: number;
  /// World-space scale: how many pixels of the 2D Stage canvas map
  /// to one meter in the 3D preview. Default 100 means a fixture
  /// 100px to the right shows up 1m to the right in 3D.
  pixelsPerMeter: number;
  /// Per-fixture placement overrides keyed by fixture id. Missing
  /// entries fall back to the 2D-canvas-derived position +
  /// rigHeight + (0,0) aim.
  fixturePlacements: Record<string, FixturePlacement>;
  /// Per-fixture-DEFINITION render tunables keyed by definition id
  /// (the library entry, e.g. "chauvet-maverick-mk2"). Brightness +
  /// beam angle + prism live here because they describe the fixture
  /// model, not the individual unit on stage. Missing entries fall
  /// back to the kind-based defaults inside Fixture3D.
  definitionOverrides: Record<string, DefinitionRenderOverrides>;
  /// Optional crowd silhouettes filling the dance floor. Off by
  /// default (clean floor reads better when first opening the
  /// preview); operator can flip on for the venue feel.
  audience: AudienceConfig;
}

/// Crowd / audience zone — a rectangle on the floor where dark
/// humanoid silhouettes are scattered with deterministic
/// pseudo-random positions. The rig's beams catch them so the
/// audience changes colour as the show plays — that's the visual
/// payoff that makes the preview feel like a real venue.
export interface AudienceConfig {
  enabled: boolean;
  /// Rectangular footprint in stage coords. Default covers the
  /// main stretch of the floor in front of the front truss.
  zone: { x1: number; z1: number; x2: number; z2: number };
  /// People per square metre. 0.3 = comfortable spread, 1+ =
  /// packed festival crowd.
  density: number;
  /// Approximate height of the average person (metres). Used for
  /// the body capsule + head sphere proportions.
  averageHeight: number;
  /// Hex colour for the silhouette body + head material. Default
  /// near-black reads as silhouette against the floor; bumping it
  /// to a mid grey makes the rig's colour wash actually visible
  /// on the crowd (which is the main reason the audience exists
  /// in the preview).
  color: string;
}

const STORAGE_PREFIX = "preview3d.config.";

export function defaultStageConfig(): StageConfig {
  return {
    floor: {
      width: 10,
      depth: 8,
      cellSize: 1,
      gridColor: "#3a4a5a",
      color: "#0c1218",
    },
    backWall: {
      enabled: true,
      height: 5,
      color: "#0a0d12",
    },
    trusses: [
      // Front truss across stage width at FOH height — typical small
      // club rig. Operator can add more or move them.
      {
        id: "truss-front",
        name: "Front truss",
        fromX: -5,
        fromZ: -3,
        toX: 5,
        toZ: -3,
        height: 5,
        diameter: 0.25,
      },
      {
        id: "truss-back",
        name: "Back truss",
        fromX: -5,
        fromZ: 3,
        toX: 5,
        toZ: 3,
        height: 5,
        diameter: 0.25,
      },
    ],
    rigHeight: 5,
    atmosphere: 0.5,
    ambientLevel: 0.1,
    pixelsPerMeter: 100,
    fixturePlacements: {},
    definitionOverrides: {},
    audience: {
      enabled: false,
      zone: { x1: -4, z1: -2.5, x2: 4, z2: 2.5 },
      density: 0.3,
      averageHeight: 1.7,
      color: "#0d1014",
    },
  };
}

export function loadStageConfig(showPath: string | null): StageConfig {
  const key = STORAGE_PREFIX + (showPath ?? "_untitled");
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return defaultStageConfig();
    const parsed = JSON.parse(raw) as Partial<StageConfig>;
    // Shallow merge with defaults so older saved configs gain new
    // fields without crashing the UI.
    const def = defaultStageConfig();
    // Migrate legacy per-instance beamAngle/prism into the new
    // per-definition map, best-effort. Old saved configs keep their
    // tuning even though the UI moved one level up.
    const migratedDefOverrides: Record<string, DefinitionRenderOverrides> = {
      ...def.definitionOverrides,
      ...(parsed.definitionOverrides ?? {}),
    };
    return {
      ...def,
      ...parsed,
      floor: { ...def.floor, ...(parsed.floor ?? {}) },
      backWall: { ...def.backWall, ...(parsed.backWall ?? {}) },
      trusses: parsed.trusses ?? def.trusses,
      fixturePlacements: parsed.fixturePlacements ?? def.fixturePlacements,
      definitionOverrides: migratedDefOverrides,
      audience: { ...def.audience, ...(parsed.audience ?? {}) },
    };
  } catch {
    return defaultStageConfig();
  }
}

export function saveStageConfig(showPath: string | null, cfg: StageConfig) {
  const key = STORAGE_PREFIX + (showPath ?? "_untitled");
  try {
    localStorage.setItem(key, JSON.stringify(cfg));
  } catch {
    // localStorage unavailable — config still works in-session.
  }
}
