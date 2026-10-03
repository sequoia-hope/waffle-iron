//! N2 of `specs/agent_mechanical_design.md` §5.3: every rung of the reference
//! ladder reports WHICH rung answered, every `BestEffort` rebind says what it
//! bound and why, and every refusal is classified.
//!
//! What a test at this layer pins that the unit tests inside `resolve.rs` do
//! not: the two facts N2 adds travel on the PUBLIC surface — `ResolvedRef::via`
//! for a caller that has to decide whether it got the identity it recorded, and
//! `EngineError::resolution_reason()` / `ErrorKind::ResolutionFailed`'s payload
//! for a host that has to branch without parsing prose (ICR-2).
//!
//! One test per rung and one per refusal, so a rung that stops reporting
//! itself fails on its own line instead of inside a shared assertion.
//!
//! Mutation-checked by withdrawing one rung at a time, 2026-10-03:
//!
//! - the lineage-root rung reporting `Pid` instead of `PidRoot` ⇒ 1 red,
//!   `the_lineage_root_rung_reports_itself_and_is_not_the_pid_rung`;
//! - `ResolvedVia::rebound()` hardcoded to `false` ⇒ 5 red, every
//!   `…_is_a_reported_rebind` plus the two named fallbacks;
//! - `refuse()` going back to an unclassified `ResolutionFailed` ⇒ 4 red, the
//!   three `…_is_classified_…` tests and the `ErrorKind` payload test.
//!
//! Nothing else in the file moved under any of the three, which is what makes
//! each line a pin on its own rung rather than on the ladder as a whole.

use std::collections::HashMap;

use feature_engine::resolve::{
    resolve_by_position, resolve_geom_ref, resolve_geom_ref_live, resolve_with_fallback,
    ResolvedVia,
};
use feature_engine::types::{EngineError, ErrorKind, ResolutionReason};
use modeling_ops::{BodyOutput, Diagnostics, EntityRecord, OpResult, Provenance};
use uuid::Uuid;
use waffle_types::kernel::{EntityPid, KernelId, KernelIntrospect, KernelSolidHandle};
use waffle_types::{
    Anchor, Filter, GeomRef, OutputKey, RefScope, ResolvePolicy, Role, Selector, TopoKind,
    TopoQuery, TopoSignature,
};

// ── Fixture ──────────────────────────────────────────────────────────────────

/// A kernel whose entity tables the test writes directly, so a pid, a lineage
/// root and a live signature can each be set to the exact shape one rung of
/// the ladder needs.
struct Stub {
    /// `(kernel id, pid, root pid)` for entities of kind `kind`.
    pids: Vec<(KernelId, u64, u64)>,
    kind: TopoKind,
    /// Live per-entity signatures, for the body-wide `Query`/`Signature` path.
    live: Vec<(KernelId, TopoSignature)>,
    /// Per-entity positions, for the `Position` path.
    verts: Vec<(KernelId, [f64; 3])>,
}

impl Default for Stub {
    fn default() -> Self {
        Self {
            pids: Vec::new(),
            kind: TopoKind::Face,
            live: Vec::new(),
            verts: Vec::new(),
        }
    }
}

