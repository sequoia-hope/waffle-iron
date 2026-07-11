use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use waffle_types::{GeomRef, OutputKey, Sketch};

/// User-assigned body display names, keyed by a body's persistent identity
/// (`"{feature_id}/{output_key.tag()}"`). Absent ⇒ the body uses a derived
/// name (its producing feature's name). Stored on the tree so it persists with
/// the document; `#[serde(default)]` keeps older files (no field) loading.
pub type BodyNames = HashMap<String, String>;

/// The ordered list of modeling features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureTree {
    /// Ordered list of features. Index 0 is the first feature.
    pub features: Vec<Feature>,
    /// Features after this index are suppressed during rebuild.
    /// None means all features are active.
    pub active_index: Option<usize>,
    /// User-assigned body names, independent of feature names.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub body_names: BodyNames,
}

impl FeatureTree {
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
            active_index: None,
            body_names: HashMap::new(),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    Sketch { sketch: Sketch },
    Extrude { params: ExtrudeParams },
    Revolve { params: RevolveParams },
    Fillet { params: FilletParams },
    Chamfer { params: ChamferParams },
    Shell { params: ShellParams },
    BooleanCombine { params: BooleanParams },
    DatumPlane { params: DatumPlaneParams },
    ImportedBody { params: ImportedBodyParams },
}

/// Parameters for an imported (STEP) body feature — task #138,
/// `docs/step_import_roadmap.md` §3.3. The source STEP text is embedded
/// (compressed) so the `.waffle` file is self-contained; the import replays
/// on every rebuild (a process-wide parse cache makes transform edits cheap).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedBodyParams {
    /// Source file name (display + diagnostics), e.g. `minihexa.step`.
    pub file_name: String,
    /// Payload encoding tag (`step_import::STEP_BLOB_ENCODING`).
    pub blob_encoding: String,
    /// The STEP text, encoded per `blob_encoding`.
    pub blob: String,
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

/// Depth mode for extrude operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DepthMode {
    /// Use the `depth` field directly.
    Blind,
    /// Project target body vertices onto extrude direction, use max extent + margin.
    ThroughAll,
    /// Extrude up to a reference (face centroid, vertex, or datum plane).
    UpTo { reference: GeomRef },
}

/// Second direction for bidirectional extrude.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
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
pub struct ExtrudeParams {
    pub sketch_id: Uuid,
    pub profile_index: usize,
    pub depth: f64,
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
pub struct RevolveParams {
    pub sketch_id: Uuid,
    pub profile_index: usize,
    pub axis_origin: [f64; 3],
    pub axis_direction: [f64; 3],
    pub angle: f64,
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

/// Parameters for a fillet operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilletParams {
    pub edges: Vec<GeomRef>,
    pub radius: f64,
}

/// Parameters for a chamfer operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChamferParams {
    pub edges: Vec<GeomRef>,
    pub distance: f64,
}

/// Parameters for a shell operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellParams {
    pub faces_to_remove: Vec<GeomRef>,
    pub thickness: f64,
}

/// Parameters for a boolean combine operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BooleanParams {
    pub body_a: GeomRef,
    pub body_b: GeomRef,
    pub operation: BooleanOp,
}

/// Boolean operation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
}

/// How a construction plane is defined.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
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
    },
}

/// Parameters for a datum (construction) plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatumPlaneParams {
    pub name: String,
    pub definition: PlaneDefinition,
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
            depth: 0.01,
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
