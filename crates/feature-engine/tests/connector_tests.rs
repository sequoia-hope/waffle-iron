//! `feature_engine::connector` — what a mate connector's frame is derived
//! from (`specs/assembly_connector_frame_resolver.md` §3).
//!
//! The fixture is a hand-built `KernelIntrospect` rather than `MockKernel`:
//! the mock builds planar boxes only, and the whole point of the resolver is
//! the geometry that ISN'T a planar face. Every number below is stated
//! directly by the fixture, so a failure names the resolver's rule, not a
//! kernel's construction.
//!
//! The model: a 6 × 4 × 2 mm plate with a Ø3 mm hole drilled through it along
//! +Z, plus a sphere face, a freeform face, and one rim between two curved
//! faces (to exercise the fallback sense).

use std::collections::HashMap;

use feature_engine::assembly::AxialAnchor;
use feature_engine::connector::{resolve_connector_frame, ConnectorGeometry};
use modeling_ops::{BodyOutput, Diagnostics, OpResult, Provenance};
use uuid::Uuid;
use waffle_types::kernel::{
    AxisKind, EntityAxis, KernelId, KernelIntrospect, KernelSolidHandle, TopoSignature,
};
use waffle_types::{Anchor, GeomRef, OutputKey, ResolvePolicy, Role, Selector, TopoKind};

// ── ids ────────────────────────────────────────────────────────────────────

const TOP_FACE: KernelId = KernelId(1);
const BOTTOM_FACE: KernelId = KernelId(2);
const HOLE_FACE: KernelId = KernelId(3);
const SPHERE_FACE: KernelId = KernelId(4);
const FREEFORM_FACE: KernelId = KernelId(5);
const CONE_FACE: KernelId = KernelId(6);

const TOP_RIM: KernelId = KernelId(10);
const BOTTOM_RIM: KernelId = KernelId(11);
const CURVED_JOIN_RIM: KernelId = KernelId(12);
const STRAIGHT_EDGE: KernelId = KernelId(13);
const FREEFORM_EDGE: KernelId = KernelId(14);

const HOLE_X: f64 = 0.003;
const HOLE_Y: f64 = 0.001;
const HOLE_R: f64 = 0.0015;
const PLATE_TOP: f64 = 0.002;

// ── fixture ────────────────────────────────────────────────────────────────

