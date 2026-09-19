use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use waffle_types::{GeomRef, OutputKey, Sketch};

/// User-assigned body display names, keyed by a body's persistent identity
/// (`"{feature_id}/{output_key.tag()}"`). Absent ⇒ the body uses a derived
/// name (its producing feature's name). Stored on the tree so it persists with
/// the document; `#[serde(default)]` keeps older files (no field) loading.
pub type BodyNames = HashMap<String, String>;

/// Who or what created a feature (`specs/waffle_v4_document_model.md` §2.7).
/// Absent from the table means `User`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum ProvenanceOrigin {
    /// Authored interactively.
    User,
    /// Authored by a tool/model through the programmatic surface.
    Agent { name: String },
    /// Created by importing a source (e.g. the STEP body of `sources[i]`).
    Import { source_id: Uuid },
    /// Regenerated from a source by a named rule (e.g. a board outline from
    /// a `.kicad_pcb`); read-only in the UI, replaced on re-sync.
    Derived { source_id: Uuid, rule: String },
}

/// Provenance record for one feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Provenance {
    pub origin: ProvenanceOrigin,
    /// RFC 3339 timestamp; optional (the engine has no clock of its own).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

/// Feature id → provenance. Only non-`User` origins are worth recording.
pub type ProvenanceTable = HashMap<Uuid, Provenance>;

/// A named design variable (parameter) on the feature tree.
///
/// `expression` is evaluated in mm-space (see `crate::expr`): bare numeric
/// literals mean millimeters in length contexts / degrees in angle contexts;
/// unit suffixes (`in`, `cm`, ...) scale literals; other parameters may be
/// referenced by name in any order (cycles are a loud per-parameter error).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DesignParameter {
    /// Stable identity (error routing, undo bookkeeping).
    pub id: Uuid,
    /// Identifier used in expressions: `[A-Za-z_][A-Za-z0-9_]*`, not reserved.
    pub name: String,
    /// The defining expression, e.g. `"25"`, `"width / 2"`, `"1.5in"`.
    pub expression: String,
    /// Cached last-good evaluated value (mm-space), refreshed each rebuild.
    /// Kept on evaluation failure so dependents hold their last geometry.
    #[serde(default)]
    pub value: f64,
    /// Evaluation error from the last rebuild (`None` = evaluated cleanly).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl DesignParameter {
    pub fn new(name: impl Into<String>, expression: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            expression: expression.into(),
            value: 0.0,
            error: None,
        }
    }
}

/// The ordered list of modeling features.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct FeatureTree {
    /// Ordered list of features. Index 0 is the first feature.
    pub features: Vec<Feature>,
    /// Features after this index are suppressed during rebuild.
    /// None means all features are active.
    pub active_index: Option<usize>,
    /// User-assigned body names, independent of feature names.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub body_names: BodyNames,
    /// Named design variables. Order is display order only; expressions may
    /// reference any parameter regardless of position.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<DesignParameter>,
    /// Feature provenance (v4 §2.7): who/what created each feature. Keyed by
    /// feature id; GC'd on feature delete (captured for undo) like
    /// `body_names`. Absent ⇒ `User`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub provenance: ProvenanceTable,
    /// Unknown keys preserved across load → save (v4 §2.6). Tool-added
    /// metadata should use an `x-` prefix so a future official field cannot
    /// collide.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl FeatureTree {
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
            active_index: None,
            body_names: HashMap::new(),
            parameters: Vec::new(),
            provenance: HashMap::new(),
            extra: serde_json::Map::new(),
        }
    }

    /// Persistent identity string for a body: its producing feature plus which
    /// output of that feature it is. This is the key into `body_names`.
    pub fn body_id(feature_id: Uuid, output_key: &OutputKey) -> String {
        format!("{}/{}", feature_id, output_key.tag())
    }

    /// Set (or clear, with `None`) a body's display-name override. Returns the
    /// previous override, if any. No rebuild needed — names don't affect geometry.
    pub fn set_body_name(&mut self, body_id: &str, name: Option<String>) -> Option<String> {
        match name {
            Some(n) => self.body_names.insert(body_id.to_string(), n),
            None => self.body_names.remove(body_id),
        }
    }

    /// Look up a body's display-name override, if the user has set one.
    pub fn body_name_override(&self, body_id: &str) -> Option<&str> {
        self.body_names.get(body_id).map(String::as_str)
    }

    /// Remove and return all body-name overrides owned by `feature_id`. Called
    /// on feature delete so the names are GC'd from the live tree but captured
    /// for undo (NOT triggered by a transient empty rebuild — a feature that
    /// errors then recovers keeps its body names).
    pub fn take_body_names(&mut self, feature_id: Uuid) -> BodyNames {
        // Keys are "{feature_id}/{tag}"; UUIDs contain no '/'.
        let prefix = format!("{feature_id}/");
        let keys: Vec<String> = self
            .body_names
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        let mut taken = BodyNames::new();
        for k in keys {
            if let Some(v) = self.body_names.remove(&k) {
                taken.insert(k, v);
            }
        }
        taken
    }

    /// Re-merge body-name overrides (used to undo a feature delete).
    pub fn restore_body_names(&mut self, names: BodyNames) {
        self.body_names.extend(names);
    }

    /// Record (or clear, with `None`) a feature's provenance. Returns the
    /// previous record. No rebuild needed — provenance never affects geometry.
    pub fn set_provenance(
        &mut self,
        feature_id: Uuid,
        provenance: Option<Provenance>,
    ) -> Option<Provenance> {
        match provenance {
            Some(p) => self.provenance.insert(feature_id, p),
            None => self.provenance.remove(&feature_id),
        }
    }

    pub fn provenance_of(&self, feature_id: Uuid) -> Option<&Provenance> {
        self.provenance.get(&feature_id)
    }

    /// Remove and return a feature's provenance (feature delete; captured for
    /// undo).
    pub fn take_provenance(&mut self, feature_id: Uuid) -> Option<Provenance> {
        self.provenance.remove(&feature_id)
    }

    /// Restore a provenance record captured by [`Self::take_provenance`].
    pub fn restore_provenance(&mut self, feature_id: Uuid, provenance: Option<Provenance>) {
        if let Some(p) = provenance {
            self.provenance.insert(feature_id, p);
        }
    }

    /// Return active features (up to active_index).
    pub fn active_features(&self) -> &[Feature] {
        match self.active_index {
            Some(_) if self.features.is_empty() => &[],
            Some(idx) => &self.features[..=idx.min(self.features.len() - 1)],
            None => &self.features,
        }
    }
}

impl Default for FeatureTree {
    fn default() -> Self {
        Self::new()
    }
}

