/** Raw triangle mesh data for rendering in the 3D viewport. */
export interface MeshData {
  /** Flat array of vertex positions [x0,y0,z0, x1,y1,z1, ...] */
  positions: Float32Array;
  /** Flat array of vertex normals  [nx0,ny0,nz0, ...] */
  normals: Float32Array;
  /** Triangle index array [i0,i1,i2, ...] */
  indices: Uint32Array;
}

/** A single feature (operation) in the model history. */
export interface Feature {
  id: string;
  /** Human-readable label shown in the feature tree. */
  label: string;
  /** The kind of operation this feature represents. */
  type: FeatureType;
  /** Whether the feature is currently suppressed (excluded from evaluation). */
  suppressed: boolean;
}

export type FeatureType =
  | "box"
  | "cylinder"
  | "sphere"
  | "extrude"
  | "union"
  | "subtract"
  | "intersect";

/** Top-level information about the current model. */
export interface ModelInfo {
  name: string;
  features: Feature[];
  /** The evaluated mesh that should be displayed in the viewport. */
  mesh: MeshData | null;
}

/** Toolbar action identifiers. */
export type ToolAction =
  | "box"
  | "cylinder"
  | "sphere"
  | "extrude"
  | "union"
  | "subtract"
  | "intersect";
