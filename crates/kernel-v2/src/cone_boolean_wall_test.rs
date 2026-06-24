//! KV6c increment 5: cone-frustum boolean operands hit a LOUD, typed wall.
//!
//! kernel-v2 revolve produces frustum-band cones (two rims); yang's B-Rep
//! models only apex-pointed cones (one base rim, fanned from the apex). So a
//! cone solid cannot currently be a boolean operand — but it must fail loudly
//! and typed, never silently wrong. Unblocking this needs yang frustum-cone
//! support (increment 5b, a yang-rs change). This test pins the boundary.
#[cfg(test)]
mod tests {
    use crate::arena::{BrepArena, UnitVector3};
    use crate::boolean_op;
    use crate::cone_fixtures::build_frustum;
    use crate::construct::extrude;
    use crate::error::KernelV2Error;
    use crate::profile::Profile;
    use cad_primitives::{BoolOp, Point2, Point3, Vector3};
    use std::f64::consts::FRAC_PI_4;

    #[test]
    fn cone_frustum_boolean_operand_is_walled_typed() {
        let plus_z = UnitVector3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        };
        // Solid truncated cone (bucket): base r=1 @ z=1, top r=3 @ z=3.
        let (mut arena, frustum, _lat) = build_frustum(
            Point3::new(0.0, 0.0, 0.0),
            plus_z,
            1.0,
            3.0,
            FRAC_PI_4,
            FRAC_PI_4,
        );
        // A box overlapping the cone's side.
        let prof = Profile::new(
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(1.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            vec![
                Point2::new(1.5, -1.0),
                Point2::new(4.0, -1.0),
                Point2::new(4.0, 1.0),
                Point2::new(1.5, 1.0),
            ],
            vec![],
        )
        .expect("box profile");
        let bx = extrude(&mut arena, &prof, Vector3::new(0.0, 0.0, 1.0), 4.0).expect("box");

        // Every op walls loudly with the typed curved-boolean reason — no
        // panic, no silent-wrong output.
        for op in [BoolOp::Union, BoolOp::Subtract, BoolOp::Intersect] {
            let mut a2 = arena.clone();
            let err = boolean_op(&mut a2, frustum, bx.solid, op)
                .expect_err("cone-frustum operand must be walled");
            assert!(
                matches!(err, KernelV2Error::UnsupportedConeBoolean { .. }),
                "expected UnsupportedConeBoolean, got {err:?}"
            );
        }
    }
}