/// A single feature in the parametric feature tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Feature {
    /// Unique identifier.
    pub id: Uuid,
    /// User-visible name.
    pub name: String,
    /// The modeling operation this feature performs.
    pub operation: Operation,
    /// Whether this feature is suppressed.
    pub suppressed: bool,
    /// GeomRefs to geometry that this feature depends on.
    pub references: Vec<GeomRef>,
}

/// A parametric modeling operation with its parameters.
// clippy::large_enum_variant is a memory-layout lint, not a correctness one, and
// the fix (boxing the fat variant) is not proportionate here: `Operation` is the
// feature tree's core type, matched or constructed at 86 sites across 21 files in
// four crates, and it is serialized into the .waffle file format. Every one of
// those sites would change to buy a smaller discriminant on a type that lives one
// per feature, not one per vertex. Revisit if a feature tree ever gets large
// enough for the enum's size to show up in a profile.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum Operation {
    Sketch {
        sketch: Sketch,
    },
    Extrude {
        params: ExtrudeParams,
    },
    Revolve {
        params: RevolveParams,
    },
    Fillet {
        params: FilletParams,
    },
    Chamfer {
        params: ChamferParams,
    },
    Shell {
        params: ShellParams,
    },
    BooleanCombine {
        params: BooleanParams,
    },
    DatumPlane {
        params: DatumPlaneParams,
    },
    ImportedBody {
        params: ImportedBodyParams,
    },
    /// A named mate connector on the part (`specs/part_mate_connectors.md`):
    /// a frame an assembly's mates can use on every instance of this part.
    MateConnector {
        params: MateConnectorParams,
    },
    /// Circular pattern of seed bodies about an axis
    /// (`specs/custom_features_and_modeling_roadmap.md` §B1).
    PatternCircular {
        params: PatternCircularParams,
    },
    /// Linear (1D or 2D grid) pattern of seed bodies.
    PatternLinear {
        params: PatternLinearParams,
    },
    /// A custom feature script (`specs/custom_features_and_modeling_roadmap.md`
    /// Part A): a Rhai script from the document's sources table, run inside
    /// the engine over the same operations the tree has.
    Script {
        params: ScriptParams,
    },
    /// A well-formed `{"type": …}` operation this build does not know — one
    /// from a newer build. Kept verbatim, re-emitted on save, and its rebuild
    /// is a loud `EngineError::UnsupportedOperation`; so adding an operation
    /// kind is no longer a `MIN_READER_VERSION` bump (v4 Phase 1b,
    /// `specs/waffle_v4_document_model.md` §2.5). A malformed KNOWN kind is
    /// still a parse error (`crate::opaque`).
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

/// The known variants, for deserialization (`Operation`'s own `Deserialize`
/// routes unknown tags to `Operation::Unknown`).
#[allow(clippy::large_enum_variant)] // mirrors `Operation`; same call
#[derive(Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
enum KnownOperation {
    Sketch { sketch: Sketch },
    Extrude { params: ExtrudeParams },
    Revolve { params: RevolveParams },
    Fillet { params: FilletParams },
    Chamfer { params: ChamferParams },
    Shell { params: ShellParams },
    BooleanCombine { params: BooleanParams },
    DatumPlane { params: DatumPlaneParams },
    ImportedBody { params: ImportedBodyParams },
    MateConnector { params: MateConnectorParams },
    PatternCircular { params: PatternCircularParams },
    PatternLinear { params: PatternLinearParams },
    Script { params: ScriptParams },
}

/// The operation `type` tags this build can rebuild.
pub const OPERATION_TAGS: &[&str] = &[
    "Sketch",
    "Extrude",
    "Revolve",
    "Fillet",
    "Chamfer",
    "Shell",
    "BooleanCombine",
    "DatumPlane",
    "ImportedBody",
    "MateConnector",
    "PatternCircular",
    "PatternLinear",
    "Script",
];

impl From<KnownOperation> for Operation {
    fn from(k: KnownOperation) -> Self {
        match k {
            KnownOperation::Sketch { sketch } => Operation::Sketch { sketch },
            KnownOperation::Extrude { params } => Operation::Extrude { params },
            KnownOperation::Revolve { params } => Operation::Revolve { params },
            KnownOperation::Fillet { params } => Operation::Fillet { params },
            KnownOperation::Chamfer { params } => Operation::Chamfer { params },
            KnownOperation::Shell { params } => Operation::Shell { params },
            KnownOperation::BooleanCombine { params } => Operation::BooleanCombine { params },
            KnownOperation::DatumPlane { params } => Operation::DatumPlane { params },
            KnownOperation::ImportedBody { params } => Operation::ImportedBody { params },
            KnownOperation::MateConnector { params } => Operation::MateConnector { params },
            KnownOperation::PatternCircular { params } => Operation::PatternCircular { params },
            KnownOperation::PatternLinear { params } => Operation::PatternLinear { params },
            KnownOperation::Script { params } => Operation::Script { params },
        }
    }
}

impl<'de> Deserialize<'de> for Operation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(
            match crate::opaque::known_or_unknown::<D, KnownOperation>(
                d,
                OPERATION_TAGS,
                "operation",
            )? {
                Ok(known) => known.into(),
                Err(value) => Operation::Unknown(value),
            },
        )
    }
}

impl Operation {
    /// The `type` tag as written in the file.
    pub fn type_tag(&self) -> &str {
        match self {
            Operation::Sketch { .. } => "Sketch",
            Operation::Extrude { .. } => "Extrude",
            Operation::Revolve { .. } => "Revolve",
            Operation::Fillet { .. } => "Fillet",
            Operation::Chamfer { .. } => "Chamfer",
            Operation::Shell { .. } => "Shell",
            Operation::BooleanCombine { .. } => "BooleanCombine",
            Operation::DatumPlane { .. } => "DatumPlane",
            Operation::ImportedBody { .. } => "ImportedBody",
            Operation::MateConnector { .. } => "MateConnector",
            Operation::PatternCircular { .. } => "PatternCircular",
            Operation::PatternLinear { .. } => "PatternLinear",
            Operation::Script { .. } => "Script",
            Operation::Unknown(v) => crate::opaque::type_tag(v),
        }
    }
}

