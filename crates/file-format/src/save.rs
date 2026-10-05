use feature_engine::types::FeatureTree;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::document::WaffleDocument;
use crate::metadata::{DocumentMetadata, ProjectMetadata, Tab};
use crate::sources::SourceEntry;

/// Current file format version.
/// v1: original format (coordinates in mm-scale scene units)
/// v2: true-meters (all length coordinates in meters, angles unchanged)
/// v3: multi-tab document model
/// v4: document identity, `sources` table, opaque unknown tab/source kinds,
///     unknown-key preservation (`specs/waffle_v4_document_model.md`)
/// v5: `GeomRef.scope` — references into another tab's instance (in-context
///     editing, spec §2.8). A v4 reader would drop the field and resolve the
///     anchor locally, so the reader floor moved with it.
/// v6: `Sketch.plane_x_axis` — the sketch's own in-plane +x direction
///     (`docs/notes/eiffel/FEATURE_NOTES.md` §3). A v5 reader would drop it
///     and derive the basis from the normal, which draws the sketch — and
///     everything built on it — ROTATED. Same shape of defect as v5's, so
///     the floor moves with it.
/// v7: `FeatureTree.names` — entity names (N1,
///     `specs/agent_mechanical_design.md` §5.2). The field itself is additive
///     and defaulted, but its references persist `Selector::Pid` (drawings
///     spec D0), and `Selector` is a serde-tagged enum: a v6 reader given one
///     fails with a raw "unknown variant" parse error. A new selector variant
///     is a floor bump by the §13 rule, and this is the first increment that
///     writes one.
///   - **v8** (2026-10-03): `DesignParameter.unit` and `.comment` (P1,
///     `specs/agent_mechanical_design.md` §6). Both are additive, defaulted
///     and serialized only when present, and `unit`'s value is a bare string
///     a reader either knows or drops — so an old reader does not FAIL on
///     one. The floor still moves, because it must not silently ignore it:
///     `unit` is the author's statement that a parameter is an angle (or a
///     count), and a reader that drops it feeds that number to a length
///     field as millimetres — the exact coercion P1 exists to refuse. A
///     reader that drops it builds a DIFFERENT solid from the same file,
///     which is the v5/v6 rationale (`docs/FILE_FORMAT.md` §4, §13.3), and
///     `crates/feature-engine/tests/param_unit_floor.rs` measures it.
///   - **v9** (2026-10-03): `Sketch.plane_face` (N2,
///     `specs/agent_mechanical_design.md` §5.3 item 3) — the identity of the
///     model face a local sketch is drawn on. Additive, defaulted, omitted
///     when absent, and a reader that drops it does not FAIL. The floor moves
///     for the v8 reason: a reader that ignores it builds a DIFFERENT solid
///     from the same file. With the field, a sketch whose face has been
///     deleted refuses and nothing downstream of it builds; without it, the
///     sketch stays at its cached frame and extrudes into space — the silent
///     wrong answer N2 exists to remove. (Its content also carries a
///     `Selector::Pid`, which is the v7 reason over again, but the semantic
///     one is what settles it.)
///   - **v10** (2026-10-03): a `Selector::Pid`'s `pid` and `root_pid` are
///     written as decimal **STRINGS** rather than JSON numbers
///     (`waffle_types::pid_str`). A persistent id is a content-seeded 64-bit
///     hash and a JSON number in JavaScript is an `f64`, so every id above
///     `2^53` is silently rounded to a DIFFERENT entity when it crosses the
///     WASM↔JS boundary. The fix is one representation everywhere rather
///     than one per boundary — a type that serializes two ways is a
///     per-site decision, and `Selector::Pid` reaches the page inside a
///     dozen message fields. A v9 reader given a string pid fails with a
///     raw serde type error, so the floor moves. READING still accepts a
///     bare number, so every pre-v10 file loads unchanged
///     (`tests/format_tests.rs::a_pre_v10_numeric_pid_still_loads`).
///   - **v11** (2026-10-03): `Projection::Section` and `Projection::Detail`
///     (`specs/drawings_and_mbd.md` §8, D4b) — two new serde-tagged variants
///     inside a `Drawing` tab.
///
///     **This is the case D4a's no-bump argument explicitly excluded.** D4a
///     added a tab KIND, and since v4 a reader that does not know a tab kind
///     keeps the whole tab opaque and re-emits it verbatim, so nothing inside
///     it is deserialized and nothing can fail. A v10 reader DOES know the
///     `Drawing` tag — `TAB_KIND_TAGS` contains it — so `known_or_unknown`
///     takes the known branch and deserializes the drawing, and a
///     `Projection` tag it has never heard of is a `de::Error` for the WHOLE
///     DOCUMENT rather than an opaque tab. That is the v7 shape of the
///     problem (a new variant inside a kind every reader knows), and this
///     doc comment's own rule — "NEW `Operation`, constraint, selector and
///     `PlaneDefinition` variants (wire-breaking for old readers even though
///     they look additive)" — says the floor moves for it.
///
///     The bump is also the kinder failure. Without it a v10 build opens the
///     file, claims to have read it, and then fails somewhere inside a tab
///     with a raw serde message about a `Projection`; with it the same build
///     refuses up front with `LoadError::FutureVersion`, which names the
///     remedy. The cost is the one every floor bump pays and the one D4a
///     declined to pay for a change old readers COULD handle: a v10 build now
///     refuses documents with no drawing in them at all.
///
///     The rest of D4b is additive and defaulted — `Sheet.title_block`,
///     `Sheet.title_block_cache`, `DrawingView.cache_key`, and the `hatch`,
///     `marks` and `clip` fields of a persisted `ViewLayout` — and none of
///     those would have moved anything on their own.
///   - **v12** (2026-10-03): `DocumentMetadata.parameters` — the
///     DOCUMENT-level design-parameter table, and
///     `Instance.parameter_overrides` becoming load-bearing (P2,
///     `specs/agent_mechanical_design.md` §6). Both are additive and
///     defaulted, and an old reader does not FAIL on either — it DROPS them,
///     which is the v8/v9 harm exactly: one file, two solids. Without the
///     document table a tab's expression reading a document variable loses
///     its driver, and without the overrides every instance of a part builds
///     the part's default size instead of the size the instance was given.
///     Measured in
///     `crates/file-format/tests/param_scope_floor.rs`.
///   - **v14** (2026-10-04): M1's tolerances, materials and dimension units
///     (`specs/drawings_and_mbd.md` §9). **v12 and v13 were taken by other
///     branches in flight; this increment was assigned v14 at dispatch.**
///
///     TWO wire-breaking changes, either of which would move the floor on its
///     own:
///
///     1. `Annotation::FeatureControlFrame` — a new variant of a
///        serde-tagged enum inside a `Drawing` tab. Exactly the case v11's
///        note describes: a v13 reader KNOWS the `Drawing` tag, so it
///        deserializes the tab and an annotation tag it has never heard of
///        is a `de::Error` for the whole document. (`Dimension.tolerance`,
///        `.dual_precision`, the `materials` / `body_materials` tables and
///        ALL SIX `document` display settings — `precision`, `dual_unit`,
///        `dual_precision`, `inch_fraction`, `inch_denominator`, `fit_band`
///        — are additive and defaulted, and would have moved nothing alone.
///        Their round trip through this writer and `load::load_document` is
///        measured field by field in `tests/rev_persist_m1.rs`, including
///        that each travels alone and that an explicit `false` does not
///        collapse to absent: "explicitly decimal" and "unset" are different
///        states for a formatter that has its own default.)
///     2. `Dimension::Mass` and `Dimension::Density` — two new variants of
///        the enum `DesignParameter.unit` is written as
///        (`waffle_types::dimension`). A string value, but a serde enum
///        rather than a free-form one, so a v13 reader given
///        `"unit": "Mass"` fails with a raw unknown-variant error. The v8
///        row's own reasoning applies with more force: v8 bumped because a
///        reader must not silently IGNORE a declared unit, and this one
///        cannot even read it.
///
///     The material table is also the semantic reason of the v5/v6/v8/v9
///     kind, independently: a reader that drops `materials` and
///     `body_materials` reports a different MASS for the same file — or,
///     worse, reports the volume wearing kilograms. `mass(body)` refuses
///     outright without a material rather than defaulting its density, so a
///     reader that drops the table produces a loud absence rather than a
///     silent wrong number; the floor moves anyway, because the two wire
///     breaks above require it.
///   - **v15** (2026-10-04): the four ISOMETRIC `ProjectedDirection`
///     variants — `UpLeft`, `UpRight`, `DownLeft`, `DownRight`
///     (`specs/drawings_and_mbd.md` §8, D4e) — new serde-tagged variants
///     inside a `Drawing` tab's `Projection::ProjectedFrom`.
///
///     **The v11 shape of the problem, one level deeper.** A reader that
///     knows the `Drawing` tag deserializes the whole tab, so a
///     `ProjectedDirection` tag it has never heard of is a `de::Error` for
///     the WHOLE DOCUMENT — not an opaque tab — and the floor moves for the
///     same reason v11's new `Projection` variants moved it. With the bump a
///     v12 build refuses the file up front with `LoadError::FutureVersion`,
///     which names the remedy, instead of failing inside a tab with a raw
///     serde message about a variant.
///
///     v13 is deliberately skipped: D4e was dispatched alongside two other
///     branches that were assigned v13 and v14, and reusing one would have two
///     different changes claiming the same version. v14 is NOT skipped — M1's
///     row above took it, and both landed — so the live gap is v13 alone. A
///     gap in the sequence costs nothing (readers compare with `<=`, and
///     nothing enumerates the versions) where a collision costs a file that
///     two builds disagree about.
pub const FORMAT_VERSION: u32 = 15;

