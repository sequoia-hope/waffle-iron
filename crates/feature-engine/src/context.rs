//! In-context editing (v4 Phase 3d-4, `specs/waffle_v4_document_model.md`
//! §2.8): a Part edited *in the context of an assembly* may reference the
//! geometry of the assembly's OTHER instances — a sketch drawn on another
//! part's face, a point projected from another part's vertex, an extrude up
//! to another part's face. Such a reference is an ordinary [`GeomRef`] whose
//! `scope` names the assembly tab and the instance path of the part that owns
//! the anchor feature.
//!
//! The engine resolves scoped references through an [`EditContext`]: a
//! runtime-only snapshot, taken when the part is opened in context (and again
//! on an explicit "update context"), of every other instance's built feature
//! results together with its placement RELATIVE to the edited instance. The
//! snapshot carries kernel handles, not meshes; the handles stay valid because
//! the kernel arena is shared with the live part and is never cleared by a
//! rebuild. Geometry comes back expressed in the edited part's own frame, so
//! everything downstream (sketch planes, profiles, extrude directions) stays
//! in the coordinates the part is modelled in.
//!
//! Without a context (the part opened on its own), a scoped reference is
//! LOUD and inert: the sketch keeps its last derived plane and a warning names
//! the assembly and instance it depends on (`apply_context`); a projected point
//! stays where it was; an up-to depth fails the feature. Nothing is silently
//! resolved against the wrong part (`crate::resolve` refuses scoped refs).

use std::collections::HashMap;

use modeling_ops::{BodyOutput, OpResult};
use uuid::Uuid;
use waffle_types::kernel::KernelIntrospect;
use waffle_types::{GeomRef, ProjectedSource, RefScope};

use crate::assembly::Transform;
use crate::rebuild::{resolve_face_plane, resolve_projected_point};
use crate::resolve::resolve_with_fallback;
use crate::types::{EngineError, FeatureTree, Operation};

/// One OTHER instance of the context assembly, as the edited part sees it.
#[derive(Debug, Clone)]
pub struct ContextInstance {
    /// Chain of instance ids from the assembly down to this part instance.
    pub path: Vec<Uuid>,
    /// Display name (the top-level instance's name, ` › …` for a member).
    pub name: String,
    /// The tab the instance is of (a Part tab id; `part_source_id` when it
    /// lives in a linked document).
    pub part_tab_id: String,
    pub part_source_id: Option<Uuid>,
    /// `inv(P_edited) ∘ P_this`: maps this part's own coordinates into the
    /// edited part's frame.
    pub relative: Transform,
    /// The part's built feature results (handles and provenance; meshes are
    /// stripped — the renderer reads them from the assembly view).
    pub feature_results: HashMap<Uuid, OpResult>,
}

impl ContextInstance {
    /// Snapshot `feature_results` without their meshes and edge data.
    pub fn new(
        path: Vec<Uuid>,
        name: impl Into<String>,
        part_tab_id: impl Into<String>,
        part_source_id: Option<Uuid>,
        relative: Transform,
        feature_results: &HashMap<Uuid, OpResult>,
    ) -> Self {
        let stripped = feature_results
            .iter()
            .map(|(id, r)| {
                (
                    *id,
                    OpResult {
                        outputs: r
                            .outputs
                            .iter()
                            .map(|(k, b)| {
                                (
                                    k.clone(),
                                    BodyOutput {
                                        handle: b.handle.clone(),
                                        mesh: None,
                                        edges: None,
                                    },
                                )
                            })
                            .collect(),
                        provenance: r.provenance.clone(),
                        diagnostics: modeling_ops::Diagnostics::default(),
                    },
                )
            })
            .collect();
        Self {
            path,
            name: name.into(),
            part_tab_id: part_tab_id.into(),
            part_source_id,
            relative,
            feature_results: stripped,
        }
    }
}

/// The assembly context a Part is being edited in.
#[derive(Debug, Clone)]
pub struct EditContext {
    /// The assembly tab the context was taken from (this document).
    pub assembly_tab_id: String,
    /// The edited instance's path in that assembly.
    pub instance_path: Vec<Uuid>,
    /// World placement of the edited instance when the snapshot was taken.
    pub placement: Transform,
    /// Every other rendered part instance.
    pub instances: Vec<ContextInstance>,
}

