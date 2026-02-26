//! Coverage matrix tracking which (degeneracy, operation, primitive) combos are tested.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::strategies::{BoolOp, DegeneracyFamily};

/// Primitive type pair involved in a boolean operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrimitivePair {
    BoxBox,
    BoxCylinder,
    CylinderCylinder,
    BoxCone,
    BoxSphere,
}

impl fmt::Display for PrimitivePair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitivePair::BoxBox => write!(f, "Box-Box"),
            PrimitivePair::BoxCylinder => write!(f, "Box-Cylinder"),
            PrimitivePair::CylinderCylinder => write!(f, "Cyl-Cyl"),
            PrimitivePair::BoxCone => write!(f, "Box-Cone"),
            PrimitivePair::BoxSphere => write!(f, "Box-Sphere"),
        }
    }
}

/// A triple identifying a test coverage cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoverageKey {
    pub family: DegeneracyFamily,
    pub op: BoolOp,
    pub primitives: PrimitivePair,
}

/// Tracks which (degeneracy × operation × primitive pair) combinations have been tested.
pub struct CoverageMatrix {
    tested: BTreeSet<CoverageKey>,
}

impl CoverageMatrix {
    pub fn new() -> Self {
        Self {
            tested: BTreeSet::new(),
        }
    }

    /// Record that a specific combination has been tested.
    pub fn record_test(&mut self, family: DegeneracyFamily, op: BoolOp, primitives: PrimitivePair) {
        self.tested.insert(CoverageKey {
            family,
            op,
            primitives,
        });
    }

    /// Check if a specific combination has been tested.
    pub fn is_tested(
        &self,
        family: DegeneracyFamily,
        op: BoolOp,
        primitives: PrimitivePair,
    ) -> bool {
        self.tested.contains(&CoverageKey {
            family,
            op,
            primitives,
        })
    }

    /// Report all untested combinations from the full cross-product.
    pub fn report_gaps(&self) -> Vec<CoverageKey> {
        let families = [
            DegeneracyFamily::CoplanarFaces,
            DegeneracyFamily::CoincidentEdge,
            DegeneracyFamily::VertexOnFace,
            DegeneracyFamily::Tangential,
        ];
        let ops = [BoolOp::Union, BoolOp::Subtract, BoolOp::Intersect];
        let primitives = [
            PrimitivePair::BoxBox,
            PrimitivePair::BoxCylinder,
            PrimitivePair::CylinderCylinder,
        ];

        let mut gaps = Vec::new();
        for &family in &families {
            for &op in &ops {
                for &prim in &primitives {
                    let key = CoverageKey {
                        family,
                        op,
                        primitives: prim,
                    };
                    if !self.tested.contains(&key) {
                        gaps.push(key);
                    }
                }
            }
        }
        gaps
    }

    /// Get count of tested combinations.
    pub fn tested_count(&self) -> usize {
        self.tested.len()
    }

    /// Get total possible combinations in the core matrix.
    pub fn total_combinations(&self) -> usize {
        // 4 families × 3 ops × 3 primitive pairs = 36
        4 * 3 * 3
    }

    /// Format a coverage report as a string table.
    pub fn format_report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Coverage: {}/{} ({:.0}%)\n",
            self.tested_count(),
            self.total_combinations(),
            self.tested_count() as f64 / self.total_combinations() as f64 * 100.0
        ));

        let gaps = self.report_gaps();
        if gaps.is_empty() {
            out.push_str("No gaps — full coverage!\n");
        } else {
            out.push_str(&format!("\nUntested ({}):\n", gaps.len()));
            // Group by family
            let mut by_family: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for gap in &gaps {
                by_family
                    .entry(gap.family.to_string())
                    .or_default()
                    .push(format!("{} × {}", gap.op, gap.primitives));
            }
            for (family, combos) in &by_family {
                out.push_str(&format!("  {}:\n", family));
                for combo in combos {
                    out.push_str(&format!("    - {}\n", combo));
                }
            }
        }

        out
    }
}

impl Default for CoverageMatrix {
    fn default() -> Self {
        Self::new()
    }
}

// Need Ord/PartialOrd for BTreeSet — derived above via derive macros on the enums.
// Since we can't derive on external types, implement manually:

impl PartialOrd for DegeneracyFamily {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DegeneracyFamily {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

impl PartialOrd for BoolOp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BoolOp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_matrix_reports_all_gaps() {
        let m = CoverageMatrix::new();
        assert_eq!(m.tested_count(), 0);
        assert_eq!(m.report_gaps().len(), m.total_combinations());
    }

    #[test]
    fn recording_reduces_gaps() {
        let mut m = CoverageMatrix::new();
        m.record_test(
            DegeneracyFamily::CoplanarFaces,
            BoolOp::Union,
            PrimitivePair::BoxBox,
        );
        assert_eq!(m.tested_count(), 1);
        assert!(m.is_tested(
            DegeneracyFamily::CoplanarFaces,
            BoolOp::Union,
            PrimitivePair::BoxBox,
        ));
        assert_eq!(m.report_gaps().len(), m.total_combinations() - 1);
    }
}
