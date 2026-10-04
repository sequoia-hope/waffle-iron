//! Measuring the model from an expression — the engine half of D2
//! (`specs/drawings_and_mbd.md` §6, `specs/agent_mechanical_design.md` §6
//! P4).
//!
//! [`crate::expr::measure`] defines WHAT the measurement functions are and
//! what dimension each answer carries. This module is the one implementation
//! of [`Measurer`] that actually reads geometry: it resolves an N1 entity
//! name against the live kernel, asks `KernelMeasure`/`KernelIntrospect` the
//! question, and converts the answer into the evaluator's working space.
//!
//! ## Three things it is deliberately strict about
//!
//! **Resolution goes through `names::resolve`, not through the stored
//! reference.** That is the same resolution `names_list` reports — the pid
//! first, the authored fallback only when the pid is gone — so a name the
//! listing calls resolvable is a name an expression can measure. Resolving
//! the reference directly instead would be one question answered two ways.
//!
//! **A measurement is Strict or it is a refusal.** `names::mint` stores
//! every name under `ResolvePolicy::Strict` (§5.3 N2), so a near miss
//! refuses rather than silently measuring a different face. A vanished
//! identity becomes [`crate::expr::ExprError::MeasurementFailed`], naming
//! the function and the name.
//!
//! **Every read is RECORDED, and the ORDER is enforced here.**
//! [`TreeMeasurer::read_features`] is the set of features whose geometry
//! answered — the dependency list §6 asks for. The caller positions the
//! measurer with [`TreeMeasurer::set_floor`] before each expression, and a
//! measurement that reads geometry at or after that position is refused:
//! that is §6's cycle, caught ordinally, before any number is computed. The
//! caller positions because only it knows which field is being driven; the
//! measurer judges because only it knows which feature owns the geometry.
//!
//! ## Units
//!
//! The kernel works in metres; the evaluator works in millimetres, mm²,
//! mm³ and degrees (`crate::expr`'s working space). The conversion happens
//! here, next to each kernel call, and is the only place it happens.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};

use modeling_ops::OpResult;
use uuid::Uuid;
use waffle_types::kernel::{
    DistanceOpts, KernelId, KernelIntrospect, KernelMeasure, MeasureEntity,
};
use waffle_types::TopoKind;

use crate::expr::{MeasureCall, MeasureRefusal, Measurer};
use crate::types::FeatureTree;

/// Millimetres per metre — the evaluator's length working space.
const MM_PER_M: f64 = 1e3;

/// What an entity name names, with the feature that owns the geometry.
struct Resolved {
    entity: MeasureEntity,
    /// The kernel id, for the introspection questions `KernelMeasure` has no
    /// method for (`area`, `radius`). `None` for a whole body, which is
    /// addressed by its handle.
    id: Option<(KernelId, TopoKind)>,
    /// Every feature this geometry depends on being built — the dependency
    /// list §6 asks for, and the ordering rule's input.
    ///
    /// Up to two, and they can differ: the reference's own ANCHOR names the
    /// feature whose output the entity is found in, while the persistent
    /// id's LINEAGE ROOT names the feature that introduced the geometry. For
    /// a boolean-output face those are different features, and the rule
    /// wants the LATER of them — availability, not provenance, is what a
    /// rebuild has to wait for. Empty when neither could be determined (a
    /// mesh-backed import), which the caller is told about.
    owners: Vec<Uuid>,
    /// What kind of thing it is, for a diagnostic.
    what: &'static str,
    /// The body's `FeatureTree::body_id`, when this resolved to a whole
    /// body. `None` for a face, edge or vertex. M1's `mass` needs it: the
    /// density is a property of the BODY (through its material), not of the
    /// kernel handle, and the handle is the only thing `MeasureEntity`
    /// carries.
    body_id: Option<String>,
}

/// A body the document has given a name, and what produced it.
struct NamedBody {
    /// `FeatureTree::body_id` — the ordering key, so two bodies sharing one
    /// override name resolve the same way in every process.
    body_id: String,
    name: String,
    feature: Uuid,
    handle: waffle_types::kernel::KernelSolidHandle,
}

