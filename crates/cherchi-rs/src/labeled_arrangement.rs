//! Stage-2 output of the Yang pipeline: a full exact mesh arrangement plus
//! per-triangle labels (which input solid(s) each triangle lies on, its
//! inside/outside classification per input solid, and its Cherchi patch id).
//!
//! See `specs/yang_m2_labeled_arrangement.md`. This module is the frozen
//! `LabeledArrangement` contract; both the native arrangement (future) and
//! the interim `cherchi-sidecar-rs` producer satisfy it.
//!
//! **RED phase (Test Author):** this file currently contains ONLY the
//! `#[cfg(test)]` unit tests targeting the not-yet-existing `LabeledArrangement`
//! / `InputId` / `keep_set` API. The Implementer adds the production types to
//! this module to turn the tests GREEN.

#[cfg(test)]
mod tests {
    // These imports reference the production types the Implementer will add to
    // this module. Until then this is a compile-failure RED (intentional).
    use super::{InputId, LabeledArrangement};
    use cad_primitives::{BoolOp, Point3};
    use crate::Mesh;

    fn p(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    /// A hand-built arrangement with 3 fake triangles and num_inputs = 2.
    ///
    /// The geometry is irrelevant to the keep-rule logic (which reads only
    /// `surface`/`inside`), so we use three degenerate-but-distinct triangles
    /// over a shared 4-vertex pool. The labels are crafted so that, by the
    /// documented Cherchi keep-rules (booleans.cpp:1394-1485), each op selects
    /// a known, hand-derived index set:
    ///
    /// | tri | surface     | inside        | union | intersect | subtract | xor |
    /// |-----|-------------|---------------|-------|-----------|----------|-----|
    /// | 0   | {A}     (0) | [false,false] |  keep |           |   keep   | keep|
    /// | 1   | {A}     (0) | [false,true ] |       |   keep    |          | keep|
    /// | 2   | {B}     (1) | [true ,false] |       |   keep    |   keep   | keep|
    ///
    /// Derivations (num_inputs = 2):
    /// - **Union** (inside.count()==0): only tri 0 → `[0]`.
    /// - **Intersection** ((surface ^ inside).count()==2):
    ///     tri 0: surface{0} ^ inside{}      = {0}      → count 1, NO.
    ///     tri 1: surface{0} ^ inside{1}     = {0,1}    → count 2, KEEP.
    ///     tri 2: surface{1} ^ inside{0}     = {0,1}    → count 2, KEEP.
    ///   → `[1, 2]`.
    /// - **Subtraction** (surface[0]&&inside.count()==0  OR
    ///                    !surface[0]&&inside[0]&&inside.count()==1):
    ///     tri 0: surface[0]=true, inside.count()=0           → KEEP (branch 1).
    ///     tri 1: surface[0]=true, inside.count()=1           → branch1 NO
    ///            (inside.count()!=0); branch2 needs !surface[0] → NO. → drop.
    ///     tri 2: surface[0]=false, inside[0]=true, count==1  → KEEP (branch 2).
    ///   → `[0, 2]`.
    /// - **Xor** (inside.count()==0 OR (surface ^ inside).count()==2):
    ///     tri 0: count 0                  → KEEP.
    ///     tri 1: (surface^inside).count 2 → KEEP.
    ///     tri 2: (surface^inside).count 2 → KEEP.
    ///   → `[0, 1, 2]`.
    fn hand_built() -> LabeledArrangement {
        let verts = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.0, 1.0, 0.0),
            p(0.0, 0.0, 1.0),
        ];
        let tris = vec![[0u32, 1, 2], [0, 1, 3], [0, 2, 3]];
        let mesh = Mesh::new(verts, tris);

        let surface = vec![
            vec![InputId(0)], // tri 0 on solid A
            vec![InputId(0)], // tri 1 on solid A
            vec![InputId(1)], // tri 2 on solid B
        ];
        let inside = vec![
            vec![false, false], // tri 0
            vec![false, true],  // tri 1: inside B
            vec![true, false],  // tri 2: inside A
        ];
        let patch = vec![0u32, 0, 1];