#[cfg(feature = "json-schema")]
impl schemars::JsonSchema for Operation {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Operation".into()
    }
    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        // The known kinds exactly as the derive renders them, plus one opaque
        // branch for any other tag.
        let mut schema = <KnownOperation as schemars::JsonSchema>::json_schema(g);
        let obj = schema.as_object_mut().expect("object schema");
        obj.insert(
            "description".into(),
            serde_json::Value::String(
                "A parametric modeling operation with its parameters. Any other well-formed \
                 object with a string `type` (an operation kind from a newer build) is \
                 preserved verbatim, re-emitted on save, and fails its rebuild loudly."
                    .into(),
            ),
        );
        obj.get_mut("oneOf")
            .and_then(serde_json::Value::as_array_mut)
            .expect("tagged enum renders as oneOf")
            .push(serde_json::json!({
                "type": "object",
                "description": "Unknown operation kind (opaque, preserved; rebuild fails loudly).",
                "required": ["type"],
                "properties": { "type": { "type": "string", "not": { "enum": OPERATION_TAGS } } }
            }));
        schema
    }
}

/// Parameters for an imported (STEP) body feature — task #138,
/// `docs/step_import_roadmap.md` §3.3; v4 `specs/waffle_v4_document_model.md`
/// §2.11. Since v4 the STEP content lives in the document's `sources` table
/// and reaches the engine through its [`crate::sources::SourceStore`]; the
/// feature names its source by `source_id`. The v3 in-feature payload
/// (`blob_encoding` + `blob`) is still accepted as a legacy read path and is
/// what the single-tree API inlines for consumers that have no store. The
/// import replays on every rebuild (a process-wide parse cache makes
/// transform edits cheap).
///
/// Content resolution order at rebuild: the source store (embed or
/// host-provided), then the legacy inline blob; neither ⇒ a loud
/// `SourceUnavailable` feature error.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ImportedBodyParams {
    /// Source file name (display + diagnostics), e.g. `minihexa.step`.
    pub file_name: String,
    /// v4: the `sources[]` entry holding the STEP content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<Uuid>,
    /// Legacy (v3) payload encoding tag (`step_import::STEP_BLOB_ENCODING`).
    /// Absent with a present `blob` ⇒ that default encoding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_encoding: Option<String>,
    /// Legacy (v3) inline STEP text, encoded per `blob_encoding`. v4 writers
    /// lift it into `sources[].embed` and clear it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
    /// Placement: translation in METERS, applied after rotation.
    #[serde(default)]
    pub translation_m: [f64; 3],
    /// Placement: intrinsic X→Y→Z Euler angles in DEGREES, about the
    /// imported model's origin.
    #[serde(default)]
    pub rotation_deg: [f64; 3],
    /// Extra uniform scale on top of the file's unit conversion (1.0 = none).
    #[serde(default = "default_scale")]
    pub scale: f64,
}

fn default_scale() -> f64 {
    1.0
}

impl ImportedBodyParams {
    /// v4 shape: content by `source_id`, identity placement.
    pub fn from_source(file_name: impl Into<String>, source_id: Uuid) -> Self {
        Self {
            file_name: file_name.into(),
            source_id: Some(source_id),
            blob_encoding: None,
            blob: None,
            translation_m: [0.0; 3],
            rotation_deg: [0.0; 3],
            scale: 1.0,
        }
    }

    /// Legacy (v3) shape: the STEP text inline, identity placement.
    pub fn embedded(file_name: impl Into<String>, step_text: &str) -> Self {
        Self {
            file_name: file_name.into(),
            source_id: None,
            blob_encoding: Some(step_import::STEP_BLOB_ENCODING.to_string()),
            blob: Some(step_import::encode_step_blob(step_text)),
            translation_m: [0.0; 3],
            rotation_deg: [0.0; 3],
            scale: 1.0,
        }
    }

    /// Whether this feature still carries a legacy inline payload.
    pub fn has_inline_blob(&self) -> bool {
        self.blob.is_some()
    }
}

/// Depth mode for extrude operations.
// Same call as `Operation` above: a GeomRef-carrying variant alongside unit
// variants. Boxing would ripple through the extrude parameter plumbing and the
// serialized format for no measurable gain.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum DepthMode {
    /// Use the `depth` field directly.
    Blind,
    /// Project target body vertices onto extrude direction, use max extent + margin.
    ThroughAll,
    /// Extrude up to a reference (face centroid, vertex, or datum plane).
    UpTo { reference: GeomRef },
}

/// Second direction for bidirectional extrude.
// Same call as `Operation` above: a GeomRef-carrying variant alongside unit
// variants. Boxing would ripple through the extrude parameter plumbing and the
// serialized format for no measurable gain.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum SecondDirection {
    /// Same depth as primary direction.
    Symmetric,
    /// Independent blind depth in second direction.
    Blind { depth: f64 },
    /// Through all in second direction.
    ThroughAll,
    /// Up to a reference in second direction.
    UpTo { reference: GeomRef },
}

fn default_true() -> bool {
    true
}

fn default_depth_mode() -> DepthMode {
    DepthMode::Blind
}

/// Parameters for an extrude operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ExtrudeParams {
    pub sketch_id: Uuid,
    pub profile_index: usize,
    /// v4 §2.9 agent-friendly profile addressing. When present, the profile
    /// is the solved loop whose entity-id set equals this set
    /// (order-insensitive) and `profile_index` is ignored, so a writer that
    /// has not run the solver can say "the loop bounded by entities 3,4,5,6"
    /// (the same identity `Region::profile_entity_ids` carries). No such
    /// loop, or two loops with the same set, is a loud per-feature error
    /// (`EngineError::ProfileNotFound` / `ProfileAmbiguous`). The app's own
    /// writers address by index and leave this `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_entity_ids: Option<Vec<u32>>,
    pub depth: f64,
    /// Optional driving expression for `depth` (mm-space -> meters). When
    /// present, rebuild re-evaluates it against the design parameters and
    /// writes the result into `depth`; `depth` always holds the last
    /// evaluated value so old readers and the kernel see a plain number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth_expr: Option<String>,
    pub direction: Option<[f64; 3]>,
    pub symmetric: bool,
    pub cut: bool,
    /// Auto-union with existing body. Defaults to true for boss extrudes.
    #[serde(default = "default_true")]
    pub merge: bool,
    pub target_body: Option<GeomRef>,
    #[serde(default = "default_depth_mode")]
    pub depth_mode: DepthMode,
    #[serde(default)]
    pub second_direction: Option<SecondDirection>,
    /// Explicit region boundary for a sketch sub-region (annulus, lens, …) that
    /// no whole-loop `profile_index` denotes. When `Some`, the face is built
    /// directly from this boundary and `profile_index` is ignored. Whole-loop
    /// selections leave this `None` and use `profile_index` (analytical path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<waffle_types::Region>,
    /// Multiple selected sub-regions extruded as ONE body. When ≥2, their 2D
    /// footprints are unioned in the sketch plane into merged faces BEFORE the
    /// extrude, so adjacent regions with shared/coplanar side walls merge
    /// cleanly without a 3D boolean (which would hit the Yang Stage-0 coplanar
    /// wall). Empty for single-region (`region`) / whole-profile (`profile_index`)
    /// extrudes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<waffle_types::Region>,
    /// Explicit boolean-combine mode for this extrude. `None` ⇒ legacy file:
    /// the effective mode is derived from `cut`/`merge` (see
    /// `normalize_extrude_combine`). New features always write `Some(..)`.
    /// See `specs/optional_booleans_multibody_extrude.md`.
    #[serde(default)]
    pub combine: Option<CombineMode>,
    /// Explicit target bodies for the combine. `None` ⇒ Auto (bodies that share
    /// a face with the sketch geometry). `Some(vec![])` ⇒ forced new body / no
    /// targets. `Some([..])` ⇒ exactly those bodies. Only meaningful when
    /// `combine` is `Some(Add|Cut|Intersect)`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<GeomRef>>,
}