/// What the fixture states about a face: `(surface_type, normal, centroid)`.
type FaceSig = (&'static str, Option<[f64; 3]>, Option<[f64; 3]>);

/// A read-only introspection double: every query is a table lookup.
struct Fixture;

impl Fixture {
    fn face_sig(id: KernelId) -> Option<FaceSig> {
        match id {
            TOP_FACE => Some(("planar", Some([0.0, 0.0, 1.0]), Some([0.0, 0.0, PLATE_TOP]))),
            BOTTOM_FACE => Some(("planar", Some([0.0, 0.0, -1.0]), Some([0.0, 0.0, 0.0]))),
            HOLE_FACE => Some(("cylindrical", None, None)),
            SPHERE_FACE => Some(("spherical", None, None)),
            CONE_FACE => Some(("conical", None, None)),
            FREEFORM_FACE => Some(("nurbs", None, None)),
            _ => None,
        }
    }
}

impl KernelIntrospect for Fixture {
    fn list_faces(&self, _solid: &KernelSolidHandle) -> Vec<KernelId> {
        vec![TOP_FACE, BOTTOM_FACE, HOLE_FACE, SPHERE_FACE, FREEFORM_FACE]
    }
    fn list_edges(&self, _solid: &KernelSolidHandle) -> Vec<KernelId> {
        vec![TOP_RIM, BOTTOM_RIM, CURVED_JOIN_RIM, STRAIGHT_EDGE]
    }
    fn list_vertices(&self, _solid: &KernelSolidHandle) -> Vec<KernelId> {
        Vec::new()
    }

    fn face_edges(&self, face: KernelId) -> Vec<KernelId> {
        match face {
            // The drilled wall is bounded by its two rims — the boundary the
            // axial-extent rule measures.
            HOLE_FACE | CONE_FACE => vec![TOP_RIM, BOTTOM_RIM],
            _ => Vec::new(),
        }
    }

    fn edge_faces(&self, edge: KernelId) -> Vec<KernelId> {
        match edge {
            TOP_RIM => vec![TOP_FACE, HOLE_FACE],
            BOTTOM_RIM => vec![HOLE_FACE, BOTTOM_FACE],
            // Between two curved faces: no planar neighbour to take a sense
            // from.
            CURVED_JOIN_RIM => vec![HOLE_FACE, SPHERE_FACE],
            _ => Vec::new(),
        }
    }

    fn edge_vertices(&self, _edge: KernelId) -> (KernelId, KernelId) {
        (KernelId(0), KernelId(0))
    }

    fn edge_polyline(&self, edge: KernelId) -> Vec<[f64; 3]> {
        match edge {
            // Rims: sampled at render density, on the circle.
            TOP_RIM => rim_polyline(PLATE_TOP),
            BOTTOM_RIM => rim_polyline(0.0),
            CURVED_JOIN_RIM => rim_polyline(0.001),
            STRAIGHT_EDGE => vec![[0.0, 0.0, 0.0], [0.006, 0.0, 0.0]],
            // A curved edge with no analytic axis is sampled, never 2 points.
            FREEFORM_EDGE => vec![
                [0.0, 0.0, 0.0],
                [0.001, 0.0005, 0.0],
                [0.002, 0.0004, 0.0],
                [0.003, 0.0, 0.0],
            ],
            _ => Vec::new(),
        }
    }

    fn face_neighbors(&self, _face: KernelId) -> Vec<KernelId> {
        Vec::new()
    }

    fn compute_signature(&self, entity: KernelId, kind: TopoKind) -> TopoSignature {
        match kind {
            TopoKind::Face => match Fixture::face_sig(entity) {
                Some((st, normal, centroid)) => TopoSignature {
                    surface_type: Some(st.to_string()),
                    normal,
                    centroid,
                    ..TopoSignature::empty()
                },
                None => TopoSignature::empty(),
            },
            _ => TopoSignature::empty(),
        }
    }

    fn compute_all_signatures(
        &self,
        _solid: &KernelSolidHandle,
        _kind: TopoKind,
    ) -> Vec<(KernelId, TopoSignature)> {
        Vec::new()
    }

    fn entity_axis(&self, entity: KernelId, kind: TopoKind) -> Option<EntityAxis> {
        match (kind, entity) {
            // The arena's reference point is the base rim (z = 0), NOT the
            // middle of the face — the resolver must move it.
            (TopoKind::Face, HOLE_FACE) => Some(EntityAxis {
                kind: AxisKind::Cylindrical,
                origin: [HOLE_X, HOLE_Y, 0.0],
                direction: [0.0, 0.0, 1.0],
                radius: Some(HOLE_R),
            }),
            (TopoKind::Face, CONE_FACE) => Some(EntityAxis {
                kind: AxisKind::Conical,
                origin: [HOLE_X, HOLE_Y, -0.004], // the apex, below the plate
                direction: [0.0, 0.0, 1.0],
                radius: None,
            }),
            (TopoKind::Face, SPHERE_FACE) => Some(EntityAxis {
                kind: AxisKind::Spherical,
                origin: [0.01, 0.02, 0.03],
                direction: [0.0, 0.0, 1.0],
                radius: Some(0.005),
            }),
            // Both rims carry the traversal normal of whichever half-edge the
            // reference resolved to; here both happen to be −Z.
            (TopoKind::Edge, TOP_RIM) => Some(EntityAxis {
                kind: AxisKind::Circular,
                origin: [HOLE_X, HOLE_Y, PLATE_TOP],
                direction: [0.0, 0.0, -1.0],
                radius: Some(HOLE_R),
            }),
            (TopoKind::Edge, BOTTOM_RIM) => Some(EntityAxis {
                kind: AxisKind::Circular,
                origin: [HOLE_X, HOLE_Y, 0.0],
                direction: [0.0, 0.0, -1.0],
                radius: Some(HOLE_R),
            }),
            (TopoKind::Edge, CURVED_JOIN_RIM) => Some(EntityAxis {
                kind: AxisKind::Circular,
                origin: [HOLE_X, HOLE_Y, 0.001],
                direction: [0.0, 0.0, -1.0],
                radius: Some(HOLE_R),
            }),
            _ => None,
        }
    }
}

fn rim_polyline(z: f64) -> Vec<[f64; 3]> {
    (0..=8)
        .map(|i| {
            let t = i as f64 / 8.0 * std::f64::consts::TAU;
            [HOLE_X + HOLE_R * t.cos(), HOLE_Y + HOLE_R * t.sin(), z]
        })
        .collect()
}

// ── resolution scaffolding ─────────────────────────────────────────────────

/// One feature whose provenance gives every fixture entity a role, so a
/// `Selector::Role` reference resolves to exactly the id under test.
fn results() -> (Uuid, HashMap<Uuid, OpResult>) {
    let feature_id = Uuid::from_u128(0x1234);
    let ids = [
        TOP_FACE,
        BOTTOM_FACE,
        HOLE_FACE,
        SPHERE_FACE,
        FREEFORM_FACE,
        CONE_FACE,
        TOP_RIM,
        BOTTOM_RIM,
        CURVED_JOIN_RIM,
        STRAIGHT_EDGE,
        FREEFORM_EDGE,
    ];
    let provenance = Provenance {
        created: Vec::new(),
        deleted: Vec::new(),
        modified: Vec::new(),
        role_assignments: ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, Role::SideFace { index: i }))
            .collect(),
    };
    let result = OpResult {
        outputs: Vec::<(OutputKey, BodyOutput)>::new(),
        provenance,
        diagnostics: Diagnostics::default(),
    };
    (feature_id, HashMap::from([(feature_id, result)]))
}

