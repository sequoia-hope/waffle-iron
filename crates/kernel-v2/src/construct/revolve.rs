//! Revolve family — full/partial/on-axis/torus/sphere revolves and their
//! frame + validation helpers. Move-only split from the construct god-module
//! (design review 2026-07-12 F9); behavior byte-identical.

use super::*;

/// Entities produced by [`revolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevolveResult {
    /// The new solid.
    pub solid: SolidId,
    /// Its single shell.
    pub shell: ShellId,
    /// The profile face at sweep angle 0. For a partial revolve its outward
    /// normal opposes the sweep velocity; for the 360° branch it is the
    /// annular cap at the axial minimum (outward normal `−â`). `None` for a
    /// capless 360° ring (a non-alternating wall-only profile — e.g. the
    /// tilted-axis all-oblique rectangle — has no planar face to name; spec
    /// `kv6a_nonalternating_full_revolve.md`).
    pub start_cap: Option<FaceId>,
    /// The profile face at the sweep angle (partial) / the annular cap at
    /// the axial maximum (360°, outward normal `+â`). `None` exactly when
    /// `start_cap` is (the capless full-turn ring).
    pub end_cap: Option<FaceId>,
    /// Lateral faces, one per profile edge, in loop walk order: cylinder
    /// patches for axis-parallel edges, planar annular sectors for
    /// axis-perpendicular edges (partial); the outer + inner full cylinders
    /// (360°).
    pub walls: Vec<FaceId>,
}

/// `|â · n̂|` ceiling above which the revolve axis direction is rejected as
/// out of the profile plane, and the relative band (scaled by the geometry
/// magnitude) for the axis origin's distance to the plane. Absorbs only
/// unit-vector rounding — the assay generator emits exactly in-plane axes.
pub const REVOLVE_AXIS_IN_PLANE_TOLERANCE: f64 = 1e-9;

/// Per-edge alignment band: a profile edge is axis-parallel when its radial
/// extent is below `tol · length`, axis-perpendicular when its axial extent
/// is; anything in between is an oblique edge (a CONE — KV6c), rejected
/// typed. Corpus rectangles are exactly axis-aligned.
pub const REVOLVE_EDGE_ALIGNMENT_TOLERANCE: f64 = 1e-9;

/// Relative clearance the profile must keep from the axis (scaled by the
/// geometry magnitude). Touching or crossing the axis is invalid input
/// ([`KernelV2Error::RevolveAxisIntersectsProfile`]): crossing
/// self-intersects, touching pinches a non-manifold seam (the on-axis
/// solid-of-revolution is a later capability).
pub const REVOLVE_MIN_AXIS_CLEARANCE_REL: f64 = 1e-9;

/// `|α − 2π|` band inside which a revolve angle is the full-turn branch
/// (the washer topology); `α > 2π + band` is rejected. Absorbs only the
/// degrees→radians conversion rounding of an exact 360°.
pub const REVOLVE_FULL_TURN_TOLERANCE: f64 = 1e-9;

/// Per-edge classification of an axis-aligned profile edge.
#[derive(Debug, Clone, Copy)]
enum EdgeClass {
    /// Constant radius: sweeps a cylinder wall. `reversed` = material on
    /// the larger-radius side (the wall of an inner bore).
    Parallel { radius: f64, reversed: bool },
    /// Constant axial height: sweeps a planar annular sector (partial) or
    /// vanishes into an annulus bounded by its endpoint rims (full turn).
    /// `outward_plus_axis` = the face's outward normal is `+â`.
    Perpendicular { outward_plus_axis: bool },
    /// Both radius and axial height change: sweeps a CONE (frustum) band
    /// (KV6c). Topologically a wall, exactly like [`EdgeClass::Parallel`] —
    /// two rim circles (at the edge's two radii) plus two seams — only the
    /// surface differs. The cone parameters are derived from the slant in
    /// [`validate_revolve_geometry`]: `apex` is where the slant, extended,
    /// meets the axis; `axis_dir` is oriented so both rims have a positive
    /// axial coordinate (apex behind); `half_angle = atan|Δs/Δt|`;
    /// `reversed` = material on the larger-radius side (an inner bore),
    /// the same `dt > 0` rule as `Parallel`. Only the full-turn builder
    /// handles it; a partial revolve of an oblique edge sweeps an
    /// arc-bounded cone patch (KV6c increment 5) and is rejected typed.
    Oblique {
        apex: Point3,
        axis_dir: UnitVector3,
        half_angle: f64,
        reversed: bool,
    },
}

