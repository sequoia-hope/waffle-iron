//! `KernelV2Adapter` — legacy `waffle_types::kernel::Kernel` + `waffle_types::kernel::KernelIntrospect`
//! over a kernel-v2 arena (PR-KV4, Phase 4a EXIT).
//!
//! ## Purpose
//!
//! The app's dispatch path (feature-engine + wasm-bridge) drives the
//! `Kernel`/`KernelIntrospect` traits from `waffle_types::kernel`. kernel-v2
//! has its own, cleaner internal API. This adapter is the bridge — since the
//! Phase 6 migration (2026-06-11) it IS the production kernel behind the
//! traits (moved here from test-harness, where it was built at PR-KV4). It
//! maps trait calls onto kernel-v2 operations, and returns
//! `KernelError::NotSupported` LOUDLY for anything kernel-v2 does not yet
//! implement. The adapter MAPS; it never repairs or stubs a result (no fake
//! revolve). The two deliberate, documented exceptions are sampled-polygon
//! gear extrudes (KV12 — arc/spline profiles carry their own chord samples)
//! and polygonizing a circle RIM only when it carries holes (KV14 — a holed
//! circle needs a polygon outer); plain circles stay exact.
//!
//! ## Per-method coverage table
//!
//! | Legacy trait method | Status | Mapping |
//! |---|---|---|
//! | `make_faces_from_profiles` | SUPPORTED (polygon + circle + exact arc + spline-via-polygon + holed regions) | `ClosedProfile` polygon → `kernel_v2::Profile::new`; `CircleProfile` → `Profile::circle` (staged); arc-annotated single loops → `Profile::arc_polygon` with EXACT cylinder side patches (KV12 Tier 2, E4 — arc runs reconstructed into minor sub-arcs), falling back LOUDLY to the Tier-1 `vertex_ids` chord polygon when reconstruction / simplicity declines or the loop is holed; spline-annotated profiles (gears) extrude via their chord polygon; inner (`is_outer=false`) loops are grouped into the strictly-larger outer that contains them → one holed `Profile` (KV14); arc/spline WITHOUT a `vertex_ids` polygon → `NotSupported` |
//! | `extrude_face` | SUPPORTED | staged profile → `kernel_v2::extrude` (sweep vector = `direction · depth`, exactly the legacy semantics); circle profiles → cylinder solids (PR-KV5a) |
//! | `revolve_face` | SUPPORTED (PR-KV6a incl. KV6a-tilted non-alternating profiles; cones PR-KV6c incl. increment-5 partial patches) | staged polygon profile → `kernel_v2::revolve` (degrees → radians; world-space in-plane axis). Oblique edges sweep `Surface::Cone`: frustum bands on a FULL-turn revolve (KV6c) and arc-bounded cone patches on a PARTIAL turn (KV6c increment 5, spec `kv6c_partial_revolve_cone_patch.md`). Full-turn profiles need NOT alternate wall/annulus edges (KV6a-tilted, spec `kv6a_nonalternating_full_revolve.md` — an all-oblique tilted-axis rectangle builds the capless cone-frustum ring); only consecutive ANNULAR edges keep a typed `NotImplemented`. A PARTIAL-turn circle profile sweeps a `Surface::Torus` (a bent solid tube, KV6d); a FULL-turn circle profile builds the CLOSED genus-1 torus (off-axis) or the CLOSED sphere (on-axis, KV6d increment 2, spec `kv6d_sphere_revolve.md`). Typed walls: holed profiles → `NotSupported`; axis touching/crossing the profile and out-of-range angles → `KernelError::Other` (INVALID INPUT — the F0073/F0074 expected-rebuild-error path, never the NotSupported marker) |
//! | `boolean_union` / `_subtract` / `_intersect` | SUPPORTED (non-coplanar; cylinder×box class PR-KV5b) | `kernel_v2::boolean_op` (yang-rs native pipeline); coplanar input face pairs → `NotSupported` (Yang Stage 0 / roadmap M8); curved partial-patch RESULT operands → `NotSupported` (no yang Stage-1 re-entry); cone-frustum (two-rim) operands SUPPORTED when the lateral survives a flat ⊥-axis cut whole (KV6c increment 5c); an oblique cut makes a conic-bounded cone patch → `BooleanFailed`; cylinder×cylinder / oblique-ellipse sections → `BooleanFailed` carrying the typed wall text |
//! | `boolean_*_multi` | default impl | delegates to the single-body methods |
//! | `fillet_edges` / `chamfer_edges` / `shell` | NOT SUPPORTED | deferred indefinitely (root CLAUDE.md) |
//! | `tessellate` | SUPPORTED | `kernel_v2::validate_solid` + `kernel_v2::tessellate` (exact-rational, planar) → legacy `RenderMesh`; the tolerance argument is ignored (planar tessellation is exact) |
//! | `extract_edges` | SUPPORTED | arena half-edge walk (canonical = lower-id twin) → legacy `EdgeRenderData` |
//! | `export_step` | NOT SUPPORTED | trait default |
//! | `list_faces` / `list_edges` / `list_vertices` | SUPPORTED | arena walk, tagged `KernelId` encoding |
//! | `face_edges` / `edge_faces` / `edge_vertices` / `face_neighbors` | SUPPORTED | arena adjacency |
//! | `compute_signature` / `compute_all_signatures` | SUPPORTED | planar face area/centroid/normal/bbox, edge length/centroid/bbox, vertex point |
//!
//! ## Handle / id scheme
//!
//! - `KernelSolidHandle` raw ids index `solids: HashMap<u64, SolidId>`
//!   (constructed via the `KernelSolidHandle::from_raw` seam added for
//!   external trait implementations).
//! - `KernelId` is tag-encoded (`tag << 40 | index`): 1 = vertex
//!   (`VertexId`), 2 = edge (canonical = lower-id half-edge of the twin
//!   pair), 3 = face (`FaceId`), 4 = staged profile (from
//!   `make_faces_from_profiles`, consumed by `extrude_face`, mirroring the
//!   legacy standalone-face lifecycle).
//!
//! ## Error mapping
//!
//! - `KernelV2Error::UnsupportedCoplanar` → `NotSupported` ("coplanar")
//!   — the declared Yang Stage 0 / M8 boundary, not a bug.
//! - `KernelV2Error::EmptyBooleanResult` / `BooleanFailed` / reassembly
//!   errors → `KernelError::BooleanFailed` (loud, full text).
//! - Profile/extrude validation errors → `KernelError::Other` (loud).
//! - Nothing is masked, retried, or repaired (P9/P10).

use std::collections::{BTreeSet, HashMap};

use crate::arena::Curve;
use crate::{BrepArena, FaceId, HalfEdgeId, KernelV2Error, SolidId, Surface, VertexId};
use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use waffle_types::kernel::{
    ClosedProfile, EdgeRange, EdgeRenderData, FaceRange, KernelError, KernelId, KernelSolidHandle,
    RenderMesh, TopoKind, TopoSignature,
};
use waffle_types::kernel::{Kernel, KernelIntrospect};

mod profile_convert;
use profile_convert::*;

// ── KernelId tag encoding ──────────────────────────────────────────────────

const TAG_SHIFT: u64 = 40;
const TAG_MASK: u64 = !((1u64 << TAG_SHIFT) - 1);
const IDX_MASK: u64 = (1u64 << TAG_SHIFT) - 1;
const TAG_VERTEX: u64 = 1 << TAG_SHIFT;
const TAG_EDGE: u64 = 2 << TAG_SHIFT;
const TAG_FACE: u64 = 3 << TAG_SHIFT;
const TAG_PROFILE: u64 = 4 << TAG_SHIFT;

fn encode_vertex(v: VertexId) -> KernelId {
    KernelId(TAG_VERTEX | v.0 as u64)
}

fn encode_edge(canonical: HalfEdgeId) -> KernelId {
    KernelId(TAG_EDGE | canonical.0 as u64)
}