/// User-facing boolean-combine verb for a body-producing feature (extrude,
/// revolve, …). Maps to `modeling_ops::BooleanKind`: `Add→Union`,
/// `Cut→Subtract`, `Intersect→Intersect`; `NewBody` performs no boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum CombineMode {
    /// Emit a separate, independent body; no boolean.
    NewBody,
    /// Union the tool into each target body.
    Add,
    /// Subtract the tool from each target body.
    Cut,
    /// Intersect the tool with each target body.
    Intersect,
}

/// How the target-body set for a combine is determined, after normalization.
// `dead_code`: consumed by the rebuild dispatch in sub-increment N-mb-2; the
// N-mb-1 tests already exercise it via `normalize_extrude_combine`.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) enum TargetStrategy {
    /// Auto: bodies that share a face with the selected sketch geometry
    /// (spec §4.3). Only for new-style features with no explicit `targets`.
    ShareAFace,
    /// Legacy behavior: the single most-recent solid body. Only produced by
    /// legacy (`combine == None`) features, to keep old files byte-identical.
    MostRecentLegacy,
    /// Exactly these bodies (empty ⇒ no targets ⇒ new standalone body).
    Explicit(Vec<GeomRef>),
}

/// The normalized combine decision for an extrude, consumed by the rebuild
/// dispatch. Produced once, early, by `normalize_extrude_combine`.
#[allow(dead_code)] // wired into rebuild dispatch in N-mb-2
#[derive(Debug, Clone)]
pub(crate) struct EffectiveCombine {
    pub mode: CombineMode,
    pub targets: TargetStrategy,
}

/// Normalize the persisted `ExtrudeParams` boolean fields into a single
/// `EffectiveCombine` (Constitution §7 — normalize early, once). See
/// `specs/optional_booleans_multibody_extrude.md` §3.
///
/// - New-style (`combine == Some`): honor it; `targets == None` ⇒ `ShareAFace`,
///   `Some(list)` ⇒ `Explicit(list)`. `NewBody` ignores `targets`.
/// - Legacy (`combine == None`): derive from `cut`/`merge`/`target_body`,
///   preserving today's exact "most recent solid" behavior.
#[allow(dead_code)] // consumed by rebuild dispatch in N-mb-2
pub(crate) fn normalize_extrude_combine(params: &ExtrudeParams) -> EffectiveCombine {
    normalize_combine(
        params.combine,
        &params.targets,
        params.cut,
        params.merge,
        &params.target_body,
    )
}

/// Shared combine normalization for any body-producing feature (extrude,
/// revolve). See `normalize_extrude_combine` for the rules.
#[allow(dead_code)] // consumed by rebuild dispatch
pub(crate) fn normalize_combine(
    combine: Option<CombineMode>,
    targets: &Option<Vec<GeomRef>>,
    cut: bool,
    merge: bool,
    target_body: &Option<GeomRef>,
) -> EffectiveCombine {
    if let Some(mode) = combine {
        let targets = match mode {
            // NewBody never booleans, so its target set is irrelevant/empty.
            CombineMode::NewBody => TargetStrategy::Explicit(Vec::new()),
            CombineMode::Add | CombineMode::Cut | CombineMode::Intersect => match targets {
                None => TargetStrategy::ShareAFace,
                Some(list) => TargetStrategy::Explicit(list.clone()),
            },
        };
        return EffectiveCombine { mode, targets };
    }

    // Legacy path: derive the mode from the old boolean flags.
    let mode = if cut {
        CombineMode::Cut
    } else if merge {
        CombineMode::Add
    } else {
        CombineMode::NewBody
    };
    let targets = match mode {
        CombineMode::NewBody => TargetStrategy::Explicit(Vec::new()),
        // Legacy `target_body` override (currently never written by the UI) only
        // applies when a boolean actually happens.
        CombineMode::Add | CombineMode::Cut | CombineMode::Intersect => match target_body {
            Some(gr) => TargetStrategy::Explicit(vec![gr.clone()]),
            None => TargetStrategy::MostRecentLegacy,
        },
    };
    EffectiveCombine { mode, targets }
}

/// Normalize a revolve's combine choice (RevolveParams has no `target_body`).
#[allow(dead_code)] // consumed by rebuild dispatch in N-mb-5
pub(crate) fn normalize_revolve_combine(params: &RevolveParams) -> EffectiveCombine {
    normalize_combine(
        params.combine,
        &params.targets,
        params.cut,
        params.merge,
        &None,
    )
}

/// Parameters for a revolve operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct RevolveParams {
    pub sketch_id: Uuid,
    pub profile_index: usize,
    /// See `ExtrudeParams::profile_entity_ids`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_entity_ids: Option<Vec<u32>>,
    pub axis_origin: [f64; 3],
    pub axis_direction: [f64; 3],
    pub angle: f64,
    /// Optional driving expression for `angle` (evaluates to DEGREES).
    /// See `ExtrudeParams::depth_expr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_expr: Option<String>,
    /// If true, subtract this revolve from the target body.
    #[serde(default)]
    pub cut: bool,
    /// If true (and cut=false), auto-union with the most recent body.
    #[serde(default = "default_merge_true")]
    pub merge: bool,
    /// Explicit boolean-combine mode (see `ExtrudeParams::combine`). `None` ⇒
    /// legacy file: derive from `cut`/`merge`.
    #[serde(default)]
    pub combine: Option<CombineMode>,
    /// Explicit target bodies (see `ExtrudeParams::targets`). `None` ⇒ Auto
    /// (share-a-face).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<GeomRef>>,
}

fn default_merge_true() -> bool {
    true
}