/// A reference to one fixture entity, by the role index it was given above.
fn geom_ref(feature_id: Uuid, kind: TopoKind, id: KernelId) -> GeomRef {
    let index = [
        TOP_FACE,
        BOTTOM_FACE,
        HOLE_FACE,
        SPHERE_FACE,
        FREEFORM_FACE,
        CONE_FACE,
        TOP_RIM,
        BOTTOM_RIM,
        CURVED_JOIN_RIM,
        STRAIGHT_EDGE,
        FREEFORM_EDGE,
    ]
    .iter()
    .position(|x| *x == id)
    .expect("fixture entity");
    GeomRef {
        kind,
        anchor: Anchor::FeatureOutput {
            feature_id,
            output_key: OutputKey::Main,
        },
        selector: Selector::Role {
            role: Role::SideFace { index },
            index: 0,
        },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() < 1e-12)
}

// ── tests ──────────────────────────────────────────────────────────────────

#[test]
fn a_planar_face_keeps_its_phase_3_frame() {
    let (fid, res) = results();
    let (frame, kind) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Face, TOP_FACE),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect("a planar face resolves");
    assert_eq!(kind, ConnectorGeometry::PlanarFace);
    assert!(near(frame.origin, [0.0, 0.0, PLATE_TOP]), "face centroid");
    assert!(near(frame.z_axis, [0.0, 0.0, 1.0]), "outward normal");
}

/// The rule that makes a Revolute mate behave: the frame sits at the middle
/// of the FACE's axial extent (mid-depth of the drilled hole, z = 1 mm), not
/// at the surface's own reference point (the base rim, z = 0).
#[test]
fn a_cylindrical_face_gives_the_axis_at_the_middle_of_its_extent() {
    let (fid, res) = results();
    let (frame, kind) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Face, HOLE_FACE),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect("a cylindrical face resolves");
    assert_eq!(kind, ConnectorGeometry::AxialFace(AxisKind::Cylindrical));
    assert!(
        near(frame.origin, [HOLE_X, HOLE_Y, PLATE_TOP / 2.0]),
        "mid-depth on the axis, got {:?}",
        frame.origin
    );
    assert!(near(frame.z_axis, [0.0, 0.0, 1.0]), "the surface axis");
}

