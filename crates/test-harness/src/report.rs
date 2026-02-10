//! Structured text-based model reports for agent consumption.
//!
//! Reports are natural language, not JSON, because agents read
//! structured text better than raw data for model inspection.

use std::fmt;

use feature_engine::types::*;
use waffle_types::SketchEntity;

use crate::helpers::HarnessError;
use crate::oracle::OracleVerdict;
use crate::workflow::ModelBuilder;

/// A complete model report with all sections.
pub struct ModelReport {
    pub feature_entries: Vec<FeatureEntry>,
    pub mesh_summaries: Vec<MeshSummary>,
    pub bounding_box: Option<([f32; 3], [f32; 3])>,
    pub oracle_results: Vec<OracleVerdict>,
    pub errors: Vec<(String, String)>,
}

/// A single feature's report entry.
pub struct FeatureEntry {
    pub index: usize,
    pub name: String,
    pub op_type: String,
    pub detail: String,
    pub suppressed: bool,
    pub topology: Option<(usize, usize, usize)>,
    pub euler: Option<i64>,
    pub roles: Vec<String>,
}

/// Mesh summary for a feature.
pub struct MeshSummary {
    pub name: String,
    pub triangle_count: usize,
    pub vertex_count: usize,
    pub face_range_count: usize,
}

impl ModelReport {
    /// Format the report as text for agent consumption.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str("=== Waffle Iron Model Report ===\n\n");

        // Feature tree
        let suppressed_count = self.feature_entries.iter().filter(|e| e.suppressed).count();
        let error_count = self.errors.len();
        out.push_str(&format!(
            "Feature Tree ({} features, {} suppressed, {} errors):\n",
            self.feature_entries.len(),
            suppressed_count,
            error_count,
        ));

        for entry in &self.feature_entries {
            let sup = if entry.suppressed {
                " [SUPPRESSED]"
            } else {
                ""
            };
            out.push_str(&format!(
                "  [{}] {} \"{}\"{}\n",
                entry.index, entry.op_type, entry.name, sup,
            ));
            if !entry.detail.is_empty() {
                out.push_str(&format!("      {}\n", entry.detail));
            }
            if let Some((v, e, f)) = entry.topology {
                let euler = v as i64 - e as i64 + f as i64;
                let euler_status = if euler == 2 { "OK" } else { "WARN" };
                out.push_str(&format!(
                    "      Solid: V={} E={} F={} | Euler V-E+F={} ({})\n",
                    v, e, f, euler, euler_status,
                ));
            }
            if !entry.roles.is_empty() {
                out.push_str(&format!("      Roles: {}\n", entry.roles.join(", ")));
            }
        }

        // Mesh summary
        if !self.mesh_summaries.is_empty() {
            out.push_str("\nMesh Summary:\n");
            for ms in &self.mesh_summaries {
                out.push_str(&format!(
                    "  \"{}\": {} triangles, {} vertices, {} face ranges\n",
                    ms.name, ms.triangle_count, ms.vertex_count, ms.face_range_count,
                ));
            }
        }

        // Bounding box
        if let Some((min, max)) = self.bounding_box {
            out.push_str(&format!(
                "\nBounding Box: ({:.1}, {:.1}, {:.1}) -> ({:.1}, {:.1}, {:.1})\n",
                min[0], min[1], min[2], max[0], max[1], max[2],
            ));
        }

        // Oracle results
        if !self.oracle_results.is_empty() {
            out.push_str(&format!(
                "\nOracle Results ({} checks):\n",
                self.oracle_results.len()
            ));
            for v in &self.oracle_results {
                let status = if v.passed { "PASS" } else { "FAIL" };
                out.push_str(&format!("  [{}] {}: {}\n", status, v.oracle_name, v.detail,));
            }
        }

        // Errors
        if self.errors.is_empty() {
            out.push_str("\nErrors: none\n");
        } else {
            out.push_str(&format!("\nErrors ({}):\n", self.errors.len()));
            for (feature, msg) in &self.errors {
                out.push_str(&format!("  {}: {}\n", feature, msg));
            }
        }

        out
    }
}

impl fmt::Display for ModelReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_text())
    }
}