/// Oldest reader (by its `FORMAT_VERSION`) that can parse files we write.
///
/// Written into every file as `min_reader_version`; readers refuse files whose
/// `min_reader_version` exceeds their own `FORMAT_VERSION` with a clean
/// `LoadError::FutureVersion` instead of a raw serde parse error. Bump this
/// (together with `FORMAT_VERSION`) whenever a change lands that older readers
/// cannot parse — in this format that includes NEW `Operation`, constraint,
/// selector and `PlaneDefinition` variants (wire-breaking for old readers
/// even though they look additive). Since v4 a new **tab kind, source kind or
/// locator kind** does NOT require a bump: v4 readers preserve unknown ones
/// opaquely. Purely additive defaulted fields never require a bump. Files
/// without the field (all pre-2026-08-28 files, including the assay corpus)
/// default to 0 and always pass. See `docs/FILE_FORMAT.md` §13.
pub const MIN_READER_VERSION: u32 = 15;

// Keep the constants coherent: we can never require a reader newer than the
// version we claim to write.
const _: () = assert!(MIN_READER_VERSION <= FORMAT_VERSION);

/// The top-level v2 file structure (kept for deserialization compat).
#[derive(Debug, Clone, Serialize)]
pub struct WaffleFile {
    /// Format identifier.
    pub format: String,
    /// Format version number.
    pub version: u32,
    /// Project metadata.
    pub project: ProjectMetadata,
    /// The feature tree (the parametric recipe).
    pub features: FeatureTree,
}