/// Same rule from a reference point far off the face: a cone's axis point is
/// its apex, 4 mm below the plate, and the frame still lands mid-extent.
#[test]
fn a_conical_face_measures_its_extent_from_the_apex() {
    let (fid, res) = results();
    let (frame, kind) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Face, CONE_FACE),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect("a conical face resolves");
    assert_eq!(kind, ConnectorGeometry::AxialFace(AxisKind::Conical));
    assert!(
        near(frame.origin, [HOLE_X, HOLE_Y, PLATE_TOP / 2.0]),
        "mid-extent, not the apex, got {:?}",
        frame.origin
    );
}

/// A sphere is isotropic: the centre IS the frame origin (what a `Ball` mate
/// wants), with no extent midpoint to compute.
#[test]
fn a_spherical_face_gives_its_centre() {
    let (fid, res) = results();
    let (frame, kind) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Face, SPHERE_FACE),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect("a spherical face resolves");
    assert_eq!(kind, ConnectorGeometry::SphericalFace);
    assert!(near(frame.origin, [0.01, 0.02, 0.03]), "the centre");
}

/// A rim takes its sense from its one planar neighbour, NOT from whichever
/// half-edge the reference happened to resolve to: the top rim of a hole
/// points out of the top face (+Z) though the fixture's curve normal is −Z,
/// and the bottom rim points out of the bottom face (−Z).
#[test]
fn a_rim_points_out_of_the_face_it_sits_on() {
    let (fid, res) = results();
    let (top, kind) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Edge, TOP_RIM),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect("a circular edge resolves");
    assert_eq!(kind, ConnectorGeometry::CircularEdge(AxisKind::Circular));
    assert!(
        near(top.origin, [HOLE_X, HOLE_Y, PLATE_TOP]),
        "the rim centre"
    );
    assert!(
        near(top.z_axis, [0.0, 0.0, 1.0]),
        "out of the TOP face, got {:?}",
        top.z_axis
    );

    let (bottom, _) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Edge, BOTTOM_RIM),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect("resolves");
    assert!(
        near(bottom.z_axis, [0.0, 0.0, -1.0]),
        "out of the BOTTOM face, got {:?}",
        bottom.z_axis
    );
}

/// With no planar neighbour there is nothing better than the curve's own
/// normal — the fallback stands rather than inventing a sense.
#[test]
fn a_rim_between_two_curved_faces_keeps_the_curves_normal() {
    let (fid, res) = results();
    let (frame, _) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Edge, CURVED_JOIN_RIM),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect("resolves");
    assert!(near(frame.z_axis, [0.0, 0.0, -1.0]), "the curve's normal");
}

#[test]
fn a_straight_edge_gives_its_midpoint_along_the_edge() {
    let (fid, res) = results();
    let (frame, kind) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Edge, STRAIGHT_EDGE),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect("a straight edge resolves");
    assert_eq!(kind, ConnectorGeometry::StraightEdge);
    assert!(near(frame.origin, [0.003, 0.0, 0.0]), "midpoint");
    assert!(near(frame.z_axis, [1.0, 0.0, 0.0]), "along the edge");
}

/// The refusals. Each is loud and names what was picked — a caller must not
/// substitute a default frame for any of them (that was the silent-wrong).
#[test]
fn geometry_with_no_derivable_frame_is_loud() {
    let (fid, res) = results();

    let err = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Face, FREEFORM_FACE),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect_err("a freeform face has no frame");
    let msg = err.to_string();
    assert!(msg.contains("nurbs"), "names the surface type: {msg}");

    let err = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Edge, FREEFORM_EDGE),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect_err("a curved edge with no axis has no frame");
    assert!(err.to_string().contains("curved"), "says why: {err}");

    let err = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Vertex, TOP_FACE),
        &res,
        &Fixture,
        AxialAnchor::Middle,
    )
    .expect_err("a vertex has no direction");
    assert!(err.to_string().contains("vertex"), "says why: {err}");

    // An unresolvable reference stays the resolver's error, not a frame.
    let mut dangling = geom_ref(fid, TopoKind::Face, TOP_FACE);
    dangling.anchor = Anchor::FeatureOutput {
        feature_id: Uuid::from_u128(0xdead),
        output_key: OutputKey::Main,
    };
    assert!(
        resolve_connector_frame(&dangling, &res, &Fixture, AxialAnchor::Middle).is_err(),
        "a dangling reference does not silently produce a frame"
    );
}