fn encode_face(f: FaceId) -> KernelId {
    KernelId(TAG_FACE | f.0 as u64)
}

fn decode(id: KernelId) -> (u64, u32) {
    (id.0 & TAG_MASK, (id.0 & IDX_MASK) as u32)
}

// Imported (STEP) mesh-backed bodies live beside the arena (module docs in
// `crate::imported`). Their entities get their own tag namespace so they can
// never collide with arena ids; the 32-bit index packs the body slot (upper
// 12 bits) with the entity index (lower 20 bits).
const TAG_IMPORTED_FACE: u64 = 5 << TAG_SHIFT;
const TAG_IMPORTED_EDGE: u64 = 6 << TAG_SHIFT;
const TAG_IMPORTED_VERTEX: u64 = 7 << TAG_SHIFT;
const IMPORTED_SLOT_SHIFT: u32 = 20;
/// Max entities per imported body (2^20) and max imported bodies per
/// session (2^12) — enforced loudly at `import_body`.
const IMPORTED_MAX_ENTITIES: usize = 1 << IMPORTED_SLOT_SHIFT;
const IMPORTED_MAX_BODIES: usize = 1 << (32 - IMPORTED_SLOT_SHIFT);

fn encode_imported(tag: u64, slot: usize, idx: usize) -> KernelId {
    KernelId(tag | ((slot as u64) << IMPORTED_SLOT_SHIFT) | idx as u64)
}

fn decode_imported(idx: u32) -> (usize, usize) {
    (
        (idx >> IMPORTED_SLOT_SHIFT) as usize,
        (idx & ((1 << IMPORTED_SLOT_SHIFT) - 1)) as usize,
    )
}

// ── Adapter ────────────────────────────────────────────────────────────────

/// Legacy-trait adapter over a kernel-v2 `BrepArena`. See module docs.
#[derive(Default)]
pub struct KernelV2Adapter {
    arena: BrepArena,
    /// Staged profiles from `make_faces_from_profiles`, consumed by
    /// `extrude_face` (legacy standalone-face lifecycle).
    staged: HashMap<u64, crate::Profile>,
    /// Live solids by legacy handle raw id.
    solids: HashMap<u64, SolidId>,
    /// Imported (STEP) mesh-backed bodies, appended-only; a handle maps to
    /// its slot here via `imported_handles`.
    imported: Vec<crate::imported::ImportedBody>,
    imported_handles: HashMap<u64, usize>,
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

    fn alloc_handle(&mut self, solid: SolidId) -> KernelSolidHandle {
        let raw = self.next_handle;
        self.next_handle += 1;
        self.solids.insert(raw, solid);
        KernelSolidHandle::from_raw(raw)
    }

    fn imported_slot_of(&self, handle: &KernelSolidHandle) -> Option<usize> {
        self.imported_handles.get(&handle.raw()).copied()
    }

    /// Concatenate an imported body's per-face meshes into one `RenderMesh`
    /// with per-face pick ranges.
    fn imported_render_mesh(&self, slot: usize) -> RenderMesh {
        let body = &self.imported[slot];
        let mut mesh = RenderMesh {
            vertices: Vec::new(),
            normals: Vec::new(),
            indices: Vec::new(),
            face_ranges: Vec::new(),
        };
        for (fi, face) in body.faces.iter().enumerate() {
            let base = (mesh.vertices.len() / 3) as u32;
            let start = mesh.indices.len() as u32;
            mesh.vertices
                .extend(face.positions.iter().map(|&c| c as f32));
            mesh.normals.extend(face.normals.iter().map(|&c| c as f32));
            mesh.indices.extend(face.indices.iter().map(|&i| base + i));
            mesh.face_ranges.push(FaceRange {
                face_id: encode_imported(TAG_IMPORTED_FACE, slot, fi),
                start_index: start,
                end_index: mesh.indices.len() as u32,
            });
        }
        mesh
    }

    /// Imported edges as pair-expanded polyline segments (the render layer
    /// draws edge ranges as line segments two vertices at a time).
    fn imported_edge_render(&self, slot: usize) -> EdgeRenderData {
        let body = &self.imported[slot];
        let mut vertices: Vec<f32> = Vec::new();
        let mut edge_ranges: Vec<EdgeRange> = Vec::new();
        for (ei, edge) in body.edges.iter().enumerate() {
            let start_vertex = (vertices.len() / 3) as u32;
            for w in edge.polyline.windows(2) {
                for p in w {
                    vertices.extend_from_slice(&[p[0] as f32, p[1] as f32, p[2] as f32]);
                }
            }
            let end_vertex = (vertices.len() / 3) as u32;
            if end_vertex > start_vertex {
                edge_ranges.push(EdgeRange {
                    edge_id: encode_imported(TAG_IMPORTED_EDGE, slot, ei),
                    start_vertex,
                    end_vertex,
                    curve: None,
                });
            }
        }
        EdgeRenderData {
            vertices,
            edge_ranges,
        }
    }

    fn solid_of(&self, handle: &KernelSolidHandle) -> Result<SolidId, KernelError> {
        self.solids
            .get(&handle.raw())
            .copied()
            .ok_or_else(|| KernelError::Other {
                message: format!("unknown solid handle {}", handle.raw()),
            })
    }

    /// All faces of a solid, in shell walk order.
    fn solid_faces(&self, solid: SolidId) -> Vec<FaceId> {
        let mut out = Vec::new();
        let Ok(solid_ref) = self.arena.solid(solid) else {
            return out;
        };
        for &sh in &solid_ref.shells {
            if let Ok(shell) = self.arena.shell(sh) {
                out.extend(shell.faces.iter().copied());
            }
        }
        out
    }

    /// All half-edges of a face (outer loop + rings), in walk order.
    fn face_half_edges(&self, face: FaceId) -> Vec<HalfEdgeId> {
        let mut out = Vec::new();
        let Ok(f) = self.arena.face(face) else {
            return out;
        };
        let mut loops = vec![f.outer_loop];
        loops.extend(f.inner_loops.iter().copied());
        for lid in loops {
            if let Ok(hes) = self.arena.loop_half_edges(lid) {
                out.extend(hes);
            }
        }
        out
    }

    /// Canonical (lower-id of the twin pair) undirected edges of a solid,
    /// in deterministic id order.
    fn solid_canonical_edges(&self, solid: SolidId) -> Vec<HalfEdgeId> {
        let mut set = BTreeSet::new();
        for face in self.solid_faces(solid) {
            for h in self.face_half_edges(face) {
                if let Ok(he) = self.arena.half_edge(h) {
                    set.insert(h.min(he.twin));
                }
            }
        }
        set.into_iter().collect()
    }

    /// Unique vertices of a solid, in deterministic id order.
    fn solid_vertices(&self, solid: SolidId) -> Vec<VertexId> {
        let mut set = BTreeSet::new();
        for face in self.solid_faces(solid) {
            for h in self.face_half_edges(face) {
                if let Ok(he) = self.arena.half_edge(h) {
                    set.insert(he.origin);
                }
            }
        }
        set.into_iter().collect()
    }

    fn edge_endpoints(&self, canonical: HalfEdgeId) -> Option<(Point3, Point3)> {
        let he = self.arena.half_edge(canonical).ok()?;
        let start = self.arena.vertex(he.origin).ok()?.point;
        let end = self
            .arena
            .vertex(self.arena.half_edge(he.next).ok()?.origin)
            .ok()?
            .point;
        Some((start, end))
    }

    fn vertex_signature(&self, v: VertexId) -> TopoSignature {
        let Ok(vertex) = self.arena.vertex(v) else {
            return TopoSignature::empty();
        };
        let p = vertex.point.as_array();
        TopoSignature {
            centroid: Some(p),
            bbox: Some([p[0], p[1], p[2], p[0], p[1], p[2]]),
            ..TopoSignature::empty()
        }
    }