/// Validated revolve geometry, computed before any arena mutation.
struct RevolveFrame {
    /// Unit axis direction.
    a: UnitVector3,
    /// Unit in-plane radial direction; every profile vertex has a strictly
    /// positive radial coordinate along it.
    w: UnitVector3,
    /// `â × ŵ` — the sweep-velocity direction at θ = 0 (`±` the profile
    /// normal). The working loop is ordered so its Newell normal is `+m`.
    m: UnitVector3,
    /// Axis origin.
    a0: Point3,
    /// Working-order outer loop, embedded (Newell normal `+m`).
    ring0: Vec<Point3>,
    /// Per-vertex axial coordinate `(p − a0) · â`.
    t: Vec<f64>,
    /// Per-vertex radial coordinate `(p − a0) · ŵ` (all > clearance).
    s: Vec<f64>,
    /// Per-edge classification (edge `i` joins vertex `i` to `i + 1`).
    edges: Vec<EdgeClass>,
}

/// Revolve a validated polygon [`Profile`] about an in-plane axis by
/// `angle_rad ∈ (0, 2π]` radians (PR-KV6a). See `tests/kv6a_revolve.rs`
/// for the pinned contract: geometry, topology census, Pappus volume,
/// rejection semantics. Like [`extrude_circle`], both branches are direct
/// assemblers (arcs / closed rims are outside the Euler-operator
/// vocabulary); the safety obligation is discharged by `validate_solid`
/// at exit.
pub fn revolve(
    arena: &mut BrepArena,
    profile: &Profile,
    axis_origin: Point3,
    axis_direction: Vector3,
    angle_rad: f64,
) -> Result<RevolveResult, KernelV2Error> {
    // ---- argument validation (ALL before the first mutation) -------------
    if !angle_rad.is_finite() || angle_rad <= 0.0 {
        return Err(KernelV2Error::RevolveInvalidAngle);
    }
    let two_pi = 2.0 * std::f64::consts::PI;
    let full_turn = (angle_rad - two_pi).abs() <= REVOLVE_FULL_TURN_TOLERANCE;
    if !full_turn && angle_rad > two_pi {
        return Err(KernelV2Error::RevolveInvalidAngle);
    }

    // A circle profile sweeps a TORUS (KV6d). Partial revolves build a bent
    // solid tube (2 disk caps + a toroidal lateral with longitude-arc seams);
    // the full-turn case builds the CLOSED genus-1 ring torus (spec
    // `specs/kv6d_closed_torus_revolve.md`). The on-axis full-turn circle
    // (a SPHERE, C0067) stays a typed wall — KV6d increment 2.
    if let ProfileRegion::Circle { center, radius } = profile.region() {
        return build_torus_revolve(
            arena,
            profile,
            *center,
            *radius,
            axis_origin,
            axis_direction,
            angle_rad,
            full_turn,
        );
    }

    let frame = match validate_revolve_geometry(profile, axis_origin, axis_direction) {
        // KV6 on-axis slices 1–3 (specs `kv6_on_axis_revolve_rectangle.md`,
        // `kv6_on_axis_revolve_oblique.md`, `kv6_on_axis_revolve_partial_
        // wedge.md`): the clearance rejection conflates CROSSING (invalid
        // input) with TOUCHING (a legitimate solid of revolution). Recover
        // the single-on-axis-edge lathe family — full-turn cylinder /
        // frustum / apex cone, and the partial-angle WEDGE (slice 3);
        // every other on-axis shape keeps the typed error.
        Err(KernelV2Error::RevolveAxisIntersectsProfile) => {
            return on_axis_revolve(arena, profile, axis_origin, axis_direction, angle_rad);
        }
        f => f?,
    };

    if full_turn {
        build_full_revolve(arena, &frame)
    } else {
        build_partial_revolve(arena, &frame, angle_rad)
    }
}