impl KernelIntrospect for Stub {
    fn all_entity_pids(&self, _: &KernelSolidHandle, kind: TopoKind) -> Vec<(KernelId, EntityPid)> {
        if kind != self.kind {
            return Vec::new();
        }
        self.pids
            .iter()
            .map(|&(id, pid, root_pid)| (id, EntityPid { pid, root_pid }))
            .collect()
    }
    fn list_faces(&self, _: &KernelSolidHandle) -> Vec<KernelId> {
        self.live.iter().map(|(id, _)| *id).collect()
    }
    fn list_edges(&self, _: &KernelSolidHandle) -> Vec<KernelId> {
        Vec::new()
    }
    fn list_vertices(&self, _: &KernelSolidHandle) -> Vec<KernelId> {
        self.verts.iter().map(|(id, _)| *id).collect()
    }
    fn face_edges(&self, _: KernelId) -> Vec<KernelId> {
        Vec::new()
    }
    fn edge_faces(&self, _: KernelId) -> Vec<KernelId> {
        Vec::new()
    }
    fn edge_vertices(&self, _: KernelId) -> (KernelId, KernelId) {
        (KernelId(0), KernelId(0))
    }
    fn face_neighbors(&self, _: KernelId) -> Vec<KernelId> {
        Vec::new()
    }
    fn compute_signature(&self, id: KernelId, kind: TopoKind) -> TopoSignature {
        if kind == TopoKind::Vertex {
            if let Some((_, p)) = self.verts.iter().find(|(v, _)| *v == id) {
                return TopoSignature {
                    centroid: Some(*p),
                    ..TopoSignature::empty()
                };
            }
        }
        self.live
            .iter()
            .find(|(f, _)| *f == id)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(TopoSignature::empty)
    }
    fn compute_all_signatures(
        &self,
        _: &KernelSolidHandle,
        kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)> {
        if kind != TopoKind::Face {
            return Vec::new();
        }
        self.live.clone()
    }
}

fn planar(id: u64, area: f64, centroid: [f64; 3], normal: [f64; 3]) -> (KernelId, TopoSignature) {
    (
        KernelId(id),
        TopoSignature {
            surface_type: Some("planar".to_string()),
            area: Some(area),
            centroid: Some(centroid),
            normal: Some(normal),
            bbox: None,
            adjacency_hash: None,
            length: None,
            axis: None,
        },
    )
}

/// An `OpResult` with one `Main` body and the given created entities.
fn op(created: Vec<EntityRecord>) -> OpResult {
    OpResult {
        outputs: vec![(
            OutputKey::Main,
            BodyOutput {
                handle: KernelSolidHandle::from_raw(1),
                mesh: None,
                edges: None,
            },
        )],
        provenance: Provenance {
            created,
            deleted: vec![],
            modified: vec![],
            role_assignments: vec![],
        },
        diagnostics: Diagnostics::default(),
    }
}

fn results(fid: Uuid, r: OpResult) -> HashMap<Uuid, OpResult> {
    let mut m = HashMap::new();
    m.insert(fid, r);
    m
}

fn face_record(id: u64, sig: TopoSignature) -> EntityRecord {
    EntityRecord {
        kernel_id: KernelId(id),
        kind: TopoKind::Face,
        signature: sig,
    }
}

fn geom_ref(fid: Uuid, kind: TopoKind, selector: Selector, policy: ResolvePolicy) -> GeomRef {
    GeomRef {
        kind,
        anchor: Anchor::FeatureOutput {
            feature_id: fid,
            output_key: OutputKey::Main,
        },
        selector,
        policy,
        scope: None,
    }
}

/// The classification of a refusal, or a panic naming what came instead.
fn reason(err: &EngineError) -> &ResolutionReason {
    err.resolution_reason()
        .unwrap_or_else(|| panic!("want a classified refusal, got {err:?}"))
}

// ── The rungs that answer with the recorded identity ─────────────────────────

#[test]
fn the_pid_rung_reports_itself() {
    let fid = Uuid::new_v4();
    let k = Stub {
        pids: vec![(KernelId(10), 7001, 7001), (KernelId(11), 7002, 5000)],
        ..Stub::default()
    };
    let r = geom_ref(
        fid,
        TopoKind::Face,
        Selector::Pid {
            pid: 7002,
            root_pid: 5000,
        },
        ResolvePolicy::Strict,
    );
    let got = resolve_geom_ref_live(&r, &results(fid, op(vec![])), &k).expect("the pid is there");
    assert_eq!(got.kernel_id, KernelId(11));
    assert_eq!(got.via, ResolvedVia::Pid);
    assert!(!got.via.rebound(), "the recorded id answered");
    assert!(got.warnings.is_empty(), "{:?}", got.warnings);
}

