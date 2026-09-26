//! Derived board content from a `.kicad_pcb` — `specs/kicad_board_link.md`
//! §2.3, increment C2.
//!
//! The parser (`kicad-pcb`) hands over a [`Pcb`] in the file's Y-down frame,
//! metres. This module is where the frame flips: sketch `(u, v) = (x, −y)`,
//! so the board lies on the XY datum with its bottom at `z = 0` and its
//! copper top at `z = thickness`. Everything it produces carries `Derived`
//! provenance (features) or an `x-derived` record (instances, connectors)
//! naming the source and the rule that made it, so a re-sync (C5) can
//! replace exactly that and nothing else.
//!
//! No engine call happens here: the output is bare trees the bridge writes
//! into tabs. The first constructor of [`ProvenanceOrigin::Derived`] in the
//! codebase lives in this file.

use std::collections::{BTreeMap, HashMap};

use kicad_pcb::{Footprint, Loop, OutlineError, OutlineShape, Pcb, Segment, Side};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map};
use uuid::Uuid;
use waffle_types::{
    Anchor, GeomRef, OutputKey, ResolvePolicy, Role, Selector, Sketch, SketchEntity, SolveStatus,
    TopoKind,
};

use crate::assembly::{
    AssemblyTree, AxialAnchor, Frame, Instance, MateConnector, PartRef, Transform,
};
use crate::types::{
    CombineMode, DepthMode, ExtrudeParams, Feature, FeatureTree, Operation, Provenance,
    ProvenanceOrigin,
};

/// Rules (spec §2.3): the `rule` string of a `Derived` provenance record.
pub const RULE_BOARD_OUTLINE: &str = "kicad.board_outline";
pub const RULE_BOARD_EXTRUDE: &str = "kicad.board_extrude";
pub const RULE_BOARD_CUTOUTS: &str = "kicad.board_cutouts";
pub const RULE_BOARD_CUTOUT: &str = "kicad.board_cutout";
pub const RULE_PLACEHOLDER: &str = "kicad.placeholder";
pub const RULE_BOARD_INSTANCE: &str = "kicad.board";
pub const RULE_FOOTPRINT: &str = "kicad.footprint";
pub const RULE_MOUNTING_HOLE: &str = "kicad.mounting_hole";
/// The board Part tab's own tree carries this as its `x-derived` record
/// (key `"board"`), and a placeholder Part's carries [`RULE_PLACEHOLDER`]
/// with the footprint name as key — how a re-sync and a reload find the
/// tabs a source derived without a persisted record.
pub const RULE_BOARD_PART: &str = "kicad.board_part";

/// The `extra` key carrying derivation on instances and connectors (v4 §2.6
/// `x-` convention): `{"source_id": …, "rule": …, "key": …}`.
pub const X_DERIVED: &str = "x-derived";

/// The built-in XY datum plane (`rebuild.rs` `FRONT_PLANE_ID`, `planes.js`).
const XY_DATUM_ID: &str = "00000000-0000-0000-0000-000000000001";

/// Placeholder component height (spec §3 C1): `courtyard × 1 mm`.
const PLACEHOLDER_HEIGHT_M: f64 = 1e-3;
/// Half-extent of a placeholder for a footprint with no pads.
const PLACEHOLDER_MIN_HALF_M: f64 = 0.5e-3;

/// Endpoint weld in the sketch frame — the parser's own.
const WELD_M: f64 = kicad_pcb::OUTLINE_WELD_M;

/// What to derive (spec §2.1 `sides`, `mounting_holes`).
#[derive(Debug, Clone, Copy)]
pub struct DeriveOptions {
    pub front: bool,
    pub back: bool,
    pub mounting_holes: bool,
}

impl Default for DeriveOptions {
    fn default() -> Self {
        Self {
            front: true,
            back: true,
            mounting_holes: true,
        }
    }
}

/// Board-level record shown on hover (spec §2.3 "Metadata"). A pure
/// function of the source bytes; never persisted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardMeta {
    pub source_id: Uuid,
    pub title: String,
    pub rev: String,
    pub date: String,
    pub company: String,
    pub comments: Vec<String>,
    pub copper_layers: u32,
    pub thickness_m: f64,
    pub net_count: usize,
    pub footprint_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PadMeta {
    pub number: String,
    pub net_name: String,
}

/// Per-component record shown on hover.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentMeta {
    pub footprint_uuid: String,
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub datasheet: String,
    /// `"Front"` / `"Back"`.
    pub side: String,
    pub attrs: Vec<String>,
    pub pads: Vec<PadMeta>,
}

/// One placeholder Part (spec §3 C1): a box standing in for every footprint
/// of one library shape until a real model is linked (C3).
#[derive(Debug, Clone)]
pub struct PlaceholderPart {
    /// The footprint name (`"Resistor_SMD:R_0603_1608Metric"`) — the key
    /// the bridge maps to a tab id.
    pub footprint: String,
    pub tree: FeatureTree,
}

/// Everything derived from one board, before tabs exist.
#[derive(Debug, Clone)]
pub struct DerivedBoard {
    pub source_id: Uuid,
    pub board_tree: FeatureTree,
    pub board_sketch_id: Uuid,
    /// `None` when the outline could not be chained: the sketch still lands
    /// so the user can see the gap (spec §6).
    pub board_extrude_id: Option<Uuid>,
    pub outline_error: Option<OutlineError>,
    pub placeholders: Vec<PlaceholderPart>,
    pub board_meta: BoardMeta,
    pub warnings: Vec<String>,
    footprints: Vec<Footprint>,
    thickness_m: f64,
    options: DeriveOptions,
}