/// V4 top-level file structure.
#[derive(Debug, Clone, Serialize)]
pub struct WaffleFileV4<'a> {
    pub format: &'static str,
    pub version: u32,
    /// See [`MIN_READER_VERSION`]. Old readers ignore this unknown field.
    pub min_reader_version: u32,
    pub document: &'a DocumentMetadata,
    pub sources: &'a [SourceEntry],
    pub tabs: &'a [Tab],
    pub active_tab: &'a str,
    /// Unknown envelope keys, re-emitted (v4 §2.6).
    #[serde(flatten)]
    pub extra: &'a Map<String, Value>,
}

/// Serialize a v4 document to pretty-printed JSON. **The** writer: every
/// production save path composes its bytes here (v4 §4 invariant 7).
pub fn save_document(doc: &WaffleDocument) -> String {
    let file = WaffleFileV4 {
        format: "waffle-iron",
        version: FORMAT_VERSION,
        min_reader_version: MIN_READER_VERSION,
        document: &doc.document,
        sources: &doc.sources,
        tabs: &doc.tabs,
        active_tab: &doc.active_tab,
        extra: &doc.extra,
    };
    serde_json::to_string_pretty(&file).expect("Document serialization should never fail")
}

/// [`save_document`] plus a self-check: never hand out a file the loader
/// would refuse (v4 §4 invariant 8). The known corruption class is non-finite
/// floats — serde_json serializes NaN/∞ as `null`, which every reader then
/// rejects; the round-trip check catches that and any future class of
/// save-side corruption without enumerating float fields.
pub fn save_document_verified(doc: &WaffleDocument) -> Result<String, crate::errors::LoadError> {
    let json = save_document(doc);
    crate::load::load_document(&json)?;
    Ok(json)
}

