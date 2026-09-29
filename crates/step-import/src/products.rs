//! Per-product split of a STEP assembly (roadmap SI4; `specs/kicad_board_link.md`
//! C3). Where [`crate::parse_step`] folds every path of the product tree
//! into one world-placed composite, this walk keeps the first level of the
//! tree: every edge leaving the root product is an **occurrence** (its
//! `NEXT_ASSEMBLY_USAGE_OCCURRENCE` name, e.g. a KiCad reference designator)
//! carrying a placement, and the node it reaches is a **product** whose
//! geometry is realized once, in the product's own frame (its sub-tree
//! folded onto it). Two occurrences of one product share it.
//!
//! Nothing about KiCad lives here: this is the STEP structure as written.
//! The mapping of occurrences to footprints is the caller's.

use crate::convert::{convert_shell, place_shell, CShell};
use crate::{units::scan_length_unit_scale, StepImportError};
use std::result::Result;
use truck_assembly::dag::NodeIndex;
use truck_meshalgo::prelude::*;
use truck_stepio::r#in::{convert::ProductShape, Table};
use waffle_types::kernel::ImportedBodyData;

/// One product of the file: a body in the product's own frame, in meters.
#[derive(Debug, Clone)]
pub struct StepProduct {
    /// The `PRODUCT` name as written (`'box'`), made unique among the
    /// products of one file by a `#n` suffix when two distinct products
    /// share a name — the key an occurrence names its product by.
    pub name: String,
    /// The geometry of the product's sub-tree in the product's frame. Empty
    /// shells when the sub-tree carries no shape at all.
    pub body: ImportedBodyData,
}

/// A rigid placement (child frame → parent frame), in meters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepPlacement {
    pub translation_m: [f64; 3],
    /// The rotated basis, as columns: where the child's +X, +Y, +Z land.
    pub axes: [[f64; 3]; 3],
}