/// A scoped face resolved into the edited part's frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextPlane {
    pub origin: [f64; 3],
    pub normal: [f64; 3],
}

impl EditContext {
    pub fn new(
        assembly_tab_id: impl Into<String>,
        instance_path: Vec<Uuid>,
        placement: Transform,
    ) -> Self {
        Self {
            assembly_tab_id: assembly_tab_id.into(),
            instance_path,
            placement,
            instances: Vec::new(),
        }
    }

    /// The instance a scope names, or why it cannot be served here.
    pub fn instance_for(&self, scope: &RefScope) -> Result<&ContextInstance, EngineError> {
        let fail = |reason: String| EngineError::ResolutionFailed { reason };
        if let Some(src) = scope.source_id {
            return Err(fail(format!(
                "reference into assembly of linked document {src}: cross-document contexts are not supported"
            )));
        }
        match &scope.tab_id {
            Some(tab) if *tab != self.assembly_tab_id => {
                return Err(fail(format!(
                    "reference is into assembly `{tab}` but the part is open in the context of `{}`",
                    self.assembly_tab_id
                )));
            }
            Some(_) => {}
            None => {
                return Err(fail(
                    "scoped reference names no assembly tab (`scope.tab_id`)".to_string(),
                ));
            }
        }
        if scope.instance_path.is_empty() {
            return Err(fail(
                "scoped reference has an empty instance path".to_string(),
            ));
        }
        if scope.instance_path == self.instance_path {
            return Err(fail(
                "scoped reference names the edited instance itself; a reference to this part's own geometry must be local"
                    .to_string(),
            ));
        }
        self.instances
            .iter()
            .find(|i| i.path == scope.instance_path)
            .ok_or_else(|| {
                fail(format!(
                    "instance {} is not in the open context of assembly `{}` (removed, suppressed, or not built)",
                    path_label(&scope.instance_path),
                    self.assembly_tab_id
                ))
            })
    }

    /// A scoped planar face → its plane in the edited part's frame. The origin
    /// is the face centroid (the same definition the UI uses when it starts a
    /// sketch on a context face, so re-deriving the plane on rebuild does not
    /// slide the sketch in-plane).
    pub fn face_plane(
        &self,
        geom_ref: &GeomRef,
        introspect: &dyn KernelIntrospect,
    ) -> Result<ContextPlane, EngineError> {
        let scope = scoped(geom_ref)?;
        let inst = self.instance_for(scope)?;
        let local = unscoped(geom_ref);
        let (origin, normal) = resolve_face_plane(&local, &inst.feature_results, introspect)?;
        Ok(ContextPlane {
            origin: inst.relative.apply(origin),
            normal: inst.relative.apply_dir(normal),
        })
    }

    /// A scoped projected source (vertex / edge sample) → its 3D point in the
    /// edited part's frame. `None` when the source cannot be resolved (the
    /// caller leaves the projected point where it was, as for local sources).
    pub fn projected_point(
        &self,
        source: &ProjectedSource,
        introspect: &dyn KernelIntrospect,
    ) -> Option<[f64; 3]> {
        let scope = source.geom_ref.scope.as_ref()?;
        let inst = self.instance_for(scope).ok()?;
        let local = ProjectedSource {
            geom_ref: unscoped(&source.geom_ref),
            kind: source.kind.clone(),
        };
        let p = resolve_projected_point(&local, &inst.feature_results, introspect)?;
        Some(inst.relative.apply(p))
    }

    /// A scoped entity's centroid in the edited part's frame (extrude up-to).
    pub fn centroid(
        &self,
        geom_ref: &GeomRef,
        introspect: &dyn KernelIntrospect,
    ) -> Result<[f64; 3], EngineError> {
        let scope = scoped(geom_ref)?;
        let inst = self.instance_for(scope)?;
        let local = unscoped(geom_ref);
        let resolved = resolve_with_fallback(&local, &inst.feature_results)?;
        let sig = introspect.compute_signature(resolved.kernel_id, geom_ref.kind);
        let c = sig.centroid.ok_or_else(|| EngineError::ResolutionFailed {
            reason: format!(
                "{:?} of instance {} has no centroid",
                geom_ref.kind,
                path_label(&scope.instance_path)
            ),
        })?;
        Ok(inst.relative.apply(c))
    }

