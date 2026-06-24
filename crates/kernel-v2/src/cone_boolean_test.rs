//! KV6c increment 5c: a cone-frustum solid is a working boolean operand when
//! its lateral survives WHOLE.
//!
//! yang ingests two-rim frustum cones (`tessellate_cone_frustum_band`, 5b);
//! kernel-v2 converts cone faces to/from yang and `recover.rs` canonicalizes a
//! surviving cone band (two rims → seamed `[rim,seam,rim,seam]`). A boolean
//! that cuts the cone with a plane ⊥ its axis keeps the cone face a whole
//! two-rim frustum end to end: the op succeeds, the result validates and
//! tessellates, and a cone face survives. (Booleans that cut the lateral
//! obliquely produce conic-bounded cone patches — a later increment.)
#[cfg(test)]
mod tests {
    use crate::arena::{BrepArena, SolidId, Surface, UnitVector3};
    use crate::boolean_op;
    use crate::cone_fixtures::build_frustum;
    use crate::construct::extrude;
    use crate::profile::Profile;
    use crate::{tessellate, validate_solid};
    use cad_primitives::{BoolOp, Point2, Point3, Vector3};
    use std::f64::consts::FRAC_PI_4;

    /// Build a 45° solid frustum (apex at origin, axis +z: base r=1 @ z=1, top
    /// r=3 @ z=3) plus a big horizontal slab (x,y ∈ [-5,5], z=-1 .. 2). The
    /// slab's top plane (z=2, ⊥ the axis) cuts the cone in a CIRCLE, so any
    /// boolean leaves the upper band [z=2 rim … z=3 rim] a whole two-rim
    /// frustum. Returns (arena, frustum solid, slab solid).
    fn frustum_and_slab() -> (BrepArena, SolidId, SolidId) {
        let plus_z = UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        };
        let (mut arena, frustum, _lat) = build_frustum(
            Point3::new(0.0, 0.0, 0.0),
            plus_z,
            1.0,
            3.0,
            FRAC_PI_4,
            FRAC_PI_4,
        );
        let prof = Profile::new(
            Point3::new(0.0, 0.0, -1.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            vec![
                Point2::new(-5.0, -5.0),
                Point2::new(5.0, -5.0),
                Point2::new(5.0, 5.0),
                Point2::new(-5.0, 5.0),
            ],
            vec![],
        )
        .expect("slab profile");
        let slab = extrude(&mut arena, &prof, Vector3::new(0.0, 0.0, 1.0), 3.0).expect("slab");
        (arena, frustum, slab.solid)
    }

    fn assert_cone_survives(arena: &BrepArena, out: SolidId, what: &str) {
        validate_solid(arena, out)
            .unwrap_or_else(|e| panic!("{what}: result must validate: {e:?}"));
        let shells = &arena.solid(out).expect("solid").shells;
        let has_cone = shells.iter().any(|&sh| {
            arena.shell(sh).expect("shell").faces.iter().any(|&fc| {
                matches!(
                    arena.face(fc).expect("face").surface,
                    Some(Surface::Cone { .. })
                )
            })
        });
        assert!(has_cone, "{what}: a cone lateral survives");
        let mesh = tessellate(arena, out).unwrap_or_else(|e| panic!("{what}: tessellates: {e:?}"));
        assert!(!mesh.indices.is_empty(), "{what}: non-empty mesh");
    }

    #[test]
    fn cone_frustum_survives_union_whole() {
        let (mut arena, frustum, slab) = frustum_and_slab();
        let out = boolean_op(&mut arena, frustum, slab, BoolOp::Union)
            .expect("frustum ∪ slab succeeds with the cone band whole");
        assert_cone_survives(&arena, out, "union");
    }

    #[test]
    fn cone_frustum_survives_subtract_whole() {
        // frustum − slab removes the lower band (z∈[1,2]); the upper cone band
        // [z=2 … z=3] survives whole.
        let (mut arena, frustum, slab) = frustum_and_slab();
        let out = boolean_op(&mut arena, frustum, slab, BoolOp::Subtract)
            .expect("frustum − slab succeeds with the upper cone band whole");
        assert_cone_survives(&arena, out, "subtract");
    }
}