/// [`save_document_verified`] that does not re-check what it has already
/// checked.
///
/// The self-check above re-parses the WHOLE document on every save, and a
/// host autosaves after every committed tool: on the Eiffel Tower that was
/// 69.6 ms of re-parsing against 5.7 ms to produce the bytes — a 12x tax,
/// paid mostly on tabs that had not changed since the last save verified them
/// (docs/notes/eiffel/FEATURE_NOTES.md §0a).
///
/// A document is its tabs plus a small envelope. This verifier checks each in
/// the same way the loader would, and remembers the tab payloads it has
/// already accepted — a tab whose bytes are byte-for-byte what was verified
/// before cannot have become unparseable. So a save costs a parse of the tabs
/// that changed, not of the document.
#[derive(Debug, Default)]
pub struct SaveVerifier {
    /// Hashes of tab payloads that parsed back cleanly.
    verified: std::collections::HashSet<u64>,
}

/// Bound on remembered payloads: enough for every tab of a document across
/// many edits, small enough that a long session cannot grow it without limit.
const VERIFIED_CAP: usize = 512;

impl SaveVerifier {
    /// Serialize `doc` and verify it, skipping the tabs already known good.
    pub fn save(&mut self, doc: &WaffleDocument) -> Result<String, crate::errors::LoadError> {
        use std::hash::{Hash, Hasher};

        for tab in &doc.tabs {
            // Canonical bytes, via `serde_json::Value` — whose maps are
            // BTreeMaps, so keys come out sorted. Serializing the tab directly
            // would NOT do: the tree holds HashMaps (`body_names`,
            // `provenance`, a sketch's `solved_positions`), so the same tab
            // serializes differently run to run and nothing would ever match
            // the cache.
            let value = serde_json::to_value(tab)
                .map_err(|e| crate::errors::LoadError::ParseError(e.to_string()))?;
            let payload = serde_json::to_string(&value)
                .map_err(|e| crate::errors::LoadError::ParseError(e.to_string()))?;
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            payload.hash(&mut hasher);
            let key = hasher.finish();
            if self.verified.contains(&key) {
                continue;
            }
            // The loader's own typed parse, on this tab alone.
            serde_json::from_str::<Tab>(&payload)
                .map_err(|e| crate::errors::LoadError::ParseError(e.to_string()))?;
            if self.verified.len() >= VERIFIED_CAP {
                self.verified.clear();
            }
            self.verified.insert(key);
        }

        let json = save_document(doc);
        // Everything that is not a tab, parsed as the loader parses it: the
        // envelope, the document metadata and the sources. `IgnoredAny` skips
        // the tabs, which the loop above accounted for.
        crate::load::check_document_shell(&json)?;
        // Cross-tab rules (active_tab resolves, source ids unique) — the same
        // call `load_document` ends with, and it costs nothing.
        doc.validate()?;
        Ok(json)
    }
}

/// Serialize a single feature tree as a v4 document with one Part tab.
/// Legacy in-feature STEP payloads are lifted into the `sources` table.
pub fn save_project(tree: &FeatureTree, metadata: &ProjectMetadata) -> String {
    save_document(&WaffleDocument::single_part(metadata, tree.clone()))
}

/// [`save_project`] plus the loader self-check (see
/// [`save_document_verified`]). Production single-tree save paths use this.
pub fn save_project_verified(
    tree: &FeatureTree,
    metadata: &ProjectMetadata,
) -> Result<String, crate::errors::LoadError> {
    let json = save_project(tree, metadata);
    crate::load::load_project(&json)?;
    Ok(json)
}