    fn edge_signature(&self, canonical: HalfEdgeId) -> TopoSignature {
        let Some((a, b)) = self.edge_endpoints(canonical) else {
            return TopoSignature::empty();
        };
        let (a, b) = (a.as_array(), b.as_array());
        let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
        TopoSignature {
            length: Some(length),
            centroid: Some([
                (a[0] + b[0]) / 2.0,
                (a[1] + b[1]) / 2.0,
                (a[2] + b[2]) / 2.0,
            ]),
            bbox: Some([
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].min(b[2]),
                a[0].max(b[0]),
                a[1].max(b[1]),
                a[2].max(b[2]),
            ]),
            ..TopoSignature::empty()
        }
    }

    fn face_signature(&self, fid: FaceId) -> TopoSignature {
        let Ok(face) = self.arena.face(fid) else {
            return TopoSignature::empty();
        };
        let Some(Surface::Plane(plane)) = face.surface else {
            return TopoSignature::empty();
        };
        let n = [plane.normal.x, plane.normal.y, plane.normal.z];

        // Area: Σ Newell(loop) · n̂ / 2 over outer + rings (rings wind
        // opposite, so holes subtract automatically).
        let mut loops = vec![face.outer_loop];
        loops.extend(face.inner_loops.iter().copied());
        // Exact signed area incl. arc-segment corrections (PR-KV6a — the
        // chord Newell under-counts and SIGN-FLIPS >180° annular sectors).
        let twice_area =
            crate::geom::planar_face_signed_area2(&self.arena, fid, face, n).unwrap_or(0.0);
        let mut bbox = [
            f64::INFINITY,
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        let mut centroid = [0.0f64; 3];
        let mut outer_count = 0usize;
        for (li, lid) in loops.iter().enumerate() {
            let Ok(pts) = self.arena.loop_points(*lid) else {
                continue;
            };
            for p in &pts {
                let p = p.as_array();
                for k in 0..3 {
                    bbox[k] = bbox[k].min(p[k]);
                    bbox[k + 3] = bbox[k + 3].max(p[k]);
                }
                if li == 0 {
                    for k in 0..3 {
                        centroid[k] += p[k];
                    }
                    outer_count += 1;
                }
            }
        }
        if outer_count > 0 {
            for c in centroid.iter_mut() {
                *c /= outer_count as f64;
            }
        }
        TopoSignature {
            surface_type: Some("planar".to_string()),
            area: Some(twice_area / 2.0),
            centroid: Some(centroid),
            normal: Some(n),
            bbox: if outer_count > 0 { Some(bbox) } else { None },
            ..TopoSignature::empty()
        }
    }

    /// KV15b diagnosis probe (read-only, env-gated `KV2_SUBFLOOR_TWIN_PROBE`):
    /// census DISTINCT vertex pairs of `solid` closer than MIN_FEATURE_SIZE,
    /// flagging sub-TAU_MODEL pairs and direct edge connections — localizes
    /// which op MINTS a sub-floor twin pair into a chained B-Rep (the
    /// R0076/R0007/R0071/R0053 class). Never set in production/WASM.
    fn subfloor_twin_probe(&self, solid: SolidId, label: &str) {
        if std::env::var_os("KV2_SUBFLOOR_TWIN_PROBE").is_none() {
            return;
        }
        let verts = self.solid_vertices(solid);
        let pts: Vec<(VertexId, Point3)> = verts
            .iter()
            .filter_map(|&v| self.arena.vertex(v).ok().map(|vr| (v, vr.point)))
            .collect();
        let floor2 = cad_primitives::MIN_FEATURE_SIZE * cad_primitives::MIN_FEATURE_SIZE;
        let tau2 = cad_primitives::TAU_MODEL * cad_primitives::TAU_MODEL;
        for i in 0..pts.len() {
            for j in (i + 1)..pts.len() {
                let (p, q) = (pts[i].1.as_array(), pts[j].1.as_array());
                let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
                if d2 == 0.0 || d2 >= floor2 {
                    continue;
                }
                let connected = self.solid_canonical_edges(solid).iter().any(|&h| {
                    self.edge_endpoints(h).is_some_and(|(s, e)| {
                        (s == pts[i].1 && e == pts[j].1) || (s == pts[j].1 && e == pts[i].1)
                    })
                });
                eprintln!(
                    "[subfloor-twin-probe] {label}: verts {:?}/{:?} dist={:e} sub_tau={} edge={}\n  ({},{},{})\n  ({},{},{})",
                    pts[i].0,
                    pts[j].0,
                    d2.sqrt(),
                    d2 < tau2,
                    connected,
                    p[0],
                    p[1],
                    p[2],
                    q[0],
                    q[1],
                    q[2]
                );
            }
        }
    }

    fn run_boolean(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
        op: BoolOp,
        op_name: &str,
    ) -> Result<KernelSolidHandle, KernelError> {
        let solid = self.run_boolean_solid(a, b, op, op_name)?;
        Ok(self.alloc_handle(solid))
    }

    /// Like `run_boolean`, but splits a disjoint result into one handle per
    /// body (lump). A single body returns a one-element vec.
    fn run_boolean_multi(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
        op: BoolOp,
        op_name: &str,
    ) -> Result<Vec<KernelSolidHandle>, KernelError> {
        let solid = self.run_boolean_solid(a, b, op, op_name)?;
        let bodies = crate::split_solid_into_bodies(&mut self.arena, solid).map_err(|e| {
            KernelError::BooleanFailed {
                reason: format!("kernel-v2 {op_name} body split failed: {e}"),
            }
        })?;
        Ok(bodies
            .into_iter()
            .map(|sid| self.alloc_handle(sid))
            .collect())
    }

    fn run_boolean_solid(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
        op: BoolOp,
        op_name: &str,
    ) -> Result<SolidId, KernelError> {
        // STEP-import SI1 wall: imported bodies are mesh-backed and cannot
        // enter the exact boolean pipeline. The mesh-path boolean is
        // roadmapped (docs/step_import_roadmap.md SI2).
        if self.imported_slot_of(a).is_some() || self.imported_slot_of(b).is_some() {
            return Err(Self::not_supported(&format!(
                "{op_name}: imported (STEP) body operand — booleans with imported bodies are \
                 STEP-import roadmap SI2 (mesh-path boolean, docs/step_import_roadmap.md)"
            )));
        }
        let sa = self.solid_of(a)?;
        let sb = self.solid_of(b)?;
        self.subfloor_twin_probe(sa, &format!("{op_name} operand A"));
        self.subfloor_twin_probe(sb, &format!("{op_name} operand B"));
        match crate::boolean_op(&mut self.arena, sa, sb, op) {
            Ok(result) => {
                self.subfloor_twin_probe(result, &format!("{op_name} OUTPUT"));
                Ok(result)
            }
            Err(KernelV2Error::UnsupportedCoplanar) => Err(Self::not_supported(&format!(
                "{op_name}: coplanar input face pair (Yang Stage 0 coplanar preprocessing — roadmap M8 — not yet implemented)"
            ))),
            // PR-KV5b: a curved RESULT solid (partial cylinder patches from
            // a previous boolean) cannot re-enter yang-rs Stage 1 — a
            // declared boundary, not a bug (see kernel-v2 boolean.rs docs).
            Err(KernelV2Error::UnsupportedCurvedBoolean { face, reason }) => {
                // KV14 re-entry census (2026-09-04): `KV14_REENTRY_CENSUS=1`
                // prints the refusing face's structure — surface, outer loop
                // curve pattern, inner loops — so the remaining walls can be
                // designed against what the B-Rep actually carries.
                if std::env::var_os("KV14_REENTRY_CENSUS").is_some() {
                    eprintln!(
                        "[kv14-reentry] case={} op={op_name} face={face:?} reason={reason:?}\n{}",
                        std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into()),
                        reentry_census(&self.arena, face)
                    );
                }
                Err(Self::not_supported(&format!(
                    "{op_name}: curved partial-patch operand face {face:?} [{reason}] (a previous \
                     curved boolean's result cannot re-enter yang-rs Stage 1 — no partial-patch \
                     tessellation yet)"
                )))
            }
            // Spec `cut_consumes_body`: an empty result is a CORRECT boolean
            // conclusion (the tool consumed all material), surfaced typed so
            // the engine can apply body-lifetime policy instead of recording
            // an operation error. kernel-v2 itself still has no empty solid.
            Err(KernelV2Error::EmptyBooleanResult) => Err(KernelError::BooleanEmptyResult),
            Err(e) => Err(KernelError::BooleanFailed {
                reason: format!("kernel-v2 {op_name} failed: {e}"),
            }),
        }
    }
}

