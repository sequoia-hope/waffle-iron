//! truck → `ImportedBodyData` conversion. The only module that touches truck
//! types; nothing here appears in the crate's public API.

use crate::{units::scan_length_unit_scale, StepImportError};
use truck_meshalgo::prelude::*;
use truck_stepio::r#in::{convert::ProductShape, step_geometry::*, Table};
use truck_topology::compress::CompressedShell;
use waffle_types::kernel::{
    ImportedBodyData, ImportedEdgeData, ImportedFaceData, ImportedShellData, ImportedSurface,
};

// `truck_meshalgo::prelude::*` exports its own `Result` alias; keep std's.
use std::result::Result;

/// Analytic shell straight out of the STEP topology.
type CShell = CompressedShell<Point3, Curve3D, Surface>;
/// The same shell after per-face tessellation.
type MeshedCShell = CompressedShell<Point3, PolylineCurve<Point3>, Option<PolygonMesh>>;

pub(crate) fn parse_step_impl(
    step_text: &str,
    source_name: &str,
) -> Result<ImportedBodyData, StepImportError> {
    let table = Table::from_step(step_text).ok_or(StepImportError::Parse)?;

    let mut warnings = Vec::new();
    let (unit_scale, unit_warning) = scan_length_unit_scale(step_text);
    warnings.extend(unit_warning);

    let shells = collect_placed_shells(&table, &mut warnings)?;
    if shells.is_empty() {
        return Err(StepImportError::NoSolids);
    }

    let mut body = ImportedBodyData {
        source_name: source_name.to_string(),
        shells: Vec::with_capacity(shells.len()),
        warnings,
    };
    for shell in &shells {
        body.shells
            .push(convert_shell(shell, unit_scale, &mut body.warnings));
    }
    if body.is_empty() {
        return Err(StepImportError::Convert(
            "all faces failed to tessellate".to_string(),
        ));
    }
    Ok(body)
}

/// Walk the assembly DAG and return every shell of every solid/shell-model of
/// every path, with the path's placement matrix baked into the geometry
/// (file units). Falls back to the raw `manifold_solid_brep` table when the
/// file has no usable product structure.
fn collect_placed_shells(
    table: &Table,
    warnings: &mut Vec<String>,
) -> Result<Vec<CShell>, StepImportError> {
    let mut out = Vec::new();

    match table.step_assy() {
        Ok(assy) => {
            let tops: Vec<_> = assy.top_nodes().collect();
            for top in &tops {
                for path in assy.paths_iter(top.index()) {
                    let matrix: Matrix4 =
                        path.edges().iter().fold(Matrix4::from_scale(1.0), |m, e| {
                            match Matrix4::try_from(&e.entity().matrix) {
                                Ok(step) => m * step,
                                Err(_) => m,
                            }
                        });
                    for shape in path.terminal_node().shape() {
                        let shells: Vec<&CShell> = match shape {
                            ProductShape::Solid(solid) => solid.boundaries.iter().collect(),
                            ProductShape::Shells(shells) => shells.iter().collect(),
                            ProductShape::Matrix(_) => continue,
                        };
                        for shell in shells {
                            out.push(place_shell(shell, &matrix));
                        }
                    }
                }
            }
        }
        Err(e) => {
            warnings.push(format!(
                "no usable assembly structure ({e}); importing raw solids without placements"
            ));
        }
    }

    if out.is_empty() {
        // Product-structure-free file (or an assembly walk that yielded no
        // shapes): fall back to every manifold solid in the data section.
        let identity = Matrix4::from_scale(1.0);
        for solid in table.manifold_solid_brep.values() {
            match table.to_compressed_solid(solid) {
                Ok(csolid) => {
                    for shell in &csolid.boundaries {
                        out.push(place_shell(shell, &identity));
                    }
                }
                Err(e) => warnings.push(format!("skipped a solid that failed to convert: {e}")),
            }
        }
    }

    Ok(out)
}

/// Clone a shell with a placement matrix applied to all geometry.
fn place_shell(shell: &CShell, matrix: &Matrix4) -> CShell {
    let mut placed = shell.clone();
    if *matrix != Matrix4::from_scale(1.0) {
        placed
            .vertices
            .iter_mut()
            .for_each(|v| *v = matrix.transform_point(*v));
        placed
            .edges
            .iter_mut()
            .for_each(|e| e.curve.transform_by(*matrix));
        placed
            .faces
            .iter_mut()
            .for_each(|f| f.surface.transform_by(*matrix));
    }
    placed
}

