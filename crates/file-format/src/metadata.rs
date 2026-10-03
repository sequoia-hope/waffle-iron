use chrono::{DateTime, Utc};
use feature_engine::assembly::AssemblyTree;
use feature_engine::drawing::Drawing;
use feature_engine::types::{DesignParameter, FeatureTree};

/// RFC 3339 UTC timestamps written the way JavaScript's `toISOString()`
/// writes them (`2020-01-02T03:04:05.000Z`, always ≥ 3 fractional digits) so
/// a `created` that came from the app round-trips byte-identical through the
/// Rust writer; nanosecond-precision values (Rust `Utc::now()`) keep their
/// full precision. chrono's default serializer drops the `.000`, which
/// rewrote every whole-second timestamp on the first v4 save.
pub mod rfc3339_js {
    use chrono::{DateTime, SecondsFormat, Timelike, Utc};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn format(dt: &DateTime<Utc>) -> String {
        if dt.nanosecond().is_multiple_of(1_000_000) {
            dt.to_rfc3339_opts(SecondsFormat::Millis, true)
        } else {
            dt.to_rfc3339_opts(SecondsFormat::AutoSi, true)
        }
    }

    pub fn serialize<S: Serializer>(dt: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
        format(dt).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<DateTime<Utc>, D::Error> {
        DateTime::<Utc>::deserialize(d)
    }
}
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::sources::known_or_unknown;

/// Project metadata for the single-tree API (`save_project`/`load_project`).
/// Kept for the assay generators and other consumers that deal in one
/// feature tree; the document model is [`DocumentMetadata`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    /// Human-readable project name.
    pub name: String,
    /// When the project was first created.
    #[serde(with = "rfc3339_js")]
    pub created: DateTime<Utc>,
    /// When the project was last modified.
    #[serde(with = "rfc3339_js")]
    pub modified: DateTime<Utc>,
    /// Display unit preference (mm, cm, m, in, ft). None for legacy v1 files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_unit: Option<String>,
}

impl ProjectMetadata {
    /// Create metadata with the given name and current timestamp.
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            name: name.into(),
            created: now,
            modified: now,
            display_unit: None,
        }
    }

    /// Create metadata with a display unit preference.
    pub fn with_display_unit(mut self, unit: impl Into<String>) -> Self {
        self.display_unit = Some(unit.into());
        self
    }
}

impl From<&DocumentMetadata> for ProjectMetadata {
    fn from(d: &DocumentMetadata) -> Self {
        ProjectMetadata {
            name: d.name.clone(),
            created: d.created,
            modified: d.modified,
            display_unit: d.display_unit.clone(),
        }
    }
}

/// Document-level metadata (v3+; `id` since v4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct DocumentMetadata {
    /// Stable identity of the document (v4 §2.1): survives rename, provider
    /// move and fork. Writers always emit it; a reader that finds it absent
    /// mints one (this serde default) and the loader warns.
    #[serde(default = "Uuid::new_v4")]
    // Schema: the serde default is a freshly minted UUID, which the derive
    // would evaluate and embed as the schema's `default` (a different value
    // on every generation). The always-true predicate makes the derive omit
    // the default and mark `id` optional-on-read, which is the truth.
    #[cfg_attr(
        feature = "json-schema",
        schemars(skip_serializing_if = "schema_omit_default")
    )]
    pub id: Uuid,
    pub name: String,
    #[serde(with = "rfc3339_js")]
    #[cfg_attr(feature = "json-schema", schemars(with = "DateTime<Utc>"))]
    pub created: DateTime<Utc>,
    #[serde(with = "rfc3339_js")]
    #[cfg_attr(feature = "json-schema", schemars(with = "DateTime<Utc>"))]
    pub modified: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_unit: Option<String>,
    /// The DOCUMENT's design-parameter table (v11, P2,
    /// `specs/agent_mechanical_design.md` §6).
    ///
    /// Above the tabs: every tab's expressions resolve through it after
    /// their own table, so one variable can drive two Parts — which is what
    /// `spring_od = tube_id - 1.5mm` needs when the tube and the spring are
    /// separate Parts. Same row type as `FeatureTree.parameters`
    /// (`feature_engine::types::DesignParameter`), because it is the same
    /// kind of thing and a second row type would be a second evaluator.
    ///
    /// Omitted when empty, so no document without one changes bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<DesignParameter>,
    /// Unknown keys preserved across load → save (v4 §2.6).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[cfg(feature = "json-schema")]