        LabeledArrangement {
            mesh,
            surface,
            inside,
            patch,
            num_inputs: 2,
        }
    }

    /// I1 + I2: the per-tri label vectors are all aligned 1:1 with `mesh.tris`,
    /// every `surface[t]` is non-empty, and every `inside[t]` has length
    /// `num_inputs`.
    #[test]
    fn field_shapes_line_up_with_mesh_tris() {
        let la = hand_built();
        let n = la.mesh.tris.len();
        assert_eq!(n, 3, "fixture should have 3 triangles");
        assert_eq!(la.surface.len(), n, "surface len must equal mesh.tris len");
        assert_eq!(la.inside.len(), n, "inside len must equal mesh.tris len");
        assert_eq!(la.patch.len(), n, "patch len must equal mesh.tris len");
        assert_eq!(la.num_inputs, 2);
        for (t, s) in la.surface.iter().enumerate() {
            assert!(!s.is_empty(), "surface[{t}] must be non-empty (I2)");
        }
        for (t, i) in la.inside.iter().enumerate() {
            assert_eq!(
                i.len(),
                la.num_inputs as usize,
                "inside[{t}] len must equal num_inputs (I2)"
            );
        }
    }

    /// Union keep-rule (booleans.cpp:1413-1428): keep exactly the triangles
    /// whose `inside` is all-false. Hand-derived expectation: `[0]`.
    #[test]
    fn keep_set_union_is_all_false_inside_tris() {
        let la = hand_built();
        let keep = la.keep_set(BoolOp::Union);
        assert_eq!(keep, vec![0usize], "union keeps only the all-false-inside tri");
        // Cross-check the rule directly: kept ⟺ inside all false.
        for &t in &keep {
            assert!(
                la.inside[t].iter().all(|&b| !b),
                "union-kept tri {t} must have all-false inside"
            );
        }
        for t in 0..la.mesh.tris.len() {
            if !keep.contains(&t) {
                assert!(
                    la.inside[t].iter().any(|&b| b),
                    "union-dropped tri {t} must have some inside bit set"
                );
            }
        }
    }

    /// Intersection keep-rule (booleans.cpp:1394-1409):
    /// `(surface ^ inside).count() == num_inputs`. Hand-derived: `[1, 2]`.
    #[test]
    fn keep_set_intersection_matches_hand_derived_indices() {
        let la = hand_built();
        let keep = la.keep_set(BoolOp::Intersect);
        assert_eq!(
            keep,
            vec![1usize, 2],
            "intersection keep set must match hand-derived (surface^inside).count==2"
        );
    }

    /// Subtraction keep-rule (booleans.cpp:1432-1459):
    /// (surface[0] && inside.count()==0) OR
    /// (!surface[0] && inside[0] && inside.count()==1).
    /// Hand-derived: `[0, 2]`.
    #[test]
    fn keep_set_subtraction_matches_hand_derived_indices() {
        let la = hand_built();
        let keep = la.keep_set(BoolOp::Subtract);
        assert_eq!(
            keep,
            vec![0usize, 2],
            "subtraction keep set must match hand-derived A-minus-B rule"
        );
    }

    /// Xor keep-rule (booleans.cpp:1463-1485):
    /// inside.count()==0 OR (surface ^ inside).count()==num_inputs.
    /// Hand-derived: all three tris `[0, 1, 2]`.
    #[test]
    fn keep_set_xor_matches_hand_derived_indices() {
        let la = hand_built();
        let keep = la.keep_set(BoolOp::Xor);
        assert_eq!(
            keep,
            vec![0usize, 1, 2],
            "xor keep set must match hand-derived union-of-union-and-intersection rule"
        );
    }

    /// `InputId` is a `Copy`/`Eq` newtype over the solid index (0=A, 1=B).
    #[test]
    fn input_id_equality_and_copy() {
        let a = InputId(0);
        let b = InputId(1);
        assert_ne!(a, b);
        let a2 = a; // Copy
        assert_eq!(a, a2);
        assert_eq!(a.0, 0);
        assert_eq!(b.0, 1);
    }
}