// ── anchors and adjustments (specs/assembly_connector_adjustments.md) ───────

use feature_engine::assembly::{Frame, MateConnector};
use serde_json::{json, Map};

/// A connector with explicit `frame` and adjustments, the way the app
/// stores one (no `geom_ref`).
fn adjusted_connector(
    frame: Frame,
    anchor: AxialAnchor,
    flip_z: bool,
    rotation_deg: f64,
    offset_m: [f64; 3],
) -> MateConnector {
    MateConnector {
        id: Uuid::from_u128(0xc0),
        name: "c".into(),
        instance_path: vec![Uuid::from_u128(0x1)],
        geom_ref: None,
        part_connector: None,
        frame,
        anchor,
        flip_z,
        rotation_deg,
        offset_m,
        extra: Map::new(),
    }
}

/// A connector may ask for a rim instead of the middle: the ends of the
/// FACE's extent along the derived axis (the hole spans z = 0 … 2 mm), with
/// the axis unchanged.
#[test]
fn an_axial_face_can_anchor_at_either_end_of_its_extent() {
    let (fid, res) = results();
    let hole = geom_ref(fid, TopoKind::Face, HOLE_FACE);
    let (hi, _) =
        resolve_connector_frame(&hole, &res, &Fixture, AxialAnchor::PositiveEnd).expect("resolves");
    assert!(
        near(hi.origin, [HOLE_X, HOLE_Y, PLATE_TOP]),
        "the end +z points toward (the top rim), got {:?}",
        hi.origin
    );
    let (lo, _) =
        resolve_connector_frame(&hole, &res, &Fixture, AxialAnchor::NegativeEnd).expect("resolves");
    assert!(
        near(lo.origin, [HOLE_X, HOLE_Y, 0.0]),
        "the end z points away from (the bottom rim), got {:?}",
        lo.origin
    );
    assert!(
        near(hi.z_axis, [0.0, 0.0, 1.0]) && near(lo.z_axis, [0.0, 0.0, 1.0]),
        "the axis is the same wherever the origin sits"
    );

    // A cone's ends are its rims, never its apex (4 mm below the plate).
    let cone = geom_ref(fid, TopoKind::Face, CONE_FACE);
    let (rim, _) =
        resolve_connector_frame(&cone, &res, &Fixture, AxialAnchor::NegativeEnd).expect("resolves");
    assert!(
        near(rim.origin, [HOLE_X, HOLE_Y, 0.0]),
        "the −z rim, not the apex, got {:?}",
        rim.origin
    );
}

/// Picks with no axial extent ignore the anchor: a sphere is its centre, a
/// planar face its centroid and a rim its centre, whatever is asked.
#[test]
fn the_anchor_is_ignored_where_there_is_no_axial_extent() {
    let (fid, res) = results();
    let (sphere, _) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Face, SPHERE_FACE),
        &res,
        &Fixture,
        AxialAnchor::PositiveEnd,
    )
    .expect("resolves");
    assert!(near(sphere.origin, [0.01, 0.02, 0.03]), "the centre");
    let (plane, _) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Face, TOP_FACE),
        &res,
        &Fixture,
        AxialAnchor::NegativeEnd,
    )
    .expect("resolves");
    assert!(near(plane.origin, [0.0, 0.0, PLATE_TOP]), "the centroid");
    let (rim, _) = resolve_connector_frame(
        &geom_ref(fid, TopoKind::Edge, TOP_RIM),
        &res,
        &Fixture,
        AxialAnchor::PositiveEnd,
    )
    .expect("resolves");
    assert!(
        near(rim.origin, [HOLE_X, HOLE_Y, PLATE_TOP]),
        "the rim centre"
    );
}

