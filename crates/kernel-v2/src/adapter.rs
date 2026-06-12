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
//! `KernelError::NotSupported` LOUDLY for anything kernel-v2 does not yet
//! implement. The adapter MAPS; it never repairs, approximates, or stubs a
//! result (no polygonized circles, no fake revolve).
//!
//! ## Per-method coverage table
//!
//! | Legacy trait method | Status | Mapping |
//! |---|---|---|
//! | `make_faces_from_profiles` | SUPPORTED (polygon + circle profiles, PR-KV5b) | `ClosedProfile` polygon → `kernel_v2::Profile::new`; `CircleProfile` → `kernel_v2::Profile::circle` (staged); arc-segment / spline profiles → `NotSupported` |
//! | `extrude_face` | SUPPORTED | staged profile → `kernel_v2::extrude` (sweep vector = `direction · depth`, exactly the legacy semantics); circle profiles → cylinder solids (PR-KV5a) |
//! | `revolve_face` | SUPPORTED (PR-KV6a) | staged polygon profile → `kernel_v2::revolve` (degrees → radians; world-space in-plane axis). Typed walls: oblique edges (cones, KV6c), circle profiles (torus, KV6d), holed profiles → `NotSupported`; axis touching/crossing the profile and out-of-range angles → `KernelError::Other` (INVALID INPUT — the F0073/F0074 expected-rebuild-error path, never the NotSupported marker) |
//! | `boolean_union` / `_subtract` / `_intersect` | SUPPORTED (non-coplanar; cylinder×box class PR-KV5b) | `kernel_v2::boolean_op` (yang-rs native pipeline); coplanar input face pairs → `NotSupported` (Yang Stage 0 / roadmap M8); curved partial-patch RESULT operands → `NotSupported` (no yang Stage-1 re-entry); cylinder×cylinder / oblique-ellipse sections → `BooleanFailed` carrying the typed wall text |
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