/// Answers measurement functions from the live feature tree and kernel.
///
/// Holds OWNED copies of the two small name tables (and of the named-body
/// index) rather than borrowing the tree: the apply pass that drives it
/// needs `&mut FeatureTree` at the same time, and a tree borrow here would
/// make that impossible. Both tables are a handful of entries, and the copy
/// is made only on a rebuild that actually measures something.
pub struct TreeMeasurer<'a> {
    names: crate::names::NameTable,
    bodies: Vec<NamedBody>,
    /// Feature id → its index in `FeatureTree::features`. The ordering rule
    /// below is stated in these indexes.
    index_of: HashMap<Uuid, usize>,
    /// Each feature's display name, by the same index — so a refused
    /// measurement names the two features an author has to look at rather
    /// than only their positions.
    labels: Vec<String>,
    results: &'a HashMap<Uuid, OpResult>,
    introspect: &'a dyn KernelIntrospect,
    kernel: &'a dyn KernelMeasure,
    /// Persistent id → the feature that introduced it
    /// (`Engine::pid_to_feature`). The ordering check's only source.
    pid_to_feature: &'a HashMap<u64, Uuid>,
    /// Body id → the density `mass(body)` should use, kg/m³, or the reason
    /// there is none (M1).
    ///
    /// Resolved ONCE, in [`TreeMeasurer::new`], rather than per call: it is
    /// a map lookup over two small tables and holding the answer keeps the
    /// tree out of this struct, which is the same reason `names` and
    /// `bodies` are owned copies. `None` for a body with no material at
    /// all; `Some(Err)` for a dangling or invalid assignment, so the
    /// refusal names it.
    densities: HashMap<String, Result<f64, String>>,
    /// The feature index whose expression is being evaluated right now.
    ///
    /// **The ordering rule**: a measurement may only read geometry from a
    /// feature EARLIER than the one it drives. `Some(j)` refuses a
    /// measurement of geometry owned by a feature at index `>= j` — which
    /// is §6's cycle ("a feature whose own dimension reads its own output"),
    /// caught ordinally and so by construction rather than by letting a
    /// fixpoint spin. `None` is "no position yet"; the caller must set one
    /// before evaluating anything positioned.
    floor: Cell<Option<usize>>,
    /// The highest owner index any measurement has read since the floor was
    /// last set — what a caller uses to position something that has no
    /// index of its own.
    high_water: Cell<Option<usize>>,
    /// Every feature whose geometry answered a measurement, and every name
    /// whose owner could not be determined. `RefCell` because `measure`
    /// takes `&self` — the measurer must not be able to change the model it
    /// is measuring.
    read: RefCell<Reads>,
}

/// What a run of measurements read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reads {
    /// Features whose geometry answered. These are the rebuild dependencies
    /// of the expression that was evaluated.
    pub features: BTreeSet<Uuid>,
    /// Names that resolved but whose owning feature the kernel could not
    /// name (a mesh-backed import has no persistent ids). The ordering check
    /// cannot see these, so they are reported rather than passed off as
    /// "depends on nothing".
    pub unattributed: BTreeSet<String>,
}

impl<'a> TreeMeasurer<'a> {
    pub fn new(
        tree: &FeatureTree,
        results: &'a HashMap<Uuid, OpResult>,
        introspect: &'a dyn KernelIntrospect,
        kernel: &'a dyn KernelMeasure,
        pid_to_feature: &'a HashMap<u64, Uuid>,
    ) -> Self {
        let mut bodies: Vec<NamedBody> = Vec::new();
        let mut index_of = HashMap::new();
        for (idx, feature) in tree.features.iter().enumerate() {
            index_of.insert(feature.id, idx);
            let Some(result) = results.get(&feature.id) else {
                continue;
            };
            for (key, body) in &result.outputs {
                let body_id = FeatureTree::body_id(feature.id, key);
                if let Some(name) = tree.body_name_override(&body_id) {
                    bodies.push(NamedBody {
                        name: name.to_string(),
                        body_id,
                        feature: feature.id,
                        handle: body.handle.clone(),
                    });
                }
            }
        }
        // Ordered by body id, not by `outputs`' hash order: nothing enforces
        // that one override name belongs to one body, and a measurement must
        // not answer from a different one in the next process.
        bodies.sort_by(|a, b| a.body_id.cmp(&b.body_id));
        // Only the bodies that HAVE an assignment get an entry; a body with
        // none is absent, and `mass` refuses on absence rather than
        // substituting a density.
        let densities = bodies
            .iter()
            .filter(|b| tree.body_materials.contains_key(&b.body_id))
            .map(|b| {
                let answer = match tree.density_of_body(&b.body_id) {
                    Ok(Some(rho)) => Ok(rho),
                    // Unreachable given the filter, and reported rather than
                    // asserted: this is a library.
                    Ok(None) => Err(format!("body \"{}\" has no material", b.body_id)),
                    Err(why) => Err(why),
                };
                (b.body_id.clone(), answer)
            })
            .collect();
        Self {
            names: tree.names.clone(),
            bodies,
            index_of,
            labels: tree.features.iter().map(|f| f.name.clone()).collect(),
            results,
            introspect,
            kernel,
            pid_to_feature,
            densities,
            floor: Cell::new(None),
            high_water: Cell::new(None),
            read: RefCell::new(Reads::default()),
        }
    }