    /// Human-readable target of a scope, for warnings.
    pub fn describe(&self, scope: &RefScope) -> String {
        describe_scope(
            scope,
            self.instances
                .iter()
                .find(|i| i.path == scope.instance_path),
        )
    }
}

/// The scope of a reference that must have one.
fn scoped(geom_ref: &GeomRef) -> Result<&RefScope, EngineError> {
    geom_ref
        .scope
        .as_ref()
        .ok_or_else(|| EngineError::ResolutionFailed {
            reason: "reference is local, not scoped; resolve it against the part".to_string(),
        })
}

/// The same reference without its scope, for resolution against the owning
/// part's own results.
pub fn unscoped(geom_ref: &GeomRef) -> GeomRef {
    GeomRef {
        scope: None,
        ..geom_ref.clone()
    }
}

fn path_label(path: &[Uuid]) -> String {
    path.iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(" › ")
}

/// "instance `name` (id…) of assembly `tab`" — with the name when the
/// instance is known.
pub fn describe_scope(scope: &RefScope, inst: Option<&ContextInstance>) -> String {
    let who = match inst {
        Some(i) => format!("instance `{}`", i.name),
        None => format!("instance {}", path_label(&scope.instance_path)),
    };
    match (&scope.source_id, &scope.tab_id) {
        (Some(src), Some(tab)) => format!("{who} of assembly `{tab}` in linked document {src}"),
        (Some(src), None) => format!("{who} of an assembly in linked document {src}"),
        (None, Some(tab)) => format!("{who} of assembly `{tab}`"),
        (None, None) => who,
    }
}

/// Outcome of the context pass over a tree (see [`apply_context`]).
#[derive(Debug, Default)]
pub struct ContextPassOutcome {
    /// Index of the earliest feature whose geometry changed, so an
    /// incremental rebuild widens to include it.
    pub first_changed: Option<usize>,
    /// Every sketch whose plane moved (the rebuild re-executes them and what
    /// depends on them).
    pub changed: Vec<Uuid>,
    pub warnings: Vec<String>,
    pub errors: Vec<(Uuid, String)>,
}

/// Plane agreement tolerance: below this a re-derived plane is "the same" and
/// the sketch is left untouched (no spurious rebuild widening).
const PLANE_TOL: f64 = 1e-9;

