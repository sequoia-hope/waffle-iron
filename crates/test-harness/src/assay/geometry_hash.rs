//! Canonical geometry hash of a kernel result (remediation plan O7).
//!
//! The determinism tests used to compare (V, E, F) COUNTS and a mesh volume
//! within 1.0 absolute — a check two different solids pass as long as they
//! have the same number of faces. This module hashes the result's exact
//! B-Rep instead, bit for bit, through the public [`KernelIntrospect`]
//! traits only:
//!
//! - every vertex's position (`compute_signature(..).centroid`),
//! - every edge's geometry (`edge_polyline`, canonicalised to the
//!   lexicographically smaller of its two directions, plus its length),
//! - every face's signature (surface type, area, centroid, normal, bbox,
//!   axis descriptor) together with the multiset of its bounding edges'
//!   geometry keys — the incidence structure, without any id,
//! - the solid's exact volume (`solid_volume`), or its error text.
//!
//! Every `f64` enters as `to_bits()` — no rounding, no tolerance. Each
//! entity becomes a canonical record, the records of a kind are SORTED, and
//! the sorted lists are hashed, so the result does not depend on the
//! arena's allocation order or on `KernelId` values. Multi-body results
//! hash the sorted list of per-body digests.
//!
//! The hash is FNV-1a 64 over the canonical `u64` words: fixed, platform-
//! and toolchain-independent (unlike `std`'s `DefaultHasher`, whose
//! algorithm is unspecified), so two builds of the kernel — e.g. with and
//! without `cherchi-rs/parallel` — can compare hashes printed by separate
//! processes.

use waffle_types::kernel::{KernelIntrospect, KernelSolidHandle};
use waffle_types::topo::{TopoKind, TopoSignature};

use crate::helpers::HarnessError;
use crate::workflow::ModelBuilder;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64 over little-endian `u64` words.
pub fn fnv1a_words(words: &[u64]) -> u64 {
    let mut h = FNV_OFFSET;
    for w in words {
        for b in w.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(FNV_PRIME);
        }
    }
    h
}

/// A canonical record under construction: a flat list of `u64` words with
/// explicit presence tags, so `None` and `Some(0.0)` never collide.
#[derive(Default)]
struct Rec(Vec<u64>);

impl Rec {
    fn f(&mut self, x: f64) {
        self.0.push(x.to_bits());
    }
    fn of(&mut self, x: Option<f64>) {
        match x {
            Some(v) => {
                self.0.push(1);
                self.f(v);
            }
            None => self.0.push(0),
        }
    }
    fn v3(&mut self, p: [f64; 3]) {
        p.into_iter().for_each(|c| self.f(c));
    }
    fn ov3(&mut self, p: Option<[f64; 3]>) {
        match p {
            Some(v) => {
                self.0.push(1);
                self.v3(v);
            }
            None => self.0.push(0),
        }
    }
    fn s(&mut self, s: &str) {
        self.0.push(s.len() as u64);
        self.0.extend(s.bytes().map(u64::from));
    }
    fn digest(&self) -> u64 {
        fnv1a_words(&self.0)
    }
}