/// All revolve input validation: axis in plane, polygon region without
/// holes, profile strictly on one side of the axis, every edge axis-aligned.
/// Pure — no arena access.
fn validate_revolve_geometry(
    profile: &Profile,
    axis_origin: Point3,
    axis_direction: Vector3,
) -> Result<RevolveFrame, KernelV2Error> {
    // Axis direction: finite, nonzero, in the profile plane.
    let d = [axis_direction.x(), axis_direction.y(), axis_direction.z()];
    let d_sq = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if !d_sq.is_finite()
        || d_sq <= 0.0
        || !axis_origin.x().is_finite()
        || !axis_origin.y().is_finite()
        || !axis_origin.z().is_finite()
    {
        return Err(KernelV2Error::RevolveAxisNotInPlane);
    }
    let d_len = d_sq.sqrt();
    let a = UnitVector3 {
        x: d[0] / d_len,
        y: d[1] / d_len,
        z: d[2] / d_len,
    };
    let n = profile.unit_normal();
    if (a.x * n.x + a.y * n.y + a.z * n.z).abs() > REVOLVE_AXIS_IN_PLANE_TOLERANCE {
        return Err(KernelV2Error::RevolveAxisNotInPlane);
    }
    // Axis origin on the profile plane (band scaled by the magnitudes).
    let o = profile.origin();
    let plane_dist = (axis_origin.x() - o.x()) * n.x
        + (axis_origin.y() - o.y()) * n.y
        + (axis_origin.z() - o.z()) * n.z;
    let mag = axis_origin
        .x()
        .abs()
        .max(axis_origin.y().abs())
        .max(axis_origin.z().abs())
        .max(o.x().abs())
        .max(o.y().abs())
        .max(o.z().abs());
    if plane_dist.abs() > REVOLVE_AXIS_IN_PLANE_TOLERANCE * (1.0 + mag) {
        return Err(KernelV2Error::RevolveAxisNotInPlane);
    }

    // Region: polygon, no holes (typed walls for the rest).
    let outer = match profile.region() {
        ProfileRegion::Circle { .. } => {
            return Err(KernelV2Error::RevolveCircleProfileUnsupported);
        }
        ProfileRegion::Polygon { holes, .. } if !holes.is_empty() => {
            return Err(KernelV2Error::RevolveProfileHolesUnsupported);
        }
        ProfileRegion::ArcPolygon { .. } => {
            return Err(KernelV2Error::ArcPolygonProfileUnsupported);
        }
        ProfileRegion::Polygon { outer, .. } => outer,
    };

    // Radial direction: ŵ = ±normalize(n̂ × â), signed so the profile's
    // radial coordinates come out positive. m̂ = â × ŵ = ±n̂ is then the
    // sweep-velocity direction; the working loop is reordered so its
    // Newell normal is +m̂ (the extrude `reverse` flag, revolve edition).
    let wx = [
        n.y * a.z - n.z * a.y,
        n.z * a.x - n.x * a.z,
        n.x * a.y - n.y * a.x,
    ];
    let w_len = (wx[0] * wx[0] + wx[1] * wx[1] + wx[2] * wx[2]).sqrt();
    // |n̂ × â| = 1 up to rounding (â ⊥ n̂ just verified).
    let mut w = UnitVector3 {
        x: wx[0] / w_len,
        y: wx[1] / w_len,
        z: wx[2] / w_len,
    };

    let embedded: Vec<Point3> = outer.iter().map(|&p| profile.embed(p)).collect();
    let radial = |p: &Point3, w: &UnitVector3| {
        (p.x() - axis_origin.x()) * w.x
            + (p.y() - axis_origin.y()) * w.y
            + (p.z() - axis_origin.z()) * w.z
    };
    let s_sum: f64 = embedded.iter().map(|p| radial(p, &w)).sum();
    let flip_w = s_sum < 0.0;
    if flip_w {
        w = UnitVector3 {
            x: -w.x,
            y: -w.y,
            z: -w.z,
        };
    }
    // m̂ = â × ŵ (exactly ±n̂; recompute for numerical hygiene).
    let m = UnitVector3 {
        x: a.y * w.z - a.z * w.y,
        y: a.z * w.x - a.x * w.z,
        z: a.x * w.y - a.y * w.x,
    };

    // Working order: stored loops are CCW around n̂; the construction wants
    // Newell ≡ +m̂. m̂ = +n̂ exactly when ŵ was not flipped.
    let ring0: Vec<Point3> = if flip_w {
        embedded.into_iter().rev().collect()
    } else {
        embedded
    };

    // Per-vertex axis coordinates + strict one-side clearance.
    let mut t = Vec::with_capacity(ring0.len());
    let mut s = Vec::with_capacity(ring0.len());
    let mut scale = 0.0f64;
    for p in &ring0 {
        let dx = [
            p.x() - axis_origin.x(),
            p.y() - axis_origin.y(),
            p.z() - axis_origin.z(),
        ];
        t.push(dx[0] * a.x + dx[1] * a.y + dx[2] * a.z);
        s.push(dx[0] * w.x + dx[1] * w.y + dx[2] * w.z);
        scale = scale.max(dx[0].abs()).max(dx[1].abs()).max(dx[2].abs());
    }
    let clearance = REVOLVE_MIN_AXIS_CLEARANCE_REL * (1.0 + scale);
    if s.iter().any(|&si| si <= clearance) {
        // Mixed signs = crossing; near-zero = touching. Both invalid input.
        // (Straight in-plane edges between positive-radius vertices cannot
        // dip below their endpoint minimum, so the vertex check is
        // sufficient for the polygon.)
        return Err(KernelV2Error::RevolveAxisIntersectsProfile);
    }

    // Edge classification (working order; edge i joins i → i+1).
    let k = ring0.len();
    let mut edges = Vec::with_capacity(k);
    for i in 0..k {
        let j = (i + 1) % k;
        let dt = t[j] - t[i];
        let ds = s[j] - s[i];
        let len = (dt * dt + ds * ds).sqrt();
        if ds.abs() <= REVOLVE_EDGE_ALIGNMENT_TOLERANCE * len {
            // Material lies LEFT of the working-CCW edge: +ŝ for dt > 0 —
            // that face's outward normal points toward the axis (an inner
            // bore wall), the cavity sense.
            edges.push(EdgeClass::Parallel {
                radius: s[i],
                reversed: dt > 0.0,
            });
        } else if dt.abs() <= REVOLVE_EDGE_ALIGNMENT_TOLERANCE * len {
            // Outward normal +â exactly when the material is on the −â
            // side, i.e. when the working-CCW edge runs radially outward.
            edges.push(EdgeClass::Perpendicular {
                outward_plus_axis: ds > 0.0,
            });
        } else {
            // Oblique edge → cone frustum band (KV6c). The slant, extended,
            // meets the axis at `t_apex` (where s = 0): from s = s[i] +
            // (t − t[i])·ds/dt, set s = 0. `half_angle = atan|ds/dt|`; orient
            // `axis_dir` toward increasing radius so both rims have τ > 0
            // (apex behind). `reversed = dt > 0`, the same material-sense rule
            // as `Parallel` (the cone's default outward normal points away
            // from the axis; `reversed` flips it toward the axis for a bore).
            let t_apex = t[i] - s[i] * dt / ds;
            let apex = Point3::new(
                axis_origin.x() + t_apex * a.x,
                axis_origin.y() + t_apex * a.y,
                axis_origin.z() + t_apex * a.z,
            );
            let axis_dir = if (ds > 0.0) == (dt > 0.0) { a } else { neg(a) };
            edges.push(EdgeClass::Oblique {
                apex,
                axis_dir,
                half_angle: (ds / dt).abs().atan(),
                reversed: dt > 0.0,
            });
        }
    }

    Ok(RevolveFrame {
        a,
        w,
        m,
        a0: axis_origin,
        ring0,
        t,
        s,
        edges,
    })
}