    /// What every measurement taken through this measurer so far has read.
    pub fn read_features(&self) -> Reads {
        self.read.borrow().clone()
    }

    /// Position the measurer at the feature index whose expression is about
    /// to be evaluated, and clear the high-water mark. See
    /// [`TreeMeasurer::floor`]'s docs for the rule this enforces.
    pub fn set_floor(&self, at: Option<usize>) {
        self.floor.set(at);
        self.high_water.set(None);
    }

    /// The highest owner index read since the last [`TreeMeasurer::set_floor`]
    /// — how a caller positions an expression that has no index of its own
    /// (a design parameter).
    pub fn high_water(&self) -> Option<usize> {
        self.high_water.get()
    }

    /// Where a feature sits in `FeatureTree::features`.
    pub fn index_of(&self, feature: Uuid) -> Option<usize> {
        self.index_of.get(&feature).copied()
    }

    /// Resolve one entity name to something measurable.
    fn resolve(&self, name: &str) -> Result<Resolved, MeasureRefusal> {
        // An ENTITY name first — the same order `entity_names::resolve_target`
        // uses at the bridge, so one spelling means one thing everywhere.
        if let Some(named) = self.names.get(name) {
            let resolution =
                crate::names::resolve(named, self.results, self.introspect).map_err(|e| {
                    MeasureRefusal::Entity {
                        name: name.to_string(),
                        reason: format!("the name does not resolve: {e}"),
                    }
                })?;
            let id = resolution.kernel_id;
            let kind = named.kind;
            let entity = match kind {
                TopoKind::Face => MeasureEntity::Face(id),
                TopoKind::Edge => MeasureEntity::Edge(id),
                TopoKind::Vertex => MeasureEntity::Vertex(id),
                other => {
                    return Err(MeasureRefusal::Entity {
                        name: name.to_string(),
                        reason: format!(
                            "the name is a {other:?}, which is not measurable; name the \
                             body itself to measure a solid"
                        ),
                    })
                }
            };
            let mut owners: Vec<Uuid> = Vec::new();
            // The anchor: where the entity is FOUND. Available without the
            // kernel tracking anything, and the stricter of the two for a
            // boolean output face.
            if let waffle_types::Anchor::FeatureOutput { feature_id, .. } = &named.target.anchor {
                owners.push(*feature_id);
            }
            // The lineage root: where the geometry was INTRODUCED, through
            // chained booleans (KV13 F6).
            if let Some(root) = self
                .introspect
                .entity_pid(id, kind)
                .and_then(|pid| self.pid_to_feature.get(&pid.root_pid).copied())
            {
                if !owners.contains(&root) {
                    owners.push(root);
                }
            }
            if owners.is_empty() {
                self.read.borrow_mut().unattributed.insert(name.to_string());
            }
            return Ok(Resolved {
                entity,
                id: Some((id, kind)),
                owners,
                what: match kind {
                    TopoKind::Face => "a face",
                    TopoKind::Edge => "an edge",
                    _ => "a vertex",
                },
                body_id: None,
            });
        }

        // Then a BODY name. Only a body whose display name was SET (by
        // `body_rename` / `entity_name`, or inherited) can be named here,
        // which costs nothing: a derived name ("Extrude (2)") is not an
        // identifier and the grammar could not have carried it.
        let (feature_id, handle, body_id) =
            self.body_named(name)
                .ok_or_else(|| MeasureRefusal::Entity {
                    name: name.to_string(),
                    reason: "this document has no entity or body with that name".to_string(),
                })?;
        Ok(Resolved {
            entity: MeasureEntity::Solid(handle),
            id: None,
            owners: vec![feature_id],
            what: "a body",
            body_id: Some(body_id),
        })
    }