/// Derive the board Part tree, the placeholder Parts and the metadata.
/// The assembly is built afterwards by [`DerivedBoard::assembly`], once the
/// bridge knows the tab ids the instances must reference.
pub fn derive_board(source_id: Uuid, pcb: &Pcb, options: DeriveOptions) -> DerivedBoard {
    let mut warnings = pcb.warnings.clone();
    let thickness_m = pcb.thickness_m;

    let mut board_tree = FeatureTree::new();
    board_tree.extra = derived_extra(source_id, RULE_BOARD_PART, "board");
    let mut sketch_w = SketchWriter::default();
    let (outer_ids, holes_ids, outline_error) = match pcb.outline_loops() {
        Ok(loops) => {
            let outer = sketch_w.write_loop(&loops.outer);
            // Cutouts go in their OWN sketch so the outer profile is exactly
            // the outer loop and each hole is a standalone profile to cut.
            let mut holes_w = SketchWriter::default();
            let holes: Vec<Vec<u32>> = loops.holes.iter().map(|l| holes_w.write_loop(l)).collect();
            (Some(outer), Some((holes_w, holes)), None)
        }
        Err(e) => {
            // Every primitive as drawn, so the gap is visible in the sketch.
            sketch_w.write_primitives(&pcb.outline);
            warnings.push(format!("{RULE_BOARD_EXTRUDE}: {e}"));
            (None, None, Some(e))
        }
    };

    let board_sketch_id = Uuid::new_v4();
    push_derived(
        &mut board_tree,
        Feature {
            id: board_sketch_id,
            name: "Board outline".to_string(),
            operation: Operation::Sketch {
                sketch: sketch_w.into_sketch(),
            },
            suppressed: false,
            references: Vec::new(),
        },
        source_id,
        RULE_BOARD_OUTLINE,
    );

    let mut board_extrude_id = None;
    if let Some(outer) = outer_ids {
        let id = Uuid::new_v4();
        push_derived(
            &mut board_tree,
            Feature {
                id,
                name: "Board".to_string(),
                operation: Operation::Extrude {
                    params: extrude(board_sketch_id, outer, thickness_m, None),
                },
                suppressed: false,
                references: Vec::new(),
            },
            source_id,
            RULE_BOARD_EXTRUDE,
        );
        board_extrude_id = Some(id);

        if let Some((holes_w, holes)) = holes_ids {
            if !holes.is_empty() {
                let cut_sketch_id = Uuid::new_v4();
                push_derived(
                    &mut board_tree,
                    Feature {
                        id: cut_sketch_id,
                        name: "Board cutouts".to_string(),
                        operation: Operation::Sketch {
                            sketch: holes_w.into_sketch(),
                        },
                        suppressed: false,
                        references: Vec::new(),
                    },
                    source_id,
                    RULE_BOARD_CUTOUTS,
                );
                // A cut CONSUMES its target and owns the result, so each
                // cutout is chained onto the previous feature's body.
                let mut body = id;
                for (i, ids) in holes.into_iter().enumerate() {
                    let cut_id = Uuid::new_v4();
                    push_derived(
                        &mut board_tree,
                        Feature {
                            id: cut_id,
                            name: format!("Board cutout {}", i + 1),
                            operation: Operation::Extrude {
                                // A through-hole, not a slab: a symmetric cut
                                // three thicknesses tall spans −1.5t..+1.5t,
                                // so neither cap of the cutter is coplanar
                                // with a board face (that pair is the
                                // kernel's loud Stage-0 wall).
                                params: extrude(cut_sketch_id, ids, 3.0 * thickness_m, Some(body)),
                            },
                            suppressed: false,
                            references: Vec::new(),
                        },
                        source_id,
                        RULE_BOARD_CUTOUT,
                    );
                    body = cut_id;
                }
            }
        }
    }

    // Placeholders: one Part per distinct footprint shape that gets an
    // instance, box = pad bounding box × 1 mm in the footprint's own frame.
    let mut placeholders: Vec<PlaceholderPart> = Vec::new();
    for fp in &pcb.footprints {
        if !instance_wanted(fp, &options)
            || placeholders.iter().any(|p| p.footprint == fp.footprint)
        {
            continue;
        }
        placeholders.push(PlaceholderPart {
            footprint: fp.footprint.clone(),
            tree: placeholder_tree(source_id, fp),
        });
    }

    let board_meta = board_meta(source_id, pcb);

    DerivedBoard {
        source_id,
        board_tree,
        board_sketch_id,
        board_extrude_id,
        outline_error,
        placeholders,
        board_meta,
        warnings,
        footprints: pcb.footprints.clone(),
        thickness_m,
        options,
    }
}

impl DerivedBoard {
    /// The board assembly (spec §2.3): the board instance grounded at the
    /// origin, one instance per footprint keyed by its uuid, a connector on
    /// every mounting hole. `placeholder_tabs` maps a footprint name to the
    /// tab holding its placeholder Part; a footprint with no entry is a
    /// loud warning and gets no instance.
    pub fn assembly(
        &self,
        board_tab: &str,
        placeholder_tabs: &BTreeMap<String, String>,
    ) -> (AssemblyTree, BTreeMap<Uuid, ComponentMeta>, Vec<String>) {
        let mut tree = AssemblyTree::default();
        let mut components = BTreeMap::new();
        let mut warnings = Vec::new();

        let board_id = Uuid::new_v4();
        tree.instances.push(Instance {
            id: board_id,
            name: "board".to_string(),
            source: PartRef {
                source_id: None,
                tab_id: board_tab.to_string(),
            },
            transform: Transform::identity(),
            fixed: true,
            suppressed: false,
            external_key: Some("board".to_string()),
            parameter_overrides: None,
            extra: derived_extra_named(self.source_id, RULE_BOARD_INSTANCE, "board", "board"),
        });

        for fp in &self.footprints {
            if self.options.mounting_holes && fp.is_mounting_hole_footprint() {
                for pad in fp.pads.iter().filter(|p| p.is_unconnected_hole()) {
                    tree.connectors.push(MateConnector {
                        id: Uuid::new_v4(),
                        name: format!("{} hole", fp.reference),
                        instance_path: vec![board_id],
                        geom_ref: None,
                        part_connector: None,
                        frame: Frame {
                            origin: [pad.position[0], -pad.position[1], self.thickness_m],
                            z_axis: [0.0, 0.0, 1.0],
                            x_axis: [1.0, 0.0, 0.0],
                        },
                        anchor: AxialAnchor::Middle,
                        flip_z: false,
                        rotation_deg: 0.0,
                        offset_m: [0.0; 3],
                        extra: derived_extra_named(
                            self.source_id,
                            RULE_MOUNTING_HOLE,
                            &fp.uuid,
                            &format!("{} hole", fp.reference),
                        ),
                    });
                }
            }
            if !instance_wanted(fp, &self.options) {
                continue;
            }
            let Some(tab) = placeholder_tabs.get(&fp.footprint) else {
                warnings.push(format!(
                    "{}: no placeholder part for footprint {}",
                    fp.reference, fp.footprint
                ));
                continue;
            };
            let id = Uuid::new_v4();
            tree.instances.push(Instance {
                id,
                name: fp.reference.clone(),
                source: PartRef {
                    source_id: None,
                    tab_id: tab.clone(),
                },
                transform: footprint_transform(fp, self.thickness_m),
                fixed: false,
                suppressed: false,
                external_key: Some(fp.uuid.clone()),
                parameter_overrides: None,
                extra: derived_extra_named(self.source_id, RULE_FOOTPRINT, &fp.uuid, &fp.reference),
            });
            components.insert(id, component_meta(fp));
        }
        (tree, components, warnings)
    }
}