/// Snap a trig value to exactly 0 / ±1 when within 1e-12 of it. At the
/// quadrant angles (90°, 180°, 270°, 360°) `sin`/`cos` carry ~1e-16
/// residue; snapping makes a 180° revolve's two caps carry BIT-IDENTICAL
/// planes — which yang's intra-solid near-coplanar gate excludes as the
/// benign "one plane, several faces" class. Without the snap the caps
/// differ by 1 ulp and the gate (correctly conservative for the femto-seam
/// hazard class) would defer every boolean near them.
fn snap_trig(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        0.0
    } else if (x - 1.0).abs() < 1e-12 {
        1.0
    } else if (x + 1.0).abs() < 1e-12 {
        -1.0
    } else {
        x
    }
}

impl RevolveFrame {
    /// Rotate a profile point by `theta` about the axis: the point's
    /// in-plane decomposition is `a0 + t·â + s·ŵ`, which maps to
    /// `a0 + t·â + s·(cos θ·ŵ + sin θ·m̂)`. Trig snapped at the quadrant
    /// angles (see [`snap_trig`]).
    fn rotate(&self, i: usize, theta: f64) -> Point3 {
        let (c, sn) = (snap_trig(theta.cos()), snap_trig(theta.sin()));
        let radial = self.s[i];
        Point3::new(
            self.a0.x() + self.t[i] * self.a.x + radial * (c * self.w.x + sn * self.m.x),
            self.a0.y() + self.t[i] * self.a.y + radial * (c * self.w.y + sn * self.m.y),
            self.a0.z() + self.t[i] * self.a.z + radial * (c * self.w.z + sn * self.m.z),
        )
    }

    /// Foot of the axis perpendicular through vertex `i` (= arc center).
    fn axis_foot(&self, i: usize) -> Point3 {
        Point3::new(
            self.a0.x() + self.t[i] * self.a.x,
            self.a0.y() + self.t[i] * self.a.y,
            self.a0.z() + self.t[i] * self.a.z,
        )
    }
}