/// `flip_z` is a 180° turn about x: z and y reverse, x stays — and the
/// anchor's ends are named against the FINAL z, so with the flip "+z end"
/// resolves to the bottom rim, where the flipped z now points.
#[test]
fn flip_z_reverses_z_and_y_keeps_x_and_mirrors_the_anchor() {
    let base = Frame::on_plane([0.0, 0.0, PLATE_TOP], [0.0, 0.0, 1.0]);
    let c = adjusted_connector(base, AxialAnchor::Middle, true, 0.0, [0.0; 3]);
    let f = c.adjusted(base).expect("adjusts");
    let (x, y, z) = f.basis().unwrap();
    let (x0, y0, _) = base.basis().unwrap();
    assert!(near(z, [0.0, 0.0, -1.0]), "z reversed, got {z:?}");
    assert!(near(x, x0), "x kept, got {x:?} vs {x0:?}");
    assert!(
        near(y, [-y0[0], -y0[1], -y0[2]]),
        "y reversed with z (right-handed), got {y:?}"
    );
    assert!(
        near(f.origin, base.origin),
        "a flip does not move the origin"
    );
    assert_eq!(c.derivation_anchor(), AxialAnchor::Middle);

    let (fid, res) = results();
    let hole = geom_ref(fid, TopoKind::Face, HOLE_FACE);
    let c = adjusted_connector(
        Frame::default(),
        AxialAnchor::PositiveEnd,
        true,
        0.0,
        [0.0; 3],
    );
    assert_eq!(
        c.derivation_anchor(),
        AxialAnchor::NegativeEnd,
        "the end the FINAL z points toward is the derived axis's −z end"
    );
    let (derived, _) =
        resolve_connector_frame(&hole, &res, &Fixture, c.derivation_anchor()).expect("resolves");
    let f = c.adjusted(derived).expect("adjusts");
    assert!(
        near(f.origin, [HOLE_X, HOLE_Y, 0.0]),
        "the bottom rim: where the flipped z points, got {:?}",
        f.origin
    );
    assert!(near(f.z_axis, [0.0, 0.0, -1.0]), "z points down");
}

/// A turn about z moves the secondary axis only: 90° takes x onto the old
/// y, z and the origin stay.
#[test]
fn rotation_about_z_turns_the_secondary_axis() {
    let base = Frame::on_plane([0.001, 0.002, 0.003], [0.0, 0.0, 1.0]);
    let (x0, y0, _) = base.basis().unwrap();
    assert!(
        near(x0, [1.0, 0.0, 0.0]) && near(y0, [0.0, 1.0, 0.0]),
        "the fixture's basis"
    );
    let c = adjusted_connector(base, AxialAnchor::Middle, false, 90.0, [0.0; 3]);
    let f = c.adjusted(base).expect("adjusts");
    let (x, y, z) = f.basis().unwrap();
    assert!(near(x, [0.0, 1.0, 0.0]), "x onto the old y, got {x:?}");
    assert!(near(y, [-1.0, 0.0, 0.0]), "y onto −x, got {y:?}");
    assert!(near(z, [0.0, 0.0, 1.0]), "z unchanged");
    assert!(near(f.origin, base.origin), "the origin stays");
}