impl StepPlacement {
    pub const IDENTITY: StepPlacement = StepPlacement {
        translation_m: [0.0; 3],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
}

/// One edge from the root product: what the file calls it, which product
/// it places, and where.
#[derive(Debug, Clone)]
pub struct StepOccurrence {
    /// The occurrence name (`NEXT_ASSEMBLY_USAGE_OCCURRENCE.name`), which
    /// is how `kicad-cli pcb export step` labels a component with its
    /// reference designator (KiCad 7, 8 and 9).
    pub name: String,
    /// Index into [`StepProducts::products`].
    pub product: usize,
    pub placement: StepPlacement,
}

/// The first level of a STEP product tree.
#[derive(Debug, Clone, Default)]
pub struct StepProducts {
    /// The root product's name (`'two_sided_k7 1'` for a KiCad export).
    pub root_name: String,
    pub products: Vec<StepProduct>,
    pub occurrences: Vec<StepOccurrence>,
    pub warnings: Vec<String>,
}

/// Split a STEP file into its first-level products and occurrences. A file
/// with no usable product structure, or a root with no children, yields no
/// occurrences (and no products): the caller decides what that means —
/// [`crate::parse_step`] still folds such a file into one body.
pub fn parse_step_products(
    step_text: &str,
    source_name: &str,
) -> Result<StepProducts, StepImportError> {
    let table = Table::from_step(step_text).ok_or(StepImportError::Parse)?;
    let mut out = StepProducts::default();
    let (unit_scale, unit_warning) = scan_length_unit_scale(step_text);
    out.warnings.extend(unit_warning);

    let assy = match table.step_assy() {
        Ok(assy) => assy,
        Err(e) => {
            out.warnings
                .push(format!("no usable assembly structure ({e}); no products"));
            return Ok(out);
        }
    };
    let tops: Vec<_> = assy.top_nodes().collect();
    let Some(root) = tops.first() else {
        out.warnings
            .push("the product tree has no root; no products".to_string());
        return Ok(out);
    };
    if tops.len() > 1 {
        out.warnings.push(format!(
            "{} top-level products; only the first ({}) is split into occurrences",
            tops.len(),
            root.attrs().name
        ));
    }
    out.root_name = root.attrs().name.clone();

    // truck's tables are hash maps, so the root's edges come in no fixed
    // order: sort them by (occurrence name, product name) so products,
    // their `#n` suffixes and the occurrence list are the same on every run.
    let mut edges: Vec<_> = root.edges().collect();
    edges.sort_by(|a, b| {
        let (na, nb) = (&a.entity().attrs.name, &b.entity().attrs.name);
        let (pa, pb) = (
            &assy.node(a.nodes().1).attrs().name,
            &assy.node(b.nodes().1).attrs().name,
        );
        (na, pa).cmp(&(nb, pb))
    });

    // Product node → index into `out.products`, so two occurrences of one
    // product realize its geometry once.
    let mut product_of_node: Vec<(NodeIndex, usize)> = Vec::new();
    for edge in edges {
        let (_, child) = edge.nodes();
        let child_node = assy.node(child);
        let product = match product_of_node.iter().find(|(n, _)| *n == child) {
            Some((_, p)) => *p,
            None => {
                let mut name = child_node.attrs().name.clone();
                let taken = out.products.iter().filter(|p| p.name == name).count();
                if taken > 0 {
                    name = format!("{name}#{}", taken + 1);
                }
                // The sub-tree in the product's own frame: every path from
                // the product node down, matrices folded from the node.
                let mut shells: Vec<CShell> = Vec::new();
                for path in assy.paths_iter(child) {
                    let matrix: Matrix4 =
                        path.edges().iter().fold(Matrix4::from_scale(1.0), |m, e| {
                            match Matrix4::try_from(&e.entity().matrix) {
                                Ok(step) => m * step,
                                Err(_) => m,
                            }
                        });
                    for shape in path.terminal_node().shape() {
                        let placed: Vec<&CShell> = match shape {
                            ProductShape::Solid(solid) => solid.boundaries.iter().collect(),
                            ProductShape::Shells(shells) => shells.iter().collect(),
                            ProductShape::Matrix(_) => continue,
                        };
                        for shell in placed {
                            shells.push(place_shell(shell, &matrix));
                        }
                    }
                }
                let mut body = ImportedBodyData {
                    source_name: format!("{source_name}:{name}"),
                    shells: Vec::with_capacity(shells.len()),
                    warnings: Vec::new(),
                };
                for shell in &shells {
                    body.shells
                        .push(convert_shell(shell, unit_scale, &mut body.warnings));
                }
                out.products.push(StepProduct { name, body });
                product_of_node.push((child, out.products.len() - 1));
                out.products.len() - 1
            }
        };
        let placement = match Matrix4::try_from(&edge.entity().matrix) {
            Ok(m) => placement_of(&m, unit_scale),
            Err(e) => {
                out.warnings.push(format!(
                    "occurrence `{}`: unreadable placement ({e}); identity used",
                    edge.entity().attrs.name
                ));
                StepPlacement::IDENTITY
            }
        };
        out.occurrences.push(StepOccurrence {
            name: edge.entity().attrs.name.clone(),
            product,
            placement,
        });
    }
    if out.occurrences.is_empty() {
        out.warnings.push(format!(
            "root product `{}` has no occurrences; no products",
            out.root_name
        ));
    }
    Ok(out)
}

/// The rigid part of a cgmath (column-major) placement, translation scaled
/// to meters. STEP `ITEM_DEFINED_TRANSFORMATION`s are rigid; a scale or
/// shear in the columns is not expected and is not removed.
fn placement_of(m: &Matrix4, unit_scale: f64) -> StepPlacement {
    let col = |v: Vector4| [v.x, v.y, v.z];
    StepPlacement {
        translation_m: [m.w.x * unit_scale, m.w.y * unit_scale, m.w.z * unit_scale],
        axes: [col(m.x), col(m.y), col(m.z)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE: &str = include_str!("../tests/fixtures/cube.step");

    #[test]
    fn a_structure_free_file_has_no_occurrences_and_says_so() {
        let p = parse_step_products(CUBE, "cube").unwrap();
        assert!(p.occurrences.is_empty());
        assert!(p.products.is_empty());
        assert!(
            p.warnings.iter().any(|w| w.contains("no products")),
            "{:?}",
            p.warnings
        );
    }

    #[test]
    fn garbage_is_a_parse_error() {
        assert!(matches!(
            parse_step_products("nope", "x"),
            Err(StepImportError::Parse)
        ));
    }
}

#[cfg(test)]
mod kicad_tests {
    //! The KiCad 7.0.11 `kicad-cli pcb export step` of
    //! `kicad-pcb/tests/fixtures/two_sided.kicad_pcb` (our own board, our
    //! own dummy model): the structure the C3 mapping relies on.
    use super::*;

    const TWO_SIDED: &str = include_str!("../../kicad-pcb/tests/fixtures/two_sided.step");

    #[test]
    fn kicad_export_names_occurrences_by_reference_and_products_by_model() {
        let p = parse_step_products(TWO_SIDED, "two_sided").unwrap();
        assert_eq!(p.root_name, "two_sided_k7 1");
        // Sorted by name: the auto-named board occurrence, then C1, R1.
        let names: Vec<&str> = p.occurrences.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["=>[0:1:1:4]", "C1", "R1"]);
        // R1 and C1 are two occurrences of ONE product (the same model file).
        let r1 = p.occurrences.iter().find(|o| o.name == "R1").unwrap();
        let c1 = p.occurrences.iter().find(|o| o.name == "C1").unwrap();
        assert_eq!(r1.product, c1.product);
        assert_eq!(p.products[r1.product].name, "box");
        // The board is the third occurrence, an auto-named product.
        let board = p
            .occurrences
            .iter()
            .find(|o| o.name.starts_with("=>"))
            .unwrap();
        assert_eq!(p.products[board.product].name, "two_sided PCB");
        assert_eq!(p.products.len(), 2);
        // The model is realized ONCE, in its own frame: a 10 mm cube at the origin.
        let cube = &p.products[r1.product].body;
        assert_eq!(cube.face_count(), 6);
        let mut max = [f64::MIN; 3];
        let mut min = [f64::MAX; 3];
        for f in &cube.shells[0].faces {
            for q in f.positions.chunks_exact(3) {
                for k in 0..3 {
                    max[k] = max[k].max(q[k]);
                    min[k] = min[k].min(q[k]);
                }
            }
        }
        for k in 0..3 {
            assert!(
                (min[k]).abs() < 1e-12 && (max[k] - 0.010).abs() < 1e-12,
                "{min:?} {max:?}"
            );
        }
        // Placements in meters, as written.
        assert_eq!(r1.placement.translation_m, [20e-3, -15e-3, 1.65e-3]);
        assert_eq!(r1.placement.axes[2], [0.0, 0.0, 1.0]);
        assert!((c1.placement.translation_m[2] + 0.55e-3).abs() < 1e-15);
        assert_eq!(board.placement, StepPlacement::IDENTITY);
    }
}
