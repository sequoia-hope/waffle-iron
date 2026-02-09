//! TruckKernel — real geometry kernel wrapping truck's API.

use crate::tessellation;
use crate::traits::Kernel;
use crate::types::*;
use std::collections::HashMap;

// Import truck types selectively to avoid shadowing std::result::Result
use truck_modeling::builder;
use truck_modeling::topology::{Edge, Face, Solid, Wire};
use truck_modeling::{InnerSpace, Point3, Rad, Vector3};

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

    pub(crate) fn store_solid(&mut self, solid: Solid) -> KernelSolidHandle {
        let handle = self.alloc_handle();
        self.solids.insert(handle.id(), solid);
        handle
    }

    pub(crate) fn get_solid(&self, handle: &KernelSolidHandle) -> Option<&Solid> {
        self.solids.get(&handle.id())
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

        let solid = builder::rsweep(&truck_face, origin, axis.normalize(), Rad(angle));
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

        let result = truck_shapeops::or(&solid_a, &solid_b, 0.05).ok_or_else(|| {
            KernelError::BooleanFailed {
                reason: "truck or() returned None".to_string(),
            }
        })?;
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
        let mut solid_b = self
            .solids
            .get(&b.id())
            .ok_or(KernelError::EntityNotFound {
                id: KernelId(b.id()),
            })?
            .clone();

        // Subtraction = A ∩ ¬B. not() mutates in place.
        solid_b.not();
        let result = truck_shapeops::and(&solid_a, &solid_b, 0.05).ok_or_else(|| {
            KernelError::BooleanFailed {
                reason: "truck and() returned None for subtraction".to_string(),
            }
        })?;
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

        let result = truck_shapeops::and(&solid_a, &solid_b, 0.05).ok_or_else(|| {
            KernelError::BooleanFailed {
                reason: "truck and() returned None".to_string(),
            }
        })?;
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

        tessellation::tessellate_solid(truck_solid, tolerance, &mut self.next_id)
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
}