    /// The live body whose display-name override is `name`, with the feature
    /// that produced it and its body id.
    fn body_named(
        &self,
        name: &str,
    ) -> Option<(Uuid, waffle_types::kernel::KernelSolidHandle, String)> {
        self.bodies
            .iter()
            .find(|b| b.name == name)
            .map(|b| (b.feature, b.handle.clone(), b.body_id.clone()))
    }

    /// Record a resolved operand's owning feature, and apply the ordering
    /// rule: a measurement may only read geometry EARLIER in the tree than
    /// the expression it drives.
    ///
    /// This is where §6's cycle becomes a typed error instead of a hang.
    /// It is ordinal, so it holds before anything is measured and needs no
    /// fixpoint to discover: a feature's own depth measuring its own output
    /// is `owner_index == floor`, and measuring a LATER feature is
    /// `owner_index > floor` — both refused by the same comparison, because
    /// both would make the rebuild's answer depend on the order it happened
    /// to compute things in.
    ///
    /// An owner the kernel cannot name (a mesh-backed import has no
    /// persistent ids) is ALLOWED and counted in `unattributed`: refusing it
    /// would make every measurement of an imported body impossible, and the
    /// settle budget in `Engine::rebuild` is the loud backstop for the loop
    /// this check then cannot see.
    fn note(&self, name: &str, resolved: &Resolved) -> Result<(), MeasureRefusal> {
        if resolved.owners.is_empty() {
            return Ok(());
        }
        self.read
            .borrow_mut()
            .features
            .extend(resolved.owners.iter().copied());
        // The LATEST owner: the geometry is not available until every
        // feature it depends on has been built.
        let owner_index = resolved
            .owners
            .iter()
            .filter_map(|o| self.index_of.get(o).copied())
            .max();
        if let Some(at) = owner_index {
            self.high_water
                .set(Some(self.high_water.get().map_or(at, |h: usize| h.max(at))));
        }
        let (Some(floor), Some(at)) = (self.floor.get(), owner_index) else {
            return Ok(());
        };
        if at < floor {
            return Ok(());
        }
        Err(MeasureRefusal::Entity {
            name: name.to_string(),
            reason: if at == floor {
                format!(
                    "circular measurement: it belongs to {}, the very feature this \
                     expression drives, so the feature's own output would decide its own \
                     input",
                    self.describe(at)
                )
            } else {
                format!(
                    "circular measurement: it belongs to {}, which is built AFTER {} — the \
                     feature this expression drives; a measurement can only read geometry \
                     earlier in the tree",
                    self.describe(at),
                    self.describe(floor)
                )
            },
        })
    }

    /// One feature, as an author recognises it: its name and its ONE-BASED
    /// position. Both numbers in a refusal are counted the same way — a
    /// message that mixed a 1-based position with a 0-based one is a message
    /// that cannot be acted on.
    fn describe(&self, index: usize) -> String {
        match self.labels.get(index) {
            Some(name) => format!("\"{name}\" (#{} of the tree)", index + 1),
            None => format!("#{} of the tree", index + 1),
        }
    }

    /// `area(face)` — from the face's own signature, which N0 fills exactly
    /// for every analytic surface the arena holds. m² → mm².
    fn area(&self, name: &str, r: &Resolved) -> Result<f64, MeasureRefusal> {
        let Some((id, TopoKind::Face)) = r.id else {
            return Err(wrong_kind("area", name, r.what, "a face"));
        };
        self.introspect
            .compute_signature(id, TopoKind::Face)
            .area
            .map(|m2| m2 * MM_PER_M * MM_PER_M)
            .ok_or_else(|| MeasureRefusal::Entity {
                name: name.to_string(),
                reason: "the kernel reports no area for this face".to_string(),
            })
    }

    /// `radius(entity)` — the axis descriptor's radius, which every
    /// cylinder, cone, sphere, torus and circular edge carries (N0, Q6).
    /// m → mm.
    fn radius(&self, name: &str, r: &Resolved) -> Result<f64, MeasureRefusal> {
        let Some((id, kind)) = r.id else {
            return Err(wrong_kind(
                "radius",
                name,
                r.what,
                "a curved face or a circular edge",
            ));
        };
        self.introspect
            .entity_axis(id, kind)
            .and_then(|axis| axis.radius)
            .map(|m| m * MM_PER_M)
            .ok_or_else(|| MeasureRefusal::Entity {
                name: name.to_string(),
                reason: format!("{} has no radius (it is not a circular family)", r.what),
            })
    }

