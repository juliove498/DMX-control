// Render budget tiers for the 3D preview. Per machine (instance features):
// the 2015 Intel runs "low", the M1 "medium"/"high".

export type QualityLevel = "low" | "medium" | "high";

export interface RenderQuality {
  level: QualityLevel;
  dpr: [number, number];
  bloom: boolean;
  msaa: number;
  /** Crossed planes per beam cone (spots); washes use ~60 % of this. */
  beamPlanes: number;
  /** Disc cross-sections per beam. */
  beamDiscs: number;
  audience: boolean;
  /** Frame cap via demand-mode invalidation; null = vsync. */
  fpsCap: number | null;
  /** Renderer shadow maps (needed for gobo projection). */
  shadows: boolean;
}

export function qualityFor(level: string | undefined): RenderQuality {
  switch (level) {
    case "low":
      return {
        level: "low",
        dpr: [1, 1],
        bloom: false,
        msaa: 0,
        beamPlanes: 3,
        beamDiscs: 8,
        audience: false,
        fpsCap: 24,
        shadows: false,
      };
    case "high":
      return {
        level: "high",
        dpr: [1, 2],
        bloom: true,
        msaa: 2,
        beamPlanes: 8,
        beamDiscs: 24,
        audience: true,
        fpsCap: null,
        shadows: true,
      };
    default:
      return {
        level: "medium",
        dpr: [1, 1.5],
        bloom: true,
        msaa: 0,
        beamPlanes: 5,
        beamDiscs: 14,
        audience: true,
        fpsCap: null,
        shadows: true,
      };
  }
}
