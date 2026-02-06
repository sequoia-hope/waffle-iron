import React, { useMemo } from "react";
import { Canvas } from "@react-three/fiber";
import { OrbitControls, GizmoHelper, GizmoViewport } from "@react-three/drei";
import * as THREE from "three";
import { MeshData } from "../types";

/** Props accepted by the Viewport component. */
interface ViewportProps {
  /** Optional triangle mesh to display. When null the viewport shows only the grid and axes. */
  mesh: MeshData | null;
}

/** Renders a triangle mesh from raw buffer data. */
function TriangleMesh({ mesh }: { mesh: MeshData }) {
  const geometry = useMemo(() => {
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.BufferAttribute(mesh.positions, 3));
    geo.setAttribute("normal", new THREE.BufferAttribute(mesh.normals, 3));
    geo.setIndex(new THREE.BufferAttribute(mesh.indices, 1));
    return geo;
  }, [mesh]);

  return (
    <mesh geometry={geometry}>
      <meshStandardMaterial color="#6a9fd8" side={THREE.DoubleSide} />
    </mesh>
  );
}

/** Infinite-style grid rendered as a gridHelper. */
function Grid() {
  return (
    <gridHelper args={[20, 20, "#888888", "#cccccc"]} />
  );
}

/** RGB axis lines at the origin (R=X, G=Y, B=Z). */
function CoordinateAxes() {
  const length = 3;
  return (
    <group>
      <arrowHelper args={[new THREE.Vector3(1, 0, 0), new THREE.Vector3(0, 0, 0), length, 0xff4444]} />
      <arrowHelper args={[new THREE.Vector3(0, 1, 0), new THREE.Vector3(0, 0, 0), length, 0x44cc44]} />
      <arrowHelper args={[new THREE.Vector3(0, 0, 1), new THREE.Vector3(0, 0, 0), length, 0x4488ff]} />
    </group>
  );
}

/**
 * Three.js 3D viewport with orbit controls, a reference grid, coordinate axes,
 * and the ability to render an arbitrary triangle mesh.
 */
const Viewport: React.FC<ViewportProps> = ({ mesh }) => {
  return (
    <div style={{ width: "100%", height: "100%", background: "#1a1a2e" }}>
      <Canvas
        camera={{ position: [6, 4, 6], fov: 50, near: 0.1, far: 1000 }}
        gl={{ antialias: true }}
      >
        {/* Lighting */}
        <ambientLight intensity={0.4} />
        <directionalLight position={[8, 10, 5]} intensity={0.8} />
        <directionalLight position={[-5, 3, -5]} intensity={0.3} />

        {/* Scene helpers */}
        <Grid />
        <CoordinateAxes />

        {/* Optional model mesh */}
        {mesh && <TriangleMesh mesh={mesh} />}

        {/* Navigation */}
        <OrbitControls makeDefault enableDamping dampingFactor={0.12} />

        {/* Orientation gizmo in the top-right corner */}
        <GizmoHelper alignment="top-right" margin={[72, 72]}>
          <GizmoViewport labelColor="white" axisHeadScale={0.8} />
        </GizmoHelper>
      </Canvas>
    </div>
  );
};

export default Viewport;