use crate::{BrepArena, FaceId, HalfEdgeId, KernelV2Error, SolidId, Surface, VertexId};
use cad_primitives::{BoolOp, Point2, Point3, Vector3};
use waffle_types::kernel::{
    ClosedProfile, EdgeRange, EdgeRenderData, FaceRange, KernelError, KernelId, KernelSolidHandle,
    RenderMesh, TopoKind, TopoSignature,
};
use waffle_types::kernel::{Kernel, KernelIntrospect};

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

    fn run_boolean(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
        op: BoolOp,
        op_name: &str,
    ) -> Result<KernelSolidHandle, KernelError> {
        let sa = self.solid_of(a)?;
        let sb = self.solid_of(b)?;
        match crate::boolean_op(&mut self.arena, sa, sb, op) {
            Ok(result) => Ok(self.alloc_handle(result)),
            Err(KernelV2Error::UnsupportedCoplanar) => Err(Self::not_supported(&format!(
                "{op_name}: coplanar input face pair (Yang Stage 0 coplanar preprocessing — roadmap M8 — not yet implemented)"
            ))),
            // PR-KV5b: a curved RESULT solid (partial cylinder patches from
            // a previous boolean) cannot re-enter yang-rs Stage 1 — a
            // declared boundary, not a bug (see kernel-v2 boolean.rs docs).
            Err(KernelV2Error::UnsupportedCurvedBoolean { face }) => {
                Err(Self::not_supported(&format!(
                    "{op_name}: curved partial-patch operand face {face:?} (a previous curved \
                     boolean's result cannot re-enter yang-rs Stage 1 — no partial-patch \
                     tessellation yet)"
                )))
            }
            // PR-KV7: multi-shell operands (internal voids / disjoint
            // bodies) cannot re-enter yang-rs — a declared boundary.
            Err(KernelV2Error::UnsupportedMultiShellBoolean { shells }) => {
                Err(Self::not_supported(&format!(
                    "{op_name}: multi-shell operand ({shells} shells — an internal void or                      disjoint bodies cannot re-enter yang-rs reassembly yet)"
                )))
            }
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
            KernelV2Error::RevolveObliqueEdgeUnsupported => Self::not_supported(
                "revolve_face: oblique profile edge sweeps a CONE                  (kernel-v2 roadmap KV6c)",
            ),
            KernelV2Error::RevolveCircleProfileUnsupported => Self::not_supported(
                "revolve_face: circle profile sweeps a TORUS                  (kernel-v2 roadmap KV6d)",
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

    fn tessellate(
        &mut self,
        solid: &KernelSolidHandle,
        _tolerance: f64, // planar tessellation is exact; tolerance is moot
    ) -> Result<RenderMesh, KernelError> {
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
        let sid = self.solid_of(solid)?;
        let mut vertices: Vec<f32> = Vec::new();
        let mut edge_ranges: Vec<EdgeRange> = Vec::new();
        for canonical in self.solid_canonical_edges(sid) {
            let Some((a, b)) = self.edge_endpoints(canonical) else {
                continue;
            };
            let start_vertex = (vertices.len() / 3) as u32;
            for p in [a, b] {
                let p = p.as_array();
                vertices.extend_from_slice(&[p[0] as f32, p[1] as f32, p[2] as f32]);
            }
            edge_ranges.push(EdgeRange {
                edge_id: encode_edge(canonical),
                start_vertex,
                end_vertex: start_vertex + 2,
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

        let mut out = Vec::with_capacity(profiles.len());
        for profile in profiles {
            // PR-KV5b: circle profiles map to kernel-v2's validated circle
            // profile (legacy semantics: center in sketch-plane (u, v)
            // coordinates, radius in meters, same plane frame as polygons).
            if let Some(circle) = &profile.circle {
                let kv2_profile = crate::Profile::circle(
                    Point3::new(plane_origin[0], plane_origin[1], plane_origin[2]),
                    Vector3::new(x[0], x[1], x[2]),
                    Vector3::new(y[0], y[1], y[2]),
                    Point2::new(circle.center_u, circle.center_v),
                    circle.radius,
                )
                .map_err(|e| KernelError::Other {
                    message: format!("kernel-v2 circle profile rejected: {e}"),
                })?;
                let idx = self.next_staged;
                self.next_staged += 1;
                self.staged.insert(idx, kv2_profile);
                out.push(KernelId(TAG_PROFILE | idx));
                continue;
            }
            if !profile.spline_segments.is_empty() {
                return Err(Self::not_supported(
                    "make_faces_from_profiles: spline-segment profile (curved geometry not yet in kernel-v2)",
                ));
            }
            if !profile.arc_segments.is_empty() {
                return Err(Self::not_supported(
                    "make_faces_from_profiles: arc-segment profile (curved geometry not yet in kernel-v2)",
                ));
            }

            // Vertex key selection mirrors the legacy kernel: prefer
            // vertex_ids, fall back to entity_ids, then sorted position keys.
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
                    let (u, v) = positions[k];
                    Point2::new(u, v)
                })
                .collect();

            let kv2_profile = crate::Profile::new(
                Point3::new(plane_origin[0], plane_origin[1], plane_origin[2]),
                Vector3::new(x[0], x[1], x[2]),
                Vector3::new(y[0], y[1], y[2]),
                pts2,
                Vec::new(),
            )
            .map_err(|e| KernelError::Other {
                message: format!("kernel-v2 profile rejected: {e}"),
            })?;

            let idx = self.next_staged;
            self.next_staged += 1;
            self.staged.insert(idx, kv2_profile);
            out.push(KernelId(TAG_PROFILE | idx));
        }
        Ok(out)
    }
}

impl KernelIntrospect for KernelV2Adapter {
    fn list_faces(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        let Ok(sid) = self.solid_of(solid) else {
            return Vec::new();
        };
        self.solid_faces(sid).into_iter().map(encode_face).collect()
    }

    fn list_edges(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
        let Ok(sid) = self.solid_of(solid) else {
            return Vec::new();
        };
        self.solid_canonical_edges(sid)
            .into_iter()
            .map(encode_edge)
            .collect()
    }

    fn list_vertices(&self, solid: &KernelSolidHandle) -> Vec<KernelId> {
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
            _ => TopoSignature::empty(),
        }
    }

    fn compute_all_signatures(
        &self,
        solid: &KernelSolidHandle,
        kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a staged unit-square profile and return its KernelId.
    fn stage_unit_square(adapter: &mut KernelV2Adapter) -> KernelId {
        let profile = ClosedProfile {
            entity_ids: vec![0, 1, 2, 3],
            is_outer: true,
            vertex_ids: vec![0, 1, 2, 3],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let positions: HashMap<u32, (f64, f64)> = [
            (0, (0.0, 0.0)),
            (1, (1.0, 0.0)),
            (2, (1.0, 1.0)),
            (3, (0.0, 1.0)),
        ]
        .into_iter()
        .collect();
        let ids = adapter
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .expect("square profile stages");
        assert_eq!(ids.len(), 1);
        ids[0]
    }

    #[test]
    fn extrude_square_produces_valid_box() {
        let mut adapter = KernelV2Adapter::new();
        let face = stage_unit_square(&mut adapter);
        let handle = adapter
            .extrude_face(face, [0.0, 0.0, 1.0], 2.0)
            .expect("extrude succeeds");

        // Introspection: a box has 6 faces, 12 edges, 8 vertices.
        assert_eq!(adapter.list_faces(&handle).len(), 6);
        assert_eq!(adapter.list_edges(&handle).len(), 12);
        assert_eq!(adapter.list_vertices(&handle).len(), 8);

        // Tessellation: 12 triangles, valid contiguous face ranges.
        let mesh = adapter.tessellate(&handle, 0.1).expect("tessellates");
        assert_eq!(mesh.indices.len() / 3, 12);
        assert_eq!(mesh.face_ranges.len(), 6);
        let mut expected_start = 0;
        for r in &mesh.face_ranges {
            assert_eq!(r.start_index, expected_start);
            expected_start = r.end_index;
        }
        assert_eq!(expected_start as usize, mesh.indices.len());

        // Edges: 12 segments.
        let edges = adapter.extract_edges(&handle, 0.1).expect("edges");
        assert_eq!(edges.edge_ranges.len(), 12);
        assert_eq!(edges.vertices.len(), 12 * 2 * 3);
    }

    #[test]
    fn face_signatures_carry_area_and_normal() {
        let mut adapter = KernelV2Adapter::new();
        let face = stage_unit_square(&mut adapter);
        let handle = adapter.extrude_face(face, [0.0, 0.0, 1.0], 2.0).unwrap();
        let sigs = adapter.compute_all_signatures(&handle, TopoKind::Face);
        assert_eq!(sigs.len(), 6);
        // Total surface area of a 1×1×2 box = 2·(1·1) + 4·(1·2) = 10.
        let total: f64 = sigs.iter().map(|(_, s)| s.area.unwrap()).sum();
        assert!((total - 10.0).abs() < 1e-9, "total area {total}");
        for (_, s) in &sigs {
            let n = s.normal.unwrap();
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-9);
            assert_eq!(s.surface_type.as_deref(), Some("planar"));
        }
    }

    /// PR-KV5b flipped the circle-profile wall (circles now stage — see the
    /// KV5b tests below); spline- and arc-SEGMENT profiles remain loudly
    /// unsupported.
    #[test]
    fn spline_and_arc_segment_profiles_are_loudly_unsupported() {
        let mut adapter = KernelV2Adapter::new();
        let base = ClosedProfile {
            entity_ids: vec![],
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let mut spline = base.clone();
        spline.spline_segments = vec![waffle_types::kernel::SplineSegment {
            start_point_index: 0,
            end_point_index: 1,
            control_points: vec![(0.0, 0.0), (1.0, 0.5), (2.0, 0.0)],
        }];
        let err = adapter
            .make_faces_from_profiles(
                &[spline],
                [0.0; 3],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &HashMap::new(),
            )
            .unwrap_err();
        assert!(matches!(err, KernelError::NotSupported { .. }), "{err:?}");

        let mut arc = base.clone();
        arc.arc_segments = vec![waffle_types::ArcSegment {
            start_vertex_index: 0,
            end_vertex_index: 1,
            center_u: 0.0,
            center_v: 0.0,
            radius: 1.0,
        }];
        let err = adapter
            .make_faces_from_profiles(
                &[arc],
                [0.0; 3],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &HashMap::new(),
            )
            .unwrap_err();
        assert!(matches!(err, KernelError::NotSupported { .. }), "{err:?}");
    }

    #[test]
    fn revolve_fillet_chamfer_shell_are_loudly_unsupported() {
        // Revolve is SUPPORTED since PR-KV6a; what stays loud here is its
        // input validation (an axis along the plane NORMAL is invalid input
        // → plain error, not a capability wall)…
        let mut adapter = KernelV2Adapter::new();
        let face = stage_unit_square(&mut adapter);
        let err = adapter
            .revolve_face(face, [0.0; 3], [0.0, 0.0, 1.0], 360.0)
            .unwrap_err();
        assert!(matches!(err, KernelError::Other { .. }), "{err:?}");

        // …and the indefinitely-deferred operations.

        let face2 = stage_unit_square(&mut adapter);
        let handle = adapter.extrude_face(face2, [0.0, 0.0, 1.0], 1.0).unwrap();
        assert!(matches!(
            adapter.fillet_edges(&handle, &[], 0.1).unwrap_err(),
            KernelError::NotSupported { .. }
        ));
        assert!(matches!(
            adapter.chamfer_edges(&handle, &[], 0.1).unwrap_err(),
            KernelError::NotSupported { .. }
        ));
        assert!(matches!(
            adapter.shell(&handle, &[], 0.1).unwrap_err(),
            KernelError::NotSupported { .. }
        ));
    }

    // ── PR-KV5b RED: circle profiles through the legacy trait ──────────────

    /// Stage a circle profile (legacy `CircleProfile` semantics: center in
    /// sketch-plane (u, v) coordinates, radius in meters).
    fn stage_circle(
        adapter: &mut KernelV2Adapter,
        origin: [f64; 3],
        center: (f64, f64),
        radius: f64,
    ) -> KernelId {
        let profile = ClosedProfile {
            entity_ids: vec![7],
            is_outer: true,
            vertex_ids: vec![],
            circle: Some(waffle_types::kernel::CircleProfile {
                center_u: center.0,
                center_v: center.1,
                radius,
            }),
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let ids = adapter
            .make_faces_from_profiles(
                &[profile],
                origin,
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &HashMap::new(),
            )
            .expect("circle profile stages (PR-KV5b)");
        assert_eq!(ids.len(), 1);
        ids[0]
    }

    fn render_mesh_volume(mesh: &RenderMesh) -> f64 {
        let mut vol = 0.0f64;
        let p = |i: u32| {
            let i = i as usize * 3;
            [
                mesh.vertices[i] as f64,
                mesh.vertices[i + 1] as f64,
                mesh.vertices[i + 2] as f64,
            ]
        };
        for t in mesh.indices.chunks(3) {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0;
        }
        vol
    }

    /// PR-KV5b: legacy CircleProfile → kernel-v2 cylinder, end to end
    /// through the legacy trait: stage, extrude, introspect, tessellate,
    /// extract edges. The canonical cylinder topology is 3 faces / 3 edges /
    /// 2 vertices; the tessellated volume matches πr²h within kernel-v2's
    /// render sagitta band (N = 71 at the canonical tolerance →
    /// relative deficit 1 − (N/2π)·sin(2π/N) ≈ 6.5e-4).
    #[test]
    fn circle_profile_extrudes_to_cylinder_through_legacy_trait() {
        let mut adapter = KernelV2Adapter::new();
        let face = stage_circle(&mut adapter, [0.0, 0.0, 0.0], (0.5, 0.5), 0.25);
        let handle = adapter
            .extrude_face(face, [0.0, 0.0, 1.0], 2.0)
            .expect("circle extrude succeeds (PR-KV5b)");

        assert_eq!(adapter.list_faces(&handle).len(), 3, "two caps + lateral");
        assert_eq!(adapter.list_edges(&handle).len(), 3, "two rims + seam");
        assert_eq!(adapter.list_vertices(&handle).len(), 2, "seam vertices");

        let mesh = adapter.tessellate(&handle, 0.001).expect("tessellates");
        assert!(!mesh.indices.is_empty());
        let vol = render_mesh_volume(&mesh);
        let exact = std::f64::consts::PI * 0.25 * 0.25 * 2.0;
        assert!(
            (vol - exact).abs() <= 2e-3 * exact,
            "cylinder volume {vol} vs analytic {exact}"
        );

        let edges = adapter.extract_edges(&handle, 0.001).expect("edges");
        assert_eq!(edges.edge_ranges.len(), 3, "two rim polylines + one seam");
    }

    /// PR-KV5b: cylinder ∪ box through the legacy boolean trait (the
    /// yang-proven yr8 configuration). Volume = box + the cylinder part
    /// outside it, within the documented yang Stage-1 rim faceting band
    /// (see kernel-v2 tests/kv5b_curved_boolean.rs module docs).
    #[test]
    fn boolean_union_cylinder_box_through_legacy_trait() {
        let mut adapter = KernelV2Adapter::new();
        let cyl_face = stage_circle(&mut adapter, [0.0, 0.0, -0.5], (0.5, 0.5), 0.25);
        let cyl = adapter
            .extrude_face(cyl_face, [0.0, 0.0, 1.0], 2.0)
            .expect("cylinder extrude");
        let box_face = stage_unit_square(&mut adapter);
        let bx = adapter
            .extrude_face(box_face, [0.0, 0.0, 1.0], 1.0)
            .expect("box extrude");

        let out = adapter
            .boolean_union(&cyl, &bx)
            .expect("cylinder ∪ box succeeds (PR-KV5b)");
        let mesh = adapter.tessellate(&out, 0.001).expect("tessellates");
        let vol = render_mesh_volume(&mesh);
        let cyl_term = std::f64::consts::PI * 0.25 * 0.25 * 1.0;
        let exact = 1.0 + cyl_term;
        assert!(
            (vol - exact).abs() <= 0.12 * cyl_term,
            "union volume {vol} vs analytic {exact}"
        );
    }

    #[test]
    fn boolean_subtract_offset_boxes() {
        // Tool overlaps blank's corner region but NO coplanar face pairs:
        // every tool face plane is strictly inside or outside the blank.
        let mut adapter = KernelV2Adapter::new();
        let blank_face = stage_unit_square(&mut adapter);
        let blank = adapter
            .extrude_face(blank_face, [0.0, 0.0, 1.0], 1.0)
            .unwrap();

        // Tool: square at (0.4..1.4)², z from -0.3 to 0.6 — offset on all axes.
        let profile = ClosedProfile {
            entity_ids: vec![0, 1, 2, 3],
            is_outer: true,
            vertex_ids: vec![0, 1, 2, 3],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        };
        let positions: HashMap<u32, (f64, f64)> = [
            (0, (0.4, 0.4)),
            (1, (1.4, 0.4)),
            (2, (1.4, 1.4)),
            (3, (0.4, 1.4)),
        ]
        .into_iter()
        .collect();
        let tool_face = adapter
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, -0.3],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap()[0];
        let tool = adapter
            .extrude_face(tool_face, [0.0, 0.0, 1.0], 0.9)
            .unwrap();

        let result = adapter
            .boolean_subtract(&blank, &tool)
            .expect("offset-box subtract succeeds");
        let mesh = adapter.tessellate(&result, 0.1).expect("tessellates");
        assert!(!mesh.indices.is_empty());

        // Volume check: blank 1.0 minus the overlap (0.6·0.6·0.6 = 0.216).
        let mut vol = 0.0f64;
        for t in mesh.indices.chunks(3) {
            let p = |i: u32| {
                let i = i as usize * 3;
                [
                    mesh.vertices[i] as f64,
                    mesh.vertices[i + 1] as f64,
                    mesh.vertices[i + 2] as f64,
                ]
            };
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            vol += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0;
        }
        assert!(
            (vol - (1.0 - 0.216)).abs() < 1e-6,
            "subtract volume {vol}, expected 0.784"
        );
    }
}