/// A line in space named for a pattern: a circular pattern's rotation axis or
/// a linear pattern's direction (`specs/custom_features_and_modeling_roadmap.md`
/// §B1). `#[serde(tag = "method")]`, like [`PlaneDefinition`].
// `large_enum_variant`: `Entity` carries a `GeomRef` (with its signature
// selector) next to six floats. Boxing it would change every construction site
// for a type that lives once per pattern feature — same call as `Operation`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum AxisRef {
    /// Explicit world-space line: `origin` in meters, `direction` any
    /// non-zero vector (normalized at rebuild). A linear pattern reads only
    /// the direction.
    #[serde(rename = "explicit")]
    Explicit {
        origin: [f64; 3],
        direction: [f64; 3],
    },
    /// The axis a picked entity carries, resolved through the same derivation
    /// mate connectors use (`connector::resolve_connector_frame`): a
    /// cylindrical/conical/toroidal face's axis, a circular edge's axis, a
    /// straight edge's line, or a planar face's normal through its centroid.
    /// A pick with no derivable axis (a spline edge, an imported mesh face)
    /// is a loud per-feature error, never a default.
    #[serde(rename = "entity")]
    Entity { geom_ref: GeomRef },
}

/// Parameters for a circular pattern: rigid copies of the seed bodies about
/// an axis, optionally combined into target bodies.
///
/// Instances are equally spaced. `angle_deg` is the TOTAL sweep: a full
/// turn (360°) spaces `count` instances `360/count` apart so the last does
/// not land on the seed; any other sweep puts the last instance exactly at
/// `angle_deg` (step `angle_deg / (count − 1)`). Instance 0 is the seed body
/// itself; `skip` omits instances by index (≥ 1). The pattern feature takes
/// custody of the seed bodies (their features are consumed) and emits every
/// instance as its own output: `Main` is instance 0 of the first seed, then
/// `Body { index }` instance-major.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PatternCircularParams {
    /// Seed bodies (`TopoKind::Solid` references to feature outputs).
    pub seeds: Vec<GeomRef>,
    /// Rotation axis.
    pub axis: AxisRef,
    /// Total instances INCLUDING the seed (≥ 2).
    pub count: u32,
    /// Total sweep in degrees (see the struct docs). Default: a full turn.
    #[serde(default = "default_full_turn")]
    pub angle_deg: f64,
    /// Optional driving expression for `angle_deg` (evaluates to DEGREES).
    /// See `ExtrudeParams::depth_expr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle_expr: Option<String>,
    /// Instance indices (≥ 1) to omit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip: Vec<u32>,
    /// Boolean combine of the instances against `targets`. `None` ⇒ NewBody
    /// (every instance a separate body). `Add` folds targets and instances
    /// into connected lumps; `Cut` subtracts every instance from every
    /// target; `Intersect` keeps each target ∩ (union of instances).
    #[serde(default)]
    pub combine: Option<CombineMode>,
    /// Explicit target bodies for `Add`/`Cut`/`Intersect`. `None` or empty ⇒
    /// no targets: `Add` then merges the instances among themselves only;
    /// `Cut`/`Intersect` with no targets is an error. A pattern never
    /// auto-targets by tree position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<GeomRef>>,
}

fn default_full_turn() -> f64 {
    360.0
}

/// A linear pattern's second direction (a 2D grid): `count` columns
/// `spacing` apart along `direction`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct LinearSecondDirection {
    pub direction: AxisRef,
    /// Instances along this direction INCLUDING the seed row (≥ 2).
    pub count: u32,
    /// Centre-to-centre spacing in meters (negative reverses the direction).
    pub spacing: f64,
    /// Optional driving expression for `spacing` (mm-space → meters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing_expr: Option<String>,
}

/// Parameters for a linear pattern: rigid copies of the seed bodies along a
/// direction (optionally a second direction for a grid), optionally
/// combined into target bodies. Same custody, output and combine rules as
/// [`PatternCircularParams`]. With a second direction the instance index is
/// `i + j · count` for row `i` along `direction` and column `j` along the
/// second direction; instance 0 is the seed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PatternLinearParams {
    /// Seed bodies (`TopoKind::Solid` references to feature outputs).
    pub seeds: Vec<GeomRef>,
    /// Pattern direction (only the direction of the axis is used).
    pub direction: AxisRef,
    /// Total instances along `direction` INCLUDING the seed (≥ 2).
    pub count: u32,
    /// Centre-to-centre spacing in meters (negative reverses the direction).
    pub spacing: f64,
    /// Optional driving expression for `spacing` (mm-space → meters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing_expr: Option<String>,
    /// Optional second direction (grid).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub second: Option<LinearSecondDirection>,
    /// Instance indices (≥ 1) to omit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip: Vec<u32>,
    /// See [`PatternCircularParams::combine`].
    #[serde(default)]
    pub combine: Option<CombineMode>,
    /// See [`PatternCircularParams::targets`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<GeomRef>>,
}

/// Parameters of a custom feature script node
/// (`specs/custom_features_and_modeling_roadmap.md` §A4).
///
/// The script text lives in the document's sources table (`source_id`, a
/// `Script` source); `entry` names the function the engine calls as
/// `entry(ctx, p)`. `args` are the values of the script's declared
/// `@param`s in MODEL units (meters / degrees / plain numbers / bools /
/// strings / plane objects); an `arg_exprs` entry drives that argument from
/// a design-parameter expression instead (mm-space, converted by the
/// parameter's declared type at rebuild), its last evaluated raw value
/// cached in `arg_values` so a parameter change is detected like
/// `depth_expr`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ScriptParams {
    pub source_id: Uuid,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub args: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub arg_exprs: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub arg_values: std::collections::BTreeMap<String, f64>,
}

fn default_entry() -> String {
    "feature".to_string()
}

/// The normalized combine decision of a pattern: `None` ⇒ NewBody; explicit
/// targets only (a pattern never targets by tree position).
pub(crate) fn normalize_pattern_combine(
    combine: Option<CombineMode>,
    targets: &Option<Vec<GeomRef>>,
) -> EffectiveCombine {
    let mode = combine.unwrap_or(CombineMode::NewBody);
    EffectiveCombine {
        mode,
        targets: TargetStrategy::Explicit(match mode {
            CombineMode::NewBody => Vec::new(),
            _ => targets.clone().unwrap_or_default(),
        }),
    }
}

/// Parameters for a fillet operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct FilletParams {
    pub edges: Vec<GeomRef>,
    pub radius: f64,
}

/// Parameters for a chamfer operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ChamferParams {
    pub edges: Vec<GeomRef>,
    pub distance: f64,
}

/// Parameters for a shell operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ShellParams {
    pub faces_to_remove: Vec<GeomRef>,
    pub thickness: f64,
}