fn schema_omit_default(_: &Uuid) -> bool {
    true
}

impl DocumentMetadata {
    /// Fresh metadata: new id, timestamps now, no unit.
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            created: now,
            modified: now,
            display_unit: None,
            parameters: Vec::new(),
            extra: Map::new(),
        }
    }

    pub fn with_display_unit(mut self, unit: impl Into<String>) -> Self {
        self.display_unit = Some(unit.into());
        self
    }
}

impl From<&ProjectMetadata> for DocumentMetadata {
    fn from(m: &ProjectMetadata) -> Self {
        DocumentMetadata {
            id: Uuid::new_v4(),
            name: m.name.clone(),
            created: m.created,
            modified: m.modified,
            display_unit: m.display_unit.clone(),
            // A v1/v2 document predates the document layer entirely, so it
            // has no document table to migrate.
            parameters: Vec::new(),
            extra: Map::new(),
        }
    }
}

/// A single tab in a document.
///
/// `id` is a document-level key matched by `active_tab` and, since v4, by
/// cross-tab references. New tabs get UUID strings; the type stays a
/// free-form `String` because legacy documents carry non-UUID ids (the
/// literal `"default"`) that must keep loading — the v3→v4 migration rewrites
/// those to fresh UUIDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct Tab {
    pub id: String,
    pub name: String,
    pub kind: TabKind,
    /// Unknown keys preserved across load → save (v4 §2.6).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Tab {
    /// A Part tab with a fresh UUID id.
    pub fn part(name: impl Into<String>, features: FeatureTree) -> Self {
        Tab {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            kind: TabKind::Part {
                features,
                preview_mesh: None,
            },
            extra: Map::new(),
        }
    }

    /// An Assembly tab with a fresh UUID id.
    pub fn assembly(name: impl Into<String>, assembly: AssemblyTree) -> Self {
        Tab {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            kind: TabKind::Assembly {
                assembly,
                preview_mesh: None,
            },
            extra: Map::new(),
        }
    }

    /// The tab's feature tree, if it is a Part.
    pub fn features(&self) -> Option<&FeatureTree> {
        match &self.kind {
            TabKind::Part { features, .. } => Some(features),
            _ => None,
        }
    }

    pub fn features_mut(&mut self) -> Option<&mut FeatureTree> {
        match &mut self.kind {
            TabKind::Part { features, .. } => Some(features),
            _ => None,
        }
    }

    /// A Drawing tab with a fresh UUID id (drawings spec §8, D4a).
    pub fn drawing(name: impl Into<String>, drawing: Drawing) -> Self {
        Tab {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            kind: TabKind::Drawing {
                drawing,
                preview_mesh: None,
            },
            extra: Map::new(),
        }
    }

    /// The tab's assembly, if it is an Assembly.
    pub fn assembly_tree(&self) -> Option<&AssemblyTree> {
        match &self.kind {
            TabKind::Assembly { assembly, .. } => Some(assembly),
            _ => None,
        }
    }

    /// The tab's drawing, if it is a Drawing.
    pub fn drawing_tree(&self) -> Option<&Drawing> {
        match &self.kind {
            TabKind::Drawing { drawing, .. } => Some(drawing),
            _ => None,
        }
    }

    /// The tab's drawing, mutably.
    pub fn drawing_mut(&mut self) -> Option<&mut Drawing> {
        match &mut self.kind {
            TabKind::Drawing { drawing, .. } => Some(drawing),
            _ => None,
        }
    }
}