/// The second rung, and the one N1 could not report: the pid is gone and the
/// entity still descending from its recorded lineage root answered. `via` says
/// `pid_root`, so a caller no longer has to infer it from the presence of a
/// warning (N1's last open item).
#[test]
fn the_lineage_root_rung_reports_itself_and_is_not_the_pid_rung() {
    let fid = Uuid::new_v4();
    let k = Stub {
        // pid 7002 is gone; 9100 carries its root.
        pids: vec![(KernelId(11), 9100, 5000)],
        ..Stub::default()
    };
    let r = geom_ref(
        fid,
        TopoKind::Face,
        Selector::Pid {
            pid: 7002,
            root_pid: 5000,
        },
        ResolvePolicy::Strict,
    );
    let got = resolve_geom_ref_live(&r, &results(fid, op(vec![])), &k).expect("the root answers");
    assert_eq!(got.kernel_id, KernelId(11));
    assert_eq!(got.via, ResolvedVia::PidRoot);
    assert!(
        !got.via.rebound(),
        "the recorded LINEAGE answered — not a geometric rebind"
    );
    assert!(
        got.warnings.iter().any(|w| w.contains("lineage root")),
        "and it still says so in words: {:?}",
        got.warnings
    );
}

#[test]
fn the_role_rung_reports_itself() {
    let fid = Uuid::new_v4();
    let mut r = op(vec![face_record(3, TopoSignature::empty())]);
    r.provenance.role_assignments = vec![(KernelId(3), Role::EndCapPositive)];
    let got = resolve_geom_ref(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Role {
                role: Role::EndCapPositive,
                index: 0,
            },
            ResolvePolicy::Strict,
        ),
        &results(fid, r),
    )
    .expect("the role is assigned");
    assert_eq!(got.kernel_id, KernelId(3));
    assert_eq!(got.via, ResolvedVia::Role);
    assert!(!got.via.rebound());
}

#[test]
fn the_signature_rung_reports_itself() {
    let fid = Uuid::new_v4();
    let (_, want) = planar(2, 10.0, [0.0, 0.0, 5.0], [0.0, 0.0, -1.0]);
    let r = op(vec![
        face_record(1, planar(1, 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]).1),
        face_record(2, want.clone()),
    ]);
    let got = resolve_geom_ref(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Signature { signature: want },
            ResolvePolicy::Strict,
        ),
        &results(fid, r),
    )
    .expect("the fingerprint fits one face");
    assert_eq!(got.kernel_id, KernelId(2));
    assert_eq!(got.via, ResolvedVia::Signature);
    assert!(!got.via.rebound());
}

#[test]
fn the_query_rung_reports_itself() {
    let fid = Uuid::new_v4();
    let k = Stub {
        live: vec![
            planar(1, 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
            planar(2, 99.0, [0.0, 0.0, 5.0], [0.0, 0.0, -1.0]),
        ],
        ..Stub::default()
    };
    let got = resolve_geom_ref_live(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Query {
                query: TopoQuery {
                    filters: vec![Filter::AreaRange {
                        min: 50.0,
                        max: 150.0,
                    }],
                    tie_break: None,
                },
            },
            ResolvePolicy::Strict,
        ),
        &results(fid, op(vec![])),
        &k,
    )
    .expect("one face is in the area range");
    assert_eq!(got.kernel_id, KernelId(2));
    assert_eq!(got.via, ResolvedVia::Query);
    assert!(!got.via.rebound());
}

#[test]
fn the_position_rung_reports_itself() {
    let fid = Uuid::new_v4();
    let k = Stub {
        verts: vec![(KernelId(4), [1.0, 2.0, 3.0])],
        ..Stub::default()
    };
    let got = resolve_by_position(
        &geom_ref(
            fid,
            TopoKind::Vertex,
            Selector::Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            ResolvePolicy::Strict,
        ),
        &results(fid, op(vec![])),
        &k,
        [1.0, 2.0, 3.0],
    )
    .expect("the pick is on the vertex");
    assert_eq!(got.kernel_id, KernelId(4));
    assert_eq!(got.via, ResolvedVia::Position);
    assert!(!got.via.rebound());
}

// ── The four BestEffort rebinds: each says what it bound, and that it is one ──