/// Parameters for a boolean combine operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BooleanParams {
    pub body_a: GeomRef,
    pub body_b: GeomRef,
    pub operation: BooleanOp,
}

/// Boolean operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
}

/// How a construction plane is defined.
// large_enum_variant: OffsetFromFace grew past the lint threshold with the
// `distance_expr` field. Same call as `Operation` above — a serialized
// feature-tree type constructed per datum plane, not per vertex; boxing would
// ripple through the file format for no measurable gain.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum PlaneDefinition {
    /// Explicit origin + normal.
    #[serde(rename = "point-normal")]
    PointNormal { origin: [f64; 3], normal: [f64; 3] },
    /// Parallel offset from another plane.
    #[serde(rename = "offset")]
    Offset {
        #[serde(rename = "basePlaneId")]
        base_plane_id: Uuid,
        distance: f64,
        /// Optional driving expression for `distance` (mm-space -> meters).
        /// See `ExtrudeParams::depth_expr`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        distance_expr: Option<String>,
    },
    /// Parallel offset from a planar face. The base face's plane (origin +
    /// outward normal) is resolved from the *current* geometry each rebuild,
    /// so the datum tracks the face as it moves. A non-planar base face
    /// resolves to `ResolutionFailed` (loud). A negative distance flips the
    /// offset to the back side of the face.
    #[serde(rename = "offset-face")]
    OffsetFromFace {
        /// GeomRef of the planar face that defines the base plane.
        base: GeomRef,
        distance: f64,
        /// Optional driving expression for `distance` (mm-space -> meters).
        /// See `ExtrudeParams::depth_expr`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        distance_expr: Option<String>,
    },
}

/// Parameters for a datum (construction) plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DatumPlaneParams {
    pub name: String,
    pub definition: PlaneDefinition,
}

/// Parameters for a part mate connector (`specs/part_mate_connectors.md`).
///
/// The frame is derived exactly as an assembly connector's is
/// ([`crate::connector::resolve_connector_frame`]): from `geom_ref` (a face
/// or an edge of this part) when present, else `frame` as given, in the
/// part's coordinates. The same adjustments then apply in the same order
/// (`flip_z`, `rotation_deg`, `offset_m`; `anchor` on a rotational face).
/// The connector's NAME is its feature's name; `name` is only the name the
/// feature is created with.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct MateConnectorParams {
    /// The feature's name at creation (default "Mate connector").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The face or edge the frame is derived from. Absent ⇒ `frame`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geom_ref: Option<GeomRef>,
    /// The frame when there is no `geom_ref` (meters, part coordinates;
    /// default: the part origin, z up). With a `geom_ref`, a non-zero
    /// `x_axis` is the secondary direction.
    #[serde(default)]
    pub frame: crate::assembly::Frame,
    /// Where on a cylindrical/conical/toroidal face's axis the frame sits.
    #[serde(
        default,
        skip_serializing_if = "crate::assembly::AxialAnchor::is_middle"
    )]
    pub anchor: crate::assembly::AxialAnchor,
    /// Reverse z (a 180° turn about x). Applied first.
    #[serde(default, skip_serializing_if = "crate::assembly::is_false")]
    pub flip_z: bool,
    /// Turn about z in degrees, after the flip.
    #[serde(default, skip_serializing_if = "crate::assembly::is_zero")]
    pub rotation_deg: f64,
    /// Move along the frame's own axes after the turn, meters `[x, y, z]`.
    #[serde(default, skip_serializing_if = "crate::assembly::is_zero3")]
    pub offset_m: [f64; 3],
}

/// Errors from the feature engine.
#[derive(Debug, Clone, thiserror::Error)]
pub enum EngineError {
    #[error("feature not found: {id}")]
    FeatureNotFound { id: Uuid },

    #[error("sketch not found: {id}")]
    SketchNotFound { id: Uuid },

    #[error("profile index {index} out of range (sketch has {count} profiles)")]
    ProfileOutOfRange { index: usize, count: usize },

    /// v4 Phase 1b: the feature's operation kind is one this build does not
    /// know (`Operation::Unknown`). The feature stays in the tree and the file.
    #[error("operation kind `{type_tag}` is not supported by this version (the feature is preserved, not rebuilt)")]
    UnsupportedOperation { type_tag: String },

    /// v4 §2.9: `profile_entity_ids` names a loop the solved sketch does not
    /// have.
    #[error("no profile is bounded by entities {entity_ids:?} (sketch has {count} profiles)")]
    ProfileNotFound { entity_ids: Vec<u32>, count: usize },

    /// v4 §2.9: `profile_entity_ids` matches more than one solved loop.
    #[error(
        "{matches} profiles are bounded by entities {entity_ids:?}; the reference is ambiguous"
    )]
    ProfileAmbiguous {
        entity_ids: Vec<u32>,
        matches: usize,
    },

    #[error("GeomRef resolution failed: {reason}")]
    ResolutionFailed { reason: String },

    #[error("kernel error: {0}")]
    KernelError(#[from] waffle_types::kernel::KernelError),

    #[error("operation error: {0}")]
    OpError(#[from] modeling_ops::OpError),

    #[error("rebuild failed at feature {feature_name}: {reason}")]
    RebuildFailed {
        feature_name: String,
        reason: String,
    },

    #[error("nothing to undo")]
    NothingToUndo,

    #[error("nothing to redo")]
    NothingToRedo,

    /// An import's source has no content in this session (v4 §2.3). Typed
    /// for hosts (ICR-2); the message is the same text `RebuildFailed`
    /// carried before, so existing readers see no change.
    #[error("rebuild failed at feature {feature_name}: {reason}")]
    SourceUnavailable {
        feature_name: String,
        source_id: Option<Uuid>,
        reason: String,
    },

    /// A custom feature script failed: a header/parse error, a runtime
    /// error or `ctx.fail`, an exceeded limit, or a child operation's
    /// failure (`stage` names which). The node's outputs are absent (P10).
    #[error("script {stage}: {reason}")]
    Script { stage: String, reason: String },
}

/// The class of a feature error — the machine-readable half that hosts
/// branch on (`specs/waffle_mcp_server.md` ICR-2, A6.2). The human text
/// stays in [`FeatureError::message`]; nothing should parse it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ErrorKind {
    FeatureNotFound {
        id: Uuid,
    },
    SketchNotFound {
        id: Uuid,
    },
    ProfileOutOfRange {
        index: usize,
        count: usize,
    },
    UnsupportedOperation {
        type_tag: String,
    },
    ProfileNotFound {
        entity_ids: Vec<u32>,
        count: usize,
    },
    ProfileAmbiguous {
        entity_ids: Vec<u32>,
        matches: usize,
    },
    ResolutionFailed,
    SourceUnavailable {
        source_id: Option<Uuid>,
    },
    /// A kernel capability boundary (`KernelError::NotSupported`): a roadmap
    /// item, never something to retry with different parameters.
    NotSupported {
        operation: String,
    },
    BooleanEmptyResult,
    /// Any other kernel failure; `kernel` names the `KernelError` variant.
    /// Yang STOPs arrive here today (as `BooleanFailed` / `Other`): giving
    /// them a kind of their own needs kernel-v2 to map them to a variant.
    KernelFailure {
        kernel: String,
    },
    NoProfiles,
    InvalidParameter,
    RebuildFailed,
    NothingToUndo,
    NothingToRedo,
    /// A design-parameter or driving-expression evaluation failure.
    Expression,
    /// A scoped reference could not be resolved through the assembly context.
    Context,
    /// A custom feature script failed; `stage` is `header`, `parse`,
    /// `runtime`, `limit`, `args`, or `child`.
    Script {
        stage: String,
    },
}

