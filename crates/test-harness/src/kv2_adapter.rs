//! `KernelV2Adapter` — legacy `kernel::Kernel` + `kernel::KernelIntrospect`
//! over a kernel-v2 arena (PR-KV4, Phase 4a EXIT).
//!
//! ## Purpose
//!
//! The assay corpus replay (feature-engine + wasm-bridge dispatch) drives
//! the LEGACY `Kernel`/`KernelIntrospect` traits. kernel-v2 has its own,
//! cleaner API and may not read legacy code. This adapter is the bridge —
//! it lives in test-harness (the one place allowed to see both worlds),
//! maps legacy calls onto kernel-v2 operations, and returns the legacy
//! `KernelError::NotSupported` LOUDLY for anything kernel-v2 does not yet
//! implement. The adapter MAPS; it never repairs, approximates, or stubs a
//! result (no polygonized circles, no fake revolve).
//!
//! ## Per-method coverage table
//!
//! | Legacy trait method | Status | Mapping |
//! |---|---|---|
//! | `make_faces_from_profiles` | SUPPORTED (polygon profiles) | `ClosedProfile` polygon → `kernel_v2::Profile` (staged); circle / arc-segment / spline profiles → `NotSupported` (curved geometry not in kernel-v2 Phase 4a) |
//! | `extrude_face` | SUPPORTED | staged profile → `kernel_v2::construct::extrude` |
//! | `revolve_face` | NOT SUPPORTED | revolve is a later kernel-v2 slice |
//! | `boolean_union` / `_subtract` / `_intersect` | SUPPORTED (non-coplanar) | `kernel_v2::boolean_op` (yang-rs native pipeline); coplanar input face pairs → `NotSupported` (Yang Stage 0 / roadmap M8) |
//! | `boolean_*_multi` | default impl | delegates to the single-body methods |
//! | `fillet_edges` / `chamfer_edges` / `shell` | NOT SUPPORTED | deferred indefinitely (root CLAUDE.md) |
//! | `tessellate` | SUPPORTED | `kernel_v2::tessellate` (exact-rational, planar) → legacy `RenderMesh`; the tolerance argument is ignored (planar tessellation is exact) |
//! | `extract_edges` | SUPPORTED | arena half-edge walk → legacy `EdgeRenderData` |
//! | `export_step` | NOT SUPPORTED | trait default |
//! | `list_faces` / `list_edges` / `list_vertices` | SUPPORTED | arena walk, tagged `KernelId` encoding |
//! | `face_edges` / `edge_faces` / `edge_vertices` / `face_neighbors` | SUPPORTED | arena adjacency |
//! | `compute_signature` / `compute_all_signatures` | SUPPORTED | planar face area/centroid/normal/bbox, edge length, vertex point |
//!
//! ## Handle / id scheme
//!
//! - `KernelSolidHandle` raw ids index `solids: HashMap<u64, SolidId>`.
//! - `KernelId` is tag-encoded (`tag << 40 | index`): 1 = vertex
//!   (`VertexId`), 2 = edge (canonical = lower-id half-edge of the twin
//!   pair), 3 = face (`FaceId`), 4 = staged profile (from
//!   `make_faces_from_profiles`, consumed by `extrude_face`, mirroring the
//!   legacy standalone-face lifecycle).

use std::collections::HashMap;

use kernel::types::{
    ClosedProfile, EdgeRenderData, KernelError, KernelId, KernelSolidHandle, RenderMesh, TopoKind,
    TopoSignature,
};
use kernel::{Kernel, KernelIntrospect};

/// Legacy-trait adapter over a kernel-v2 `BrepArena`. See module docs.
#[derive(Default)]
#[allow(dead_code)] // stub slice (PR-KV4 RED): fields are wired in the GREEN slice
pub struct KernelV2Adapter {
    arena: kernel_v2::BrepArena,
    /// Staged profiles from `make_faces_from_profiles`, consumed by
    /// `extrude_face` (legacy standalone-face lifecycle).
    staged: HashMap<u64, kernel_v2::Profile>,
    /// Live solids by legacy handle raw id.
    solids: HashMap<u64, kernel_v2::SolidId>,
    next_staged: u64,
    next_handle: u64,
}