/// Spec §3 C5/C6: a footprint on a side that is not wanted, or one marked
/// `board_only` / `exclude_from_bom` with no 3D model, gets no instance.
fn instance_wanted(fp: &Footprint, options: &DeriveOptions) -> bool {
    let side_ok = match fp.side {
        Side::Front => options.front,
        Side::Back => options.back,
    };
    let excluded = fp.models.is_empty()
        && fp
            .attrs
            .iter()
            .any(|a| a == "board_only" || a == "exclude_from_bom");
    side_ok && !excluded
}

/// Spec §2.3: `T_board_side ∘ T_place`. Front: the footprint origin on the
/// copper top, rotated `rot` about +Z (KiCad's counter-clockwise-on-screen
/// angle IS counter-clockwise about +Z once Y is flipped). Back: origin on
/// the bottom face, the part turned over about X (its +Z points down) and
/// then rotated about +Z. The back-side composition is the one the O5
/// oracle pins against `kicad-cli pcb export step` in C3.
pub fn footprint_transform(fp: &Footprint, thickness_m: f64) -> Transform {
    let (s, c) = (fp.rotation_deg.to_radians() / 2.0).sin_cos();
    let x = fp.at[0];
    let y = -fp.at[1];
    match fp.side {
        Side::Front => Transform {
            translation_m: [x, y, thickness_m],
            rotation_quat: [0.0, 0.0, s, c],
        },
        // rotZ(rot) ⊗ rotX(π) = [cos(rot/2), sin(rot/2), 0, 0].
        Side::Back => Transform {
            translation_m: [x, y, 0.0],
            rotation_quat: [c, s, 0.0, 0.0],
        },
    }
}

/// The board record (spec §2.3 "Metadata"), a pure function of the file.
pub fn board_meta(source_id: Uuid, pcb: &Pcb) -> BoardMeta {
    BoardMeta {
        source_id,
        title: pcb.title_block.title.clone(),
        rev: pcb.title_block.rev.clone(),
        date: pcb.title_block.date.clone(),
        company: pcb.title_block.company.clone(),
        comments: pcb.title_block.comments.clone(),
        copper_layers: pcb.copper_layers,
        thickness_m: pcb.thickness_m,
        net_count: pcb.nets.len(),
        footprint_count: pcb.footprints.len(),
    }
}

/// The component record of one footprint.
pub fn component_meta(fp: &Footprint) -> ComponentMeta {
    ComponentMeta {
        footprint_uuid: fp.uuid.clone(),
        reference: fp.reference.clone(),
        value: fp.value.clone(),
        footprint: fp.footprint.clone(),
        datasheet: fp.datasheet.clone(),
        side: match fp.side {
            Side::Front => "Front",
            Side::Back => "Back",
        }
        .to_string(),
        attrs: fp.attrs.clone(),
        pads: fp
            .pads
            .iter()
            .map(|p| PadMeta {
                number: p.number.clone(),
                net_name: p.net.as_ref().map(|n| n.name.clone()).unwrap_or_default(),
            })
            .collect(),
    }
}

/// `{"source_id", "rule", "key", "name"}`: `name` is the name the rule
/// minted, kept beside the rule so a re-sync can tell a user's rename (the
/// live name differs from it) from the rule's own (spec §3 R3).
fn derived_extra(source_id: Uuid, rule: &str, key: &str) -> Map<String, serde_json::Value> {
    let mut m = Map::new();
    m.insert(
        X_DERIVED.to_string(),
        json!({ "source_id": source_id, "rule": rule, "key": key }),
    );
    m
}

fn derived_extra_named(
    source_id: Uuid,
    rule: &str,
    key: &str,
    name: &str,
) -> Map<String, serde_json::Value> {
    let mut m = Map::new();
    m.insert(
        X_DERIVED.to_string(),
        json!({ "source_id": source_id, "rule": rule, "key": key, X_DERIVED_NAME: name }),
    );
    m
}

/// The `x-derived` record on an instance or connector, if it carries one.
/// `(source_id, rule, key)`.
pub fn derived_of(extra: &Map<String, serde_json::Value>) -> Option<(Uuid, &str, &str)> {
    let d = extra.get(X_DERIVED)?;
    let source_id = d.get("source_id")?.as_str()?.parse().ok()?;
    Some((source_id, d.get("rule")?.as_str()?, d.get("key")?.as_str()?))
}

/// The `x-derived` field holding the rule's own name.
const X_DERIVED_NAME: &str = "name";

