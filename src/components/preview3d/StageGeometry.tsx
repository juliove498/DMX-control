import { Grid } from "@react-three/drei";
import type { StageConfig } from "./stageConfig";

/// Floor + grid + back wall + trusses. All geometry sits at world
/// origin; the floor centre is (0,0,0) and Y is up. Trusses are
/// rendered as thin elongated boxes — a real ladder-truss visual is
/// future work; for now the silhouette is enough to feel rigged.
export function StageGeometry({ config }: { config: StageConfig }) {
  const { floor, backWall, trusses } = config;

  return (
    <group>
      {/* Dark floor — slightly receding to the back so the audience
          camera angle reads. `receiveShadow` is essential: it adds
          the USE_SHADOWMAP define to the floor's material program,
          which is what unlocks SpotLight.map sampling so gobo
          projections actually appear here. */}
      <mesh receiveShadow rotation={[-Math.PI / 2, 0, 0]} position={[0, 0, 0]}>
        <planeGeometry args={[floor.width, floor.depth]} />
        <meshStandardMaterial color="#0c1218" roughness={0.85} metalness={0.05} />
      </mesh>

      {/* Grid lines — drei's <Grid> draws an infinite grid by
          default; we cap it to the floor footprint. */}
      <Grid
        args={[floor.width, floor.depth]}
        cellSize={floor.cellSize}
        cellThickness={0.6}
        cellColor={floor.gridColor}
        sectionSize={floor.cellSize * 5}
        sectionThickness={1.2}
        sectionColor={floor.gridColor}
        fadeDistance={Math.max(floor.width, floor.depth) * 1.2}
        fadeStrength={1}
        infiniteGrid={false}
        position={[0, 0.001, 0]}
      />

      {backWall.enabled ? (
        // receiveShadow already set — but the gotcha is that
        // BOTH the renderer's shadowMap.enabled AND each
        // receiver's `receiveShadow` are needed for the spotLight
        // gobo projection to render on this surface.
        <mesh position={[0, backWall.height / 2, floor.depth / 2]} receiveShadow>
          <planeGeometry args={[floor.width, backWall.height]} />
          <meshStandardMaterial color="#0a0d12" roughness={0.95} side={2} />
        </mesh>
      ) : null}

      {trusses.map((t) => (
        <TrussSegmentMesh key={t.id} segment={t} />
      ))}
    </group>
  );
}

function TrussSegmentMesh({ segment }: { segment: StageConfig["trusses"][number] }) {
  const dx = segment.toX - segment.fromX;
  const dz = segment.toZ - segment.fromZ;
  const length = Math.hypot(dx, dz);
  const cx = (segment.fromX + segment.toX) / 2;
  const cz = (segment.fromZ + segment.toZ) / 2;
  // Truss lies horizontally on the XZ plane at the configured height;
  // we orient a unit-length box along the segment direction by
  // rotating around Y by the angle from +X.
  const angleY = Math.atan2(-dz, dx);
  return (
    <group position={[cx, segment.height, cz]} rotation={[0, angleY, 0]}>
      {/* Main truss tube — silvered cylinder. Slightly emissive so
          it's visible even when no fixtures are pointing at it. */}
      <mesh receiveShadow>
        <boxGeometry args={[length, segment.diameter, segment.diameter]} />
        <meshStandardMaterial
          color="#7a8088"
          metalness={0.7}
          roughness={0.35}
          emissive="#1a1d22"
        />
      </mesh>
      {/* End caps — visual hint of where the segment terminates. */}
      <mesh position={[length / 2, 0, 0]} receiveShadow>
        <boxGeometry args={[segment.diameter, segment.diameter * 1.3, segment.diameter * 1.3]} />
        <meshStandardMaterial color="#5a6068" metalness={0.7} roughness={0.4} />
      </mesh>
      <mesh position={[-length / 2, 0, 0]} receiveShadow>
        <boxGeometry args={[segment.diameter, segment.diameter * 1.3, segment.diameter * 1.3]} />
        <meshStandardMaterial color="#5a6068" metalness={0.7} roughness={0.4} />
      </mesh>
    </group>
  );
}