/// Partial-angle branch: caps + one wall per profile edge, sweep arcs
/// between the θ=0 and θ=α vertex rings. Topology (k = edge count):
/// V = 2k, E = 3k (k cap segments ×2 + k arcs), F = k + 2, χ = 2.
fn build_partial_revolve(
    arena: &mut BrepArena,
    fr: &RevolveFrame,
    angle: f64,
) -> Result<RevolveResult, KernelV2Error> {
    let k = fr.ring0.len();
    let neg = |u: UnitVector3| UnitVector3 {
        x: -u.x,
        y: -u.y,
        z: -u.z,
    };

    // ---- vertices: ring 0 (working order) + ring α -------------------------
    let vb = arena.vertices.len() as u32;
    for p in &fr.ring0 {
        arena.vertices.push(Some(Vertex { point: *p }));
    }
    for i in 0..k {
        arena.vertices.push(Some(Vertex {
            point: fr.rotate(i, angle),
        }));
    }
    let v0 = |i: usize| VertexId(vb + (i % k) as u32);
    let v1 = |i: usize| VertexId(vb + k as u32 + (i % k) as u32);

    // ---- half-edge id layout (6 per edge index i) ---------------------------
    // sc[i]: start cap, ring0[i+1] → ring0[i]   (cap winds CCW around −m̂)
    // ec[i]: end cap,   ring1[i]   → ring1[i+1] (cap winds CCW around rot(+m̂))
    // wb[i]: wall i bottom, ring0[i] → ring0[i+1] (twin sc[i])
    // wt[i]: wall i top,    ring1[i+1] → ring1[i] (twin ec[i])
    // af[i]: forward sweep arc at vertex i, ring0[i] → ring1[i], normal +â
    //        (lives in wall (i−1+k)%k's loop)
    // ab[i]: backward arc at vertex i, ring1[i] → ring0[i], normal −â
    //        (twin af[i]; lives in wall i's loop)
    let hb = arena.half_edges.len() as u32;
    let sc = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32));
    let ec = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32) + 1);
    let wb = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32) + 2);
    let wt = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32) + 3);
    let af = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32) + 4);
    let ab = |i: usize| HalfEdgeId(hb + 6 * ((i % k) as u32) + 5);

    let lb = arena.loops.len() as u32;
    let loop_start = LoopId(lb);
    let loop_end = LoopId(lb + 1);
    let loop_wall = |i: usize| LoopId(lb + 2 + (i % k) as u32);
    let fb = arena.faces.len() as u32;
    let f_start = FaceId(fb);
    let f_end = FaceId(fb + 1);
    let f_wall = |i: usize| FaceId(fb + 2 + (i % k) as u32);
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    for i in 0..k {
        let arc_curve = |normal: UnitVector3| Curve::Arc {
            center: fr.axis_foot(i),
            normal,
            radius: fr.s[i],
        };
        // sc[i]: origin ring0[i+1]; cap cycle visits vertices in reverse,
        // so next(sc[i]) starts at ring0[i] — that is sc[i−1].
        arena.half_edges.push(Some(HalfEdge {
            twin: wb(i),
            next: sc(i + k - 1),
            prev: sc(i + 1),
            origin: v0(i + 1),
            loop_id: loop_start,
            curve: Curve::LineSegment,
        }));
        // ec[i]: origin ring1[i]; forward cycle.
        arena.half_edges.push(Some(HalfEdge {
            twin: wt(i),
            next: ec(i + 1),
            prev: ec(i + k - 1),
            origin: v1(i),
            loop_id: loop_end,
            curve: Curve::LineSegment,
        }));
        // Wall i cycle: wb[i] → af[i+1] → wt[i] → ab[i] → wb[i].
        arena.half_edges.push(Some(HalfEdge {
            twin: sc(i),
            next: af(i + 1),
            prev: ab(i),
            origin: v0(i),
            loop_id: loop_wall(i),
            curve: Curve::LineSegment,
        }));
        arena.half_edges.push(Some(HalfEdge {
            twin: ec(i),
            next: ab(i),
            prev: af(i + 1),
            origin: v1(i + 1),
            loop_id: loop_wall(i),
            curve: Curve::LineSegment,
        }));
        // af[i] lives in wall (i−1)'s loop: prev = wb[i−1], next = wt[i−1].
        arena.half_edges.push(Some(HalfEdge {
            twin: ab(i),
            next: wt(i + k - 1),
            prev: wb(i + k - 1),
            origin: v0(i),
            loop_id: loop_wall(i + k - 1),
            curve: arc_curve(fr.a),
        }));
        arena.half_edges.push(Some(HalfEdge {
            twin: af(i),
            next: wb(i),
            prev: wt(i),
            origin: v1(i),
            loop_id: loop_wall(i),
            curve: arc_curve(neg(fr.a)),
        }));
    }

    // ---- loops, faces ------------------------------------------------------
    arena.loops.push(Some(Loop {
        face: f_start,
        boundary: LoopBoundary::Edges(sc(0)),
        kind: LoopKind::Outer,
    }));
    arena.loops.push(Some(Loop {
        face: f_end,
        boundary: LoopBoundary::Edges(ec(0)),
        kind: LoopKind::Outer,
    }));
    for i in 0..k {
        arena.loops.push(Some(Loop {
            face: f_wall(i),
            boundary: LoopBoundary::Edges(wb(i)),
            kind: LoopKind::Outer,
        }));
    }

    // Start cap: outward normal −m̂ (opposes the sweep velocity); end cap:
    // +m̂ rotated by the sweep angle = cos α·m̂ − sin α·ŵ.
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: fr.ring0[0],
            normal: neg(fr.m),
        })),
        outer_loop: loop_start,
        inner_loops: Vec::new(),
        shell,
    }));
    let (ca, sa) = (snap_trig(angle.cos()), snap_trig(angle.sin()));
    arena.faces.push(Some(Face {
        surface: Some(Surface::Plane(Plane {
            point: fr.rotate(0, angle),
            normal: UnitVector3 {
                x: ca * fr.m.x - sa * fr.w.x,
                y: ca * fr.m.y - sa * fr.w.y,
                z: ca * fr.m.z - sa * fr.w.z,
            },
        })),
        outer_loop: loop_end,
        inner_loops: Vec::new(),
        shell,
    }));
    let mut walls = Vec::with_capacity(k);
    for (i, cls) in fr.edges.iter().enumerate() {
        let surface = match *cls {
            EdgeClass::Parallel { radius, reversed } => Surface::Cylinder {
                axis_point: fr.a0,
                axis_dir: fr.a,
                radius,
                reversed,
            },
            EdgeClass::Perpendicular { outward_plus_axis } => Surface::Plane(Plane {
                point: fr.ring0[i],
                normal: if outward_plus_axis { fr.a } else { neg(fr.a) },
            }),
            // KV6c increment 5 (spec `kv6c_partial_revolve_cone_patch.md`):
            // an oblique edge sweeps an arc-bounded CONE patch — the same
            // [seg, arc, seg, arc] wall loop as `Parallel`, only the surface
            // differs. Parameters pass through from the classification.
            EdgeClass::Oblique {
                apex,
                axis_dir,
                half_angle,
                reversed,
            } => Surface::Cone {
                apex,
                axis_dir,
                half_angle,
                reversed,
            },
        };
        arena.faces.push(Some(Face {
            surface: Some(surface),
            outer_loop: loop_wall(i),
            inner_loops: Vec::new(),
            shell,
        }));
        walls.push(f_wall(i));
    }

    let mut shell_faces = vec![f_start, f_end];
    shell_faces.extend(walls.iter().copied());
    arena.shells.push(Some(Shell {
        solid,
        faces: shell_faces,
        genus: 0,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(RevolveResult {
        solid,
        shell,
        start_cap: Some(f_start),
        end_cap: Some(f_end),
        walls,
    })
}

mod closed;
mod on_axis;
pub(crate) use closed::*;
pub(crate) use on_axis::*;

/// Full-turn branch: the washer. Perpendicular profile edges become
/// seamless annuli (outer circle loop + circle ring); parallel edges
/// become canonical full cylinders (2 rims + a seam at θ = 0, the KV5a
/// lateral shape). Rectangle: V=4, E=6 (4 rims + 2 seams), F=4, R=2,
/// G=1 ⇒ χ = 0 = 2(S − G).
///
/// KV6a-tilted (spec `kv6a_nonalternating_full_revolve.md`): every vertex
/// pairing of wall (Parallel cylinder / Oblique cone) and annulus
/// (Perpendicular) edges is supported EXCEPT two consecutive annuli. The
/// twin arithmetic (`rim_on_edge`) is class-agnostic, and the wall rim
/// normals are consistent at wall-wall junctions by construction: with
/// `toward = sign(Δt)·â` and `reversed ⟺ sign(Δt)` fixed by the CCW
/// profile's outward side, every wall rim half-edge carries one sign of
/// `â` at its head vertex and the opposite at its tail — adjacent walls
/// meet head-to-tail, so twin rims always traverse oppositely (the
/// alternating washer is the special case where one neighbour is an
/// annulus). Two consecutive ANNULI (a subdivided radial edge through a
/// collinear vertex — coplanar adjacent faces) stay typed
/// [`KernelV2Error::NotImplemented`].
fn build_full_revolve(
    arena: &mut BrepArena,
    fr: &RevolveFrame,
) -> Result<RevolveResult, KernelV2Error> {
    let k = fr.ring0.len();
    let neg = |u: UnitVector3| UnitVector3 {
        x: -u.x,
        y: -u.y,
        z: -u.z,
    };
    let is_wall =
        |c: EdgeClass| matches!(c, EdgeClass::Parallel { .. } | EdgeClass::Oblique { .. });
    for i in 0..k {
        if !is_wall(fr.edges[i]) && !is_wall(fr.edges[(i + 1) % k]) {
            return Err(KernelV2Error::NotImplemented(
                "PR-KV6a full-turn revolve with consecutive annular (axis-perpendicular) edges",
            ));
        }
    }

    // ---- vertices: the θ=0 ring only (every rim circle is anchored there) --
    let vb = arena.vertices.len() as u32;
    for p in &fr.ring0 {
        arena.vertices.push(Some(Vertex { point: *p }));
    }
    let v = |i: usize| VertexId(vb + (i % k) as u32);

    // ---- half-edge layout (4 per edge index) --------------------------------
    // Per PARALLEL edge i (cylinder wall, vertices i and i+1):
    //   rim_w[i]   : rim circle at vertex i, in the wall loop
    //   seam_f[i]  : seam ring0[i] → ring0[i+1], in the wall loop
    //   rim_w2[i]  : rim circle at vertex i+1, in the wall loop
    //   seam_b[i]  : seam ring0[i+1] → ring0[i], in the wall loop (twin of f)
    //   wall cycle: rim_w[i] → seam_f[i] → rim_w2[i] → seam_b[i] → rim_w[i]
    // Per PERPENDICULAR edge j (annulus, vertices j and j+1): two closed
    //   circle half-edges, ann_o[j] (outer loop, at the larger-radius
    //   vertex) and ann_r[j] (ring, at the smaller-radius vertex); they twin
    //   with the adjacent walls' rim half-edges at the same vertices.
    let hb = arena.half_edges.len() as u32;
    let he = |i: usize, slot: u32| HalfEdgeId(hb + 4 * ((i % k) as u32) + slot);

    let lb = arena.loops.len() as u32;
    let fb = arena.faces.len() as u32;
    let shell = ShellId(arena.shells.len() as u32);
    let solid = SolidId(arena.solids.len() as u32);

    // Loop/face layout: one outer loop per edge face; annuli get one ring
    // loop each (allocated after the k outer loops).
    let outer_loop = |i: usize| LoopId(lb + (i % k) as u32);
    let face_of = |i: usize| FaceId(fb + (i % k) as u32);

    // Rim circle half-edge ids at a VERTEX: vertex i is shared by edge
    // (i−1) and edge i, and the rim at i twins between those two edges'
    // faces whatever their classes (wall-wall junctions included —
    // KV6a-tilted). Rim at vertex i: slot 0 if the owning edge is edge i
    // (rim_w / annulus at its first vertex), slot 2 if it is edge i−1
    // (rim_w2 / annulus at its second vertex).
    let rim_on_edge = |edge: usize, vertex: usize| -> HalfEdgeId {
        if vertex % k == edge % k {
            he(edge, 0)
        } else {
            he(edge, 2)
        }
    };

    let mut ring_loops: Vec<(usize, LoopId)> = Vec::new(); // (edge idx, ring loop)
    let mut next_ring = lb + k as u32;
    for (i, cls) in fr.edges.iter().enumerate() {
        if matches!(cls, EdgeClass::Perpendicular { .. }) {
            ring_loops.push((i, LoopId(next_ring)));
            next_ring += 1;
        }
    }
    let ring_loop_of = |edge: usize, ring_loops: &[(usize, LoopId)]| -> LoopId {
        ring_loops
            .iter()
            .find(|(e, _)| *e == edge)
            .map(|(_, l)| *l)
            .expect("perpendicular edge has a ring loop")
    };

    // ---- emit half-edges (4 dense slots per edge; perpendicular edges use
    //      slots 0/2 and leave 1/3 as dead `None` slots so the id arithmetic
    //      stays uniform) ----------------------------------------------------
    for (i, cls) in fr.edges.iter().enumerate() {
        let j = (i + 1) % k;
        match *cls {
            EdgeClass::Parallel { reversed, .. } | EdgeClass::Oblique { reversed, .. } => {
                // Rim normals: for an outward wall the rim's traversal axis
                // points TOWARD the opposite rim (the KV5a canonical rule);
                // for a reversed (inner-bore) wall it points AWAY — the
                // mirrored material sense, forced by the twin structure
                // (each rim twin lives in an adjacent annulus whose
                // outer/ring winding rules fix the sign).
                let toward_j = if fr.t[j] >= fr.t[i] { fr.a } else { neg(fr.a) };
                let (n_i, n_j) = if reversed {
                    (neg(toward_j), toward_j)
                } else {
                    (toward_j, neg(toward_j))
                };
                let rim = |vi: usize, normal: UnitVector3| Curve::Circle {
                    center: fr.axis_foot(vi),
                    normal,
                    radius: fr.s[vi],
                };
                // Wall cycle: rim(i) → seam i→j → rim(j) → seam j→i.
                arena.half_edges.push(Some(HalfEdge {
                    twin: rim_on_edge((i + k - 1) % k, i),
                    next: he(i, 1),
                    prev: he(i, 3),
                    origin: v(i),
                    loop_id: outer_loop(i),
                    curve: rim(i, n_i),
                }));
                arena.half_edges.push(Some(HalfEdge {
                    twin: he(i, 3),
                    next: he(i, 2),
                    prev: he(i, 0),
                    origin: v(i),
                    loop_id: outer_loop(i),
                    curve: Curve::LineSegment,
                }));
                arena.half_edges.push(Some(HalfEdge {
                    twin: rim_on_edge(j, j),
                    next: he(i, 3),
                    prev: he(i, 1),
                    origin: v(j),
                    loop_id: outer_loop(i),
                    curve: rim(j, n_j),
                }));
                arena.half_edges.push(Some(HalfEdge {
                    twin: he(i, 1),
                    next: he(i, 0),
                    prev: he(i, 2),
                    origin: v(j),
                    loop_id: outer_loop(i),
                    curve: Curve::LineSegment,
                }));
            }
            EdgeClass::Perpendicular { outward_plus_axis } => {
                // Annulus face normal ±â; the outer circle (larger radius)
                // traverses CCW around the face normal, the ring circle CCW
                // around its negation. Each twins with the wall-side rim at
                // the same vertex.
                let normal = if outward_plus_axis { fr.a } else { neg(fr.a) };
                let vo = if fr.s[i] >= fr.s[j] { i } else { j };
                let ring_l = ring_loop_of(i, &ring_loops);
                for (slot, vi) in [(0u32, i), (2u32, j)] {
                    let is_outer = vi == vo;
                    let nu = if is_outer { normal } else { neg(normal) };
                    let lid = if is_outer { outer_loop(i) } else { ring_l };
                    let hid = he(i, slot);
                    let other_edge = if vi == i { (i + k - 1) % k } else { j };
                    arena.half_edges.push(Some(HalfEdge {
                        twin: rim_on_edge(other_edge, vi),
                        next: hid,
                        prev: hid,
                        origin: v(vi),
                        loop_id: lid,
                        curve: Curve::Circle {
                            center: fr.axis_foot(vi),
                            normal: nu,
                            radius: fr.s[vi],
                        },
                    }));
                    if slot == 0 {
                        arena.half_edges.push(None); // dead slot 1
                    }
                }
                arena.half_edges.push(None); // dead slot 3
            }
        }
    }

    // ---- loops --------------------------------------------------------------
    // k outer loops (edge order), then one ring loop per perpendicular edge
    // (the order ring_loops was collected in).
    for (i, cls) in fr.edges.iter().enumerate() {
        let j = (i + 1) % k;
        let boundary = match *cls {
            EdgeClass::Parallel { .. } | EdgeClass::Oblique { .. } => he(i, 0),
            EdgeClass::Perpendicular { .. } => {
                let vo = if fr.s[i] >= fr.s[j] { i } else { j };
                he(i, if vo == i { 0 } else { 2 })
            }
        };
        arena.loops.push(Some(Loop {
            face: face_of(i),
            boundary: LoopBoundary::Edges(boundary),
            kind: LoopKind::Outer,
        }));
    }
    for &(i, _lid) in &ring_loops {
        let j = (i + 1) % k;
        let vr = if fr.s[i] >= fr.s[j] { j } else { i };
        arena.loops.push(Some(Loop {
            face: face_of(i),
            boundary: LoopBoundary::Edges(he(i, if vr == i { 0 } else { 2 })),
            kind: LoopKind::Inner,
        }));
    }

    // ---- faces ----------------------------------------------------------------
    let mut start_cap = None; // perpendicular face at the axial minimum
    let mut end_cap = None;
    let mut walls = Vec::new();
    for (i, cls) in fr.edges.iter().enumerate() {
        let surface = match *cls {
            EdgeClass::Parallel { radius, reversed } => Surface::Cylinder {
                axis_point: fr.a0,
                axis_dir: fr.a,
                radius,
                reversed,
            },
            EdgeClass::Perpendicular { outward_plus_axis } => Surface::Plane(Plane {
                point: fr.ring0[i],
                normal: if outward_plus_axis { fr.a } else { neg(fr.a) },
            }),
            EdgeClass::Oblique {
                apex,
                axis_dir,
                half_angle,
                reversed,
            } => Surface::Cone {
                apex,
                axis_dir,
                half_angle,
                reversed,
            },
        };
        let inner: Vec<LoopId> = ring_loops
            .iter()
            .filter(|(e, _)| *e == i)
            .map(|(_, l)| *l)
            .collect();
        arena.faces.push(Some(Face {
            surface: Some(surface),
            outer_loop: outer_loop(i),
            inner_loops: inner,
            shell,
        }));
        match *cls {
            EdgeClass::Perpendicular { outward_plus_axis } => {
                // The first −â annulus is the start cap, the first +â one
                // the end cap; a staircase's extra annuli join `walls`. An
                // all-wall profile (KV6a-tilted diamond ring) has none —
                // both caps stay `None`.
                if !outward_plus_axis && start_cap.is_none() {
                    start_cap = Some(face_of(i));
                } else if outward_plus_axis && end_cap.is_none() {
                    end_cap = Some(face_of(i));
                } else {
                    walls.push(face_of(i));
                }
            }
            EdgeClass::Parallel { .. } | EdgeClass::Oblique { .. } => walls.push(face_of(i)),
        }
    }
    arena.shells.push(Some(Shell {
        solid,
        faces: (0..k).map(face_of).collect(),
        genus: 1,
    }));
    arena.solids.push(Some(Solid {
        shells: vec![shell],
    }));

    finalize_solid(arena, solid)?;
    Ok(RevolveResult {
        solid,
        shell,
        start_cap,
        end_cap,
        walls,
    })
}
