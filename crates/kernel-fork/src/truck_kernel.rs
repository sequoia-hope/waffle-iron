//! TruckKernel — real geometry kernel wrapping truck's API.

use crate::tessellation;
use crate::traits::Kernel;
use crate::types::*;
use std::collections::HashMap;

// Import truck types selectively to avoid shadowing std::result::Result
use truck_modeling::builder;
use truck_modeling::geometry::Surface;
use truck_modeling::topology::{Edge, Face, Solid, Wire};
use truck_modeling::{InnerSpace, Point3, Rad, Vector3};

/// Compute the maximum extent of a solid's bounding box from its boundary vertices.
fn solid_max_extent(solid: &Solid) -> f64 {
    let mut min = [f64::MAX; 3];
    let mut max = [f64::MIN; 3];
    let mut has_pts = false;
    for shell in solid.boundaries() {
        for v in shell.vertex_iter() {
            let p = v.point();
            min[0] = min[0].min(p.x);
            min[1] = min[1].min(p.y);
            min[2] = min[2].min(p.z);
            max[0] = max[0].max(p.x);
            max[1] = max[1].max(p.y);
            max[2] = max[2].max(p.z);
            has_pts = true;
        }
    }
    if !has_pts {
        return 1.0;
    }
    let dx = max[0] - min[0];
    let dy = max[1] - min[1];
    let dz = max[2] - min[2];
    dx.max(dy).max(dz).max(1e-10)
}

/// Compute scale-aware boolean tolerance from two solids' bounding boxes.
///
/// Uses `(extent * 0.005).clamp(0.005, 0.05)` — only scales DOWN from the default
/// 0.05 for small geometry (extent < 10). Never exceeds 0.05 because truck's boolean
/// pipeline (especially cylinder triangulation) is calibrated around that value.
fn compute_adaptive_tol(solid_a: &Solid, solid_b: &Solid) -> f64 {
    let extent = solid_max_extent(solid_a).max(solid_max_extent(solid_b));
    (extent * 0.005).clamp(0.005, 0.05)
}

/// Compute scale-aware healing tolerance from two solids' bounding boxes.
///
/// Healing tolerance is 10% of the boolean tolerance — tight enough to
/// preserve curve accuracy while still being proportional to geometry size.
fn compute_healing_tol(solid_a: &Solid, solid_b: &Solid) -> f64 {
    compute_adaptive_tol(solid_a, solid_b) * 0.1
}

/// Real geometry kernel backed by the truck BREP library.
pub struct TruckKernel {
    next_handle: u64,
    next_id: u64,
    solids: HashMap<u64, Solid>,
    /// Standalone faces created by make_faces_from_profiles, awaiting extrude.
    standalone_faces: HashMap<u64, Face>,
}

impl TruckKernel {
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            next_id: 1,
            solids: HashMap::new(),
            standalone_faces: HashMap::new(),
        }
    }

    fn alloc_handle(&mut self) -> KernelSolidHandle {
        let h = KernelSolidHandle(self.next_handle);
        self.next_handle += 1;
        h
    }

    fn alloc_id(&mut self) -> KernelId {
        let id = KernelId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn store_solid(&mut self, solid: Solid) -> KernelSolidHandle {
        let handle = self.alloc_handle();
        self.solids.insert(handle.id(), solid);
        handle
    }

    pub(crate) fn get_solid(&self, handle: &KernelSolidHandle) -> Option<&Solid> {
        self.solids.get(&handle.id())
    }

    /// Export a solid to STEP AP203 format string.
    pub fn export_step(
        &self,
        handle: &KernelSolidHandle,
        file_name: &str,
    ) -> Result<String, KernelError> {
        use truck_stepio::out::*;
        use truck_topology::compress::CompressedSolid;

        let solid = self
            .solids
            .get(&handle.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(handle.id()),
            })?;

        let compressed: CompressedSolid<_, _, _> = solid.compress();
        let step_model = StepModel::from(&compressed);
        let header = StepHeaderDescriptor {
            file_name: file_name.to_string(),
            ..Default::default()
        };
        let complete = CompleteStepDisplay::new(step_model, header);
        Ok(complete.to_string())
    }
}

impl Default for TruckKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel for TruckKernel {
    fn extrude_face(
        &mut self,
        face: KernelId,
        direction: [f64; 3],
        depth: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        let truck_face = self
            .standalone_faces
            .remove(&face.0)
            .ok_or(KernelError::EntityNotFound { id: face })?;

        let dir = Vector3::new(direction[0], direction[1], direction[2]);
        let dir_len = dir.magnitude();
        if dir_len < 1e-12 {
            return Err(KernelError::Other {
                message: "extrude direction has zero length".to_string(),
            });
        }
        let sweep_vec = dir.normalize() * depth;

        // When the sweep direction opposes the face normal, tsweep creates a
        // solid with inside-out face orientations (all normals point inward).
        // This causes boolean_subtract to compute intersection instead of
        // subtraction, and breaks chained booleans entirely.
        // Fix: invert the face so its normal aligns with the sweep direction.
        let face_normal: Option<Vector3> = match truck_face.surface() {
            Surface::Plane(ref p) => Some(p.normal()),
            _ => None,
        };
        let truck_face = if let Some(n) = face_normal {
            if InnerSpace::dot(n, sweep_vec) < 0.0 {
                truck_face.inverse()
            } else {
                truck_face
            }
        } else {
            truck_face
        };

        let solid = builder::tsweep(&truck_face, sweep_vec);
        Ok(self.store_solid(solid))
    }

    fn revolve_face(
        &mut self,
        face: KernelId,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        let truck_face = self
            .standalone_faces
            .remove(&face.0)
            .ok_or(KernelError::EntityNotFound { id: face })?;

        let origin = Point3::new(axis_origin[0], axis_origin[1], axis_origin[2]);
        let axis = Vector3::new(axis_direction[0], axis_direction[1], axis_direction[2]);
        if axis.magnitude() < 1e-12 {
            return Err(KernelError::Other {
                message: "revolve axis has zero length".to_string(),
            });
        }

        let angle_rad = angle.to_radians();
        let solid = builder::rsweep(&truck_face, origin, axis.normalize(), Rad(angle_rad));
        Ok(self.store_solid(solid))
    }