/// Whether the user renamed a derived instance or connector: its live name
/// differs from the one its rule minted. A record without the stamp (none
/// is written without it; a hand-edited file might lack it) is taken as
/// renamed when the name differs from the fresh one — a rename is never
/// lost for want of a stamp.
fn user_renamed(extra: &Map<String, serde_json::Value>, live: &str, fresh: &str) -> bool {
    match extra
        .get(X_DERIVED)
        .and_then(|d| d.get(X_DERIVED_NAME))
        .and_then(|n| n.as_str())
    {
        Some(minted) => minted != live,
        None => live != fresh,
    }
}

/// Provenance of a feature in `tree`, when it is `Derived` from `source_id`:
/// the rule.
fn derived_rule(tree: &FeatureTree, feature_id: Uuid, source_id: Uuid) -> Option<&str> {
    match tree.provenance.get(&feature_id).map(|p| &p.origin) {
        Some(ProvenanceOrigin::Derived { source_id: s, rule }) if *s == source_id => Some(rule),
        _ => None,
    }
}

/// What a re-sync could not carry over (spec §3 R4): a user mate whose
/// connector belonged to an instance the source no longer has. The mate is
/// left in place, loud.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MateTargetGone {
    pub mate_id: Uuid,
    pub mate_name: String,
    pub instance_name: String,
    pub footprint_uuid: String,
}

/// Spec §3 R1 (and §4 inv. 5): replace the features `Derived` from
/// `source_id` in `old` with the freshly derived `fresh` tree, keeping every
/// other feature exactly where and as it was. A fresh feature takes the id
/// of the old feature with the same rule and ordinal — the board sketch
/// stays the board sketch, cutout 2 stays cutout 2 — so a user's sketch on
/// the board's face, or an extrude that names the board's body, re-resolves
/// through the same feature id; a derived feature with no counterpart is
/// dropped (a reference to it then fails loudly, as any reference does) and
/// a new one lands after the last derived feature.
pub fn resync_features(old: &FeatureTree, fresh: &FeatureTree, source_id: Uuid) -> FeatureTree {
    // (rule, ordinal) → old feature id.
    let mut old_by_slot: HashMap<(String, usize), Uuid> = HashMap::new();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for f in &old.features {
        if let Some(rule) = derived_rule(old, f.id, source_id) {
            let n = counts.entry(rule.to_string()).or_default();
            old_by_slot.insert((rule.to_string(), *n), f.id);
            *n += 1;
        }
    }
    // fresh id → kept id.
    let mut id_map: HashMap<Uuid, Uuid> = HashMap::new();
    counts.clear();
    for f in &fresh.features {
        let Some(rule) = derived_rule(fresh, f.id, source_id) else {
            continue;
        };
        let n = counts.entry(rule.to_string()).or_default();
        if let Some(&kept) = old_by_slot.get(&(rule.to_string(), *n)) {
            id_map.insert(f.id, kept);
        }
        *n += 1;
    }

    let remap = |id: Uuid| id_map.get(&id).copied().unwrap_or(id);
    let remapped: Vec<Feature> = fresh
        .features
        .iter()
        .map(|f| {
            let mut f = f.clone();
            let kept = remap(f.id);
            f.id = kept;
            match &mut f.operation {
                Operation::Extrude { params } => {
                    params.sketch_id = remap(params.sketch_id);
                    for t in params.targets.iter_mut().flatten() {
                        if let Anchor::FeatureOutput { feature_id, .. } = &mut t.anchor {
                            *feature_id = remap(*feature_id);
                        }
                    }
                }
                Operation::Sketch { sketch } => {
                    // The sketch's own id survives too: nothing about the
                    // regeneration is a new sketch to a reader of the file.
                    if let Some(Operation::Sketch { sketch: prior }) = old
                        .features
                        .iter()
                        .find(|o| o.id == kept)
                        .map(|o| &o.operation)
                    {
                        sketch.id = prior.id;
                    }
                }
                _ => {}
            }
            f
        })
        .collect();

    // Splice: an old derived feature is replaced in place by its fresh
    // counterpart or dropped; a fresh feature with no counterpart goes right
    // after the previously placed fresh feature (or first, if none yet).
    let kept_ids: std::collections::HashSet<Uuid> = id_map.values().copied().collect();
    let mut out: Vec<Feature> = old
        .features
        .iter()
        .filter(|f| derived_rule(old, f.id, source_id).is_none() || kept_ids.contains(&f.id))
        .cloned()
        .collect();
    let mut cursor: Option<usize> = None;
    for f in remapped {
        if let Some(pos) = out.iter().position(|o| o.id == f.id) {
            out[pos] = f;
            cursor = Some(pos);
        } else {
            let at = cursor.map(|c| c + 1).unwrap_or(0);
            out.insert(at, f);
            cursor = Some(at);
        }
    }

    let mut tree = old.clone();
    tree.features = out;
    tree.provenance
        .retain(|id, _| derived_rule(old, *id, source_id).is_none() || kept_ids.contains(id));
    for (id, p) in &fresh.provenance {
        tree.provenance.insert(remap(*id), p.clone());
    }
    tree
}

