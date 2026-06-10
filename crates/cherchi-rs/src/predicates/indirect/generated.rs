//! STUB (PR-CR-M7a RED) — replaced by `cargo run -p predicate-gen` in the
//! GREEN commit. Every function below is a transparently wrong placeholder
//! so the oracle tests have a compiling target to fail against.

use super::{GenericPoint3D, LambdaExact, LambdaF64, Sign};
use cad_primitives::Point3;
use dashu::rational::RBig;

pub(super) fn lpi_lambda_f64(
    _p: &Point3,
    _q: &Point3,
    _r: &Point3,
    _s: &Point3,
    _t: &Point3,
) -> LambdaF64 {
    LambdaF64 {
        l: [0.0; 3],
        d: 0.0,
        beta: 0.0,
        d_reliable: false,
    }
}

pub(super) fn tpi_lambda_f64(
    _v: &[Point3; 3],
    _w: &[Point3; 3],
    _u: &[Point3; 3],
) -> LambdaF64 {
    LambdaF64 {
        l: [0.0; 3],
        d: 0.0,
        beta: 0.0,
        d_reliable: false,
    }
}

pub(super) fn lpi_lambda_exact(
    _p: &Point3,
    _q: &Point3,
    _r: &Point3,
    _s: &Point3,
    _t: &Point3,
) -> LambdaExact {
    LambdaExact {
        l: [RBig::ZERO, RBig::ZERO, RBig::ZERO],
        d: RBig::ZERO,
    }
}

pub(super) fn tpi_lambda_exact(
    _v: &[Point3; 3],
    _w: &[Point3; 3],
    _u: &[Point3; 3],
) -> LambdaExact {
    LambdaExact {
        l: [RBig::ZERO, RBig::ZERO, RBig::ZERO],
        d: RBig::ZERO,
    }
}

pub(super) fn dispatch_canonical(
    _a: &GenericPoint3D,
    _b: &GenericPoint3D,
    _c: &GenericPoint3D,
    _d: &GenericPoint3D,
) -> Sign {
    Sign::Undefined
}

pub(super) fn dispatch_filtered_canonical(
    _a: &GenericPoint3D,
    _b: &GenericPoint3D,
    _c: &GenericPoint3D,
    _d: &GenericPoint3D,
) -> Option<Sign> {
    None
}

pub(super) fn dispatch_exact_canonical(
    _a: &GenericPoint3D,
    _b: &GenericPoint3D,
    _c: &GenericPoint3D,
    _d: &GenericPoint3D,
) -> Sign {
    Sign::Undefined
}