    fn boolean_union(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        let solid_a = self
            .solids
            .get(&a.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(a.id()),
            })?
            .clone();
        let solid_b = self
            .solids
            .get(&b.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(b.id()),
            })?
            .clone();

        // Heal inputs if they have IntersectionCurve edges from previous booleans
        let heal_tol = compute_healing_tol(&solid_a, &solid_b);
        crate::healing::heal_intersection_curves(&solid_a, heal_tol);
        crate::healing::heal_intersection_curves(&solid_b, heal_tol);

        let tol = compute_adaptive_tol(&solid_a, &solid_b);
        let result =
            crate::healing::try_boolean_with_perturbation(&solid_a, &solid_b, tol, |a, b| {
                truck_shapeops::or(a, b, tol)
            })
            .ok_or_else(|| KernelError::BooleanFailed {
                reason: "truck or() returned None".to_string(),
            })?;
        crate::healing::heal_intersection_curves(&result, heal_tol);
        Ok(self.store_solid(result))
    }

    fn boolean_subtract(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        let solid_a = self
            .solids
            .get(&a.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(a.id()),
            })?
            .clone();
        let solid_b = self
            .solids
            .get(&b.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(b.id()),
            })?
            .clone();

        // Subtraction = A \ B via proper difference().
        // Heal inputs if they have IntersectionCurve edges from previous booleans.
        let heal_tol = compute_healing_tol(&solid_a, &solid_b);
        crate::healing::heal_intersection_curves(&solid_a, heal_tol);
        crate::healing::heal_intersection_curves(&solid_b, heal_tol);

        let tol = compute_adaptive_tol(&solid_a, &solid_b);
        let result =
            crate::healing::try_boolean_with_perturbation(&solid_a, &solid_b, tol, |a, b| {
                truck_shapeops::difference(a, b, tol)
            })
            .ok_or_else(|| KernelError::BooleanFailed {
                reason: "truck difference() returned None".to_string(),
            })?;
        crate::healing::heal_intersection_curves(&result, heal_tol);
        Ok(self.store_solid(result))
    }

    fn boolean_intersect(
        &mut self,
        a: &KernelSolidHandle,
        b: &KernelSolidHandle,
    ) -> Result<KernelSolidHandle, KernelError> {
        let solid_a = self
            .solids
            .get(&a.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(a.id()),
            })?
            .clone();
        let solid_b = self
            .solids
            .get(&b.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(b.id()),
            })?
            .clone();

        // Heal inputs if they have IntersectionCurve edges from previous booleans
        let heal_tol = compute_healing_tol(&solid_a, &solid_b);
        crate::healing::heal_intersection_curves(&solid_a, heal_tol);
        crate::healing::heal_intersection_curves(&solid_b, heal_tol);

        let tol = compute_adaptive_tol(&solid_a, &solid_b);
        let result =
            crate::healing::try_boolean_with_perturbation(&solid_a, &solid_b, tol, |a, b| {
                truck_shapeops::and(a, b, tol)
            })
            .ok_or_else(|| KernelError::BooleanFailed {
                reason: "truck and() returned None".to_string(),
            })?;
        crate::healing::heal_intersection_curves(&result, heal_tol);
        Ok(self.store_solid(result))
    }

    fn fillet_edges(
        &mut self,
        _solid: &KernelSolidHandle,
        _edges: &[KernelId],
        _radius: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "fillet_edges".to_string(),
        })
    }

    fn chamfer_edges(
        &mut self,
        _solid: &KernelSolidHandle,
        _edges: &[KernelId],
        _distance: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "chamfer_edges".to_string(),
        })
    }

    fn shell(
        &mut self,
        _solid: &KernelSolidHandle,
        _faces_to_remove: &[KernelId],
        _thickness: f64,
    ) -> Result<KernelSolidHandle, KernelError> {
        Err(KernelError::NotSupported {
            operation: "shell".to_string(),
        })
    }

    fn tessellate(
        &mut self,
        solid: &KernelSolidHandle,
        tolerance: f64,
    ) -> Result<RenderMesh, KernelError> {
        let truck_solid = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?;

        tessellation::tessellate_solid(truck_solid, tolerance, &mut self.next_id, solid)
    }

    fn extract_edges(
        &mut self,
        solid: &KernelSolidHandle,
        tolerance: f64,
    ) -> Result<EdgeRenderData, KernelError> {
        let truck_solid = self
            .solids
            .get(&solid.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(solid.id()),
            })?;

        Ok(tessellation::extract_edges(
            truck_solid,
            tolerance,
            &mut self.next_id,
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
        let origin = Point3::new(plane_origin[0], plane_origin[1], plane_origin[2]);
        let normal = Vector3::new(plane_normal[0], plane_normal[1], plane_normal[2]).normalize();
        let x_axis = Vector3::new(plane_x_axis[0], plane_x_axis[1], plane_x_axis[2]).normalize();
        let y_axis = normal.cross(x_axis).normalize();

        let mut face_ids = Vec::new();

        for profile in profiles {
            let pts_3d: Vec<Point3> = profile
                .entity_ids
                .iter()
                .filter_map(|id| {
                    positions
                        .get(id)
                        .map(|&(u, v)| origin + x_axis * u + y_axis * v)
                })
                .collect();

            if pts_3d.len() < 3 {
                return Err(KernelError::Other {
                    message: "Profile has fewer than 3 points".to_string(),
                });
            }

            // Build wire from consecutive point pairs with shared vertices.
            // Create all vertices first so edges share endpoints.
            let n = pts_3d.len();
            let vertices: Vec<_> = pts_3d.iter().map(|&p| builder::vertex(p)).collect();
            let mut wire_edges: Vec<Edge> = Vec::new();
            for i in 0..n {
                let j = (i + 1) % n;
                let edge = Edge::new(
                    &vertices[i],
                    &vertices[j],
                    truck_modeling::geometry::Curve::Line(truck_modeling::geometry::Line(
                        pts_3d[i], pts_3d[j],
                    )),
                );
                wire_edges.push(edge);
            }
            let wire = Wire::from_iter(wire_edges);

            let face = builder::try_attach_plane(&[wire]).map_err(|e| KernelError::Other {
                message: format!("Failed to create planar face: {}", e),
            })?;

            let face_id = self.alloc_id();
            self.standalone_faces.insert(face_id.0, face);
            face_ids.push(face_id);
        }

        Ok(face_ids)
    }

    fn export_step(
        &mut self,
        solid: &KernelSolidHandle,
        file_name: &str,
    ) -> Result<String, KernelError> {
        TruckKernel::export_step(self, solid, file_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives;

    #[test]
    fn test_truck_kernel_make_faces_and_extrude() {
        let mut kernel = TruckKernel::new();

        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        positions.insert(3, (1.0, 1.0));
        positions.insert(4, (0.0, 1.0));

        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();

        assert_eq!(face_ids.len(), 1);

        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], 2.0)
            .unwrap();

        let solid = kernel.get_solid(&handle).unwrap();
        let boundaries = solid.boundaries();
        assert_eq!(boundaries.len(), 1);

        let shell = &boundaries[0];
        let faces: Vec<_> = shell.face_iter().collect();
        assert_eq!(faces.len(), 6, "Extruded rectangle should have 6 faces");
    }

    /// Verify box-cylinder boolean subtract (punched cube).
    /// The cylinder must pierce through the box (not at edges/corners/coplanar faces).
    #[test]
    fn test_boolean_subtract_box_cylinder() {
        use truck_modeling::builder;

        // Cube [0,1]^3
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = builder::tsweep(&v, Vector3::unit_x());
        let f = builder::tsweep(&e, Vector3::unit_y());
        let cube: Solid = builder::tsweep(&f, Vector3::unit_z());

        // Cylinder centered at (0.5, 0.5), r=0.25, extends z=-0.5 to z=1.5
        // (fully pierces top and bottom faces, well inside edges)
        let v = builder::vertex(Point3::new(0.5, 0.25, -0.5));
        let w = builder::rsweep(&v, Point3::new(0.5, 0.5, 0.0), Vector3::unit_z(), Rad(7.0));
        let f = builder::try_attach_plane(&[w]).unwrap();
        let mut cylinder = builder::tsweep(&f, Vector3::unit_z() * 2.0);
        cylinder.not();

        let result = truck_shapeops::and(&cube, &cylinder, 0.05);
        assert!(result.is_some(), "Box-cylinder boolean should succeed");

        let solid = result.unwrap();
        let shell = &solid.boundaries()[0];
        use truck_topology::shell::ShellCondition;
        assert_eq!(
            shell.shell_condition(),
            ShellCondition::Closed,
            "Result shell must be Closed"
        );
    }

    /// Verify box-box boolean operations with offset (non-coplanar faces).
    #[test]
    fn test_boolean_ops_box_box_offset() {
        use truck_modeling::builder;
        use truck_topology::shell::ShellCondition;

        let box_a: Solid = {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(2.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 2.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 2.0))
        };
        let box_b: Solid = {
            let v = builder::vertex(Point3::new(0.5, 0.5, 0.5));
            let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0))
        };

        // AND (intersection)
        let result = truck_shapeops::and(&box_a, &box_b, 0.05);
        assert!(result.is_some(), "Box-box AND should succeed");
        let solid = result.unwrap();
        assert_eq!(
            solid.boundaries()[0].shell_condition(),
            ShellCondition::Closed
        );

        // OR (union)
        let result = truck_shapeops::or(&box_a, &box_b, 0.05);
        assert!(result.is_some(), "Box-box OR should succeed");
        let solid = result.unwrap();
        assert_eq!(
            solid.boundaries()[0].shell_condition(),
            ShellCondition::Closed
        );

        // Subtract (AND with NOT)
        let mut box_b_neg = box_b.clone();
        box_b_neg.not();
        let result = truck_shapeops::and(&box_a, &box_b_neg, 0.05);
        assert!(result.is_some(), "Box-box SUBTRACT should succeed");
        let solid = result.unwrap();
        assert_eq!(
            solid.boundaries()[0].shell_condition(),
            ShellCondition::Closed
        );
    }

    #[test]
    fn test_adaptive_tolerance_scaling() {
        // Test that compute_adaptive_tol produces scale-appropriate values
        let make_box = |s: f64| -> Solid {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(s, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, s, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, s))
        };

        // Unit scale: extent=1.0, tol = 1.0*0.005 = 0.005
        let small = make_box(1.0);
        let tol_small = compute_adaptive_tol(&small, &small);
        assert!(
            (tol_small - 0.005).abs() < 1e-10,
            "Unit scale tol={} should be 0.005",
            tol_small
        );

        // 10x scale: extent=10.0, tol = 10*0.005 = 0.05 (at upper clamp)
        let medium = make_box(10.0);
        let tol_medium = compute_adaptive_tol(&medium, &medium);
        assert!(
            (tol_medium - 0.05).abs() < 1e-10,
            "10x scale tol={} should be 0.05 (upper clamp)",
            tol_medium
        );

        // 100x scale: extent=100.0, tol = 0.05 (clamped at upper bound)
        let large = make_box(100.0);
        let tol_large = compute_adaptive_tol(&large, &large);
        assert!(
            (tol_large - 0.05).abs() < 1e-10,
            "100x scale tol={} should be 0.05 (clamped)",
            tol_large
        );

        // Verify monotonicity up to clamp
        assert!(
            tol_medium >= tol_small,
            "Medium geometry should get >= tol than small"
        );
        assert!(
            tol_large >= tol_medium,
            "Large should be >= medium (both at clamp)"
        );
    }

    #[test]
    fn test_parametric_scale_boolean() {
        use truck_topology::shell::ShellCondition;

        // Test that boolean union works at various scales with adaptive tolerance.
        // Uses overlapping boxes (not fully contained) to ensure proper intersection
        // curves are generated.
        for scale in &[1.0, 2.0, 5.0, 10.0] {
            let s = *scale;
            // Box A: [0, 2s]^3
            let box_a = {
                let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
                let e = builder::tsweep(&v, Vector3::new(2.0 * s, 0.0, 0.0));
                let f = builder::tsweep(&e, Vector3::new(0.0, 2.0 * s, 0.0));
                builder::tsweep(&f, Vector3::new(0.0, 0.0, 2.0 * s))
            };
            // Box B: [s, 3s]^3 — overlapping (no coplanar, partial overlap)
            let box_b = {
                let v = builder::vertex(Point3::new(1.0 * s, 1.0 * s, 1.0 * s));
                let e = builder::tsweep(&v, Vector3::new(2.0 * s, 0.0, 0.0));
                let f = builder::tsweep(&e, Vector3::new(0.0, 2.0 * s, 0.0));
                builder::tsweep(&f, Vector3::new(0.0, 0.0, 2.0 * s))
            };

            let tol = compute_adaptive_tol(&box_a, &box_b);
            let result = truck_shapeops::or(&box_a, &box_b, tol);
            assert!(
                result.is_some(),
                "Box-box union at scale {}x should succeed (tol={})",
                s,
                tol
            );
            let solid = result.unwrap();
            assert_eq!(
                solid.boundaries()[0].shell_condition(),
                ShellCondition::Closed,
                "Union at scale {}x should produce closed shell",
                s
            );
        }
    }

    // Coplanar partial overlap: pair-specific skip allows non-coplanar pairs to intersect normally
    #[test]
    fn test_coplanar_partial_overlap_union() {
        use truck_topology::shell::ShellCondition;

        // Box1: [0,1]^3
        let box1 = {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0))
        };
        // Box2: [0.5, 1.5] x [0, 1] x [0, 1] — shares the x=1 face partially
        let box2 = {
            let v = builder::vertex(Point3::new(0.5, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
            builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0))
        };

        let tol = compute_adaptive_tol(&box1, &box2);
        // Use perturbation system since direct truck call fails on coplanar faces
        let result = crate::healing::try_boolean_with_perturbation(&box1, &box2, tol, |a, b| {
            truck_shapeops::or(a, b, tol)
        });
        assert!(
            result.is_some(),
            "Coplanar partial overlap union should succeed"
        );
        let solid = result.unwrap();
        assert_eq!(
            solid.boundaries()[0].shell_condition(),
            ShellCondition::Closed,
            "Coplanar union should produce closed shell"
        );
    }

    /// Benchmark boolean operations at various tolerances.
    /// Uses catch_unwind since truck panics at certain tolerances.
    /// Run with: cargo test -p kernel-fork bench_boolean -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bench_boolean_tolerances() {
        let tolerances = [0.05, 0.1, 0.2, 0.5];
        let box_solid = primitives::make_box(2.0, 2.0, 2.0);
        let cyl_solid = primitives::make_cylinder(0.5, 3.0);

        println!("\n=== Boolean: Box(2x2x2) vs Cylinder(r=0.5, h=3) ===");

        for (name, op_name) in [("union", "or"), ("subtract", "sub"), ("intersect", "and")] {
            println!("--- {} ---", name);
            for tol in &tolerances {
                let a = box_solid.clone();
                let b = cyl_solid.clone();
                let t = *tol;
                let start = std::time::Instant::now();
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match op_name {
                        "or" => truck_shapeops::or(&a, &b, t),
                        "sub" => {
                            let mut b2 = b;
                            b2.not();
                            truck_shapeops::and(&a, &b2, t)
                        }
                        "and" => truck_shapeops::and(&a, &b, t),
                        _ => unreachable!(),
                    }));
                let elapsed = start.elapsed();
                let status = match &result {
                    Ok(Some(_)) => "OK",
                    Ok(None) => "FAILED (None)",
                    Err(_) => "PANIC",
                };
                println!("  tol={:.2} → {:?} [{}]", tol, elapsed, status);
            }
        }

        // Box-box booleans with coplanar faces (shared origin)
        println!("\n=== Box-Box Booleans (coplanar, origin-aligned, tol=0.05) ===");
        let box_a = primitives::make_box(2.0, 2.0, 2.0);
        let box_b = primitives::make_box(1.0, 1.0, 1.0);

        for (name, op_name) in [("union", "or"), ("subtract", "sub"), ("intersect", "and")] {
            let a = box_a.clone();
            let b = box_b.clone();
            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match op_name {
                "or" => truck_shapeops::or(&a, &b, 0.05),
                "sub" => {
                    let mut b2 = b;
                    b2.not();
                    truck_shapeops::and(&a, &b2, 0.05)
                }
                "and" => truck_shapeops::and(&a, &b, 0.05),
                _ => unreachable!(),
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(_)) => "OK",
                Ok(None) => "FAILED (None)",
                Err(_) => "PANIC",
            };
            println!("  {} → {:?} [{}]", name, elapsed, status);
        }

        // Box-box with offset (no coplanar faces)
        println!("\n=== Box-Box Booleans (offset, no coplanar faces, tol=0.05) ===");
        // Create offset box by extruding a face at (0.5, 0.5, 0.5)
        let box_a2 = primitives::make_box(2.0, 2.0, 2.0);
        // Build an offset box manually using tsweep from an offset position
        let v = truck_modeling::builder::vertex(Point3::new(0.5, 0.5, 0.5));
        let e = truck_modeling::builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
        let f = truck_modeling::builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
        let box_b2: Solid = truck_modeling::builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));

        for (name, op_name) in [("union", "or"), ("subtract", "sub"), ("intersect", "and")] {
            let a = box_a2.clone();
            let b = box_b2.clone();
            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match op_name {
                "or" => truck_shapeops::or(&a, &b, 0.05),
                "sub" => {
                    let mut b2 = b;
                    b2.not();
                    truck_shapeops::and(&a, &b2, 0.05)
                }
                "and" => truck_shapeops::and(&a, &b, 0.05),
                _ => unreachable!(),
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(_)) => "OK",
                Ok(None) => "FAILED (None)",
                Err(_) => "PANIC",
            };
            println!("  {} → {:?} [{}]", name, elapsed, status);
        }
    }

    /// Diagnostic: test boolean ops with different cylinder placements to isolate
    /// which geometric configurations work vs. fail.
    /// Run with: cargo test -p kernel-fork diag_boolean_configs -- --ignored --nocapture
    #[test]
    #[ignore]
    fn diag_boolean_configs() {
        use truck_modeling::builder;

        println!("\n=== Diagnostic: Boolean Operation Configurations ===\n");

        // Config 1: Punched cube (truck's own test case — cylinder fully inside box)
        {
            let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
            let e = builder::tsweep(&v, Vector3::unit_x());
            let f = builder::tsweep(&e, Vector3::unit_y());
            let cube: Solid = builder::tsweep(&f, Vector3::unit_z());

            let v = builder::vertex(Point3::new(0.5, 0.25, -0.5));
            let w = builder::rsweep(&v, Point3::new(0.5, 0.5, 0.0), Vector3::unit_z(), Rad(7.0));
            let f = builder::try_attach_plane(&[w]).unwrap();
            let mut cylinder = builder::tsweep(&f, Vector3::unit_z() * 2.0);
            cylinder.not();

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&cube, &cylinder, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!(
                "1. Punched cube (cyl inside box): {:?} [{}]",
                elapsed, status
            );
        }

        // Config 2: Cylinder centered in box face (partially overlapping)
        {
            let cube = primitives::make_box(2.0, 2.0, 2.0);
            // Cylinder at center of box, radius 0.5, extends through
            let v = builder::vertex(Point3::new(1.5, 1.0, -0.5));
            let w = builder::rsweep(&v, Point3::new(1.0, 1.0, 0.0), Vector3::unit_z(), Rad(7.0));
            let f = builder::try_attach_plane(&[w]).unwrap();
            let cylinder: Solid = builder::tsweep(&f, Vector3::unit_z() * 3.0);

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&cube, &cylinder, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!("2. Cylinder at box center: {:?} [{}]", elapsed, status);
        }

        // Config 3: Our original test — cylinder at origin corner
        {
            let cube = primitives::make_box(2.0, 2.0, 2.0);
            let cylinder = primitives::make_cylinder(0.5, 3.0);

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&cube, &cylinder, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!(
                "3. Cylinder at box corner (original): {:?} [{}]",
                elapsed, status
            );
        }

        // Config 4: Cylinder centered (2pi), z-offset to avoid coplanar bottom
        {
            let cube = primitives::make_box(2.0, 2.0, 2.0);
            // Build cylinder at (1,1,-0.5) with r=0.3, h=3 — NO coplanar faces
            let v = builder::vertex(Point3::new(1.3, 1.0, -0.5));
            let w = builder::rsweep(
                &v,
                Point3::new(1.0, 1.0, 0.0),
                Vector3::unit_z(),
                Rad(2.0 * std::f64::consts::PI),
            );
            let f = builder::try_attach_plane(&[w]).unwrap();
            let cylinder: Solid = builder::tsweep(&f, Vector3::unit_z() * 3.0);

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&cube, &cylinder, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!(
                "4. Cylinder centered (2pi), inside box: {:?} [{}]",
                elapsed, status
            );
        }

        // Config 5: Cylinder intersecting box face (partially in, partially out)
        {
            let cube = primitives::make_box(2.0, 2.0, 2.0);
            // Cylinder at (1,2,0) — half inside, half outside the y=2 face
            let v = builder::vertex(Point3::new(1.5, 2.0, -0.5));
            let w = builder::rsweep(&v, Point3::new(1.0, 2.0, 0.0), Vector3::unit_z(), Rad(7.0));
            let f = builder::try_attach_plane(&[w]).unwrap();
            let cylinder: Solid = builder::tsweep(&f, Vector3::unit_z() * 3.0);

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&cube, &cylinder, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!("5. Cylinder at face boundary: {:?} [{}]", elapsed, status);
        }

        // Config 6: Box-box fully overlapping (offset)
        {
            let box_a = primitives::make_box(2.0, 2.0, 2.0);
            let v = builder::vertex(Point3::new(0.5, 0.5, 0.5));
            let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
            let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
            let box_b: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));

            let start = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                truck_shapeops::and(&box_a, &box_b, 0.05)
            }));
            let elapsed = start.elapsed();
            let status = match &result {
                Ok(Some(solid)) => {
                    let cond = solid.boundaries()[0].shell_condition();
                    format!("OK (shell: {:?})", cond)
                }
                Ok(None) => "FAILED (None)".to_string(),
                Err(_) => "PANIC".to_string(),
            };
            println!("6. Box-box offset (intersect): {:?} [{}]", elapsed, status);
        }
    }

    /// Test export_step via the Kernel trait method.
    #[test]
    fn test_export_step_via_trait() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 2.0, 3.0);
        let handle = kernel.store_solid(solid);

        let step_data = kernel.export_step(&handle, "trait_test.step").unwrap();

        assert!(
            step_data.contains("ISO-10303-21"),
            "Should have STEP header"
        );
        assert!(
            step_data.contains("MANIFOLD_SOLID_BREP"),
            "Should have solid BREP entity"
        );
    }

    /// STEP export investigation: simple box (should work)
    #[test]
    fn test_step_export_simple_box() {
        use truck_stepio::out::*;
        use truck_topology::compress::CompressedSolid;

        let solid = primitives::make_box(1.0, 2.0, 3.0);
        let compressed: CompressedSolid<_, _, _> = solid.compress();
        let step_model = StepModel::from(&compressed);
        let header = StepHeaderDescriptor {
            file_name: "test_box.step".to_string(),
            ..Default::default()
        };
        let complete = CompleteStepDisplay::new(step_model, header);
        let step_string = complete.to_string();

        assert!(
            step_string.contains("ISO-10303-21"),
            "Should have STEP header"
        );
        assert!(
            step_string.contains("MANIFOLD_SOLID_BREP"),
            "Should have solid BREP entity"
        );
        assert!(
            step_string.contains("FACE_SURFACE"),
            "Should have face entities"
        );
        assert!(
            step_string.contains("ENDSEC"),
            "Should have proper STEP footer"
        );
    }

    /// STEP export investigation: cylinder (revolved geometry)
    #[test]
    fn test_step_export_cylinder() {
        use truck_stepio::out::*;
        use truck_topology::compress::CompressedSolid;

        let solid = primitives::make_cylinder(1.0, 2.0);
        let compressed: CompressedSolid<_, _, _> = solid.compress();
        let step_model = StepModel::from(&compressed);
        let header = StepHeaderDescriptor {
            file_name: "test_cylinder.step".to_string(),
            ..Default::default()
        };
        let complete = CompleteStepDisplay::new(step_model, header);
        let step_string = complete.to_string();

        assert!(
            step_string.contains("ISO-10303-21"),
            "Should have STEP header"
        );
        assert!(
            step_string.contains("MANIFOLD_SOLID_BREP"),
            "Should have solid BREP entity"
        );
    }

    /// STEP export of a boolean union result via the TruckKernel API.
    /// The kernel auto-heals IntersectionCurve edges, making STEP export safe.
    #[test]
    fn test_step_export_boolean_result() {
        let mut kernel = TruckKernel::new();

        // Create two offset boxes and store them in the kernel
        let box_a = primitives::make_box(2.0, 2.0, 2.0);
        let handle_a = kernel.store_solid(box_a);

        let v = builder::vertex(Point3::new(0.5, 0.5, 0.5));
        let e = builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
        let f = builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
        let box_b: Solid = builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));
        let handle_b = kernel.store_solid(box_b);

        // Boolean union via kernel (auto-heals intersection curves)
        let union_handle = kernel
            .boolean_union(&handle_a, &handle_b)
            .expect("boolean union should succeed for offset boxes");

        // Export to STEP via kernel
        let step_string = kernel
            .export_step(&union_handle, "test_boolean.step")
            .expect("STEP export of healed boolean result should succeed");

        assert!(
            step_string.contains("ISO-10303-21"),
            "Should have STEP header"
        );
        assert!(
            step_string.contains("MANIFOLD_SOLID_BREP"),
            "Should have solid BREP entity"
        );
    }

    /// Face IDs from introspection match tessellation for make_faces_from_profiles → extrude_face.
    /// This is the realistic app path (not primitives::make_box).
    #[test]
    fn test_extruded_rect_face_id_consistency() {
        use crate::traits::KernelIntrospect;

        let mut kernel = TruckKernel::new();

        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (10.0, 0.0));
        positions.insert(3, (10.0, 10.0));
        positions.insert(4, (0.0, 10.0));

        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();

        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], 5.0)
            .unwrap();

        let introspect_ids: std::collections::HashSet<_> =
            kernel.list_faces(&handle).into_iter().collect();
        let mesh = kernel.tessellate(&handle, 0.1).unwrap();
        let tess_ids: std::collections::HashSet<_> =
            mesh.face_ranges.iter().map(|fr| fr.face_id).collect();

        assert_eq!(
            introspect_ids, tess_ids,
            "Face IDs must match for extruded rectangle"
        );
        assert_eq!(introspect_ids.len(), 6, "Extruded rect should have 6 faces");
    }

    /// GAP K6: export_step with nonexistent handle returns EntityNotFound.
    #[test]
    fn test_export_step_nonexistent_handle() {
        let kernel = TruckKernel::new();
        let bad_handle = KernelSolidHandle(99999);
        let result = kernel.export_step(&bad_handle, "nonexistent.step");
        assert!(
            matches!(result, Err(KernelError::EntityNotFound { .. })),
            "export_step with bad handle should return EntityNotFound, got {:?}",
            result
        );
    }

    /// GAP K7: extrude result must have ShellCondition::Closed.
    #[test]
    fn test_extrude_shell_condition_closed() {
        use truck_topology::shell::ShellCondition;

        let mut kernel = TruckKernel::new();

        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (2.0, 0.0));
        positions.insert(3, (2.0, 2.0));
        positions.insert(4, (0.0, 2.0));

        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();

        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], 3.0)
            .unwrap();

        let solid = kernel.get_solid(&handle).unwrap();
        let shell = &solid.boundaries()[0];
        assert_eq!(
            shell.shell_condition(),
            ShellCondition::Closed,
            "Extruded solid must have a closed (watertight) shell"
        );
    }

    /// Tessellation from TruckKernel must have all-finite vertices and normals.
    #[test]
    fn test_truck_tessellation_no_nan() {
        let mut kernel = TruckKernel::new();

        // Box
        let box_solid = primitives::make_box(1.0, 2.0, 3.0);
        let h_box = kernel.store_solid(box_solid);
        let mesh_box = kernel.tessellate(&h_box, 0.1).unwrap();

        for (i, v) in mesh_box.vertices.iter().enumerate() {
            assert!(v.is_finite(), "Box vertex[{}] = {} is not finite", i, v);
        }
        for (i, n) in mesh_box.normals.iter().enumerate() {
            assert!(n.is_finite(), "Box normal[{}] = {} is not finite", i, n);
        }

        // Cylinder
        let cyl_solid = primitives::make_cylinder(1.0, 2.0);
        let h_cyl = kernel.store_solid(cyl_solid);
        let mesh_cyl = kernel.tessellate(&h_cyl, 0.1).unwrap();

        for (i, v) in mesh_cyl.vertices.iter().enumerate() {
            assert!(
                v.is_finite(),
                "Cylinder vertex[{}] = {} is not finite",
                i,
                v
            );
        }
        for (i, n) in mesh_cyl.normals.iter().enumerate() {
            assert!(
                n.is_finite(),
                "Cylinder normal[{}] = {} is not finite",
                i,
                n
            );
        }
    }

    /// Tessellation triangles from TruckKernel must not have zero area.
    #[test]
    fn test_truck_tessellation_no_zero_area_triangles() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 2.0, 3.0);
        let handle = kernel.store_solid(solid);
        let mesh = kernel.tessellate(&handle, 0.1).unwrap();

        let epsilon = 1e-12_f32;
        assert_eq!(mesh.indices.len() % 3, 0);
        for tri in mesh.indices.chunks(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;

            let v0 = [
                mesh.vertices[i0 * 3],
                mesh.vertices[i0 * 3 + 1],
                mesh.vertices[i0 * 3 + 2],
            ];
            let v1 = [
                mesh.vertices[i1 * 3],
                mesh.vertices[i1 * 3 + 1],
                mesh.vertices[i1 * 3 + 2],
            ];
            let v2 = [
                mesh.vertices[i2 * 3],
                mesh.vertices[i2 * 3 + 1],
                mesh.vertices[i2 * 3 + 2],
            ];

            let e1 = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
            let e2 = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

            let cx = e1[1] * e2[2] - e1[2] * e2[1];
            let cy = e1[2] * e2[0] - e1[0] * e2[2];
            let cz = e1[0] * e2[1] - e1[1] * e2[0];
            let area_2x = (cx * cx + cy * cy + cz * cz).sqrt();

            assert!(
                area_2x > epsilon,
                "Triangle [{}, {}, {}] has zero area (2*area={})",
                i0,
                i1,
                i2,
                area_2x
            );
        }
    }

    #[test]
    fn test_truck_kernel_store_and_tessellate_box() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let mesh = kernel.tessellate(&handle, 0.1).unwrap();

        assert!(!mesh.vertices.is_empty(), "Mesh should have vertices");
        assert!(!mesh.indices.is_empty(), "Mesh should have indices");
        assert!(!mesh.normals.is_empty(), "Mesh should have normals");
        assert_eq!(mesh.face_ranges.len(), 6, "Box should have 6 face ranges");

        let total_indices = mesh.indices.len() as u32;
        let covered: u32 = mesh
            .face_ranges
            .iter()
            .map(|r| r.end_index - r.start_index)
            .sum();
        assert_eq!(
            covered, total_indices,
            "Face ranges should cover all indices"
        );
    }

    #[test]
    fn test_truck_kernel_extract_edges_box() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);

        let edges = kernel.extract_edges(&handle, 0.1).unwrap();

        assert!(!edges.vertices.is_empty(), "Edge data should have vertices");
        // A box has 12 edges
        assert_eq!(
            edges.edge_ranges.len(),
            12,
            "Box should have 12 edge ranges"
        );

        // Verify each edge range references valid vertex data
        for range in &edges.edge_ranges {
            assert!(
                range.end_vertex > range.start_vertex,
                "Edge range should have at least 2 vertices"
            );
            assert!(
                (range.end_vertex as usize) * 3 <= edges.vertices.len(),
                "Edge range end should not exceed vertex count"
            );
        }
    }

    #[test]
    fn test_truck_kernel_extract_edges_cylinder() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);

        let edges = kernel.extract_edges(&handle, 0.1).unwrap();

        assert!(
            !edges.vertices.is_empty(),
            "Cylinder edge data should have vertices"
        );
        // A cylinder has 3 edges: top circle, bottom circle, seam
        assert!(
            edges.edge_ranges.len() >= 3,
            "Cylinder should have at least 3 edge ranges, got {}",
            edges.edge_ranges.len()
        );
    }

    // ── Coverage: error paths ────────────────────────────────────────

    /// TruckKernel::default() produces the same state as new().
    #[test]
    fn test_truck_kernel_default() {
        let k1 = TruckKernel::new();
        let k2 = TruckKernel::default();
        // Both should have empty solid storage
        let bad = KernelSolidHandle(1);
        assert!(k1.get_solid(&bad).is_none());
        assert!(k2.get_solid(&bad).is_none());
    }

    /// extrude_face on a nonexistent face returns EntityNotFound.
    #[test]
    fn test_extrude_face_not_found() {
        let mut kernel = TruckKernel::new();
        let result = kernel.extrude_face(KernelId(999), [0.0, 0.0, 1.0], 1.0);
        assert!(
            matches!(result, Err(KernelError::EntityNotFound { .. })),
            "Expected EntityNotFound, got {:?}",
            result
        );
    }

    /// extrude_face with zero-length direction returns Other error.
    #[test]
    fn test_extrude_zero_direction() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        positions.insert(3, (0.5, 1.0));
        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();
        let result = kernel.extrude_face(face_ids[0], [0.0, 0.0, 0.0], 1.0);
        assert!(
            matches!(result, Err(KernelError::Other { .. })),
            "Expected Other error for zero direction, got {:?}",
            result
        );
    }

    /// revolve_face basic: create a face and revolve it.
    #[test]
    fn test_revolve_face_basic() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        // Small rectangle offset from Y axis for revolve
        positions.insert(1, (1.0, 0.0));
        positions.insert(2, (2.0, 0.0));
        positions.insert(3, (2.0, 1.0));
        positions.insert(4, (1.0, 1.0));

        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();

        let handle = kernel
            .revolve_face(
                face_ids[0],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                std::f64::consts::PI,
            )
            .unwrap();

        let solid = kernel.get_solid(&handle).unwrap();
        assert!(
            !solid.boundaries().is_empty(),
            "Revolved solid should have at least one shell"
        );
    }

    /// revolve_face on nonexistent face returns EntityNotFound.
    #[test]
    fn test_revolve_face_not_found() {
        let mut kernel = TruckKernel::new();
        let result = kernel.revolve_face(
            KernelId(999),
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::PI,
        );
        assert!(
            matches!(result, Err(KernelError::EntityNotFound { .. })),
            "Expected EntityNotFound, got {:?}",
            result
        );
    }

    /// revolve_face with zero-length axis returns Other error.
    #[test]
    fn test_revolve_zero_axis() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        positions.insert(1, (1.0, 0.0));
        positions.insert(2, (2.0, 0.0));
        positions.insert(3, (1.5, 1.0));
        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();
        let result = kernel.revolve_face(
            face_ids[0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0], // zero axis
            std::f64::consts::PI,
        );
        assert!(
            matches!(result, Err(KernelError::Other { .. })),
            "Expected Other error for zero axis, got {:?}",
            result
        );
    }

    /// boolean_union with nonexistent handle A returns EntityNotFound.
    #[test]
    fn test_boolean_union_not_found_a() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let h_b = kernel.store_solid(solid);
        let bad = KernelSolidHandle(999);
        let result = kernel.boolean_union(&bad, &h_b);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// boolean_union with nonexistent handle B returns EntityNotFound.
    #[test]
    fn test_boolean_union_not_found_b() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let h_a = kernel.store_solid(solid);
        let bad = KernelSolidHandle(999);
        let result = kernel.boolean_union(&h_a, &bad);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// boolean_subtract with nonexistent handles returns EntityNotFound.
    #[test]
    fn test_boolean_subtract_not_found() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let h_a = kernel.store_solid(solid);
        let bad = KernelSolidHandle(999);
        // Missing A
        let result = kernel.boolean_subtract(&bad, &h_a);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
        // Missing B
        let result = kernel.boolean_subtract(&h_a, &bad);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// boolean_intersect with nonexistent handles returns EntityNotFound.
    #[test]
    fn test_boolean_intersect_not_found() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let h_a = kernel.store_solid(solid);
        let bad = KernelSolidHandle(999);
        let result = kernel.boolean_intersect(&bad, &h_a);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
        let result = kernel.boolean_intersect(&h_a, &bad);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// fillet_edges returns NotSupported on TruckKernel.
    #[test]
    fn test_fillet_returns_not_supported() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);
        let result = kernel.fillet_edges(&handle, &[KernelId(1)], 0.1);
        assert!(
            matches!(result, Err(KernelError::NotSupported { .. })),
            "fillet_edges should return NotSupported, got {:?}",
            result
        );
        if let Err(KernelError::NotSupported { operation }) = result {
            assert_eq!(operation, "fillet_edges");
        }
    }

    /// chamfer_edges returns NotSupported on TruckKernel.
    #[test]
    fn test_chamfer_returns_not_supported() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);
        let result = kernel.chamfer_edges(&handle, &[KernelId(1)], 0.1);
        assert!(
            matches!(result, Err(KernelError::NotSupported { .. })),
            "chamfer_edges should return NotSupported, got {:?}",
            result
        );
        if let Err(KernelError::NotSupported { operation }) = result {
            assert_eq!(operation, "chamfer_edges");
        }
    }

    /// shell returns NotSupported on TruckKernel.
    #[test]
    fn test_shell_returns_not_supported() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_box(1.0, 1.0, 1.0);
        let handle = kernel.store_solid(solid);
        let result = kernel.shell(&handle, &[KernelId(1)], 0.1);
        assert!(
            matches!(result, Err(KernelError::NotSupported { .. })),
            "shell should return NotSupported, got {:?}",
            result
        );
        if let Err(KernelError::NotSupported { operation }) = result {
            assert_eq!(operation, "shell");
        }
    }

    /// tessellate with nonexistent handle returns EntityNotFound.
    #[test]
    fn test_tessellate_not_found() {
        let mut kernel = TruckKernel::new();
        let bad = KernelSolidHandle(999);
        let result = kernel.tessellate(&bad, 0.1);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// extract_edges with nonexistent handle returns EntityNotFound.
    #[test]
    fn test_extract_edges_not_found() {
        let mut kernel = TruckKernel::new();
        let bad = KernelSolidHandle(999);
        let result = kernel.extract_edges(&bad, 0.1);
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// make_faces_from_profiles with fewer than 3 points returns error.
    #[test]
    fn test_make_faces_too_few_points() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        let result = kernel.make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        );
        assert!(
            matches!(result, Err(KernelError::Other { .. })),
            "Expected error for fewer than 3 points, got {:?}",
            result
        );
    }

    /// make_faces_from_profiles with multiple profiles.
    #[test]
    fn test_make_faces_multiple_profiles() {
        let mut kernel = TruckKernel::new();
        let p1 = ClosedProfile {
            entity_ids: vec![1, 2, 3],
            is_outer: true,
        };
        let p2 = ClosedProfile {
            entity_ids: vec![4, 5, 6],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        positions.insert(3, (0.5, 1.0));
        positions.insert(4, (3.0, 0.0));
        positions.insert(5, (4.0, 0.0));
        positions.insert(6, (3.5, 1.0));
        let face_ids = kernel
            .make_faces_from_profiles(
                &[p1, p2],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();
        assert_eq!(face_ids.len(), 2, "Should create 2 faces from 2 profiles");
        assert_ne!(face_ids[0], face_ids[1], "Face IDs should be distinct");
    }

    /// make_faces_from_profiles with triangle profile, then extrude to wedge.
    #[test]
    fn test_make_faces_triangle_extrude() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (2.0, 0.0));
        positions.insert(3, (1.0, 2.0));
        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();
        assert_eq!(face_ids.len(), 1);
        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 0.0, 1.0], 3.0)
            .unwrap();
        let solid = kernel.get_solid(&handle).unwrap();
        let shell = &solid.boundaries()[0];
        let faces: Vec<_> = shell.face_iter().collect();
        // Triangular prism: 2 triangles + 3 quads = 5 faces
        assert_eq!(faces.len(), 5, "Triangular extrusion should have 5 faces");
    }

    /// make_faces_from_profiles on XZ plane (non-default normal).
    #[test]
    fn test_make_faces_xz_plane() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        positions.insert(3, (1.0, 1.0));
        positions.insert(4, (0.0, 1.0));
        let face_ids = kernel
            .make_faces_from_profiles(
                &[profile],
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0], // XZ plane normal = Y
                [1.0, 0.0, 0.0],
                &positions,
            )
            .unwrap();
        assert_eq!(face_ids.len(), 1);
        // Extrude along Y
        let handle = kernel
            .extrude_face(face_ids[0], [0.0, 1.0, 0.0], 2.0)
            .unwrap();
        let solid = kernel.get_solid(&handle).unwrap();
        let shell = &solid.boundaries()[0];
        let faces: Vec<_> = shell.face_iter().collect();
        assert_eq!(faces.len(), 6, "XZ-plane extruded box should have 6 faces");
    }

    /// Tessellation of a cylinder covers all faces.
    #[test]
    fn test_tessellate_cylinder_face_ranges() {
        use crate::traits::KernelIntrospect;

        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);

        let mesh = kernel.tessellate(&handle, 0.1).unwrap();
        let introspect_ids: std::collections::HashSet<_> =
            kernel.list_faces(&handle).into_iter().collect();

        // Every face range face_id should be in the introspect list
        for fr in &mesh.face_ranges {
            assert!(
                introspect_ids.contains(&fr.face_id),
                "Tessellation face_id {:?} not in introspect list",
                fr.face_id
            );
        }

        // Face ranges should cover all indices contiguously
        let total_indices = mesh.indices.len() as u32;
        let covered: u32 = mesh
            .face_ranges
            .iter()
            .map(|r| r.end_index - r.start_index)
            .sum();
        assert_eq!(
            covered, total_indices,
            "Face ranges should cover all indices"
        );
    }

    /// export_step (inherent method) on nonexistent solid returns EntityNotFound.
    #[test]
    fn test_export_step_inherent_not_found() {
        let kernel = TruckKernel::new();
        let bad = KernelSolidHandle(999);
        let result = TruckKernel::export_step(&kernel, &bad, "test.step");
        assert!(matches!(result, Err(KernelError::EntityNotFound { .. })));
    }

    /// Verify STEP export content has expected structure with file name.
    #[test]
    fn test_export_step_content_depth() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);
        let step_data = kernel.export_step(&handle, "cyl_test.step").unwrap();

        assert!(step_data.contains("ISO-10303-21"));
        assert!(step_data.contains("MANIFOLD_SOLID_BREP"));
        assert!(step_data.contains("DATA"));
        assert!(step_data.contains("ENDSEC"));
        // File name should appear in header
        assert!(
            step_data.contains("cyl_test.step"),
            "STEP header should contain the file name"
        );
    }

    /// Boolean union via Kernel trait (not raw truck_shapeops).
    #[test]
    fn test_boolean_union_via_kernel_trait() {
        let mut kernel = TruckKernel::new();
        let box_a = primitives::make_box(2.0, 2.0, 2.0);
        let h_a = kernel.store_solid(box_a);

        // Offset box B
        let v = truck_modeling::builder::vertex(Point3::new(0.5, 0.5, 0.5));
        let e = truck_modeling::builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
        let f = truck_modeling::builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
        let box_b: Solid = truck_modeling::builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));
        let h_b = kernel.store_solid(box_b);

        let h_result = kernel.boolean_union(&h_a, &h_b).unwrap();
        let mesh = kernel.tessellate(&h_result, 0.1).unwrap();
        assert!(
            !mesh.vertices.is_empty(),
            "Union result should be tessellatable"
        );
        assert!(
            !mesh.face_ranges.is_empty(),
            "Union should have face ranges"
        );
    }

    /// Boolean subtract via Kernel trait.
    #[test]
    fn test_boolean_subtract_via_kernel_trait() {
        let mut kernel = TruckKernel::new();

        // Cube [0,1]^3
        let v = truck_modeling::builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = truck_modeling::builder::tsweep(&v, Vector3::unit_x());
        let f = truck_modeling::builder::tsweep(&e, Vector3::unit_y());
        let cube: Solid = truck_modeling::builder::tsweep(&f, Vector3::unit_z());
        let h_a = kernel.store_solid(cube);

        // Cylinder fully inside cube
        let v = truck_modeling::builder::vertex(Point3::new(0.5, 0.25, -0.5));
        let w = truck_modeling::builder::rsweep(
            &v,
            Point3::new(0.5, 0.5, 0.0),
            Vector3::unit_z(),
            Rad(7.0),
        );
        let f = truck_modeling::builder::try_attach_plane(&[w]).unwrap();
        let cyl: Solid = truck_modeling::builder::tsweep(&f, Vector3::unit_z() * 2.0);
        let h_b = kernel.store_solid(cyl);

        let h_result = kernel.boolean_subtract(&h_a, &h_b).unwrap();
        let mesh = kernel.tessellate(&h_result, 0.1).unwrap();
        assert!(
            !mesh.vertices.is_empty(),
            "Subtract result should produce geometry"
        );
    }

    /// Boolean intersect via Kernel trait.
    #[test]
    fn test_boolean_intersect_via_kernel_trait() {
        let mut kernel = TruckKernel::new();
        let box_a = primitives::make_box(2.0, 2.0, 2.0);
        let h_a = kernel.store_solid(box_a);

        let v = truck_modeling::builder::vertex(Point3::new(0.5, 0.5, 0.5));
        let e = truck_modeling::builder::tsweep(&v, Vector3::new(1.0, 0.0, 0.0));
        let f = truck_modeling::builder::tsweep(&e, Vector3::new(0.0, 1.0, 0.0));
        let box_b: Solid = truck_modeling::builder::tsweep(&f, Vector3::new(0.0, 0.0, 1.0));
        let h_b = kernel.store_solid(box_b);

        let h_result = kernel.boolean_intersect(&h_a, &h_b).unwrap();
        let mesh = kernel.tessellate(&h_result, 0.1).unwrap();
        assert!(
            !mesh.vertices.is_empty(),
            "Intersect result should be tessellatable"
        );
    }

    /// make_faces_from_profiles with entity IDs not in positions map produces error.
    #[test]
    fn test_make_faces_missing_positions() {
        let mut kernel = TruckKernel::new();
        let profile = ClosedProfile {
            entity_ids: vec![1, 2, 3, 4],
            is_outer: true,
        };
        let mut positions = HashMap::new();
        // Only provide 2 positions out of 4
        positions.insert(1, (0.0, 0.0));
        positions.insert(2, (1.0, 0.0));
        let result = kernel.make_faces_from_profiles(
            &[profile],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
            &positions,
        );
        assert!(
            result.is_err(),
            "Should fail with fewer than 3 matched positions"
        );
    }

    /// alloc_handle and alloc_id produce monotonically increasing values.
    #[test]
    fn test_alloc_id_monotonic() {
        let mut kernel = TruckKernel::new();
        let id1 = kernel.alloc_id();
        let id2 = kernel.alloc_id();
        let id3 = kernel.alloc_id();
        assert!(id1.0 < id2.0);
        assert!(id2.0 < id3.0);

        let h1 = kernel.alloc_handle();
        let h2 = kernel.alloc_handle();
        assert!(h1.id() < h2.id());
    }

    /// Tessellate a revolved solid (cylinder) and verify no NaN/Inf.
    #[test]
    fn test_tessellate_cylinder_no_nan() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(0.5, 3.0);
        let handle = kernel.store_solid(solid);
        let mesh = kernel.tessellate(&handle, 0.1).unwrap();

        assert!(!mesh.vertices.is_empty());
        for (i, v) in mesh.vertices.iter().enumerate() {
            assert!(
                v.is_finite(),
                "Cylinder vertex[{}] = {} is not finite",
                i,
                v
            );
        }
        for (i, n) in mesh.normals.iter().enumerate() {
            assert!(
                n.is_finite(),
                "Cylinder normal[{}] = {} is not finite",
                i,
                n
            );
        }
        // Cylinder should have face ranges for all its faces
        assert!(
            mesh.face_ranges.len() >= 3,
            "Cylinder should have >= 3 face ranges, got {}",
            mesh.face_ranges.len()
        );
    }

    /// Extract edges from revolved solid and verify edge range validity.
    #[test]
    fn test_extract_edges_cylinder_ranges_valid() {
        let mut kernel = TruckKernel::new();
        let solid = primitives::make_cylinder(1.0, 2.0);
        let handle = kernel.store_solid(solid);
        let edges = kernel.extract_edges(&handle, 0.05).unwrap();

        for range in &edges.edge_ranges {
            assert!(
                range.end_vertex > range.start_vertex,
                "Edge range should have at least 2 vertices"
            );
            assert!(
                (range.end_vertex as usize) * 3 <= edges.vertices.len(),
                "Edge range end should not exceed vertex count"
            );
        }
        // All vertices should be finite
        for (i, v) in edges.vertices.iter().enumerate() {
            assert!(v.is_finite(), "Edge vertex[{}] = {} is not finite", i, v);
        }
    }

    #[test]
    fn test_healing_tolerance_scales_with_geometry() {
        for &scale in &[0.1, 1.0, 10.0, 50.0] {
            let solid = primitives::make_box(scale, scale, scale);
            let heal = compute_healing_tol(&solid, &solid);
            let bool_tol = compute_adaptive_tol(&solid, &solid);
            assert!(
                heal < bool_tol,
                "scale={}: heal_tol={} must be < bool_tol={}",
                scale,
                heal,
                bool_tol
            );
        }
    }
}
