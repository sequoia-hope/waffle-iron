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
pub(crate) type CShell = CompressedShell<Point3, Curve3D, Surface>;
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
    for placed in &shells {
        body.shells
            .push(convert_shell(&placed.shell, unit_scale, &mut body.warnings));
    }
    if body.is_empty() {
        return Err(StepImportError::Convert(
            "all faces failed to tessellate".to_string(),
        ));
    }
    Ok(body)
}

/// One shell of the file, placed, with the one piece of solid-level topology
/// the flat shell list would otherwise lose: which `MANIFOLD_SOLID_BREP` it
/// belongs to, and how many shells that solid has. A `BREP_WITH_VOIDS`
/// arrives as `[outer, void, void, …]` (truck's `CompressedSolid::boundaries`),
/// and a consumer that builds one solid per shell must know that a shell is
/// one of several — an outer shell ingested without its voids is a silently
/// filled block (SI5 spec §5.2).
pub(crate) struct PlacedShell {
    pub(crate) shell: CShell,
    /// Index of the owning solid in this parse's own numbering.
    pub(crate) solid: usize,
    /// Number of shells (outer + voids) of the owning solid. `1` for a
    /// void-free solid and for every shell of a `SHELL_BASED_SURFACE_MODEL`.
    pub(crate) shells_in_solid: usize,
}

/// Walk the assembly DAG and return every shell of every solid/shell-model of
/// every path, with the path's placement matrix baked into the geometry
/// (file units). Falls back to the raw `manifold_solid_brep` table when the
/// file has no usable product structure.
///
/// **The returned order is canonical** (SI5 spec §5.7): truck's tables are
/// `HashMap`s, so the raw walk order of a multi-solid file is seeded per
/// process, and two parses of one text could hand the same geometry different
/// positions — hence different persistent face ids. Shells are sorted on a
/// geometric key (their placed vertex multiset, then face and edge counts),
/// which depends only on the file's content. Shells that tie on the key are
/// geometrically interchangeable, so their relative order cannot matter.
pub(crate) fn collect_placed_shells(
    table: &Table,
    warnings: &mut Vec<String>,
) -> Result<Vec<PlacedShell>, StepImportError> {
    let mut out: Vec<PlacedShell> = Vec::new();
    let push_solid = |out: &mut Vec<PlacedShell>, shells: Vec<&CShell>, matrix: &Matrix4| {
        let solid = out.last().map_or(0, |p| p.solid + 1);
        let n = shells.len();
        for shell in shells {
            out.push(PlacedShell {
                shell: place_shell(shell, matrix),
                solid,
                shells_in_solid: n,
            });
        }
    };

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
                        match shape {
                            ProductShape::Solid(solid) => {
                                push_solid(&mut out, solid.boundaries.iter().collect(), &matrix)
                            }
                            // A shell model's shells are independent surfaces,
                            // not one solid: each is its own group.
                            ProductShape::Shells(shells) => {
                                for shell in shells {
                                    push_solid(&mut out, vec![shell], &matrix);
                                }
                            }
                            ProductShape::Matrix(_) => continue,
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
                Ok(csolid) => push_solid(&mut out, csolid.boundaries.iter().collect(), &identity),
                Err(e) => warnings.push(format!("skipped a solid that failed to convert: {e}")),
            }
        }
    }

    out.sort_by(|a, b| cmp_shell_canonical(&a.shell, &b.shell));
    Ok(out)
}

/// The canonical shell order: compare two shells by their sorted vertex
/// lists (lexicographic over `(x, y, z)` with `total_cmp`), then by face
/// count, then by edge count. A pure function of the shells' own content.
pub(crate) fn cmp_shell_canonical(a: &CShell, b: &CShell) -> std::cmp::Ordering {
    fn sorted_vertices(s: &CShell) -> Vec<[f64; 3]> {
        let mut v: Vec<[f64; 3]> = s.vertices.iter().map(|p| [p.x, p.y, p.z]).collect();
        v.sort_by(cmp_point);
        v
    }
    fn cmp_point(p: &[f64; 3], q: &[f64; 3]) -> std::cmp::Ordering {
        p[0].total_cmp(&q[0])
            .then(p[1].total_cmp(&q[1]))
            .then(p[2].total_cmp(&q[2]))
    }
    let (va, vb) = (sorted_vertices(a), sorted_vertices(b));
    va.len()
        .cmp(&vb.len())
        .then_with(|| {
            va.iter()
                .zip(&vb)
                .map(|(p, q)| cmp_point(p, q))
                .find(|o| o.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then(a.faces.len().cmp(&b.faces.len()))
        .then(a.edges.len().cmp(&b.edges.len()))
}

/// Clone a shell with a placement matrix applied to all geometry.
pub(crate) fn place_shell(shell: &CShell, matrix: &Matrix4) -> CShell {
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
pub(crate) fn convert_shell(
    shell: &CShell,
    unit_scale: f64,
    warnings: &mut Vec<String>,
) -> ImportedShellData {
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

#[cfg(test)]
mod canonical_order {
    use super::*;

    /// The shell order is a function of the shells' content, not of the
    /// walk: a translated copy of a shell sorts by its coordinates, and the
    /// same pair sorts the same way whichever order it is offered in.
    #[test]
    fn shells_sort_on_their_geometry_not_their_arrival() {
        let text = include_str!("../tests/fixtures/cube.step");
        let table = Table::from_step(text).expect("parses");
        let mut warnings = Vec::new();
        let placed = collect_placed_shells(&table, &mut warnings).expect("shells");
        let base = placed[0].shell.clone();
        let moved = place_shell(
            &base,
            &Matrix4::from_translation(Vector3::new(100.0, 0.0, 0.0)),
        );
        let moved_back = place_shell(
            &base,
            &Matrix4::from_translation(Vector3::new(-100.0, 0.0, 0.0)),
        );

        assert_eq!(cmp_shell_canonical(&base, &base), std::cmp::Ordering::Equal);
        assert_eq!(
            cmp_shell_canonical(&moved_back, &base),
            std::cmp::Ordering::Less
        );
        assert_eq!(cmp_shell_canonical(&base, &moved), std::cmp::Ordering::Less);

        let mut a = [&moved, &base, &moved_back];
        let mut b = [&moved_back, &moved, &base];
        a.sort_by(|x, y| cmp_shell_canonical(x, y));
        b.sort_by(|x, y| cmp_shell_canonical(x, y));
        let key = |s: &CShell| s.vertices.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        assert_eq!(
            a.iter().map(|s| key(s)).collect::<Vec<_>>(),
            b.iter().map(|s| key(s)).collect::<Vec<_>>()
        );
        assert!(a.windows(2).all(|w| key(w[0]) <= key(w[1])));
    }

    /// Every shell of a void-free solid is its own group of one; the group
    /// size is what the exact tier refuses on.
    #[test]
    fn a_void_free_solid_reports_one_shell_per_solid() {
        let text = include_str!("../tests/fixtures/cube.step");
        let table = Table::from_step(text).expect("parses");
        let mut warnings = Vec::new();
        let placed = collect_placed_shells(&table, &mut warnings).expect("shells");
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].shells_in_solid, 1);
    }
}