/// The offset is along the connector's OWN final axes, after the turn and
/// the flip: 1 mm along x after a 90° turn is 1 mm along the part's +Y, and
/// 1 mm along z on a flipped frame is 1 mm DOWN.
#[test]
fn offset_moves_along_the_final_axes() {
    let base = Frame::on_plane([0.001, 0.002, 0.003], [0.0, 0.0, 1.0]);

    let turned = adjusted_connector(base, AxialAnchor::Middle, false, 90.0, [0.001, 0.0, 0.0]);
    let f = turned.adjusted(base).expect("adjusts");
    assert!(
        near(f.origin, [0.001, 0.003, 0.003]),
        "along the TURNED x (the part's +Y), got {:?}",
        f.origin
    );

    let flipped = adjusted_connector(base, AxialAnchor::Middle, true, 0.0, [0.0, 0.0, 0.001]);
    let f = flipped.adjusted(base).expect("adjusts");
    assert!(
        near(f.origin, [0.001, 0.002, 0.002]),
        "along the FLIPPED z (down), got {:?}",
        f.origin
    );

    let plain = adjusted_connector(base, AxialAnchor::Middle, false, 0.0, [0.001, 0.002, 0.003]);
    let f = plain.adjusted(base).expect("adjusts");
    assert!(
        near(f.origin, [0.002, 0.004, 0.006]),
        "x, y, z of an unturned +Z frame are the part's axes, got {:?}",
        f.origin
    );
}

/// A degenerate z cannot carry a turn or an offset: loud, never a frame
/// silently left unadjusted. (Nothing to adjust passes the frame through —
/// the solver reports the degenerate z itself.)
#[test]
fn adjusting_a_degenerate_frame_is_loud() {
    let bad = Frame {
        origin: [0.0; 3],
        z_axis: [0.0; 3],
        x_axis: [0.0; 3],
    };
    assert!(
        adjusted_connector(bad, AxialAnchor::Middle, false, 0.0, [0.0, 0.0, 0.001])
            .adjusted(bad)
            .is_err()
    );
    assert!(
        adjusted_connector(bad, AxialAnchor::Middle, false, 10.0, [0.0; 3])
            .adjusted(bad)
            .is_err()
    );
    assert!(
        adjusted_connector(bad, AxialAnchor::Middle, false, 0.0, [0.0; 3])
            .adjusted(bad)
            .is_ok()
    );
}

/// The four adjustments are additive on the wire: at their defaults they are
/// omitted, so a connector without them serializes exactly as before they
/// existed; set, they round-trip; a document written before they existed
/// reads as the defaults.
#[test]
fn adjustments_are_additive_on_the_wire() {
    let plain = adjusted_connector(Frame::default(), AxialAnchor::Middle, false, 0.0, [0.0; 3]);
    let v = serde_json::to_value(&plain).unwrap();
    for key in ["anchor", "flip_z", "rotation_deg", "offset_m"] {
        assert!(v.get(key).is_none(), "`{key}` omitted at its default: {v}");
    }

    let set = adjusted_connector(
        Frame::default(),
        AxialAnchor::PositiveEnd,
        true,
        30.0,
        [0.001, 0.0, -0.002],
    );
    let v = serde_json::to_value(&set).unwrap();
    assert_eq!(v["anchor"], json!("positive_end"));
    assert_eq!(v["flip_z"], json!(true));
    assert_eq!(v["rotation_deg"], json!(30.0));
    assert_eq!(v["offset_m"], json!([0.001, 0.0, -0.002]));
    let back: MateConnector = serde_json::from_value(v).unwrap();
    assert_eq!(back.anchor, AxialAnchor::PositiveEnd);
    assert!(back.flip_z);
    assert_eq!(back.rotation_deg, 30.0);
    assert_eq!(back.offset_m, [0.001, 0.0, -0.002]);
    assert!(back.extra.is_empty(), "known keys are fields, not `extra`");

    let old: MateConnector = serde_json::from_value(json!({
        "id": "6f1c2a4e-1111-4222-8333-444455556666",
        "name": "top",
        "instance_path": ["6f1c2a4e-1111-4222-8333-444455556677"],
        "frame": { "origin": [0.0, 0.0, 0.01], "z_axis": [0.0, 0.0, 1.0] }
    }))
    .unwrap();
    assert_eq!(old.anchor, AxialAnchor::Middle);
    assert!(!old.flip_z);
    assert_eq!(old.rotation_deg, 0.0);
    assert_eq!(old.offset_m, [0.0; 3]);
}

// ── part mate connectors (specs/part_mate_connectors.md) ────────────────────

use feature_engine::connector::part_connector_frame;
use feature_engine::types::{MateConnectorParams, Operation};