/// Tessellate one shell and flatten it into the neutral contract, converting
/// file units to meters.
fn convert_shell(shell: &CShell, unit_scale: f64, warnings: &mut Vec<String>) -> ImportedShellData {
    // Tolerance from the shell's own extent: diameter/1000 in file units
    // (matches upstream practice), floored to keep degenerate shells sane.
    let bbox: BoundingBox<Point3> = shell.vertices.iter().collect();
    let tol = (bbox.diameter() * 1e-3).max(1e-6);
    let meshed: MeshedCShell = shell.robust_triangulation(tol);

    let mut out = ImportedShellData {
        faces: Vec::with_capacity(meshed.faces.len()),
        edges: Vec::with_capacity(meshed.edges.len()),
    };

    for edge in &meshed.edges {
        let mut polyline: Vec<[f64; 3]> = edge
            .curve
            .0
            .iter()
            .map(|p| [p.x * unit_scale, p.y * unit_scale, p.z * unit_scale])
            .collect();
        if polyline.len() < 2 {
            let p = polyline.first().copied().unwrap_or([0.0; 3]);
            polyline = vec![p, p];
        }
        out.edges.push(ImportedEdgeData { polyline });
    }

    let mut failed_faces = 0usize;
    for (meshed_face, source_face) in meshed.faces.iter().zip(&shell.faces) {
        let Some(poly) = &meshed_face.surface else {
            failed_faces += 1;
            continue;
        };
        let poly = match meshed_face.orientation {
            true => poly.clone(),
            false => poly.inverse(),
        };

        let mut positions = Vec::with_capacity(poly.tri_faces().len() * 9);
        let mut normals = Vec::with_capacity(poly.tri_faces().len() * 9);
        let mut indices = Vec::with_capacity(poly.tri_faces().len() * 3);
        for tri in poly.tri_faces() {
            let ps = tri.map(|v| poly.positions()[v.pos]);
            let flat = flat_normal(&ps);
            for (k, v) in tri.iter().enumerate() {
                positions.extend_from_slice(&[
                    ps[k].x * unit_scale,
                    ps[k].y * unit_scale,
                    ps[k].z * unit_scale,
                ]);
                let n = v
                    .nor
                    .map(|ni| poly.normals()[ni])
                    .filter(|n| n.magnitude2() > 0.25)
                    .unwrap_or(flat);
                normals.extend_from_slice(&[n.x, n.y, n.z]);
                indices.push((indices.len()) as u32);
            }
        }
        if indices.is_empty() {
            failed_faces += 1;
            continue;
        }

        let edge_indices = {
            let mut seen = Vec::new();
            for boundary in &meshed_face.boundaries {
                for ei in boundary {
                    let idx = ei.index as u32;
                    if !seen.contains(&idx) {
                        seen.push(idx);
                    }
                }
            }
            seen
        };

        out.faces.push(ImportedFaceData {
            surface: classify_surface(&source_face.surface, meshed_face.orientation, unit_scale),
            positions,
            normals,
            indices,
            edge_indices,
        });
    }

    if failed_faces > 0 {
        warnings.push(format!(
            "{failed_faces} face(s) failed to tessellate and were skipped"
        ));
    }
    out
}

fn flat_normal(ps: &[Point3; 3]) -> Vector3 {
    let n = (ps[1] - ps[0]).cross(ps[2] - ps[0]);
    let m = n.magnitude();
    if m > 0.0 {
        n / m
    } else {
        Vector3::new(0.0, 0.0, 1.0)
    }
}

/// Map a truck surface to the neutral classification. Planes carry exact
/// parameters (origin scaled to meters, OUTWARD unit normal — the face
/// orientation flag folds the surface normal to outward).
fn classify_surface(surface: &Surface, orientation: bool, unit_scale: f64) -> ImportedSurface {
    match surface {
        Surface::ElementarySurface(es) => match es {
            ElementarySurface::Plane(p) => {
                let o = p.subs(0.0, 0.0);
                let mut n = p.normal();
                if !orientation {
                    n = -n;
                }
                ImportedSurface::Plane {
                    origin: [o.x * unit_scale, o.y * unit_scale, o.z * unit_scale],
                    normal: [n.x, n.y, n.z],
                }
            }
            ElementarySurface::CylindricalSurface(_) => ImportedSurface::Cylindrical,
            ElementarySurface::ConicalSurface(_) => ImportedSurface::Conical,
            ElementarySurface::Sphere(_) => ImportedSurface::Spherical,
            ElementarySurface::ToroidalSurface(_) => ImportedSurface::Toroidal,
        },
        _ => ImportedSurface::Freeform,
    }
}