impl From<&waffle_types::kernel::KernelError> for ErrorKind {
    fn from(e: &waffle_types::kernel::KernelError) -> Self {
        use waffle_types::kernel::KernelError as K;
        let kernel = |name: &str| ErrorKind::KernelFailure {
            kernel: name.to_string(),
        };
        match e {
            K::NotSupported { operation } => ErrorKind::NotSupported {
                operation: operation.clone(),
            },
            K::BooleanEmptyResult => ErrorKind::BooleanEmptyResult,
            K::BooleanFailed { .. } => kernel("BooleanFailed"),
            K::FilletFailed { .. } => kernel("FilletFailed"),
            K::ShellFailed { .. } => kernel("ShellFailed"),
            K::TessellationFailed { .. } => kernel("TessellationFailed"),
            K::EntityNotFound { .. } => kernel("EntityNotFound"),
            K::Other { .. } => kernel("Other"),
        }
    }
}

impl From<&EngineError> for ErrorKind {
    fn from(e: &EngineError) -> Self {
        match e {
            EngineError::FeatureNotFound { id } => ErrorKind::FeatureNotFound { id: *id },
            EngineError::SketchNotFound { id } => ErrorKind::SketchNotFound { id: *id },
            EngineError::ProfileOutOfRange { index, count } => ErrorKind::ProfileOutOfRange {
                index: *index,
                count: *count,
            },
            EngineError::UnsupportedOperation { type_tag } => ErrorKind::UnsupportedOperation {
                type_tag: type_tag.clone(),
            },
            EngineError::ProfileNotFound { entity_ids, count } => ErrorKind::ProfileNotFound {
                entity_ids: entity_ids.clone(),
                count: *count,
            },
            EngineError::ProfileAmbiguous {
                entity_ids,
                matches,
            } => ErrorKind::ProfileAmbiguous {
                entity_ids: entity_ids.clone(),
                matches: *matches,
            },
            EngineError::ResolutionFailed { .. } => ErrorKind::ResolutionFailed,
            EngineError::KernelError(k) => k.into(),
            EngineError::OpError(op) => match op {
                modeling_ops::OpError::Kernel(k) => k.into(),
                modeling_ops::OpError::NoProfiles => ErrorKind::NoProfiles,
                modeling_ops::OpError::InvalidParameter { .. } => ErrorKind::InvalidParameter,
            },
            EngineError::RebuildFailed { .. } => ErrorKind::RebuildFailed,
            EngineError::NothingToUndo => ErrorKind::NothingToUndo,
            EngineError::NothingToRedo => ErrorKind::NothingToRedo,
            EngineError::SourceUnavailable { source_id, .. } => ErrorKind::SourceUnavailable {
                source_id: *source_id,
            },
            EngineError::Script { stage, .. } => ErrorKind::Script {
                stage: stage.clone(),
            },
        }
    }
}

/// One feature's error, typed (ICR-2). [`crate::Engine::feature_errors`]
/// holds exactly the errors of [`crate::Engine::errors`], in the same order,
/// with the same messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureError {
    pub feature_id: Uuid,
    pub kind: ErrorKind,
    pub message: String,
}

#[cfg(test)]
mod combine_normalization_tests {
    //! RED tests for sub-increment N-mb-1: the parameter-normalization function
    //! `normalize_extrude_combine` and the new `CombineMode` / `combine` /
    //! `targets` surface (spec §3, §2, §6). These will NOT compile until the
    //! Implementer adds:
    //!   - `pub enum CombineMode { NewBody, Add, Cut, Intersect }` (serde tag="type",
    //!     derives Debug + PartialEq)
    //!   - `pub combine: Option<CombineMode>` and `pub targets: Option<Vec<GeomRef>>`
    //!     on `ExtrudeParams` (both `#[serde(default)]`)
    //!   - `pub(crate) fn normalize_extrude_combine(&ExtrudeParams) -> EffectiveCombine`
    //!   - `pub(crate) enum TargetStrategy { ShareAFace, MostRecentLegacy, Explicit(Vec<GeomRef>) }`
    //!   - `pub(crate) struct EffectiveCombine { pub mode: CombineMode, pub targets: TargetStrategy }`
    //!
    //! Until then this module is the expected RED state.
    use super::*;
    use waffle_types::{Anchor, OutputKey, ResolvePolicy, Selector, TopoKind};