#[test]
fn a_clamped_role_index_is_a_reported_rebind() {
    let fid = Uuid::new_v4();
    let mut r = op(vec![]);
    r.provenance.role_assignments = vec![
        (KernelId(3), Role::EndCapPositive),
        (KernelId(4), Role::EndCapPositive),
    ];
    let got = resolve_geom_ref(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Role {
                role: Role::EndCapPositive,
                index: 7,
            },
            ResolvePolicy::BestEffort,
        ),
        &results(fid, r),
    )
    .expect("BestEffort clamps");
    assert_eq!(got.kernel_id, KernelId(4));
    assert_eq!(got.via, ResolvedVia::RoleClamped);
    assert!(got.via.rebound());
    let w = got.warnings.join(" | ");
    assert!(w.contains("CLAMPED"), "what it did: {w}");
    assert!(
        w.contains("not the one the reference recorded"),
        "and why that matters: {w}"
    );
}

#[test]
fn a_sub_floor_fingerprint_is_a_reported_rebind() {
    let fid = Uuid::new_v4();
    // One candidate, nothing like the target: scores below the 50% floor.
    let r = op(vec![face_record(
        1,
        planar(1, 1.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]).1,
    )]);
    let target = planar(9, 900.0, [50.0, 60.0, 70.0], [1.0, 0.0, 0.0]).1;
    let got = resolve_geom_ref(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Signature { signature: target },
            ResolvePolicy::BestEffort,
        ),
        &results(fid, r),
    )
    .expect("BestEffort binds below the floor");
    assert_eq!(got.via, ResolvedVia::SignatureLowConfidence);
    assert!(got.via.rebound());
    let w = got.warnings.join(" | ");
    assert!(w.contains("below the 50% floor"), "what it did: {w}");
    assert!(
        w.contains("not the entity the reference recorded"),
        "and why that matters: {w}"
    );
}

#[test]
fn a_query_that_matches_nothing_is_a_reported_rebind() {
    let fid = Uuid::new_v4();
    let k = Stub {
        live: vec![planar(1, 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0])],
        ..Stub::default()
    };
    let got = resolve_geom_ref_live(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Query {
                query: TopoQuery {
                    filters: vec![Filter::AreaRange {
                        min: 1000.0,
                        max: 2000.0,
                    }],
                    tie_break: None,
                },
            },
            ResolvePolicy::BestEffort,
        ),
        &results(fid, op(vec![])),
        &k,
    )
    .expect("BestEffort takes the first of the kind");
    assert_eq!(got.kernel_id, KernelId(1));
    assert_eq!(got.via, ResolvedVia::QueryFirstOfKind);
    assert!(got.via.rebound());
    let w = got.warnings.join(" | ");
    assert!(w.contains("matched none"), "what it did: {w}");
    assert!(w.contains("rebind by listing order"), "and why: {w}");
}

#[test]
fn a_role_that_is_gone_falls_back_to_kind_and_reports_it() {
    let fid = Uuid::new_v4();
    // No role assignments at all, but one created face of the right kind.
    let r = op(vec![face_record(5, TopoSignature::empty())]);
    let got = resolve_with_fallback(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Role {
                role: Role::EndCapPositive,
                index: 0,
            },
            ResolvePolicy::BestEffort,
        ),
        &results(fid, r),
    )
    .expect("BestEffort falls back to the kind");
    assert_eq!(got.kernel_id, KernelId(5));
    assert_eq!(got.via, ResolvedVia::KindFallback);
    assert!(got.via.rebound());
    let w = got.warnings.join(" | ");
    assert!(w.contains("bound the FIRST"), "what it did: {w}");
    assert!(
        w.contains("not the entity the reference recorded"),
        "and why: {w}"
    );
}

