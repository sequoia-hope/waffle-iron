//! Common types and utilities for the Cherchi mesh arrangement port.
//!
//! Ported from Cherchi common.h + utils.h
//! MIT License (c) 2020 Cherchi, Livesu, Scateni, Attene

/// Number of bits for coordinate representation.
/// Ported from common.h:41
#[allow(dead_code)]
pub(crate) const NBIT: u32 = 32;

/// Projection plane for 2D orientation tests.
/// Ported from common.h:44
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plane {
    XY,
    YZ,
    ZX,
}

/// Convert an integer normal-axis index to a Plane.
/// 0 → YZ, 1 → ZX, 2 → XY.
/// Ported from common.h:46-51
#[allow(dead_code)]
pub(crate) fn int_to_plane(norm: u32) -> Plane {
    match norm {
        0 => Plane::YZ,
        1 => Plane::ZX,
        _ => Plane::XY,
    }
}

/// Remove the first occurrence of `elem` from `vec`.
/// Ported from fast_trimesh.cpp:840-843 (removeFromVec)
pub(crate) fn remove_from_vec(vec: &mut smallvec::SmallVec<[usize; 16]>, elem: usize) {
    vec.retain(|x| *x != elem);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_int_to_plane() {
        assert_eq!(int_to_plane(0), Plane::YZ);
        assert_eq!(int_to_plane(1), Plane::ZX);
        assert_eq!(int_to_plane(2), Plane::XY);
        assert_eq!(int_to_plane(99), Plane::XY);
    }

    #[test]
    fn test_remove_from_vec() {
        let mut v: smallvec::SmallVec<[usize; 16]> = smallvec::smallvec![1, 2, 3, 2, 4];
        remove_from_vec(&mut v, 2);
        assert_eq!(v.as_slice(), &[1, 3, 4]);
    }
}