/// Spec §3 R2–R4: reconcile a freshly derived assembly (`fresh`, new ids
/// throughout) with the tab's current tree (`old`) by `external_key`.
///
/// An instance or connector derived from `source_id` whose key the fresh
/// tree still has keeps its id (inv. 4), its `suppressed`, and its `name`
/// when the user changed it from the rule's own; its transform, source and
/// derivation are regenerated. One with no fresh counterpart is removed
/// with every connector on it; a mate that used such a connector is left in
/// place and reported. Everything not derived from `source_id` — the user's
/// instances, connectors, mates, and content derived from another board —
/// is untouched. Returns the reconciled tree, the map from fresh instance
/// ids to the ids that stand for them, and the mates left dangling.
pub fn resync_assembly(
    old: &AssemblyTree,
    mut fresh: AssemblyTree,
    source_id: Uuid,
) -> (AssemblyTree, BTreeMap<Uuid, Uuid>, Vec<MateTargetGone>) {
    let mine = |extra: &Map<String, serde_json::Value>| -> Option<String> {
        derived_of(extra)
            .filter(|(s, _, _)| *s == source_id)
            .map(|(_, rule, key)| format!("{rule}\u{0}{key}"))
    };

    // Instances: fresh id → kept id, by (rule, key).
    let old_instances: HashMap<String, &Instance> = old
        .instances
        .iter()
        .filter_map(|i| mine(&i.extra).map(|k| (k, i)))
        .collect();
    let mut id_map: BTreeMap<Uuid, Uuid> = BTreeMap::new();
    for inst in &mut fresh.instances {
        let Some(k) = mine(&inst.extra) else { continue };
        if let Some(prior) = old_instances.get(&k) {
            id_map.insert(inst.id, prior.id);
            inst.id = prior.id;
            inst.suppressed = prior.suppressed;
            if user_renamed(&prior.extra, &prior.name, &inst.name) {
                inst.name = prior.name.clone();
            }
        }
    }
    let remap_path = |path: &mut Vec<Uuid>| {
        for id in path.iter_mut() {
            if let Some(kept) = id_map.get(id) {
                *id = *kept;
            }
        }
    };

    // Connectors: by (rule, key, ordinal within the key) — a footprint with
    // two mounting holes keeps both.
    let mut old_conns: HashMap<String, Vec<&MateConnector>> = HashMap::new();
    for c in &old.connectors {
        if let Some(k) = mine(&c.extra) {
            old_conns.entry(k).or_default().push(c);
        }
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    for c in &mut fresh.connectors {
        remap_path(&mut c.instance_path);
        let Some(k) = mine(&c.extra) else { continue };
        let n = seen.entry(k.clone()).or_default();
        if let Some(prior) = old_conns.get(&k).and_then(|v| v.get(*n)) {
            c.id = prior.id;
            if user_renamed(&prior.extra, &prior.name, &c.name) {
                c.name = prior.name.clone();
            }
        }
        *n += 1;
    }

    // Assemble: the user's content, then what survived or is new.
    let kept_instances: std::collections::HashSet<Uuid> =
        fresh.instances.iter().map(|i| i.id).collect();
    let removed_instances: Vec<&Instance> = old
        .instances
        .iter()
        .filter(|i| mine(&i.extra).is_some() && !kept_instances.contains(&i.id))
        .collect();
    let removed_ids: std::collections::HashSet<Uuid> =
        removed_instances.iter().map(|i| i.id).collect();

    let mut out = old.clone();
    out.instances = Vec::new();
    for i in &old.instances {
        if removed_ids.contains(&i.id) {
            continue;
        }
        match fresh.instances.iter().position(|f| f.id == i.id) {
            Some(pos) => out.instances.push(fresh.instances.remove(pos)),
            None if mine(&i.extra).is_some() => {}
            None => out.instances.push(i.clone()),
        }
    }
    out.instances.append(&mut fresh.instances);

    let fresh_conn_ids: std::collections::HashSet<Uuid> =
        fresh.connectors.iter().map(|c| c.id).collect();
    let mut gone_connectors: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    out.connectors = Vec::new();
    for c in &old.connectors {
        let on_removed = c
            .instance_path
            .first()
            .is_some_and(|i| removed_ids.contains(i));
        if on_removed || (mine(&c.extra).is_some() && !fresh_conn_ids.contains(&c.id)) {
            gone_connectors.insert(c.id);
            continue;
        }
        match fresh.connectors.iter().position(|f| f.id == c.id) {
            Some(pos) => out.connectors.push(fresh.connectors.remove(pos)),
            None => out.connectors.push(c.clone()),
        }
    }
    out.connectors.append(&mut fresh.connectors);

    let owner_of = |conn: Uuid| -> Option<&Instance> {
        let c = old.connectors.iter().find(|c| c.id == conn)?;
        let top = *c.instance_path.first()?;
        old.instances.iter().find(|i| i.id == top)
    };
    let mut dangling = Vec::new();
    for m in &old.mates {
        for conn in m.connectors {
            if gone_connectors.contains(&conn) {
                if let Some(inst) = owner_of(conn) {
                    dangling.push(MateTargetGone {
                        mate_id: m.id,
                        mate_name: m.name.clone(),
                        instance_name: inst.name.clone(),
                        footprint_uuid: inst.external_key.clone().unwrap_or_default(),
                    });
                }
            }
        }
    }
    out.placements.retain(|id, _| !removed_ids.contains(id));
    (out, id_map, dangling)
}

fn push_derived(tree: &mut FeatureTree, feature: Feature, source_id: Uuid, rule: &str) {
    tree.provenance.insert(
        feature.id,
        Provenance {
            origin: ProvenanceOrigin::Derived {
                source_id,
                rule: rule.to_string(),
            },
            at: None,
        },
    );
    tree.features.push(feature);
}

/// A blind extrude of one profile (by entity ids, §2.9) along the sketch
/// normal: a new body, or — with `cut_from` — a cut of exactly that body.
fn extrude(
    sketch_feature: Uuid,
    ids: Vec<u32>,
    depth: f64,
    cut_from: Option<Uuid>,
) -> ExtrudeParams {
    let (cut, combine, targets) = match cut_from {
        None => (false, CombineMode::NewBody, None),
        Some(target) => (
            true,
            CombineMode::Cut,
            Some(vec![GeomRef {
                kind: TopoKind::Solid,
                anchor: Anchor::FeatureOutput {
                    feature_id: target,
                    output_key: OutputKey::Main,
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
                scope: None,
            }]),
        ),
    };
    ExtrudeParams {
        sketch_id: sketch_feature,
        profile_index: 0,
        profile_entity_ids: Some(ids),
        depth,
        depth_expr: None,
        direction: None,
        // Cuts are symmetric (see the cutout site); a body stands on z = 0.
        symmetric: cut,
        cut,
        merge: false,
        target_body: None,
        depth_mode: DepthMode::Blind,
        second_direction: None,
        region: None,
        regions: Vec::new(),
        combine: Some(combine),
        targets,
    }
}

/// Placeholder Part: a box over the footprint's pads in its own frame
/// (`(u, v) = (x_local, −y_local)`), 1 mm tall, standing on `z = 0`.
fn placeholder_tree(source_id: Uuid, fp: &Footprint) -> FeatureTree {
    let (mut min, mut max) = (
        [f64::INFINITY, f64::INFINITY],
        [f64::NEG_INFINITY, f64::NEG_INFINITY],
    );
    for pad in &fp.pads {
        let local = unplace(fp, pad.position);
        let half = pad.size[0].max(pad.size[1]) / 2.0;
        min[0] = min[0].min(local[0] - half);
        min[1] = min[1].min(local[1] - half);
        max[0] = max[0].max(local[0] + half);
        max[1] = max[1].max(local[1] + half);
    }
    if fp.pads.is_empty() {
        min = [-PLACEHOLDER_MIN_HALF_M; 2];
        max = [PLACEHOLDER_MIN_HALF_M; 2];
    }
    // Sketch frame: flip y.
    let (u0, u1) = (min[0], max[0]);
    let (v0, v1) = (-max[1], -min[1]);
    let mut w = SketchWriter::default();
    let ids = w.write_loop(&Loop {
        segments: vec![
            Segment::Line {
                start: [u0, -v0],
                end: [u1, -v0],
            },
            Segment::Line {
                start: [u1, -v0],
                end: [u1, -v1],
            },
            Segment::Line {
                start: [u1, -v1],
                end: [u0, -v1],
            },
            Segment::Line {
                start: [u0, -v1],
                end: [u0, -v0],
            },
        ],
        signed_area_m2: 0.0,
    });
    let mut tree = FeatureTree::new();
    tree.extra = derived_extra(source_id, RULE_PLACEHOLDER, &fp.footprint);
    let sketch_id = Uuid::new_v4();
    push_derived(
        &mut tree,
        Feature {
            id: sketch_id,
            name: format!("{} footprint", fp.footprint),
            operation: Operation::Sketch {
                sketch: w.into_sketch(),
            },
            suppressed: false,
            references: Vec::new(),
        },
        source_id,
        RULE_PLACEHOLDER,
    );
    push_derived(
        &mut tree,
        Feature {
            id: Uuid::new_v4(),
            name: format!("{} placeholder", fp.footprint),
            operation: Operation::Extrude {
                params: extrude(sketch_id, ids, PLACEHOLDER_HEIGHT_M, None),
            },
            suppressed: false,
            references: Vec::new(),
        },
        source_id,
        RULE_PLACEHOLDER,
    );
    tree
}

/// Inverse of `kicad_pcb::place`: a board-frame point back into the
/// footprint's local (Y-down) frame.
fn unplace(fp: &Footprint, board: [f64; 2]) -> [f64; 2] {
    let (s, c) = fp.rotation_deg.to_radians().sin_cos();
    let dx = board[0] - fp.at[0];
    let dy = board[1] - fp.at[1];
    [dx * c - dy * s, dx * s + dy * c]
}

/// Writes loops into sketch entities, welding shared vertices, flipping the
/// frame from Y-down to the sketch's (u, v).
#[derive(Default)]
struct SketchWriter {
    entities: Vec<SketchEntity>,
    points: Vec<([f64; 2], u32)>,
    next_id: u32,
}

impl SketchWriter {
    fn alloc(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Sketch point for a Y-down board point (welded).
    fn point(&mut self, p: [f64; 2], construction: bool) -> u32 {
        let uv = [p[0], -p[1]];
        if let Some((_, id)) = self
            .points
            .iter()
            .find(|(q, _)| (q[0] - uv[0]).hypot(q[1] - uv[1]) <= WELD_M)
        {
            return *id;
        }
        let id = self.alloc();
        self.points.push((uv, id));
        self.entities.push(SketchEntity::Point {
            id,
            x: uv[0],
            y: uv[1],
            construction,
        });
        id
    }

    /// One loop's segments; returns the ids of its curve entities (the
    /// profile's identity for `profile_entity_ids`).
    fn write_loop(&mut self, l: &Loop) -> Vec<u32> {
        let mut ids = Vec::new();
        for s in &l.segments {
            ids.push(self.write_segment(s));
        }
        ids
    }

    fn write_segment(&mut self, s: &Segment) -> u32 {
        match s {
            Segment::Line { start, end } => {
                let a = self.point(*start, false);
                let b = self.point(*end, false);
                let id = self.alloc();
                self.entities.push(SketchEntity::Line {
                    id,
                    start_id: a,
                    end_id: b,
                    construction: false,
                });
                id
            }
            Segment::Arc {
                start,
                end,
                center,
                ccw,
                ..
            } => {
                // Sketch arcs sweep counter-clockwise start → end. A loop arc
                // that is angle-increasing in the Y-down frame becomes
                // clockwise once Y flips, so its ends swap.
                let (s_id, e_id) = if *ccw {
                    (self.point(*end, false), self.point(*start, false))
                } else {
                    (self.point(*start, false), self.point(*end, false))
                };
                let c_id = self.point(*center, true);
                let id = self.alloc();
                self.entities.push(SketchEntity::Arc {
                    id,
                    center_id: c_id,
                    start_id: s_id,
                    end_id: e_id,
                    construction: false,
                });
                id
            }
            Segment::Circle { center, radius } => {
                let c_id = self.point(*center, true);
                let id = self.alloc();
                self.entities.push(SketchEntity::Circle {
                    id,
                    center_id: c_id,
                    radius: *radius,
                    construction: false,
                });
                id
            }
        }
    }

    /// Unchained primitives, as drawn (the outline-error case).
    fn write_primitives(&mut self, prims: &[kicad_pcb::OutlinePrimitive]) {
        for p in prims {
            match &p.shape {
                OutlineShape::Line { start, end } => {
                    self.write_segment(&Segment::Line {
                        start: *start,
                        end: *end,
                    });
                }
                OutlineShape::Arc { start, mid, end } => {
                    if let Some(center) = kicad_pcb::circumcenter(*start, *mid, *end) {
                        let radius = (start[0] - center[0]).hypot(start[1] - center[1]);
                        let ccw = kicad_pcb::arc_is_ccw(center, *start, *mid, *end);
                        self.write_segment(&Segment::Arc {
                            start: *start,
                            end: *end,
                            center,
                            radius,
                            ccw,
                        });
                    }
                }
                OutlineShape::Circle { center, radius } => {
                    self.write_segment(&Segment::Circle {
                        center: *center,
                        radius: *radius,
                    });
                }
            }
        }
    }

    fn into_sketch(self) -> Sketch {
        let mut sketch = Sketch {
            id: Uuid::new_v4(),
            plane: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::Datum {
                    datum_id: XY_DATUM_ID.parse().expect("well-known datum id"),
                },
                selector: Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
                scope: None,
            },
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            // Explicit +X: the derived basis would send u along −Y.
            plane_x_axis: Some([1.0, 0.0, 0.0]),
            entities: self.entities,
            constraints: Vec::new(),
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: HashMap::new(),
            solved_profiles: Vec::new(),
            projected: Vec::new(),
        };
        // The same finish the script host gives an engine-authored sketch:
        // loops extracted WITH kernel-ready arc segments. Leaving it to the
        // rebuild's `recompute_derived` extracts chord loops only, and every
        // arc is extruded as its chord (measured: a rounded 60×40 board came
        // out 50 mm² short).
        crate::script::host::derive_sketch(&mut sketch);
        sketch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECT: &str = include_str!("../../kicad-pcb/tests/fixtures/rect_v8.kicad_pcb");
    const HOLES: &str = include_str!("../../kicad-pcb/tests/fixtures/holes_v6.kicad_pcb");

    fn user_feature(name: &str) -> Feature {
        Feature {
            id: Uuid::new_v4(),
            name: name.to_string(),
            operation: Operation::Sketch {
                sketch: SketchWriter::default().into_sketch(),
            },
            suppressed: false,
            references: Vec::new(),
        }
    }

    fn rules(tree: &FeatureTree, source_id: Uuid) -> Vec<String> {
        tree.features
            .iter()
            .map(|f| {
                derived_rule(tree, f.id, source_id)
                    .unwrap_or("user")
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn resync_drops_cutouts_the_source_lost_and_keeps_the_rest_in_place() {
        let source = Uuid::new_v4();
        let holes = kicad_pcb::parse_kicad_pcb(HOLES).unwrap();
        let rect = kicad_pcb::parse_kicad_pcb(RECT).unwrap();
        let mut old = derive_board(source, &holes, DeriveOptions::default()).board_tree;
        assert_eq!(
            old.features.len(),
            5,
            "outline, board, cutouts sketch, 2 cuts"
        );
        let user = user_feature("enclosure");
        let user_id = user.id;
        old.features.push(user);
        let old_ids: Vec<Uuid> = old.features.iter().map(|f| f.id).collect();

        let fresh = derive_board(source, &rect, DeriveOptions::default()).board_tree;
        let out = resync_features(&old, &fresh, source);
        assert_eq!(
            rules(&out, source),
            [RULE_BOARD_OUTLINE, RULE_BOARD_EXTRUDE, "user"]
        );
        assert_eq!(out.features[0].id, old_ids[0]);
        assert_eq!(out.features[1].id, old_ids[1]);
        assert_eq!(out.features[2].id, user_id);
        // The extrude names the (kept) sketch id.
        let Operation::Extrude { params } = &out.features[1].operation else {
            panic!()
        };
        assert_eq!(params.sketch_id, old_ids[0]);
        // Dropped features lose their provenance rows; the kept keep theirs.
        assert_eq!(out.provenance.len(), 2);
        assert!(out.provenance.contains_key(&old_ids[1]));
        assert!(!out.provenance.contains_key(&old_ids[3]));
        // The tree's own stamp survives.
        assert_eq!(
            derived_of(&out.extra),
            Some((source, RULE_BOARD_PART, "board"))
        );
    }

    #[test]
    fn resync_inserts_new_cutouts_after_the_board_and_before_the_users() {
        let source = Uuid::new_v4();
        let holes = kicad_pcb::parse_kicad_pcb(HOLES).unwrap();
        let rect = kicad_pcb::parse_kicad_pcb(RECT).unwrap();
        let mut old = derive_board(source, &rect, DeriveOptions::default()).board_tree;
        let user = user_feature("enclosure");
        let user_id = user.id;
        old.features.push(user);
        let board_id = old.features[1].id;

        let fresh = derive_board(source, &holes, DeriveOptions::default()).board_tree;
        let out = resync_features(&old, &fresh, source);
        assert_eq!(
            rules(&out, source),
            [
                RULE_BOARD_OUTLINE,
                RULE_BOARD_EXTRUDE,
                RULE_BOARD_CUTOUTS,
                RULE_BOARD_CUTOUT,
                RULE_BOARD_CUTOUT,
                "user"
            ]
        );
        assert_eq!(out.features[1].id, board_id);
        assert_eq!(out.features[5].id, user_id);
        // The first cut targets the kept board id; the second chains on the
        // first (fresh ids, consistently remapped).
        let Operation::Extrude { params } = &out.features[3].operation else {
            panic!()
        };
        let Anchor::FeatureOutput { feature_id, .. } = params.targets.as_ref().unwrap()[0].anchor
        else {
            panic!()
        };
        assert_eq!(feature_id, board_id);
        let Operation::Extrude { params } = &out.features[4].operation else {
            panic!()
        };
        let Anchor::FeatureOutput { feature_id, .. } = params.targets.as_ref().unwrap()[0].anchor
        else {
            panic!()
        };
        assert_eq!(feature_id, out.features[3].id);
    }

    #[test]
    fn resync_assembly_keeps_ids_renames_and_reports_dangling_mates() {
        let source = Uuid::new_v4();
        let rect = kicad_pcb::parse_kicad_pcb(RECT).unwrap();
        let derived = derive_board(source, &rect, DeriveOptions::default());
        let mut tabs = BTreeMap::new();
        for p in &derived.placeholders {
            tabs.insert(p.footprint.clone(), format!("tab-{}", p.footprint));
        }
        let (mut old, _, _) = derived.assembly("board-tab", &tabs);
        let r1 = old.instances.iter_mut().find(|i| i.name == "R1").unwrap();
        r1.name = "pull-up".to_string();
        let r1_id = r1.id;
        let hole = old.connectors[0].id;
        let user_conn = MateConnector {
            id: Uuid::new_v4(),
            name: "mine".into(),
            instance_path: vec![r1_id],
            geom_ref: None,
            part_connector: None,
            frame: Frame::default(),
            anchor: AxialAnchor::Middle,
            flip_z: false,
            rotation_deg: 0.0,
            offset_m: [0.0; 3],
            extra: Map::new(),
        };
        let user_conn_id = user_conn.id;
        old.connectors.push(user_conn);
        old.mates.push(crate::assembly::Mate {
            id: Uuid::new_v4(),
            name: "to the hole".into(),
            kind: crate::assembly::MateKind::Fastened {
                flip: false,
                rotation_deg: 0.0,
            },
            connectors: [hole, user_conn_id],
            suppressed: false,
            extra: Map::new(),
        });

        // Fresh: R1 still there (new ids), the mounting hole gone.
        let mut pruned = rect.clone();
        pruned.footprints.retain(|f| f.reference != "H1");
        let fresh = derive_board(source, &pruned, DeriveOptions::default())
            .assembly("board-tab", &tabs)
            .0;
        let (out, id_map, gone) = resync_assembly(&old, fresh, source);
        let r1 = out
            .instances
            .iter()
            .find(|i| i.external_key.as_deref() == Some("0a1b2c3d-0000-4000-8000-000000000001"))
            .unwrap();
        assert_eq!(r1.id, r1_id);
        assert_eq!(r1.name, "pull-up");
        assert_eq!(id_map.len(), 3, "board, R1, C1 mapped");
        assert!(out.connectors.iter().all(|c| c.id != hole));
        assert!(out.connectors.iter().any(|c| c.id == user_conn_id));
        assert_eq!(out.mates.len(), 1, "the mate is kept");
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].mate_name, "to the hole");
        assert_eq!(gone[0].instance_name, "board");
    }

    #[test]
    fn front_transform_is_a_z_rotation_on_the_copper_top() {
        let fp = Footprint {
            uuid: "u".into(),
            library_id: "L:N".into(),
            side: Side::Front,
            at: [0.02, 0.015],
            rotation_deg: 90.0,
            reference: "R1".into(),
            value: "".into(),
            footprint: "L:N".into(),
            datasheet: "".into(),
            attrs: vec![],
            pads: vec![],
            models: vec![],
        };
        let t = footprint_transform(&fp, 1.6e-3);
        assert_eq!(t.translation_m, [0.02, -0.015, 1.6e-3]);
        let h = std::f64::consts::FRAC_PI_4;
        assert!((t.rotation_quat[2] - h.sin()).abs() < 1e-15);
        assert!((t.rotation_quat[3] - h.cos()).abs() < 1e-15);
    }

    #[test]
    fn back_transform_turns_the_part_over_on_the_bottom_face() {
        let fp = Footprint {
            uuid: "u".into(),
            library_id: "L:N".into(),
            side: Side::Back,
            at: [0.035, 0.010],
            rotation_deg: 0.0,
            reference: "C1".into(),
            value: "".into(),
            footprint: "L:N".into(),
            datasheet: "".into(),
            attrs: vec![],
            pads: vec![],
            models: vec![],
        };
        let t = footprint_transform(&fp, 1.6e-3);
        assert_eq!(t.translation_m, [0.035, -0.010, 0.0]);
        assert_eq!(t.rotation_quat, [1.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn unplace_inverts_place() {
        let fp = Footprint {
            uuid: "u".into(),
            library_id: "L:N".into(),
            side: Side::Front,
            at: [0.02, 0.015],
            rotation_deg: 37.0,
            reference: "R1".into(),
            value: "".into(),
            footprint: "L:N".into(),
            datasheet: "".into(),
            attrs: vec![],
            pads: vec![],
            models: vec![],
        };
        let local = [0.0031, -0.0007];
        let back = unplace(&fp, fp.place(local));
        assert!((back[0] - local[0]).abs() < 1e-15 && (back[1] - local[1]).abs() < 1e-15);
    }

    #[test]
    fn arc_ends_swap_when_the_flip_reverses_the_sweep() {
        // In the Y-down frame: angle-increasing (ccw=true) arc from (1,0)
        // through (0,1) to (−1,0). Flipped, that runs clockwise, so the
        // sketch arc must go from (−1,0) to (1,0) about the origin, i.e.
        // start_id is the point at (−1, 0).
        let mut w = SketchWriter::default();
        let id = w.write_segment(&Segment::Arc {
            start: [1.0, 0.0],
            end: [-1.0, 0.0],
            center: [0.0, 0.0],
            radius: 1.0,
            ccw: true,
        });
        let SketchEntity::Arc { start_id, .. } = w.entities.iter().find(|e| e.id() == id).unwrap()
        else {
            panic!()
        };
        let start = w
            .entities
            .iter()
            .find_map(|e| match e {
                SketchEntity::Point { id, x, y, .. } if id == start_id => Some((*x, *y)),
                _ => None,
            })
            .unwrap();
        assert_eq!(start, (-1.0, 0.0));
    }
}
