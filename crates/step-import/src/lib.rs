//! STEP file import (task #138 — `docs/step_import_roadmap.md`).
//!
//! Converts ISO-10303-21 STEP text (canonical case: a KiCad-exported PCB,
//! AP214 written by OpenCascade) into the neutral
//! [`waffle_types::ImportedBodyData`] contract: every solid/shell from every
//! assembly path, tessellated per-face, with analytic surface classification,
//! baked into world coordinates in meters — the "composite wrap".
//!
//! Parsing and geometry realization ride `truck-stepio`/`truck-meshalgo`
//! (Apache-2.0, git-pinned — see Cargo.toml). truck types must never leak
//! past this crate's API.

pub mod blob;
mod convert;
mod units;

pub use blob::{decode_step_blob, encode_step_blob, STEP_BLOB_ENCODING};
pub use units::scan_length_unit_scale;

use std::sync::{Arc, Mutex, OnceLock};
use waffle_types::kernel::ImportedBodyData;

/// Errors from STEP import. `Parse` means the exchange structure itself was
/// rejected; `Convert` means entities resolved but geometry realization
/// failed; `NoSolids` means the file contained nothing we can turn into a
/// body (e.g. wireframe-only data).
#[derive(Debug, Clone, thiserror::Error)]
pub enum StepImportError {
    #[error("STEP parse failed: not a readable ISO-10303-21 exchange structure")]
    Parse,
    #[error("STEP conversion failed: {0}")]
    Convert(String),
    #[error("STEP file contains no solids or shells")]
    NoSolids,
}

/// Parse STEP text into a single composite imported body, in meters.
///
/// Deterministic: same text → same output. The tessellation tolerance is
/// derived per shell from its bounding box (diameter / 1000).
pub fn parse_step(step_text: &str, source_name: &str) -> Result<ImportedBodyData, StepImportError> {
    convert::parse_step_impl(step_text, source_name)
}

/// `parse_step` behind a small process-wide cache keyed by the text's hash,
/// so a parametric rebuild (transform edits replay the import feature) does
/// not re-parse and re-tessellate an unchanged file. Callers clone the Arc'd
/// canonical result and apply their own scale/placement to the clone.
pub fn parse_step_cached(
    step_text: &str,
    source_name: &str,
) -> Result<Arc<ImportedBodyData>, StepImportError> {
    // Keep the last few DISTINCT imports (a session rarely juggles more).
    const CACHE_CAP: usize = 4;
    type CacheEntries = Vec<(u64, Arc<ImportedBodyData>)>;
    static CACHE: OnceLock<Mutex<CacheEntries>> = OnceLock::new();

    let key = fnv1a(step_text.as_bytes());
    let cache = CACHE.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(guard) = cache.lock() {
        if let Some((_, data)) = guard.iter().find(|(k, _)| *k == key) {
            return Ok(Arc::clone(data));
        }
    }
    let parsed = Arc::new(parse_step(step_text, source_name)?);
    if let Ok(mut guard) = cache.lock() {
        if guard.len() >= CACHE_CAP {
            guard.remove(0);
        }
        guard.push((key, Arc::clone(&parsed)));
    }
    Ok(parsed)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUBE: &str = include_str!("../tests/fixtures/cube.step");

    #[test]
    fn parse_rejects_garbage() {
        assert!(matches!(
            parse_step("not a step file", "garbage"),
            Err(StepImportError::Parse)
        ));
    }

    #[test]
    fn parse_cube_fixture() {
        let body = parse_step(CUBE, "cube").expect("cube parses");
        assert_eq!(body.shells.len(), 1);
        assert_eq!(body.face_count(), 6);
        let shell = &body.shells[0];
        // Every face planar, meshed, with boundary edges attached.
        for face in &shell.faces {
            assert_eq!(face.surface.surface_type_str(), "planar");
            assert!(!face.indices.is_empty());
            assert_eq!(face.positions.len(), face.normals.len());
            assert!(!face.edge_indices.is_empty());
        }
        assert_eq!(shell.edges.len(), 12);
        // The fixture cube is 10×10×10 mm at the origin: verify meter scaling.
        let mut max = [f64::MIN; 3];
        let mut min = [f64::MAX; 3];
        for f in &shell.faces {
            for p in f.positions.chunks_exact(3) {
                for k in 0..3 {
                    max[k] = max[k].max(p[k]);
                    min[k] = min[k].min(p[k]);
                }
            }
        }
        for k in 0..3 {
            assert!((max[k] - min[k] - 0.010).abs() < 1e-9, "10mm → 0.010m");
        }
    }

    #[test]
    fn parse_cylinder_fixture_classifies_lateral() {
        let body = parse_step(include_str!("../tests/fixtures/cylinder.step"), "cylinder")
            .expect("cylinder parses");
        let shell = &body.shells[0];
        let types: Vec<&str> = shell
            .faces
            .iter()
            .map(|f| f.surface.surface_type_str())
            .collect();
        assert!(
            types.contains(&"planar"),
            "caps should be planar: {types:?}"
        );
        // truck writes the full-turn lateral as swept/rotated geometry — it
        // must classify as a non-planar kind, never as "planar".
        assert!(
            types.iter().any(|t| *t != "planar"),
            "lateral must not be planar: {types:?}"
        );
    }

    /// Real-world KiCad fixtures live in `refs/step/` (gitignored — license;
    /// see docs/step_import_roadmap.md §5). Run manually with `-- --ignored`.
    #[test]
    #[ignore = "refs-fixture: needs local refs/step/USB_C.step (gitignored)"]
    fn parse_kicad_usb_c_composite() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../refs/step/USB_C.step");
        let text = std::fs::read_to_string(path).expect("download per roadmap §0");
        let body = parse_step(&text, "USB_C").expect("USB_C parses");
        assert_eq!(body.shells.len(), 34, "34 solids wrap into one composite");
        assert!(body.face_count() >= 500);
    }

    #[test]
    fn parse_cached_returns_shared_result() {
        let a = parse_step_cached(CUBE, "cube").unwrap();
        let b = parse_step_cached(CUBE, "cube").unwrap();
        assert!(Arc::ptr_eq(&a, &b), "second parse must hit the cache");
    }

    #[test]
    fn parse_cube_deterministic() {
        let a = parse_step(CUBE, "cube").unwrap();
        let b = parse_step(CUBE, "cube").unwrap();
        assert_eq!(a.face_count(), b.face_count());
        let fa = &a.shells[0].faces[0];
        let fb = &b.shells[0].faces[0];
        assert_eq!(fa.positions, fb.positions);
        assert_eq!(fa.indices, fb.indices);
    }
}