impl Kernel for KernelV2Adapter {
    fn extrude_face(
        &mut self,
        face: KernelId,
        direction: [f64; 3],
        depth: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        let (tag, idx) = decode(face);
        if tag != TAG_PROFILE {
            return Err(KernelError::EntityNotFound { id: face });
        }
        if !(depth.is_finite() && depth > 0.0) {
            return Err(KernelError::Other {
                message: "extrude depth must be positive".to_string(),
            });
        }
        // Legacy semantics: sweep vector = direction · depth (the legacy
        // kernel does NOT normalize the direction).
        let w = [
            direction[0] * depth,
            direction[1] * depth,
            direction[2] * depth,
        ];
        let dist = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
        if !(dist.is_finite() && dist > 0.0) {
            return Err(KernelError::Other {
                message: "extrude direction is zero or non-finite".to_string(),
            });
        }
        let profile = self
            .staged
            .remove(&(idx as u64))
            .ok_or(KernelError::EntityNotFound { id: face })?;
        let result = crate::extrude(
            &mut self.arena,
            &profile,
            Vector3::new(w[0], w[1], w[2]),
            dist,
        )
        .map_err(|e| KernelError::Other {
            message: format!("kernel-v2 extrude failed: {e}"),
        })?;
        self.subfloor_twin_probe(result.solid, "extrude OUTPUT");
        Ok(self.alloc_handle(result.solid))
    }

    fn revolve_face(
        &mut self,
        face: KernelId,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        let (tag, idx) = decode(face);
        if tag != TAG_PROFILE {
            return Err(KernelError::EntityNotFound { id: face });
        }
        let profile = self
            .staged
            .remove(&(idx as u64))
            .ok_or(KernelError::EntityNotFound { id: face })?;
        // Legacy trait convention: angle in DEGREES (modeling-ops passes
        // RevolveParams.angle through unchanged).
        let result = crate::revolve(
            &mut self.arena,
            &profile,
            Point3::new(axis_origin[0], axis_origin[1], axis_origin[2]),
            Vector3::new(axis_direction[0], axis_direction[1], axis_direction[2]),
            angle.to_radians(),
        )
        .map_err(|e| match e {
            // Capability walls: typed NotSupported (assay UNSUPPORTED).
            KernelV2Error::RevolveCircleProfileUnsupported => Self::not_supported(
                "revolve_face: full-turn circle profile sweeps a CLOSED torus (kernel-v2 roadmap KV6d; PARTIAL-turn circle revolve → torus is supported)",
            ),
            KernelV2Error::RevolveProfileHolesUnsupported => Self::not_supported(
                "revolve_face: holed profile revolve not implemented (kernel-v2)",
            ),
            // Invalid input: plain errors — the message must NOT carry the
            // NotSupported marker (F0073/F0074 pin expect_rebuild_error).
            other => KernelError::Other {
                message: format!("kernel-v2 revolve failed: {other}"),
            },
        })?;
        self.subfloor_twin_probe(result.solid, "revolve OUTPUT");
        Ok(self.alloc_handle(result.solid))
    }

    fn boolean_union(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        self.run_boolean(a, b, BoolOp::Union, "boolean_union")
    }

    fn boolean_subtract(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        self.run_boolean(a, b, BoolOp::Subtract, "boolean_subtract")
    }

    fn boolean_intersect(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        self.run_boolean(a, b, BoolOp::Intersect, "boolean_intersect")
    }