    /// A dummy `GeomRef` pointing at some feature output. Each call uses a fresh
    /// UUID so distinct `gr`s are distinguishable by `feature_id`.
    fn dummy_geom_ref() -> GeomRef {
        GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::FeatureOutput {
                feature_id: Uuid::new_v4(),
                output_key: OutputKey::Main,
            },
            selector: Selector::Role {
                role: waffle_types::roles::Role::ProfileFace,
                index: 0,
            },
            policy: ResolvePolicy::BestEffort,
            scope: None,
        }
    }

    /// The `feature_id` inside a `GeomRef`, used to compare `Explicit` contents
    /// without requiring `GeomRef: PartialEq`.
    fn feature_id_of(gr: &GeomRef) -> Uuid {
        match &gr.anchor {
            Anchor::FeatureOutput { feature_id, .. } => *feature_id,
            Anchor::Datum { datum_id } => *datum_id,
        }
    }

    /// Build an `ExtrudeParams` with the legacy geometry fields fixed and the
    /// four boolean-relevant fields caller-controlled.
    fn params(
        combine: Option<CombineMode>,
        targets: Option<Vec<GeomRef>>,
        cut: bool,
        merge: bool,
        target_body: Option<GeomRef>,
    ) -> ExtrudeParams {
        ExtrudeParams {
            sketch_id: Uuid::new_v4(),
            profile_index: 0,
            profile_entity_ids: None,
            depth: 0.01,
            depth_expr: None,
            direction: None,
            symmetric: false,
            cut,
            merge,
            target_body,
            depth_mode: DepthMode::Blind,
            second_direction: None,
            region: None,
            regions: Vec::new(),
            combine,
            targets,
        }
    }

    /// Assert a `TargetStrategy` is `Explicit` with exactly the given feature ids.
    fn assert_explicit_ids(ts: &TargetStrategy, expected: &[Uuid]) {
        match ts {
            TargetStrategy::Explicit(list) => {
                let got: Vec<Uuid> = list.iter().map(feature_id_of).collect();
                assert_eq!(got, expected, "Explicit target list mismatch");
            }
            other => panic!("expected TargetStrategy::Explicit, got {other:?}"),
        }
    }

    // --- Rule 1: Some(NewBody) ignores targets ---

    #[test]
    fn newbody_none_targets() {
        let p = params(Some(CombineMode::NewBody), None, false, false, None);
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::NewBody);
    }

    #[test]
    fn newbody_ignores_explicit_targets() {
        let gr = dummy_geom_ref();
        let p = params(
            Some(CombineMode::NewBody),
            Some(vec![gr]),
            false,
            false,
            None,
        );
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::NewBody);
    }

    // --- Rule 2: Some(Add), targets None => ShareAFace ---

    #[test]
    fn add_none_targets_share_a_face() {
        let p = params(Some(CombineMode::Add), None, false, false, None);
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::Add);
        assert!(
            matches!(eff.targets, TargetStrategy::ShareAFace),
            "expected ShareAFace, got {:?}",
            eff.targets
        );
    }

    // --- Rule 3: Some(Add), targets Some([]) => Explicit([]) ---

    #[test]
    fn add_empty_targets_explicit_empty() {
        let p = params(Some(CombineMode::Add), Some(vec![]), false, false, None);
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::Add);
        assert_explicit_ids(&eff.targets, &[]);
    }

    // --- Rule 4: Some(Add), targets Some([gr]) => Explicit([gr]) ---

    #[test]
    fn add_one_target_explicit() {
        let gr = dummy_geom_ref();
        let id = feature_id_of(&gr);
        let p = params(Some(CombineMode::Add), Some(vec![gr]), false, false, None);
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::Add);
        assert_explicit_ids(&eff.targets, &[id]);
    }

    // --- Rule 5: Some(Cut), targets None => ShareAFace ---

    #[test]
    fn cut_none_targets_share_a_face() {
        let p = params(Some(CombineMode::Cut), None, false, false, None);
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::Cut);
        assert!(
            matches!(eff.targets, TargetStrategy::ShareAFace),
            "expected ShareAFace, got {:?}",
            eff.targets
        );
    }

    // --- Rule 6: Some(Intersect), targets Some([gr]) => Explicit([gr]) ---

    #[test]
    fn intersect_one_target_explicit() {
        let gr = dummy_geom_ref();
        let id = feature_id_of(&gr);
        let p = params(
            Some(CombineMode::Intersect),
            Some(vec![gr]),
            false,
            false,
            None,
        );
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::Intersect);
        assert_explicit_ids(&eff.targets, &[id]);
    }

    // --- Rule 7: legacy cut=true => Cut + MostRecentLegacy ---

    #[test]
    fn legacy_cut_true_maps_to_cut_most_recent() {
        // merge value must be ignored when cut is true.
        for merge in [true, false] {
            let p = params(None, None, true, merge, None);
            let eff = normalize_extrude_combine(&p);
            assert_eq!(eff.mode, CombineMode::Cut, "merge={merge}");
            assert!(
                matches!(eff.targets, TargetStrategy::MostRecentLegacy),
                "expected MostRecentLegacy, got {:?}",
                eff.targets
            );
        }
    }

    // --- Rule 8: legacy cut=false, merge=true => Add + MostRecentLegacy ---

    #[test]
    fn legacy_merge_true_maps_to_add_most_recent() {
        let p = params(None, None, false, true, None);
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::Add);
        assert!(
            matches!(eff.targets, TargetStrategy::MostRecentLegacy),
            "expected MostRecentLegacy, got {:?}",
            eff.targets
        );
    }

    // --- Rule 9: legacy cut=false, merge=false => NewBody ---

    #[test]
    fn legacy_neither_maps_to_new_body() {
        let p = params(None, None, false, false, None);
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::NewBody);
    }

    // --- Rule 10: legacy target_body override (only when a boolean happens) ---

    #[test]
    fn legacy_target_body_overrides_merge_to_explicit() {
        let gr = dummy_geom_ref();
        let id = feature_id_of(&gr);
        let p = params(None, None, false, true, Some(gr));
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::Add);
        assert_explicit_ids(&eff.targets, &[id]);
    }

    #[test]
    fn legacy_target_body_overrides_cut_to_explicit() {
        let gr = dummy_geom_ref();
        let id = feature_id_of(&gr);
        let p = params(None, None, true, false, Some(gr));
        let eff = normalize_extrude_combine(&p);
        assert_eq!(eff.mode, CombineMode::Cut);
        assert_explicit_ids(&eff.targets, &[id]);
    }

    // --- Serde back-compat: omitting combine/targets => None/None ---

    #[test]
    fn deserialize_without_combine_and_targets_defaults_none() {
        // A JSON ExtrudeParams that pre-dates the new fields (old .waffle file).
        let json = r#"{
            "sketch_id": "00000000-0000-0000-0000-000000000000",
            "profile_index": 0,
            "depth": 0.01,
            "direction": null,
            "symmetric": false,
            "cut": false,
            "merge": true,
            "target_body": null
        }"#;
        let p: ExtrudeParams =
            serde_json::from_str(json).expect("old params must still deserialize");
        assert!(p.combine.is_none(), "combine must default to None");
        assert!(p.targets.is_none(), "targets must default to None");
    }

    // --- Serde round-trip for all 4 CombineMode variants ---

    #[test]
    fn combine_mode_serde_round_trip_all_variants() {
        for m in [
            CombineMode::NewBody,
            CombineMode::Add,
            CombineMode::Cut,
            CombineMode::Intersect,
        ] {
            let s = serde_json::to_string(&m).expect("serialize CombineMode");
            let back: CombineMode = serde_json::from_str(&s).expect("deserialize CombineMode");
            assert_eq!(m, back, "round-trip mismatch for {m:?} via {s}");
        }
    }
}