impl ModelBuilder {
    /// Generate a complete model report.
    pub fn report(&mut self) -> Result<ModelReport, HarnessError> {
        let mut feature_entries = Vec::new();
        let mut mesh_summaries = Vec::new();
        let mut overall_min = [f32::MAX; 3];
        let mut overall_max = [f32::MIN; 3];
        let mut has_mesh = false;
        let mut all_oracle_results = Vec::new();

        for (idx, feature) in self.state.engine.tree.features.iter().enumerate() {
            let op_type = match &feature.operation {
                Operation::Sketch { .. } => "Sketch",
                Operation::Extrude { .. } => "Extrude",
                Operation::Revolve { .. } => "Revolve",
                Operation::Fillet { .. } => "Fillet",
                Operation::Chamfer { .. } => "Chamfer",
                Operation::Shell { .. } => "Shell",
                Operation::BooleanCombine { .. } => "Boolean",
            };

            let detail = describe_operation(&feature.operation);

            let mut topology = None;
            let mut roles = Vec::new();

            // Get topology and roles from OpResult if available
            if let Some(result) = self.state.engine.get_result(feature.id) {
                if !result.outputs.is_empty() {
                    let handle = &result.outputs[0].1.handle;
                    let introspect = self.kernel.as_introspect();
                    let v = introspect.list_vertices(handle).len();
                    let e = introspect.list_edges(handle).len();
                    let f = introspect.list_faces(handle).len();
                    topology = Some((v, e, f));

                    // Collect topology oracle results
                    let topo_checks = crate::oracle::run_topology_checks(introspect, handle);
                    all_oracle_results.extend(topo_checks);
                }

                // Collect role info
                for (_, role) in &result.provenance.role_assignments {
                    roles.push(format!("{:?}", role));
                }
            }

            feature_entries.push(FeatureEntry {
                index: idx,
                name: feature.name.clone(),
                op_type: op_type.to_string(),
                detail,
                suppressed: feature.suppressed,
                topology,
                euler: topology.map(|(v, e, f)| v as i64 - e as i64 + f as i64),
                roles,
            });
        }

        // Tessellate features that have solids
        for feature in &self.state.engine.tree.features {
            if let Some(result) = self.state.engine.get_result(feature.id) {
                if !result.outputs.is_empty() {
                    let handle = &result.outputs[0].1.handle;
                    if let Ok(mesh) = self.kernel.tessellate(handle, 0.1) {
                        let tri_count = mesh.indices.len() / 3;
                        let vert_count = mesh.vertices.len() / 3;
                        let fr_count = mesh.face_ranges.len();

                        // Update bounding box
                        if mesh.vertices.len() >= 3 {
                            let (bmin, bmax) = crate::helpers::mesh_bounding_box(&mesh);
                            for i in 0..3 {
                                overall_min[i] = overall_min[i].min(bmin[i]);
                                overall_max[i] = overall_max[i].max(bmax[i]);
                            }
                            has_mesh = true;
                        }

                        // Run mesh oracles
                        let mesh_checks = crate::oracle::run_all_mesh_checks(&mesh);
                        all_oracle_results.extend(mesh_checks);

                        mesh_summaries.push(MeshSummary {
                            name: feature.name.clone(),
                            triangle_count: tri_count,
                            vertex_count: vert_count,
                            face_range_count: fr_count,
                        });
                    }
                }
            }
        }

        let bounding_box = if has_mesh {
            Some((overall_min, overall_max))
        } else {
            None
        };

        let errors: Vec<(String, String)> = self
            .state
            .engine
            .errors
            .iter()
            .map(|(id, msg)| {
                let name = self
                    .state
                    .engine
                    .tree
                    .find_feature(*id)
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| id.to_string());
                (name, msg.clone())
            })
            .collect();

        Ok(ModelReport {
            feature_entries,
            mesh_summaries,
            bounding_box,
            oracle_results: all_oracle_results,
            errors,
        })
    }
}

/// Describe an operation's parameters in a human-readable way.
fn describe_operation(op: &Operation) -> String {
    match op {
        Operation::Sketch { sketch } => {
            let point_count = sketch
                .entities
                .iter()
                .filter(|e| matches!(e, SketchEntity::Point { .. }))
                .count();
            let line_count = sketch
                .entities
                .iter()
                .filter(|e| matches!(e, SketchEntity::Line { .. }))
                .count();
            let profile_count = sketch.solved_profiles.len();
            let outer = sketch.solved_profiles.iter().filter(|p| p.is_outer).count();
            format!(
                "Entities: {} points, {} lines | Profiles: {} ({} outer) | Plane: origin=({:.1}, {:.1}, {:.1}) normal=({:.1}, {:.1}, {:.1})",
                point_count, line_count, profile_count, outer,
                sketch.plane_origin[0], sketch.plane_origin[1], sketch.plane_origin[2],
                sketch.plane_normal[0], sketch.plane_normal[1], sketch.plane_normal[2],
            )
        }
        Operation::Extrude { params } => {
            let cut = if params.cut { " (cut)" } else { "" };
            let sym = if params.symmetric { " symmetric" } else { "" };
            format!("Params: depth={:.3}{}{}", params.depth, cut, sym)
        }
        Operation::Revolve { params } => {
            format!(
                "Params: angle={:.1}deg, axis=({:.1},{:.1},{:.1})",
                params.angle,
                params.axis_direction[0],
                params.axis_direction[1],
                params.axis_direction[2],
            )
        }
        Operation::Fillet { params } => {
            format!(
                "Params: radius={:.3}, {} edges",
                params.radius,
                params.edges.len()
            )
        }
        Operation::Chamfer { params } => {
            format!(
                "Params: distance={:.3}, {} edges",
                params.distance,
                params.edges.len()
            )
        }
        Operation::Shell { params } => {
            format!(
                "Params: thickness={:.3}, {} faces removed",
                params.thickness,
                params.faces_to_remove.len()
            )
        }
        Operation::BooleanCombine { params } => {
            let op_name = match params.operation {
                BooleanOp::Union => "union",
                BooleanOp::Subtract => "subtract",
                BooleanOp::Intersect => "intersect",
            };
            format!("Params: {}", op_name)
        }
    }
}