    /// `length(edge)` — the ARC length, not the chord between the endpoints
    /// (Q6: a full circle's chord is 0, and "how long is this rim" wants the
    /// arc). m → mm.
    fn length(&self, name: &str, r: &Resolved) -> Result<f64, MeasureRefusal> {
        let Some((id, TopoKind::Edge)) = r.id else {
            return Err(wrong_kind("length", name, r.what, "an edge"));
        };
        self.kernel
            .edge_length(id)
            .map(|l| l.value * MM_PER_M)
            .map_err(|e| MeasureRefusal::Entity {
                name: name.to_string(),
                reason: format!("the kernel refused the arc length: {e}"),
            })
    }

    /// `volume(body)` — the kernel's own integral over the B-Rep (Q3).
    /// m³ → mm³.
    fn volume(&self, name: &str, r: &Resolved) -> Result<f64, MeasureRefusal> {
        let MeasureEntity::Solid(handle) = &r.entity else {
            return Err(wrong_kind("volume", name, r.what, "a body"));
        };
        self.kernel
            .mass_properties(handle, None)
            .map(|m| m.volume * MM_PER_M * MM_PER_M * MM_PER_M)
            .map_err(|e| MeasureRefusal::Entity {
                name: name.to_string(),
                reason: format!("the kernel refused the volume: {e}"),
            })
    }

    /// `mass(body)` — the kernel's volume times the body's material's
    /// density (M1, `specs/drawings_and_mbd.md` §9). KILOGRAMS, which is the
    /// evaluator's working space for a mass.
    ///
    /// Three refusals, and none of them is a number. A body with NO material
    /// is refused by name: at `DEFAULT_DENSITY_KG_M3` the mass would be
    /// numerically the volume, which is the most plausible wrong answer this
    /// function could give. A DANGLING material is refused naming it. And a
    /// name that is not a body is refused like every other wrong kind.
    ///
    /// The density goes to the KERNEL rather than being multiplied in here,
    /// so one implementation scales the mass and the inertia tensor, and
    /// `mass(body)` and `measure_mass` cannot disagree.
    fn mass(&self, name: &str, r: &Resolved) -> Result<f64, MeasureRefusal> {
        let MeasureEntity::Solid(handle) = &r.entity else {
            return Err(wrong_kind("mass", name, r.what, "a body"));
        };
        let body_id = r.body_id.as_deref().ok_or_else(|| MeasureRefusal::Entity {
            name: name.to_string(),
            reason: "this body has no identity to read a material from".to_string(),
        })?;
        let density = match self.densities.get(body_id) {
            None => {
                return Err(MeasureRefusal::Entity {
                    name: name.to_string(),
                    reason: "this body has no material, so it has no mass; assign one \
                             (material_set) — at a default density a mass is just the \
                             volume wearing kilograms"
                        .to_string(),
                })
            }
            Some(Err(why)) => {
                return Err(MeasureRefusal::Entity {
                    name: name.to_string(),
                    reason: why.clone(),
                })
            }
            Some(Ok(rho)) => *rho,
        };
        self.kernel
            .mass_properties(handle, Some(density))
            .map(|m| m.mass)
            .map_err(|e| MeasureRefusal::Entity {
                name: name.to_string(),
                reason: format!("the kernel refused the mass properties: {e}"),
            })
    }

    /// `distance(a, b)` — the minimum distance, through `KernelMeasure`
    /// (Q1), which reports whether it is exact or mesh-bounded. m → mm.
    fn distance(&self, call: &MeasureCall<'_>) -> Result<f64, MeasureRefusal> {
        let a = self.resolve(call.name(0))?;
        let b = self.resolve(call.name(1))?;
        self.note(call.name(0), &a)?;
        self.note(call.name(1), &b)?;
        self.kernel
            .distance(&a.entity, &b.entity, &DistanceOpts::default())
            .map(|d| d.value * MM_PER_M)
            .map_err(|e| MeasureRefusal::Entity {
                // Blamed on the first operand: the pair is in the message,
                // and an `ExprError` carries one name.
                name: call.name(0).to_string(),
                reason: format!(
                    "the kernel refused the distance to \"{}\": {e}",
                    call.name(1)
                ),
            })
    }