fn bits3(p: [f64; 3]) -> [u64; 3] {
    [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
}

/// Direction-independent key of an edge's geometry: its polyline in bits,
/// taken in whichever direction is lexicographically smaller.
fn polyline_key(poly: &[[f64; 3]]) -> u64 {
    let fwd: Vec<[u64; 3]> = poly.iter().map(|p| bits3(*p)).collect();
    let mut rev = fwd.clone();
    rev.reverse();
    let pick = if rev < fwd { rev } else { fwd };
    let mut words = vec![pick.len() as u64];
    for p in pick {
        words.extend(p);
    }
    fnv1a_words(&words)
}

fn face_record(sig: &TopoSignature, edge_keys: &mut [u64]) -> u64 {
    let mut r = Rec::default();
    r.s(sig.surface_type.as_deref().unwrap_or("<none>"));
    r.of(sig.area);
    r.ov3(sig.centroid);
    r.ov3(sig.normal);
    match sig.bbox {
        Some(b) => {
            r.0.push(1);
            b.into_iter().for_each(|c| r.f(c));
        }
        None => r.0.push(0),
    }
    match sig.axis {
        Some(a) => {
            r.0.push(1);
            r.ov3(a.direction);
            r.of(a.radius);
            r.of(a.half_angle);
            r.of(a.minor_radius);
            r.of(a.extent);
        }
        None => r.0.push(0),
    }
    edge_keys.sort_unstable();
    r.0.push(edge_keys.len() as u64);
    r.0.extend(edge_keys.iter().copied());
    r.digest()
}

/// The canonical digest of ONE solid's exact B-Rep (see the module doc).
pub fn solid_geometry_digest(k: &dyn KernelIntrospect, solid: &KernelSolidHandle) -> u64 {
    let mut vertices: Vec<u64> = k
        .list_vertices(solid)
        .into_iter()
        .map(|v| {
            let mut r = Rec::default();
            r.ov3(k.compute_signature(v, TopoKind::Vertex).centroid);
            r.digest()
        })
        .collect();
    vertices.sort_unstable();

    let mut edges: Vec<u64> = k
        .list_edges(solid)
        .into_iter()
        .map(|e| {
            let mut r = Rec::default();
            r.0.push(polyline_key(&k.edge_polyline(e)));
            r.of(k.compute_signature(e, TopoKind::Edge).length);
            r.digest()
        })
        .collect();
    edges.sort_unstable();

    let mut faces: Vec<u64> = k
        .list_faces(solid)
        .into_iter()
        .map(|f| {
            let mut edge_keys: Vec<u64> = k
                .face_edges(f)
                .into_iter()
                .map(|e| polyline_key(&k.edge_polyline(e)))
                .collect();
            face_record(&k.compute_signature(f, TopoKind::Face), &mut edge_keys)
        })
        .collect();
    faces.sort_unstable();

    let mut r = Rec::default();
    match k.solid_volume(solid) {
        Ok(v) => {
            r.0.push(1);
            r.f(v);
        }
        Err(e) => {
            r.0.push(0);
            r.s(&e.to_string());
        }
    }
    for list in [&vertices, &edges, &faces] {
        r.0.push(list.len() as u64);
        r.0.extend(list.iter().copied());
    }
    r.digest()
}

/// Order-independent hash over several solids (a multi-body result).
pub fn bodies_geometry_hash(k: &dyn KernelIntrospect, solids: &[KernelSolidHandle]) -> u64 {
    let mut digests: Vec<u64> = solids.iter().map(|s| solid_geometry_digest(k, s)).collect();
    digests.sort_unstable();
    let mut words = vec![digests.len() as u64];
    words.extend(digests);
    fnv1a_words(&words)
}

/// Canonical geometry hash of every body a named feature produced.
pub fn feature_geometry_hash(builder: &ModelBuilder, feature: &str) -> Result<u64, HarnessError> {
    let handles = builder.solid_handles(feature)?;
    Ok(bodies_geometry_hash(
        builder.kernel_ref().as_introspect(),
        &handles,
    ))
}

/// Canonical geometry hash of every LIVE body of the model
/// ([`ModelBuilder::live_solid_handles`]).
pub fn live_geometry_hash(builder: &ModelBuilder) -> u64 {
    bodies_geometry_hash(
        builder.kernel_ref().as_introspect(),
        &builder.live_solid_handles(),
    )
}

/// Append `<label> <hash>` to the file named by `WAFFLE_GEOMETRY_HASH_OUT`,
/// when that variable is set — the hook `scripts/determinism_parallel_parity.sh`
/// uses to diff the hashes of two separately-built kernels (`cherchi-rs`'s
/// `parallel` feature off and on). One `write` per line on an `O_APPEND`
/// file, so lines from parallel test threads do not interleave.
pub fn record_hash_for_parity(label: &str, hash: &str) {
    let Ok(path) = std::env::var("WAFFLE_GEOMETRY_HASH_OUT") else {
        return;
    };
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap_or_else(|e| panic!("WAFFLE_GEOMETRY_HASH_OUT={path}: {e}"));
    f.write_all(format!("{label} {hash}\n").as_bytes())
        .unwrap_or_else(|e| panic!("WAFFLE_GEOMETRY_HASH_OUT={path}: {e}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv_is_the_published_function() {
        // FNV-1a 64 of the empty input is the offset basis.
        assert_eq!(fnv1a_words(&[]), FNV_OFFSET);
        assert_ne!(fnv1a_words(&[0]), fnv1a_words(&[1]));
    }

    #[test]
    fn polyline_key_ignores_direction_but_not_geometry() {
        let a = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let b = [[1.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
        assert_eq!(polyline_key(&a), polyline_key(&b));
        // One ulp moves the key.
        let c = [
            [0.0, 0.0, 0.0],
            [f64::from_bits(1.0f64.to_bits() + 1), 0.0, 0.0],
        ];
        assert_ne!(polyline_key(&a), polyline_key(&c));
        // -0.0 and 0.0 are different bits, so a different key: the hash is
        // bit-exact by design.
        let d = [[-0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        assert_ne!(polyline_key(&a), polyline_key(&d));
    }

    #[test]
    fn none_and_zero_do_not_collide() {
        let mut a = Rec::default();
        a.of(None);
        a.of(Some(0.0));
        let mut b = Rec::default();
        b.of(Some(0.0));
        b.of(None);
        assert_ne!(a.digest(), b.digest());
    }
}