#[test]
fn a_moved_pick_binds_the_nearest_and_reports_it() {
    let fid = Uuid::new_v4();
    let k = Stub {
        verts: vec![(KernelId(4), [1.0, 2.0, 3.5])],
        ..Stub::default()
    };
    let got = resolve_by_position(
        &geom_ref(
            fid,
            TopoKind::Vertex,
            Selector::Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            ResolvePolicy::BestEffort,
        ),
        &results(fid, op(vec![])),
        &k,
        [1.0, 2.0, 3.0],
    )
    .expect("BestEffort takes the nearest");
    assert_eq!(got.via, ResolvedVia::PositionNearest);
    assert!(got.via.rebound());
    let w = got.warnings.join(" | ");
    assert!(w.contains("NEAREST"), "what it did: {w}");
    assert!(w.contains("this is a rebind"), "and why: {w}");
}

/// The counterpart of the four above, and the reason they are safe: under
/// `Strict` not one of them rebinds. An agent's reference carries `Strict`
/// (§5.3 item 1), so this is the behaviour every agent-authored reference gets.
#[test]
fn strict_refuses_every_rebind_the_four_above_take() {
    let fid = Uuid::new_v4();

    let mut roles = op(vec![]);
    roles.provenance.role_assignments = vec![(KernelId(3), Role::EndCapPositive)];
    let err = resolve_geom_ref(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Role {
                role: Role::EndCapPositive,
                index: 7,
            },
            ResolvePolicy::Strict,
        ),
        &results(fid, roles),
    )
    .expect_err("Strict does not clamp");
    assert_eq!(reason(&err), &ResolutionReason::NoMatch);

    let sig = op(vec![face_record(
        1,
        planar(1, 1.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0]).1,
    )]);
    let err = resolve_geom_ref(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Signature {
                signature: planar(9, 900.0, [50.0, 60.0, 70.0], [1.0, 0.0, 0.0]).1,
            },
            ResolvePolicy::Strict,
        ),
        &results(fid, sig),
    )
    .expect_err("Strict does not bind below the floor");
    assert_eq!(reason(&err), &ResolutionReason::NoMatch);

    let k = Stub {
        live: vec![planar(1, 10.0, [0.0, 0.0, 0.0], [0.0, 0.0, 1.0])],
        ..Stub::default()
    };
    let err = resolve_geom_ref_live(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Query {
                query: TopoQuery {
                    filters: vec![Filter::AreaRange {
                        min: 1000.0,
                        max: 2000.0,
                    }],
                    tie_break: None,
                },
            },
            ResolvePolicy::Strict,
        ),
        &results(fid, op(vec![])),
        &k,
    )
    .expect_err("Strict does not take the first of the kind");
    assert_eq!(reason(&err), &ResolutionReason::NoMatch);

    let kind_only = op(vec![face_record(5, TopoSignature::empty())]);
    assert!(
        resolve_with_fallback(
            &geom_ref(
                fid,
                TopoKind::Face,
                Selector::Role {
                    role: Role::EndCapPositive,
                    index: 0,
                },
                ResolvePolicy::Strict,
            ),
            &results(fid, kind_only),
        )
        .is_err(),
        "Strict does not fall back to the kind"
    );
}

// ── Each refusal is classified ───────────────────────────────────────────────

#[test]
fn a_vanished_pid_is_classified_pid_gone_with_its_numbers() {
    let fid = Uuid::new_v4();
    let k = Stub {
        pids: vec![(KernelId(11), 9100, 9100)],
        ..Stub::default()
    };
    let err = resolve_geom_ref_live(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Pid {
                pid: 7002,
                root_pid: 5000,
            },
            ResolvePolicy::BestEffort,
        ),
        &results(fid, op(vec![])),
        &k,
    )
    .expect_err("neither the id nor its root survives");
    assert_eq!(
        reason(&err),
        &ResolutionReason::PidGone {
            pid: 7002,
            root_pid: 5000,
            last_seen_feature: Some(fid),
        }
    );
}