/// A part connector's frame is derived exactly as an assembly connector's is,
/// and its own adjustments apply in the same order (flip, turn, offset along
/// the FINAL axes).
#[test]
fn a_part_connector_derives_like_an_assembly_connector_then_adjusts() {
    let (fid, res) = results();
    let params = MateConnectorParams {
        geom_ref: Some(geom_ref(fid, TopoKind::Face, TOP_FACE)),
        flip_z: true,
        offset_m: [0.0, 0.0, 0.001],
        ..Default::default()
    };
    let (frame, kind) = part_connector_frame(&params, &res, &Fixture).expect("resolves");
    assert_eq!(kind, Some(ConnectorGeometry::PlanarFace));
    assert!(near(frame.z_axis, [0.0, 0.0, -1.0]), "flipped: {frame:?}");
    assert!(
        near(frame.origin, [0.0, 0.0, PLATE_TOP - 0.001]),
        "1 mm along the flipped z, into the plate: {:?}",
        frame.origin
    );

    // An anchor names its end against the final z: flipped, "+z end" of the
    // hole is the bottom rim.
    let params = MateConnectorParams {
        geom_ref: Some(geom_ref(fid, TopoKind::Face, HOLE_FACE)),
        anchor: AxialAnchor::PositiveEnd,
        flip_z: true,
        ..Default::default()
    };
    let (frame, kind) = part_connector_frame(&params, &res, &Fixture).expect("resolves");
    assert_eq!(
        kind,
        Some(ConnectorGeometry::AxialFace(AxisKind::Cylindrical))
    );
    assert!(
        near(frame.origin, [HOLE_X, HOLE_Y, 0.0]),
        "{:?}",
        frame.origin
    );
    assert!(near(frame.z_axis, [0.0, 0.0, -1.0]));

    // No reference: the explicit frame, adjusted.
    let params = MateConnectorParams {
        frame: Frame::on_plane([0.001, 0.0, 0.0], [1.0, 0.0, 0.0]),
        offset_m: [0.0, 0.0, 0.002],
        ..Default::default()
    };
    let (frame, kind) = part_connector_frame(&params, &res, &Fixture).expect("explicit");
    assert_eq!(kind, None);
    assert!(near(frame.origin, [0.003, 0.0, 0.0]), "{:?}", frame.origin);
}

/// What cannot define a frame is loud — never the default frame in its place.
#[test]
fn a_part_connector_without_a_frame_is_loud() {
    let (fid, res) = results();
    let freeform = MateConnectorParams {
        geom_ref: Some(geom_ref(fid, TopoKind::Face, FREEFORM_FACE)),
        ..Default::default()
    };
    assert!(part_connector_frame(&freeform, &res, &Fixture).is_err());

    let degenerate = MateConnectorParams {
        frame: Frame {
            origin: [0.0; 3],
            z_axis: [0.0; 3],
            x_axis: [0.0; 3],
        },
        ..Default::default()
    };
    let err = part_connector_frame(&degenerate, &res, &Fixture).expect_err("zero z");
    assert!(err.to_string().contains("degenerate"), "{err}");
}

/// The operation's wire form: adjustments at their defaults are omitted, and
/// a minimal `{"name"}` parses.
#[test]
fn the_mate_connector_operation_is_additive_on_the_wire() {
    let op: Operation = serde_json::from_value(json!({
        "type": "MateConnector",
        "params": { "name": "Pin" }
    }))
    .expect("parses");
    let Operation::MateConnector { params } = &op else {
        panic!("{op:?}")
    };
    assert_eq!(params.name, "Pin");
    assert!(params.geom_ref.is_none() && !params.flip_z);
    let back = serde_json::to_value(&op).unwrap();
    let p = back["params"].as_object().unwrap();
    for key in ["geom_ref", "anchor", "flip_z", "rotation_deg", "offset_m"] {
        assert!(!p.contains_key(key), "{key} omitted at its default: {back}");
    }
    assert_eq!(op.type_tag(), "MateConnector");
}
