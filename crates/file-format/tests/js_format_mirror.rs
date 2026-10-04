//! The app's JS mirror of the format constants must equal the Rust writer's.
//!
//! `app/src/lib/engine/format.js` carries `FORMAT_VERSION` / `MIN_READER_VERSION`
//! for the read paths that run without the engine — including `fileTooNew`,
//! which refuses a file whose version exceeds the JS constant. When the Rust
//! writer is bumped and the mirror is not, the app refuses the engine's OWN
//! saves ("saved by a newer version"): measured 2026-10-04 after the D4b bump
//! to v11 left the mirror at 10 — eight gui-relay specs red for a day (every
//! `document_new` → save → reopen). No test tier compared the two; this does.
use file_format::{FORMAT_VERSION, MIN_READER_VERSION};

fn js_const(src: &str, name: &str) -> u32 {
    let needle = format!("export const {name} = ");
    let start = src
        .find(&needle)
        .unwrap_or_else(|| panic!("`{needle}` not found in format.js"))
        + needle.len();
    let digits: String = src[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("`{name}` in format.js is not an integer literal"))
}

#[test]
fn app_format_mirror_matches_the_rust_writer() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src/lib/engine/format.js");
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(
        js_const(&src, "FORMAT_VERSION"),
        FORMAT_VERSION,
        "app/src/lib/engine/format.js FORMAT_VERSION must equal file_format::FORMAT_VERSION — \
         bump both together (the app refuses the engine's own saves otherwise)"
    );
    assert_eq!(
        js_const(&src, "MIN_READER_VERSION"),
        MIN_READER_VERSION,
        "app/src/lib/engine/format.js MIN_READER_VERSION must equal file_format::MIN_READER_VERSION"
    );
}