impl KernelV2Adapter {
    /// Fresh adapter with an empty arena.
    pub fn new() -> Self {
        Self::default()
    }

    fn not_supported(operation: &str) -> KernelError {
        KernelError::NotSupported {
            operation: operation.to_string(),
        }
    }
}

#[allow(unused_variables)]
impl Kernel for KernelV2Adapter {
    fn extrude_face(
        &mut self,
        face: KernelId,
        direction: [f64; 3],
        depth: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported("extrude_face (kernel-v2 adapter stub)"))
    }

    fn revolve_face(
        &mut self,
        face: KernelId,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported(
            "revolve_face (kernel-v2: revolve not yet implemented)",
        ))
    }

    fn boolean_union(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported(
            "boolean_union (kernel-v2 adapter stub)",
        ))
    }

    fn boolean_subtract(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported(
            "boolean_subtract (kernel-v2 adapter stub)",
        ))
    }

    fn boolean_intersect(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported(
            "boolean_intersect (kernel-v2 adapter stub)",
        ))
    }

    fn fillet_edges(
        &mut self,
        solid: &KernelSolidHandle,
        edges: &[KernelId],
        radius: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported(
            "fillet_edges (deferred indefinitely; not in kernel-v2)",
        ))
    }

    fn chamfer_edges(
        &mut self,
        solid: &KernelSolidHandle,
        edges: &[KernelId],
        distance: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported(
            "chamfer_edges (deferred indefinitely; not in kernel-v2)",
        ))
    }

    fn shell(
        &mut self,
        solid: &KernelSolidHandle,
        faces_to_remove: &[KernelId],
        thickness: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported(
            "shell (deferred indefinitely; not in kernel-v2)",
        ))
    }

    fn tessellate(
        &mut self,
        solid: &KernelSolidHandle,
        tolerance: f64,
    ) -> Result<RenderMesh, KernelError> {
        Err(Self::not_supported("tessellate (kernel-v2 adapter stub)"))
    }

    fn extract_edges(
        &mut self,
        solid: &KernelSolidHandle,
        tolerance: f64,
    ) -> Result<EdgeRenderData, KernelError> {
        Err(Self::not_supported(
            "extract_edges (kernel-v2 adapter stub)",
        ))
    }

    fn make_faces_from_profiles(
        &mut self,
        profiles: &[ClosedProfile],
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
        positions: &HashMap<u32, (f64, f64)>,
    ) -> Result<Vec<KernelId>, KernelError> {
        Err(Self::not_supported(
            "make_faces_from_profiles (kernel-v2 adapter stub)",
        ))
    }
}

#[allow(unused_variables)]
impl KernelIntrospect for KernelV2Adapter {
    fn list_faces(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        Vec::new()
    }

    fn list_edges(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        Vec::new()
    }

    fn list_vertices(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        Vec::new()
    }

    fn face_edges(&self, face: KernelId) -> Vec<KernelId> {
        Vec::new()
    }

    fn edge_faces(&self, edge: KernelId) -> Vec<KernelId> {
        Vec::new()
    }

    fn edge_vertices(&self, edge: KernelId) -> (KernelId, KernelId) {
        (KernelId(0), KernelId(0))
    }

    fn face_neighbors(&self, face: KernelId) -> Vec<KernelId> {
        Vec::new()
    }

    fn compute_signature(&self, entity: KernelId, kind: TopoKind) -> TopoSignature {
        TopoSignature::empty()
    }

    fn compute_all_signatures(
        &self,
        solid: &KernelSolidHandle,
        kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)> {
        Vec::new()
    }
}
