//! Stage-2 output of the Yang pipeline: a full exact mesh arrangement plus
//! per-triangle labels (which input solid(s) each triangle lies on, its
//! inside/outside classification per input solid, and its Cherchi patch id).
//!
//! See `specs/yang_m2_labeled_arrangement.md`. This module is the frozen
//! `LabeledArrangement` contract; both the native arrangement (future) and
//! the interim `cherchi-sidecar-rs` producer satisfy it.
//!
//! The production types below are pure Rust / WASM-safe (no FFI): the
//! `keep_set` logic reads only `surface` / `inside` / `num_inputs` and applies
//! Cherchi's exact op keep-rules (`code/booleans.cpp`).

use cad_primitives::BoolOp;

use crate::Mesh;

/// Index of an input solid in a binary (or n-ary) boolean: `0` = A, `1` = B.
///
/// A newtype over the raw solid index so per-triangle surface labels are
/// self-describing rather than bare integers.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct InputId(pub u32);

/// Stage-2 output of the Yang pipeline: the FULL exact mesh arrangement plus,
/// per arrangement triangle, which input solid(s) it lies on (`surface`), its
/// inside/outside classification per input solid (`inside`), and its Cherchi
/// patch id (`patch`).
///
/// `surface`, `inside`, and `patch` are each indexed 1:1 with `mesh.tris`
/// (invariant I1). Every `surface[t]` is non-empty (≥2 entries at a coplanar
/// overlap), and every `inside[t]` has exactly `num_inputs` entries (I2).
///
/// See `specs/yang_m2_labeled_arrangement.md`.
#[derive(Clone, Debug, PartialEq)]
pub struct LabeledArrangement {
    /// The full arrangement mesh (all sub-triangles, pre-filter).
    pub mesh: Mesh,
    /// Per triangle: which input solid(s) the triangle lies on. Length ≥ 1;
    /// length 2 at a coplanar overlap (multi-attribution).
    pub surface: Vec<Vec<InputId>>,
    /// Per triangle: `inside[t][k]` is true iff the triangle is inside solid
    /// `k`. Each inner vec has length `num_inputs`.
    pub inside: Vec<Vec<bool>>,
    /// Per triangle: the Cherchi patch id the triangle belongs to.
    pub patch: Vec<u32>,
    /// Number of input solids (2 for a binary boolean).
    pub num_inputs: u32,
}

impl LabeledArrangement {
    /// Apply Cherchi's exact op keep-rules to the arrangement, returning the
    /// kept triangle indices in ASCENDING order.
    ///
    /// The rules read only `surface` / `inside` / `num_inputs` — no geometry,
    /// no tolerance. Each rule mirrors the corresponding `code/booleans.cpp`
    /// keep-loop. `surface[t]` is the set of solids whose surface the triangle
    /// lies on (the C++ `labels.surface[t]` bitset); `inside[t][k]` is the C++
    /// `labels.inside[t][k]` bit; `num_inputs` is `labels.num`.
    pub fn keep_set(&self, op: BoolOp) -> Vec<usize> {
        let n = self.mesh.tris.len();
        let mut keep = Vec::new();
        for t in 0..n {
            if self.keep_tri(op, t) {
                keep.push(t);
            }
        }
        keep
    }

    /// True iff triangle `t` survives op `op` under Cherchi's keep-rule.
    fn keep_tri(&self, op: BoolOp, t: usize) -> bool {
        let surface = &self.surface[t];
        let inside = &self.inside[t];
        let num = self.num_inputs as usize;

        // Number of solids the triangle is inside (`labels.inside[t].count()`).
        let inside_count = inside.iter().filter(|&&b| b).count();
        // `surface[t][k]` membership test (the C++ bitset bit-`k`).
        let on_surface = |k: u32| surface.iter().any(|&InputId(id)| id == k);
        // `(surface ^ inside).count()`: count solids where surface-membership
        // and inside-bit differ.
        let xor_count = (0..num)
            .filter(|&k| on_surface(k as u32) != inside[k])
            .count();

        match op {
            // Union (booleans.cpp:1413-1428): keep iff inside is all-false.
            BoolOp::Union => inside_count == 0,
            // Intersection (booleans.cpp:1394-1409):
            // keep iff (surface ^ inside).count() == num_inputs.
            BoolOp::Intersect => xor_count == num,
            // Subtraction A−B, A = solid 0 (booleans.cpp:1432-1459):
            // keep iff (surface[0] && inside.count()==0)
            //       OR (!surface[0] && inside[0] && inside.count()==1).
            BoolOp::Subtract => {
                (on_surface(0) && inside_count == 0)
                    || (!on_surface(0) && inside[0] && inside_count == 1)
            }
            // Xor (booleans.cpp:1463-1485):
            // keep iff inside.count()==0 OR (surface ^ inside).count()==num_inputs.
            BoolOp::Xor => inside_count == 0 || xor_count == num,
        }
    }
}

#[cfg(test)]
mod tests {
    // These imports reference the production types the Implementer will add to
    // this module. Until then this is a compile-failure RED (intentional).
    use super::{InputId, LabeledArrangement};
    use crate::Mesh;
    use cad_primitives::{BoolOp, Point3};

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
    /// Derivations (num_inputs = 2) — fenced to keep the alignment verbatim:
    /// ```text
    /// Union  (inside.count()==0):        only tri 0            -> [0]
    /// Inter  ((surface ^ inside).count()==2):
    ///   tri 0: {0} ^ {}    = {0}    count 1  NO
    ///   tri 1: {0} ^ {1}   = {0,1}  count 2  KEEP
    ///   tri 2: {1} ^ {0}   = {0,1}  count 2  KEEP   -> [1, 2]
    /// Subtr  (surface[0]&&inside.count()==0  OR  !surface[0]&&inside[0]&&inside.count()==1):
    ///   tri 0: surface[0]=t, inside.count()=0            KEEP (branch 1)
    ///   tri 1: surface[0]=t, inside.count()=1            drop  (b1 no; b2 needs !surface[0])
    ///   tri 2: surface[0]=f, inside[0]=t, count==1       KEEP (branch 2)   -> [0, 2]
    /// Xor    (inside.count()==0 OR (surface ^ inside).count()==2):
    ///   tri 0: count 0                   KEEP
    ///   tri 1: (surface^inside).count 2  KEEP
    ///   tri 2: (surface^inside).count 2  KEEP            -> [0, 1, 2]
    /// ```
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
        assert_eq!(
            keep,
            vec![0usize],
            "union keeps only the all-false-inside tri"
        );
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