/// Re-derive every sketch plane that is a scoped reference from the open
/// context, mutating the tree's `plane_origin`/`plane_normal` (the fields the
/// rest of the rebuild reads). Without a context each such sketch keeps its
/// last plane and is reported once, by name, with what it depends on.
pub fn apply_context(
    tree: &mut FeatureTree,
    context: Option<&EditContext>,
    introspect: &dyn KernelIntrospect,
) -> ContextPassOutcome {
    let mut out = ContextPassOutcome::default();
    for (idx, feature) in tree.features.iter_mut().enumerate() {
        let name = feature.name.clone();
        let fid = feature.id;
        let Operation::Sketch { sketch } = &mut feature.operation else {
            continue;
        };
        let Some(scope) = sketch.plane.scope.clone() else {
            continue;
        };
        let Some(ctx) = context else {
            out.warnings.push(format!(
                "Sketch `{name}` is drawn on a face of {}; open the part in that assembly's context to update it (using its last known plane)",
                describe_scope(&scope, None)
            ));
            continue;
        };
        match ctx.face_plane(&sketch.plane, introspect) {
            Ok(plane) => {
                let moved = dist(plane.origin, sketch.plane_origin) > PLANE_TOL
                    || dist(plane.normal, sketch.plane_normal) > PLANE_TOL;
                if moved {
                    sketch.plane_origin = plane.origin;
                    sketch.plane_normal = plane.normal;
                    out.first_changed = Some(out.first_changed.map_or(idx, |c| c.min(idx)));
                    out.changed.push(fid);
                }
            }
            Err(e) => out.errors.push((
                fid,
                format!(
                    "Sketch `{name}`: its plane on {} could not be resolved ({e}); using its last known plane",
                    ctx.describe(&scope)
                ),
            )),
        }
    }
    out
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use waffle_types::{Anchor, OutputKey, ResolvePolicy, Role, Selector, TopoKind};

    fn face_ref(feature_id: Uuid, scope: Option<RefScope>) -> GeomRef {
        GeomRef {
            kind: TopoKind::Face,
            anchor: Anchor::FeatureOutput {
                feature_id,
                output_key: OutputKey::Main,
            },
            selector: Selector::Role {
                role: Role::EndCapPositive,
                index: 0,
            },
            policy: ResolvePolicy::BestEffort,
            scope,
        }
    }

    #[test]
    fn instance_lookup_is_loud_about_every_mismatch() {
        let me = Uuid::new_v4();
        let other = Uuid::new_v4();
        let mut ctx = EditContext::new("asm", vec![me], Transform::identity());
        ctx.instances.push(ContextInstance::new(
            vec![other],
            "Other 1",
            "part-b",
            None,
            Transform::translation([1.0, 0.0, 0.0]),
            &HashMap::new(),
        ));

        let ok = RefScope::in_assembly("asm", vec![other]);
        assert_eq!(ctx.instance_for(&ok).unwrap().name, "Other 1");

        let wrong_tab = RefScope::in_assembly("asm-2", vec![other]);
        let e = ctx.instance_for(&wrong_tab).unwrap_err().to_string();
        assert!(e.contains("`asm-2`") && e.contains("`asm`"), "{e}");

        let cross_doc = RefScope {
            source_id: Some(Uuid::new_v4()),
            ..RefScope::in_assembly("asm", vec![other])
        };
        assert!(ctx
            .instance_for(&cross_doc)
            .unwrap_err()
            .to_string()
            .contains("cross-document"));

        let no_tab = RefScope {
            tab_id: None,
            ..RefScope::in_assembly("asm", vec![other])
        };
        assert!(ctx
            .instance_for(&no_tab)
            .unwrap_err()
            .to_string()
            .contains("names no assembly tab"));

        let myself = RefScope::in_assembly("asm", vec![me]);
        assert!(ctx
            .instance_for(&myself)
            .unwrap_err()
            .to_string()
            .contains("edited instance itself"));

        let gone = RefScope::in_assembly("asm", vec![Uuid::new_v4()]);
        assert!(ctx
            .instance_for(&gone)
            .unwrap_err()
            .to_string()
            .contains("not in the open context"));

        let empty = RefScope::in_assembly("asm", vec![]);
        assert!(ctx
            .instance_for(&empty)
            .unwrap_err()
            .to_string()
            .contains("empty instance path"));
    }

    #[test]
    fn unscoped_strips_only_the_scope() {
        let fid = Uuid::new_v4();
        let r = face_ref(
            fid,
            Some(RefScope::in_assembly("asm", vec![Uuid::new_v4()])),
        );
        let u = unscoped(&r);
        assert!(u.scope.is_none());
        assert!(matches!(u.anchor, Anchor::FeatureOutput { feature_id, .. } if feature_id == fid));
        assert!(matches!(u.selector, Selector::Role { .. }));
    }

    #[test]
    fn describe_names_the_instance_when_known() {
        let other = Uuid::new_v4();
        let mut ctx = EditContext::new("asm", vec![Uuid::new_v4()], Transform::identity());
        ctx.instances.push(ContextInstance::new(
            vec![other],
            "Bracket 2",
            "part-b",
            None,
            Transform::identity(),
            &HashMap::new(),
        ));
        let s = ctx.describe(&RefScope::in_assembly("asm", vec![other]));
        assert_eq!(s, "instance `Bracket 2` of assembly `asm`");
        let s = describe_scope(&RefScope::in_assembly("asm", vec![other]), None);
        assert!(
            s.starts_with("instance ") && s.ends_with("of assembly `asm`"),
            "{s}"
        );
    }
}
