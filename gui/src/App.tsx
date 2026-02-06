import React, { useCallback, useState } from "react";
import Toolbar from "./components/Toolbar";
import FeatureTree from "./components/FeatureTree";
import Viewport from "./components/Viewport";
import { Feature, MeshData, ToolAction } from "./types";

/** Generate a simple unique id. */
let nextId = 1;
function uid(): string {
  return `feat-${nextId++}`;
}

/** Human-readable default labels for each feature type. */
const defaultLabels: Record<ToolAction, string> = {
  box: "Box",
  cylinder: "Cylinder",
  sphere: "Sphere",
  extrude: "Extrude",
  union: "Boolean Union",
  subtract: "Boolean Subtract",
  intersect: "Boolean Intersect",
};

const styles: Record<string, React.CSSProperties> = {
  root: {
    width: "100vw",
    height: "100vh",
    display: "flex",
    flexDirection: "column",
    overflow: "hidden",
    background: "#12121f",
  },
  body: {
    flex: 1,
    display: "flex",
    overflow: "hidden",
  },
  viewport: {
    flex: 1,
    position: "relative",
  },
};

const App: React.FC = () => {
  const [features, setFeatures] = useState<Feature[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [mesh] = useState<MeshData | null>(null);

  const handleAction = useCallback((action: ToolAction) => {
    const id = uid();
    const feature: Feature = {
      id,
      label: defaultLabels[action],
      type: action,
      suppressed: false,
    };
    setFeatures((prev) => [...prev, feature]);
    setSelectedId(id);
  }, []);

  const handleSelect = useCallback((id: string) => {
    setSelectedId(id);
  }, []);

  return (
    <div style={styles.root}>
      {/* Top toolbar */}
      <Toolbar onAction={handleAction} />

      {/* Main body: sidebar + viewport */}
      <div style={styles.body}>
        <FeatureTree
          features={features}
          selectedId={selectedId}
          onSelect={handleSelect}
        />
        <div style={styles.viewport}>
          <Viewport mesh={mesh} />
        </div>
      </div>
    </div>
  );
};

export default App;