    // Multi-body variants: a boolean can yield spatially-disjoint lumps (e.g.
    // a union of far-apart bodies). Split them so each renders/selects as its
    // own body instead of collapsing into one.
    fn boolean_union_multi(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<Vec<KernelSolidHandle>, KernelError> {
        self.run_boolean_multi(a, b, BoolOp::Union, "boolean_union")
    }

    fn boolean_subtract_multi(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<Vec<KernelSolidHandle>, KernelError> {
        self.run_boolean_multi(a, b, BoolOp::Subtract, "boolean_subtract")
    }

    fn boolean_intersect_multi(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<Vec<KernelSolidHandle>, KernelError> {
        self.run_boolean_multi(a, b, BoolOp::Intersect, "boolean_intersect")
    }

    fn fillet_edges(
        &mut self,
        _solid: &KernelSolidHandle,
        _edges: &[KernelId],
        _radius: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported(
            "fillet_edges (deferred indefinitely; not in kernel-v2)",
        ))
    }

    fn chamfer_edges(
        &mut self,
        _solid: &KernelSolidHandle,
        _edges: &[KernelId],
        _distance: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported(
            "chamfer_edges (deferred indefinitely; not in kernel-v2)",
        ))
    }

    fn shell(
        &mut self,
        _solid: &KernelSolidHandle,
        _faces_to_remove: &[KernelId],
        _thickness: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(Self::not_supported(
            "shell (deferred indefinitely; not in kernel-v2)",
        ))
    }

    fn import_body(
        &mut self,
        data: &waffle_types::kernel::ImportedBodyData,
    ) -> Result<KernelSolidHandle, KernelError> {
        if data.is_empty() {
            return Err(KernelError::Other {
                message: "import_body: no faces to import".to_string(),
            });
        }
        if self.imported.len() >= IMPORTED_MAX_BODIES {
            return Err(KernelError::Other {
                message: format!(
                    "import_body: imported-body slots exhausted ({IMPORTED_MAX_BODIES} per session)"
                ),
            });
        }
        let body = crate::imported::ImportedBody::from_data(data);
        let entities = body
            .faces
            .len()
            .max(body.edges.len())
            .max(body.vertices.len());
        if entities >= IMPORTED_MAX_ENTITIES {
            return Err(KernelError::Other {
                message: format!(
                    "import_body: body has {entities} entities of one kind (max {IMPORTED_MAX_ENTITIES})"
                ),
            });
        }
        let slot = self.imported.len();
        self.imported.push(body);
        let raw = self.next_handle;
        self.next_handle += 1;
        self.imported_handles.insert(raw, slot);
        Ok(KernelSolidHandle::from_raw(raw))
    }

    fn tessellate(
        &mut self,
        solid: &KernelSolidHandle,
        _tolerance: f64, // planar tessellation is exact; tolerance is moot
    ) -> Result<RenderMesh, KernelError> {
        if let Some(slot) = self.imported_slot_of(solid) {
            return Ok(self.imported_render_mesh(slot));
        }
        let sid = self.solid_of(solid)?;
        // Defense in depth: a solid that fails kernel-v2 validation must
        // never be silently rendered.
        crate::validate_solid(&self.arena, sid).map_err(|e| KernelError::Other {
            message: format!("kernel-v2 validate_solid failed before tessellation: {e}"),
        })?;
        let mesh =
            crate::tessellate(&self.arena, sid).map_err(|e| KernelError::TessellationFailed {
                reason: format!("kernel-v2 tessellation failed: {e}"),
            })?;
        Ok(RenderMesh {
            vertices: mesh.positions.iter().map(|&c| c as f32).collect(),
            normals: mesh.normals.iter().map(|&c| c as f32).collect(),
            indices: mesh.indices,
            face_ranges: mesh
                .face_ranges
                .iter()
                .map(|r| FaceRange {
                    face_id: encode_face(r.face),
                    start_index: r.start,
                    end_index: r.start + r.count,
                })
                .collect(),
        })
    }

    fn extract_edges(
        &mut self,
        solid: &KernelSolidHandle,
        _tolerance: f64,
    ) -> Result<EdgeRenderData, KernelError> {
        if let Some(slot) = self.imported_slot_of(solid) {
            return Ok(self.imported_edge_render(slot));
        }
        let sid = self.solid_of(solid)?;
        // Curved edges export as their render polylines (the SAME chord-bound
        // samples `introspect::extract_edges` reports); before this they were
        // exported as bare endpoint chords, so a rounded outline projected as
        // straight lines (the step_extrude.waffle offset regression).
        // Circular edges additionally carry their analytic descriptor so
        // sketch projection can mint TRUE arcs.
        let n_seg =
            crate::tessellate::circle_segment_count(crate::tessellate::RENDER_CHORD_TOLERANCE_REL);
        let mut vertices: Vec<f32> = Vec::new();
        let mut edge_ranges: Vec<EdgeRange> = Vec::new();
        for canonical in self.solid_canonical_edges(sid) {
            let Ok(polyline) = crate::introspect::edge_polyline(&self.arena, canonical, n_seg)
            else {
                continue;
            };
            if polyline.len() < 2 {
                continue;
            }
            let Ok(he) = self.arena.half_edge(canonical) else {
                continue;
            };
            let curve = match he.curve {
                Curve::Circle {
                    center,
                    normal,
                    radius,
                } => Some(waffle_types::kernel::EdgeCurve::Circle {
                    center: center.as_array(),
                    normal: [normal.x, normal.y, normal.z],
                    radius,
                }),
                Curve::Arc {
                    center,
                    normal,
                    radius,
                } => Some(waffle_types::kernel::EdgeCurve::Arc {
                    center: center.as_array(),
                    normal: [normal.x, normal.y, normal.z],
                    radius,
                }),
                _ => None,
            };
            let start_vertex = (vertices.len() / 3) as u32;
            for p in polyline {
                let p = p.as_array();
                vertices.extend_from_slice(&[p[0] as f32, p[1] as f32, p[2] as f32]);
            }
            edge_ranges.push(EdgeRange {
                edge_id: encode_edge(canonical),
                start_vertex,
                end_vertex: (vertices.len() / 3) as u32,
                curve,
            });
        }
        Ok(EdgeRenderData {
            vertices,
            edge_ranges,
        })
    }

    fn make_faces_from_profiles(
        &mut self,
        profiles: &[ClosedProfile],
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
        positions: &HashMap<u32, (f64, f64)>,
    ) -> Result<Vec<KernelId>, KernelError> {
        // Legacy frame convention: y axis = normal × x axis.
        let n = plane_normal;
        let x = plane_x_axis;
        let y = [
            n[1] * x[2] - n[2] * x[1],
            n[2] * x[0] - n[0] * x[2],
            n[0] * x[1] - n[1] * x[0],
        ];

        // Build the plane frame once (shared by every staged profile).
        let origin = Point3::new(plane_origin[0], plane_origin[1], plane_origin[2]);
        let ux = Vector3::new(x[0], x[1], x[2]);
        let vy = Vector3::new(y[0], y[1], y[2]);

        // A profile's planar shape, classified before staging so KV14 can group
        // inner loops into the outer that contains them.
        enum Shape {
            Circle {
                center: Point2,
                radius: f64,
            },
            Polygon {
                pts: Vec<Point2>,
                is_outer: bool,
            },
            /// KV12 Tier 2: an exact line/arc loop reconstructed from
            /// `arc_segments`. `chord_pts` is the Tier-1 chord polygon, kept
            /// for hole grouping and as the loud fallback if `arc_polygon`
            /// validation rejects the loop.
            ArcPolygon {
                edges: Vec<crate::ProfileEdge>,
                chord_pts: Vec<Point2>,
                is_outer: bool,
            },
        }

        // ── pass 1: classify each profile (capability walls fire here) ──────
        let mut shapes: Vec<Shape> = Vec::with_capacity(profiles.len());
        for profile in profiles {
            // PR-KV5b: circle profiles (legacy semantics: center in (u, v),
            // radius in meters).
            if let Some(circle) = &profile.circle {
                shapes.push(Shape::Circle {
                    center: Point2::new(circle.center_u, circle.center_v),
                    radius: circle.radius,
                });
                continue;
            }
            // KV12 — an arc-segment profile carries a fully sampled
            // `vertex_ids` polygon (the same chord points the solver/viewport
            // use). Without a polygon there is nothing to extrude — stay
            // walled.
            if !profile.arc_segments.is_empty() && profile.vertex_ids.is_empty() {
                return Err(Self::not_supported(
                    "make_faces_from_profiles: arc-segment profile without an authored \
                     vertex_ids polygon",
                ));
            }
            // PR-KV8: spline-annotated profiles (gears) extrude via their
            // authored `vertex_ids` polygon; without one, stay walled.
            if !profile.spline_segments.is_empty() && profile.vertex_ids.is_empty() {
                return Err(Self::not_supported(
                    "make_faces_from_profiles: spline-segment profile without an authored \
                     vertex_ids polygon",
                ));
            }

            // Vertex key selection: prefer vertex_ids, fall back to entity_ids,
            // then sorted position keys.
            let keys: Vec<u32> = if !profile.vertex_ids.is_empty()
                && profile
                    .vertex_ids
                    .iter()
                    .all(|id| positions.contains_key(id))
            {
                profile.vertex_ids.clone()
            } else if !profile.entity_ids.is_empty()
                && profile
                    .entity_ids
                    .iter()
                    .all(|id| positions.contains_key(id))
            {
                profile.entity_ids.clone()
            } else {
                let mut k: Vec<u32> = positions.keys().copied().collect();
                k.sort();
                k
            };

            if keys.len() < 3 {
                return Err(KernelError::Other {
                    message: format!("Need at least 3 vertices for a polygon, got {}", keys.len()),
                });
            }

            let pts2: Vec<Point2> = keys
                .iter()
                .map(|k| {
                    let (pu, pv) = positions[k];
                    Point2::new(pu, pv)
                })
                .collect();

            // KV12 Tier 2: if this profile carries arc segments over an
            // authored `vertex_ids` polygon, reconstruct an exact line/arc
            // loop (arc edges → cylinder side patches). The chord polygon is
            // retained as the loud fallback if reconstruction or simplicity
            // validation declines the exact form. The arc-segment indices are
            // into `vertex_ids`, which is exactly `keys` here.
            let used_vertex_ids = !profile.vertex_ids.is_empty()
                && keys.len() == profile.vertex_ids.len()
                && keys.iter().zip(&profile.vertex_ids).all(|(a, b)| a == b);
            if !profile.arc_segments.is_empty() && used_vertex_ids {
                if let Some(edges) = reconstruct_arc_polygon_edges(&pts2, &profile.arc_segments) {
                    shapes.push(Shape::ArcPolygon {
                        edges,
                        chord_pts: pts2,
                        is_outer: profile.is_outer,
                    });
                    continue;
                }
                eprintln!(
                    "kernel-v2 KV12: arc-segment reconstruction declined; \
                     falling back to the Tier-1 chord polygon"
                );
            }
            shapes.push(Shape::Polygon {
                pts: pts2,
                is_outer: profile.is_outer,
            });
        }

        // ── pass 2 (KV14): assign each inner loop to its containing outer ───
        // A single sketch's holed region arrives as an `is_outer` outer plus
        // `is_outer=false` inner loops. Attach each inner to the SMALLEST outer
        // whose interior contains it (witness vertex). Grouping is an f64
        // heuristic — `Profile::new` validates containment/disjointness/nesting
        // exactly and CCW-normalizes every loop, so a mis-assignment fails loud.
        let shape_is_outer = |s: &Shape| match s {
            Shape::Circle { .. } => true,
            Shape::Polygon { is_outer, .. } | Shape::ArcPolygon { is_outer, .. } => *is_outer,
        };
        let shape_area = |s: &Shape| -> f64 {
            match s {
                Shape::Circle { radius, .. } => std::f64::consts::PI * radius * radius,
                Shape::Polygon { pts, .. } => polygon_area_abs(pts),
                Shape::ArcPolygon { chord_pts, .. } => polygon_area_abs(chord_pts),
            }
        };
        let outer_contains = |s: &Shape, p: Point2| -> bool {
            match s {
                Shape::Circle { center, radius } => {
                    let dx = p.x() - center.x();
                    let dy = p.y() - center.y();
                    dx * dx + dy * dy < radius * radius
                }
                Shape::Polygon { pts, is_outer } => *is_outer && point_in_polygon_2d(p, pts),
                Shape::ArcPolygon {
                    chord_pts,
                    is_outer,
                    ..
                } => *is_outer && point_in_polygon_2d(p, chord_pts),
            }
        };
        // outer index → indices of the inner loops it contains.
        let mut holes_for: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for i in 0..shapes.len() {
            // An inner (`is_outer == false`) polygon or arc loop is a hole;
            // group it by its chord polygon.
            let inner_pts: Option<&Vec<Point2>> = match &shapes[i] {
                Shape::Polygon {
                    pts,
                    is_outer: false,
                } => Some(pts),
                Shape::ArcPolygon {
                    chord_pts,
                    is_outer: false,
                    ..
                } => Some(chord_pts),
                _ => None,
            };
            if let Some(pts) = inner_pts {
                // Witness = centroid (robustly interior, unlike a vertex which
                // can sit on a coincident outer's boundary).
                let n = pts.len() as f64;
                let cx = pts.iter().map(|p| p.x()).sum::<f64>() / n;
                let cy = pts.iter().map(|p| p.y()).sum::<f64>() / n;
                let witness = Point2::new(cx, cy);
                let hole_area = polygon_area_abs(pts);
                // Candidate outers must STRICTLY enclose the hole — a real hole
                // has strictly smaller area. This rejects the app's redundant
                // same-loop pairing (a loop emitted as both outer and hole),
                // which would otherwise build a degenerate hole == outer.
                let container = (0..shapes.len())
                    .filter(|&j| {
                        j != i
                            && shape_is_outer(&shapes[j])
                            && shape_area(&shapes[j]) > hole_area * (1.0 + cad_primitives::TAU_EVAL)
                            && outer_contains(&shapes[j], witness)
                    })
                    .min_by(|&a, &b| {
                        shape_area(&shapes[a])
                            .partial_cmp(&shape_area(&shapes[b]))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                if let Some(j) = container {
                    holes_for.entry(j).or_default().push(i);
                }
            }
        }

        // A hole loop's Tier-1 chord polygon (for Profile::new holes).
        let shape_chord_pts = |s: &Shape| -> Vec<Point2> {
            match s {
                Shape::Circle { center, radius } => polygonize_circle(*center, *radius),
                Shape::Polygon { pts, .. } => pts.clone(),
                Shape::ArcPolygon { chord_pts, .. } => chord_pts.clone(),
            }
        };
        // A hole loop's exact `ProfileEdge` loop (for Tier-2 arc_polygon holes):
        // an arc hole keeps its arcs, a polygon/circle hole becomes line edges.
        let shape_edges = |s: &Shape| -> Vec<crate::ProfileEdge> {
            match s {
                Shape::ArcPolygon { edges, .. } => edges.clone(),
                Shape::Polygon { pts, .. } => pts_to_line_edges(pts),
                Shape::Circle { center, radius } => {
                    pts_to_line_edges(&polygonize_circle(*center, *radius))
                }
            }
        };

        // ── pass 3: stage one profile per input index (profile_index contract)
        // An outer's index → its holed face; an inner's index → that inner as a
        // standalone face (so selecting just the hole region still extrudes).
        let no_holes: Vec<usize> = Vec::new();
        let mut out = Vec::with_capacity(shapes.len());
        for (i, s) in shapes.iter().enumerate() {
            let hole_idx: &Vec<usize> = holes_for.get(&i).unwrap_or(&no_holes);
            let kv2_profile = match s {
                Shape::Circle { center, radius } => {
                    if !hole_idx.is_empty() {
                        // A circle rim with holes needs a polygon outer to carry
                        // them (a true holed circle): polygonize the rim.
                        let holes = hole_idx
                            .iter()
                            .map(|&h| shape_chord_pts(&shapes[h]))
                            .collect();
                        crate::Profile::new(
                            origin,
                            ux,
                            vy,
                            polygonize_circle(*center, *radius),
                            holes,
                        )
                        .map_err(|e| KernelError::Other {
                            message: format!("kernel-v2 holed circle profile rejected: {e}"),
                        })?
                    } else {
                        crate::Profile::circle(origin, ux, vy, *center, *radius).map_err(|e| {
                            KernelError::Other {
                                message: format!("kernel-v2 circle profile rejected: {e}"),
                            }
                        })?
                    }
                }
                Shape::Polygon { pts, is_outer } => {
                    let holes = if *is_outer {
                        hole_idx
                            .iter()
                            .map(|&h| shape_chord_pts(&shapes[h]))
                            .collect()
                    } else {
                        Vec::new()
                    };
                    crate::Profile::new(origin, ux, vy, pts.clone(), holes).map_err(|e| {
                        KernelError::Other {
                            message: format!("kernel-v2 profile rejected: {e}"),
                        }
                    })?
                }
                Shape::ArcPolygon {
                    edges,
                    chord_pts,
                    is_outer,
                } => {
                    let holes_here: &[usize] = if *is_outer { hole_idx } else { &[] };
                    // Tier 2 (E4 + E4b): the exact arc loop with exact arc
                    // holes — `Profile::arc_polygon` is the exact gate (cylinder
                    // walls on the outer AND arc holes). On ANY decline (failed
                    // simplicity / containment, or a malformed loop) fall back
                    // LOUDLY to the Tier-1 chord polygon, so no input regresses.
                    let hole_edge_loops: Vec<Vec<crate::ProfileEdge>> = holes_here
                        .iter()
                        .map(|&h| shape_edges(&shapes[h]))
                        .collect();
                    let tier2 =
                        crate::Profile::arc_polygon(origin, ux, vy, edges.clone(), hole_edge_loops)
                            .ok();
                    if let Some(p) = tier2 {
                        p
                    } else {
                        eprintln!(
                            "kernel-v2 KV12: arc profile declined Tier 2 (simplicity / \
                             containment) → Tier-1 chord polygon"
                        );
                        let holes = holes_here
                            .iter()
                            .map(|&h| shape_chord_pts(&shapes[h]))
                            .collect();
                        crate::Profile::new(origin, ux, vy, chord_pts.clone(), holes).map_err(
                            |e| KernelError::Other {
                                message: format!("kernel-v2 profile rejected: {e}"),
                            },
                        )?
                    }
                }
            };
            let idx = self.next_staged;
            self.next_staged += 1;
            self.staged.insert(idx, kv2_profile);
            out.push(KernelId(TAG_PROFILE | idx));
        }
        Ok(out)
    }

    fn make_face_from_region(
        &mut self,
        region: &waffle_types::Region,
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        plane_x_axis: [f64; 3],
    ) -> Result<KernelId, KernelError> {
        // Legacy frame convention: y axis = normal × x axis (matches
        // make_faces_from_profiles).
        let n = plane_normal;
        let x = plane_x_axis;
        let y = [
            n[1] * x[2] - n[2] * x[1],
            n[2] * x[0] - n[0] * x[2],
            n[0] * x[1] - n[1] * x[0],
        ];
        let origin = Point3::new(plane_origin[0], plane_origin[1], plane_origin[2]);
        let ux = Vector3::new(x[0], x[1], x[2]);
        let vy = Vector3::new(y[0], y[1], y[2]);

        // Tier 2: recovered arc edges → exact cylinder walls (Profile::arc_polygon
        // is the exact gate). On ANY decline, fall back LOUDLY to the tessellated
        // polygon so no region regresses to "unsupported".
        if !region.outer_edges.is_empty() {
            let outer = region_edges_to_profile(&region.outer_edges);
            let holes: Vec<Vec<crate::ProfileEdge>> = region
                .hole_edges
                .iter()
                .map(|h| region_edges_to_profile(h))
                .collect();
            match crate::Profile::arc_polygon(origin, ux, vy, outer, holes.clone()) {
                Ok(profile) => {
                    let idx = self.next_staged;
                    self.next_staged += 1;
                    self.staged.insert(idx, profile);
                    return Ok(KernelId(TAG_PROFILE | idx));
                }
                Err(e) => {
                    // Tier 2b (MIXED). The full arc profile declined — typically
                    // the OUTER loop (e.g. a non-simple gear-tooth polyline /
                    // spline), NOT the holes. If a HOLE is a clean arc (a bearing
                    // bore circle), retry with the OUTER as the tessellated
                    // POLYGON (line edges — the same simple loop Tier 1 accepts)
                    // while keeping the HOLES as arc edges → EXACT cylinder bore
                    // walls. This preserves a bore as a true cylinder so it welds
                    // with a coincident cylinder-bore solid (the gear-flange
                    // union), instead of a polygon-vs-cylinder mismatch that
                    // self-intersects. On any decline here, fall through to the
                    // full polygon Tier 1 (LOUD, no regression).
                    let holes_have_arc = region.hole_edges.iter().any(|h| {
                        h.iter()
                            .any(|e| matches!(e, waffle_types::RegionEdge::Arc { .. }))
                    });
                    if holes_have_arc && !region.outer.is_empty() {
                        let outer_poly: Vec<crate::ProfileEdge> = region
                            .outer
                            .iter()
                            .zip(region.outer.iter().cycle().skip(1))
                            .map(|(&(ax, ay), &(bx, by))| crate::ProfileEdge::Line {
                                a: Point2::new(ax, ay),
                                b: Point2::new(bx, by),
                            })
                            .collect();
                        match crate::Profile::arc_polygon(origin, ux, vy, outer_poly, holes) {
                            Ok(profile) => {
                                let idx = self.next_staged;
                                self.next_staged += 1;
                                self.staged.insert(idx, profile);
                                return Ok(KernelId(TAG_PROFILE | idx));
                            }
                            Err(e2) => {
                                eprintln!(
                                    "kernel-v2 region: mixed poly-outer+arc-hole also declined ({e2})"
                                );
                            }
                        }
                    }
                    eprintln!(
                        "kernel-v2 region: arc_polygon declined ({e}) → tessellated polygon fallback"
                    );
                }
            }
        }

        // Tier 1: tessellated polygon. Profile::new is the exact gate (simplicity,
        // disjointness, containment) and normalizes loop winding.
        let outer_pts: Vec<Point2> = region
            .outer
            .iter()
            .map(|&(u, v)| Point2::new(u, v))
            .collect();
        let hole_pts: Vec<Vec<Point2>> = region
            .holes
            .iter()
            .map(|h| h.iter().map(|&(u, v)| Point2::new(u, v)).collect())
            .collect();
        let profile = crate::Profile::new(origin, ux, vy, outer_pts, hole_pts).map_err(|e| {
            KernelError::Other {
                message: format!("kernel-v2 region profile rejected: {e}"),
            }
        })?;

        let idx = self.next_staged;
        self.next_staged += 1;
        self.staged.insert(idx, profile);
        Ok(KernelId(TAG_PROFILE | idx))
    }
}

impl KernelIntrospect for KernelV2Adapter {
    fn list_faces(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        if let Some(slot) = self.imported_slot_of(solid) {
            return (0..self.imported[slot].faces.len())
                .map(|i| encode_imported(TAG_IMPORTED_FACE, slot, i))
                .collect();
        }
        let Ok(sid) = self.solid_of(solid) else {
            return Vec::new();
        };
        self.solid_faces(sid).into_iter().map(encode_face).collect()
    }

    fn list_edges(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        if let Some(slot) = self.imported_slot_of(solid) {
            return (0..self.imported[slot].edges.len())
                .map(|i| encode_imported(TAG_IMPORTED_EDGE, slot, i))
                .collect();
        }
        let Ok(sid) = self.solid_of(solid) else {
            return Vec::new();
        };
        self.solid_canonical_edges(sid)
            .into_iter()
            .map(encode_edge)
            .collect()
    }

    fn list_vertices(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        if let Some(slot) = self.imported_slot_of(solid) {
            return (0..self.imported[slot].vertices.len())
                .map(|i| encode_imported(TAG_IMPORTED_VERTEX, slot, i))
                .collect();
        }
        let Ok(sid) = self.solid_of(solid) else {
            return Vec::new();
        };
        self.solid_vertices(sid)
            .into_iter()
            .map(encode_vertex)
            .collect()
    }

    fn face_edges(&self, face: KernelId) -> Vec<KernelId> {
        let (tag, idx) = decode(face);
        if tag == TAG_IMPORTED_FACE {
            let (slot, fi) = decode_imported(idx);
            return match self.imported.get(slot).and_then(|b| b.faces.get(fi)) {
                Some(f) => f
                    .edge_indices
                    .iter()
                    .map(|&e| encode_imported(TAG_IMPORTED_EDGE, slot, e as usize))
                    .collect(),
                None => Vec::new(),
            };
        }
        if tag != TAG_FACE {
            return Vec::new();
        }
        let mut set = BTreeSet::new();
        for h in self.face_half_edges(FaceId(idx)) {
            if let Ok(he) = self.arena.half_edge(h) {
                set.insert(h.min(he.twin));
            }
        }
        set.into_iter().map(encode_edge).collect()
    }

    fn edge_faces(&self, edge: KernelId) -> Vec<KernelId> {
        let (tag, idx) = decode(edge);
        if tag == TAG_IMPORTED_EDGE {
            let (slot, ei) = decode_imported(idx);
            return match self.imported.get(slot).and_then(|b| b.edges.get(ei)) {
                Some(e) => e
                    .face_indices
                    .iter()
                    .map(|&f| encode_imported(TAG_IMPORTED_FACE, slot, f as usize))
                    .collect(),
                None => Vec::new(),
            };
        }
        if tag != TAG_EDGE {
            return Vec::new();
        }
        let h = HalfEdgeId(idx);
        let mut out = Vec::new();
        let mut push_face_of = |he_id: HalfEdgeId| {
            if let Ok(he) = self.arena.half_edge(he_id) {
                if let Ok(lp) = self.arena.loop_(he.loop_id) {
                    out.push(encode_face(lp.face));
                }
            }
        };
        push_face_of(h);
        if let Ok(he) = self.arena.half_edge(h) {
            push_face_of(he.twin);
        }
        out.dedup();
        out
    }

    fn edge_vertices(&self, edge: KernelId) -> (KernelId, KernelId) {
        let (tag, idx) = decode(edge);
        if tag == TAG_IMPORTED_EDGE {
            let (slot, ei) = decode_imported(idx);
            return match self.imported.get(slot).and_then(|b| b.edges.get(ei)) {
                Some(e) => (
                    encode_imported(TAG_IMPORTED_VERTEX, slot, e.endpoints.0 as usize),
                    encode_imported(TAG_IMPORTED_VERTEX, slot, e.endpoints.1 as usize),
                ),
                None => (KernelId(0), KernelId(0)),
            };
        }
        if tag != TAG_EDGE {
            return (KernelId(0), KernelId(0));
        }
        let h = HalfEdgeId(idx);
        let Ok(he) = self.arena.half_edge(h) else {
            return (KernelId(0), KernelId(0));
        };
        let Ok(twin) = self.arena.half_edge(he.twin) else {
            return (KernelId(0), KernelId(0));
        };
        (encode_vertex(he.origin), encode_vertex(twin.origin))
    }

    fn face_neighbors(&self, face: KernelId) -> Vec<KernelId> {
        let (tag, idx) = decode(face);
        if tag == TAG_IMPORTED_FACE {
            let (slot, fi) = decode_imported(idx);
            let Some(body) = self.imported.get(slot) else {
                return Vec::new();
            };
            let Some(f) = body.faces.get(fi) else {
                return Vec::new();
            };
            let mut neighbors = BTreeSet::new();
            for &e in &f.edge_indices {
                if let Some(edge) = body.edges.get(e as usize) {
                    for &other in &edge.face_indices {
                        if other as usize != fi {
                            neighbors.insert(other as usize);
                        }
                    }
                }
            }
            return neighbors
                .into_iter()
                .map(|n| encode_imported(TAG_IMPORTED_FACE, slot, n))
                .collect();
        }
        if tag != TAG_FACE {
            return Vec::new();
        }
        let fid = FaceId(idx);
        let mut set = BTreeSet::new();
        for h in self.face_half_edges(fid) {
            if let Ok(he) = self.arena.half_edge(h) {
                if let Ok(twin) = self.arena.half_edge(he.twin) {
                    if let Ok(lp) = self.arena.loop_(twin.loop_id) {
                        if lp.face != fid {
                            set.insert(lp.face);
                        }
                    }
                }
            }
        }
        set.into_iter().map(encode_face).collect()
    }

    fn compute_signature(&self, entity: KernelId, kind: TopoKind) -> TopoSignature {
        let (tag, idx) = decode(entity);
        match (kind, tag) {
            (TopoKind::Vertex, TAG_VERTEX) => self.vertex_signature(VertexId(idx)),
            (TopoKind::Edge, TAG_EDGE) => self.edge_signature(HalfEdgeId(idx)),
            (TopoKind::Face, TAG_FACE) => self.face_signature(FaceId(idx)),
            (TopoKind::Face, TAG_IMPORTED_FACE) => {
                let (slot, fi) = decode_imported(idx);
                self.imported
                    .get(slot)
                    .map(|b| b.face_signature(fi))
                    .unwrap_or_else(TopoSignature::empty)
            }
            (TopoKind::Edge, TAG_IMPORTED_EDGE) => {
                let (slot, ei) = decode_imported(idx);
                self.imported
                    .get(slot)
                    .map(|b| b.edge_signature(ei))
                    .unwrap_or_else(TopoSignature::empty)
            }
            (TopoKind::Vertex, TAG_IMPORTED_VERTEX) => {
                let (slot, vi) = decode_imported(idx);
                self.imported
                    .get(slot)
                    .map(|b| b.vertex_signature(vi))
                    .unwrap_or_else(TopoSignature::empty)
            }
            _ => TopoSignature::empty(),
        }
    }

    fn compute_all_signatures(
        &self,
        solid: &KernelSolidHandle,
        kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)> {
        if let Some(slot) = self.imported_slot_of(solid) {
            let body = &self.imported[slot];
            return match kind {
                TopoKind::Face => (0..body.faces.len())
                    .map(|i| {
                        (
                            encode_imported(TAG_IMPORTED_FACE, slot, i),
                            body.face_signature(i),
                        )
                    })
                    .collect(),
                TopoKind::Edge => (0..body.edges.len())
                    .map(|i| {
                        (
                            encode_imported(TAG_IMPORTED_EDGE, slot, i),
                            body.edge_signature(i),
                        )
                    })
                    .collect(),
                TopoKind::Vertex => (0..body.vertices.len())
                    .map(|i| {
                        (
                            encode_imported(TAG_IMPORTED_VERTEX, slot, i),
                            body.vertex_signature(i),
                        )
                    })
                    .collect(),
                _ => Vec::new(),
            };
        }
        let Ok(sid) = self.solid_of(solid) else {
            return Vec::new();
        };
        match kind {
            TopoKind::Face => self
                .solid_faces(sid)
                .into_iter()
                .map(|f| (encode_face(f), self.face_signature(f)))
                .collect(),
            TopoKind::Edge => self
                .solid_canonical_edges(sid)
                .into_iter()
                .map(|h| (encode_edge(h), self.edge_signature(h)))
                .collect(),
            TopoKind::Vertex => self
                .solid_vertices(sid)
                .into_iter()
                .map(|v| (encode_vertex(v), self.vertex_signature(v)))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// KV13 F5: a face's persistent id + its lineage root (walked back through
    /// the boolean journal). `None` for a non-face id or an untagged face.
    fn face_provenance(&self, face: KernelId) -> Option<waffle_types::kernel::FaceProvenance> {
        let (tag, idx) = decode(face);
        if tag != TAG_FACE {
            return None;
        }
        let pid = self.arena.face_pid(FaceId(idx))?;
        let root = crate::journal::face_lineage(&self.arena.journal, pid).root;
        Some(waffle_types::kernel::FaceProvenance {
            pid: pid.0,
            root_pid: root.0,
        })
    }
}

#[cfg(test)]
mod tests;

/// The KV14 re-entry census line for a face: its surface, every loop's
/// half-edge curve pattern (with start/end points), for `KV14_REENTRY_CENSUS`.
fn reentry_census(arena: &crate::BrepArena, face: crate::FaceId) -> String {
    use crate::arena::{Curve, Surface};
    let mut out = String::new();
    let Ok(f) = arena.face(face) else {
        return "  (face not in arena)".into();
    };
    let surf = match f.surface {
        Some(Surface::Plane(_)) => "Plane".to_string(),
        Some(Surface::Cylinder {
            axis_point,
            axis_dir,
            radius,
            ..
        }) => format!("Cylinder axis_point={axis_point:?} axis_dir={axis_dir:?} r={radius:.6e}"),
        Some(Surface::Cone {
            apex,
            axis_dir,
            half_angle,
            ..
        }) => format!("Cone apex={apex:?} axis_dir={axis_dir:?} half_angle={half_angle:.6e}"),
        Some(Surface::Torus {
            center,
            axis_dir,
            major_radius,
            minor_radius,
            ..
        }) => format!(
            "Torus center={center:?} axis_dir={axis_dir:?} R={major_radius:.6e} r={minor_radius:.6e}"
        ),
        Some(Surface::Sphere { .. }) => "Sphere".to_string(),
        None => "None".to_string(),
    };
    out.push_str(&format!(
        "  surface={surf} inner_loops={}\n",
        f.inner_loops.len()
    ));
    let curve_name = |c: &Curve| -> String {
        match c {
            Curve::LineSegment => "Line".into(),
            Curve::SurfacePair { a, b } => format!("SurfacePair({a:?}|{b:?})"),
            Curve::Circle { radius, .. } => format!("Circle(r={radius:.4e})"),
            Curve::Arc { radius, .. } => format!("Arc(r={radius:.4e})"),
            Curve::EllipseArc { .. } => "EllipseArc".into(),
            Curve::HyperbolaArc { .. } => "HyperbolaArc".into(),
        }
    };
    let loops =
        std::iter::once(("outer", f.outer_loop)).chain(f.inner_loops.iter().map(|&l| ("inner", l)));
    for (kind, lid) in loops {
        let Ok(hes) = arena.loop_half_edges(lid) else {
            continue;
        };
        out.push_str(&format!("  {kind} loop {lid:?}: {} edges\n", hes.len()));
        for h in hes {
            let Ok(he) = arena.half_edge(h) else {
                continue;
            };
            let p0 = arena.vertex(he.origin).map(|v| v.point).ok();
            let p1 = arena
                .half_edge(he.next)
                .and_then(|n| arena.vertex(n.origin))
                .map(|v| v.point)
                .ok();
            let twin_face = arena
                .half_edge(he.twin)
                .and_then(|t| arena.loop_(t.loop_id))
                .map(|l| l.face)
                .ok();
            out.push_str(&format!(
                "    {h:?} {} from={p0:?} to={p1:?} twin_face={twin_face:?}\n",
                curve_name(&he.curve)
            ));
        }
    }
    out
}