#[test]
fn a_split_lineage_root_is_classified_ambiguous_and_names_the_halves() {
    let fid = Uuid::new_v4();
    let k = Stub {
        pids: vec![(KernelId(11), 9100, 5000), (KernelId(12), 9101, 5000)],
        ..Stub::default()
    };
    let err = resolve_geom_ref_live(
        &geom_ref(
            fid,
            TopoKind::Face,
            Selector::Pid {
                pid: 7002,
                root_pid: 5000,
            },
            ResolvePolicy::BestEffort,
        ),
        &results(fid, op(vec![])),
        &k,
    )
    .expect_err("a split root names no single entity");
    assert_eq!(
        reason(&err),
        &ResolutionReason::Ambiguous {
            candidates: vec![11, 12],
        }
    );
}

#[test]
fn a_scoped_reference_with_no_context_is_classified_scope_missing() {
    let fid = Uuid::new_v4();
    let mut r = geom_ref(
        fid,
        TopoKind::Face,
        Selector::Pid {
            pid: 1,
            root_pid: 1,
        },
        ResolvePolicy::BestEffort,
    );
    r.scope = Some(RefScope::in_assembly("tab-1", vec![Uuid::new_v4()]));
    let err = resolve_geom_ref_live(&r, &results(fid, op(vec![])), &Stub::default())
        .expect_err("a scoped reference resolves only through the open context");
    match reason(&err) {
        ResolutionReason::ScopeMissing { scope } => {
            assert!(!scope.is_empty(), "the scope is named: {scope}");
        }
        other => panic!("want ScopeMissing, got {other:?}"),
    }
}

// ── The payload reaches a host through ErrorKind ──────────────────────────────

/// §5.3 item 2: the refusal is routed to `ModelDelta.errors[].kind` as
/// `{"type": "ResolutionFailed", …}` with the reason, the reference and the
/// name beside it — so a host branches on fields, never on the message.
#[test]
fn the_refusal_reaches_a_host_as_a_typed_error_kind_with_its_payload() {
    let fid = Uuid::new_v4();
    let k = Stub {
        pids: vec![(KernelId(11), 9100, 9100)],
        ..Stub::default()
    };
    let r = geom_ref(
        fid,
        TopoKind::Face,
        Selector::Pid {
            pid: 7002,
            root_pid: 5000,
        },
        ResolvePolicy::Strict,
    );
    let err = resolve_geom_ref_live(&r, &results(fid, op(vec![])), &k).expect_err("gone");
    let kind = ErrorKind::from(&err);
    let ErrorKind::ResolutionFailed {
        reason,
        reference,
        name,
    } = kind
    else {
        panic!("want ResolutionFailed, got {kind:?}");
    };
    assert!(matches!(reason, Some(ResolutionReason::PidGone { .. })));
    let reference = reference.expect("the reference that refused is named");
    assert_eq!(reference.kind, TopoKind::Face);
    assert_eq!(reference.anchor_feature, Some(fid));
    assert_eq!(reference.selector, "Pid");
    assert_eq!(name, None, "this reference carries no entity name");

    // And the tag is the one it has always been, so nothing branching on
    // `ResolutionFailed` stops recognising it.
    let json = serde_json::to_value(ErrorKind::from(&err)).expect("serializes");
    assert_eq!(json["type"], "ResolutionFailed");
    assert_eq!(json["reason"]["type"], "PidGone");
    assert_eq!(json["reason"]["pid"], 7002);
}

/// An unclassified `ResolutionFailed` — a resolution step that is not one
/// reference lookup (an empty boolean target list, a datum plane that is not in
/// the tree) — keeps the same tag with the payload absent. A host therefore
/// reads `reason == null` as "no reference to re-author", not as a missing
/// field it has to guess at.
#[test]
fn an_unclassified_resolution_failure_carries_an_absent_payload() {
    let err = EngineError::ResolutionFailed {
        reason: "Datum plane 1234 not found".to_string(),
    };
    let json = serde_json::to_value(ErrorKind::from(&err)).expect("serializes");
    assert_eq!(json["type"], "ResolutionFailed");
    assert_eq!(json.get("reason"), None, "omitted, not null: {json}");
    assert_eq!(json.get("reference"), None);
    assert_eq!(
        err.resolution_text(),
        Some("Datum plane 1234 not found"),
        "and the text is still readable through one accessor"
    );
}