/// The kind/content of a tab.
///
/// Known kinds: `Part`, `Assembly` (Phase 3) and `Drawing` (drawings spec
/// D4a). A well-formed `{"type": …}` this reader does not know is kept as
/// [`TabKind::Unknown`] and re-emitted verbatim (v4 §2.5), so adding a tab
/// kind is not a `MIN_READER_VERSION` bump — including `Drawing`, whose
/// annotation anchors carry a `Selector::Pid`: a reader that does not know
/// the kind never deserializes what is inside it, which is exactly the
/// difference from v7's `FeatureTree.names`. A malformed known kind is still
/// a parse error.
// One TabKind lives per tab, not per vertex; boxing the FeatureTree to shrink
// the Unknown variant's discriminant would touch every `TabKind::Part` site
// for no measurable gain (same call as `Operation` in feature-engine).
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum TabKind {
    Part {
        features: FeatureTree,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview_mesh: Option<PreviewMesh>,
    },
    /// Phase 3 (`feature_engine::assembly`): placed instances of parts,
    /// mate connectors and mates.
    Assembly {
        assembly: AssemblyTree,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview_mesh: Option<PreviewMesh>,
    },
    /// D4a (`feature_engine::drawing`): sheets of projected views of another
    /// tab's bodies, with the annotations drawn on them.
    ///
    /// `preview_mesh` is carried for shape with the other two and is `None`:
    /// a drawing's preview is its views' own cached `ViewLayout`s, which are
    /// 2-D curves rather than a mesh, so there is nothing for the document
    /// browser's 3-D thumbnail to hold. Dropping the field would make the
    /// variant's JSON asymmetric with its siblings for no gain.
    Drawing {
        drawing: Drawing,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preview_mesh: Option<PreviewMesh>,
    },
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum KnownTabKind {
    Part {
        features: FeatureTree,
        #[serde(default)]
        preview_mesh: Option<PreviewMesh>,
    },
    Assembly {
        assembly: AssemblyTree,
        #[serde(default)]
        preview_mesh: Option<PreviewMesh>,
    },
    Drawing {
        #[serde(default)]
        drawing: Drawing,
        #[serde(default)]
        preview_mesh: Option<PreviewMesh>,
    },
}

/// The tab kinds this build can open.
pub const TAB_KIND_TAGS: &[&str] = &["Part", "Assembly", "Drawing"];

impl<'de> Deserialize<'de> for TabKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(
            match known_or_unknown::<D, KnownTabKind>(d, TAB_KIND_TAGS, "tab kind")? {
                Ok(KnownTabKind::Part {
                    features,
                    preview_mesh,
                }) => TabKind::Part {
                    features,
                    preview_mesh,
                },
                Ok(KnownTabKind::Assembly {
                    assembly,
                    preview_mesh,
                }) => TabKind::Assembly {
                    assembly,
                    preview_mesh,
                },
                Ok(KnownTabKind::Drawing {
                    drawing,
                    preview_mesh,
                }) => TabKind::Drawing {
                    drawing,
                    preview_mesh,
                },
                Err(v) => TabKind::Unknown(v),
            },
        )
    }
}

#[cfg(feature = "json-schema")]
impl schemars::JsonSchema for TabKind {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TabKind".into()
    }
    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "A tab's content: a `Part` (feature tree), an `Assembly` (instances, mate connectors, mates) or a `Drawing` (sheets of projected views with their annotations). Any other well-formed object with a string `type` (a tab kind from a newer build) is preserved verbatim and re-emitted on save.",
            "oneOf": [
                {
                    "type": "object",
                    "required": ["type", "features"],
                    "properties": {
                        "type": { "const": "Part" },
                        "features": g.subschema_for::<FeatureTree>(),
                        "preview_mesh": { "anyOf": [ g.subschema_for::<PreviewMesh>(), { "type": "null" } ] }
                    }
                },
                {
                    "type": "object",
                    "required": ["type", "assembly"],
                    "properties": {
                        "type": { "const": "Assembly" },
                        "assembly": g.subschema_for::<AssemblyTree>(),
                        "preview_mesh": { "anyOf": [ g.subschema_for::<PreviewMesh>(), { "type": "null" } ] }
                    }
                },
                {
                    "type": "object",
                    "required": ["type"],
                    "properties": {
                        "type": { "const": "Drawing" },
                        "drawing": g.subschema_for::<Drawing>(),
                        "preview_mesh": { "anyOf": [ g.subschema_for::<PreviewMesh>(), { "type": "null" } ] }
                    }
                },
                {
                    "type": "object",
                    "description": "Unknown tab kind (opaque, preserved).",
                    "required": ["type"],
                    "properties": { "type": { "type": "string", "not": { "enum": TAB_KIND_TAGS } } }
                }
            ]
        })
    }
}

impl TabKind {
    /// The `type` tag as written in the file.
    pub fn type_tag(&self) -> &str {
        match self {
            TabKind::Part { .. } => "Part",
            TabKind::Assembly { .. } => "Assembly",
            TabKind::Drawing { .. } => "Drawing",
            TabKind::Unknown(v) => v.get("type").and_then(Value::as_str).unwrap_or("?"),
        }
    }
}

/// A lightweight mesh for 3D thumbnail previews.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PreviewMesh {
    pub vertices: Vec<f32>,
    pub normals: Vec<f32>,
    pub indices: Vec<u32>,
}