    /// `angle(a, b)` — between two planar faces' normals, or two axes /
    /// straight edges' directions.
    ///
    /// Not a kernel method by design (§4.1: "angles are not a kernel
    /// method"), so it is computed here from the normals and axes that
    /// already exist. Degrees, which is the evaluator's angle working space,
    /// and in `[0, 180]`: the measured angle between two unoriented
    /// directions. A face whose normal is `None` (a full-turn surface of
    /// revolution, N0) is refused rather than given the axis as a normal —
    /// those are different quantities.
    fn angle(&self, call: &MeasureCall<'_>) -> Result<f64, MeasureRefusal> {
        let a = self.resolve(call.name(0))?;
        let b = self.resolve(call.name(1))?;
        self.note(call.name(0), &a)?;
        self.note(call.name(1), &b)?;
        let direction = |name: &str, r: &Resolved| -> Result<[f64; 3], MeasureRefusal> {
            let Some((id, kind)) = r.id else {
                return Err(wrong_kind(
                    "angle",
                    name,
                    r.what,
                    "a planar face, an axis or a straight edge",
                ));
            };
            if kind == TopoKind::Face {
                let sig = self.introspect.compute_signature(id, TopoKind::Face);
                if let Some(normal) = sig.normal {
                    return Ok(normal);
                }
            }
            // An edge (or a curved face) contributes its axis direction: a
            // circular edge's axis, a cylinder's axis.
            if let Some(axis) = self.introspect.entity_axis(id, kind) {
                return Ok(axis.direction);
            }
            // A STRAIGHT edge is §6's other angle family ("two planar faces
            // or two LINES") and has no axis descriptor, so its direction is
            // the segment itself. The kernel contract for `edge_polyline` is
            // "two points for a straight edge; for a curved edge, its chord
            // samples at the kernel's render density" — so exactly two
            // points IS the kernel saying this edge is a segment. A curved
            // edge still refuses rather than being handed its chord, which
            // points somewhere else entirely (and is zero for a closed
            // circle).
            if kind == TopoKind::Edge {
                if let [a, b] = self.introspect.edge_polyline(id).as_slice() {
                    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    if d.iter().any(|c| *c != 0.0) {
                        return Ok(d);
                    }
                }
            }
            Err(MeasureRefusal::Entity {
                name: name.to_string(),
                reason: format!(
                    "{} has no direction to measure an angle from: no normal, no axis, and \
                     it is not a straight segment",
                    r.what
                ),
            })
        };
        let u = direction(call.name(0), &a)?;
        let v = direction(call.name(1), &b)?;
        let dot = u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
        let norm = |w: [f64; 3]| (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt();
        let denominator = norm(u) * norm(v);
        if denominator <= 0.0 {
            return Err(MeasureRefusal::Entity {
                name: call.name(0).to_string(),
                reason: "one of the directions is degenerate (zero length)".to_string(),
            });
        }
        Ok((dot / denominator).clamp(-1.0, 1.0).acos().to_degrees())
    }
}

/// A name that resolved to the wrong kind of thing for this function.
fn wrong_kind(function: &str, name: &str, got: &str, want: &str) -> MeasureRefusal {
    MeasureRefusal::Entity {
        name: name.to_string(),
        reason: format!("{function}() measures {want}, and this name is {got}"),
    }
}

impl Measurer for TreeMeasurer<'_> {
    fn measure(&self, call: &MeasureCall<'_>) -> Result<f64, MeasureRefusal> {
        match call.function {
            "distance" => self.distance(call),
            "angle" => self.angle(call),
            one => {
                let name = call.name(0);
                let r = self.resolve(name)?;
                self.note(name, &r)?;
                match one {
                    "area" => self.area(name, &r),
                    "radius" => self.radius(name, &r),
                    "length" => self.length(name, &r),
                    "volume" => self.volume(name, &r),
                    "mass" => self.mass(name, &r),
                    // Every other name was rejected at parse time. Reported
                    // rather than `unreachable!`: this is a library.
                    other => Err(MeasureRefusal::Unavailable {
                        reason: format!("{other}() is not implemented by this measurer"),
                    }),
                }
            }
        }
    }
}
