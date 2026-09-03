//! R0053 topology adjudication on EXACT analytic membership — no
//! tessellation anywhere (spec `yang_451_corner_transit.md` §3ah).
//!
//! §3af left R0053's genus adjudicated by the Cherchi sidecar on the
//! harness's operand TESSELLATIONS (genus 15) against the kernel's 1, and the
//! lattice ladder on those same tessellations UNSTABLE. Both readings go
//! through a mesh. This probe removes the mesh: each operand is the analytic
//! solid the document names — the revolve of the rectangle about its axis,
//! the extrude of the box, the revolve of the involute gear (root / base /
//! addendum radii and the involute half-width in closed form, as
//! `waffle_types::gear` constructs them) — and the union's cubical `χ` is
//! read on a resolution ladder from the exact set-union predicate. A
//! predicate cannot alias a coplanar overlap (the union is solid across the
//! shared plane) and cannot mint a sliver; only a genuine near-tangency can
//! move `χ` between rungs.
//!
//! Run:
//! ```text
//! EXACT_H=2,1,0.7 EXACT_PHASE=0.5,0.25 cargo test -p test-harness --release \
//!   --test s453_r0053_exact_topology -- --ignored --nocapture r0053_exact_ladder
//! ```
//! `EXACT_SETS="ring;box;gear;ring,box;ring,box,gear"` selects the operand
//! subsets (default: every operand alone, the op-1 prefix, the full union).

use std::f64::consts::TAU;

use test_harness::assay::topology_oracle::VoxelGrid;

// ---- R0053 as authored (`app/tests/cases/assay/R0053.waffle`) ------------
const PLANE_ORIGIN: [f64; 3] = [-29.426732539583355, 20.01545028630079, 32.77417929942979];
const PLANE_NORMAL: [f64; 3] = [
    -0.824101194972795,
    -0.5653857327001586,
    -0.03458603335909444,
];
const AXIS0_ORIGIN: [f64; 3] = [-27.645902992976346, 21.23721240018261, -29.631087096247377];
const AXIS2_ORIGIN: [f64; 3] = [-26.875272092639932, 21.76591411653247, -56.63615713221034];
const AXIS_DIR: [f64; 3] = [-0.5657241918041889, 0.82459452994032, 0.0];
const ANGLE0_DEG: f64 = 287.6116342299196;
const ANGLE2_DEG: f64 = 301.92350994114435;
/// Ring rectangle half-extents (sketch x = axial, y = radial).
const RING_HALF: (f64, f64) = (20.814208102619652, 44.556148175861644);
/// Box rectangle half-extents and blind depth along +normal.
const BOX_HALF: (f64, f64) = (41.49383940309333, 52.07460730903383);
const BOX_DEPTH: f64 = 100.27185565935144;
const GEAR_TEETH: u32 = 16;
const GEAR_MODULE: f64 = 7.4553217080021374;
const GEAR_PRESSURE_DEG: f64 = 20.0;

fn norm(v: [f64; 3]) -> [f64; 3] {
    let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / m, v[1] / m, v[2] / m]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The sketch frame `(u, v, w)`: `u` = sketch x (mirrors
/// `SketchPlaneBasis::from_origin_normal`), `w` = the plane normal.
struct Frame {
    x: [f64; 3],
    y: [f64; 3],
    n: [f64; 3],
}

impl Frame {
    fn new() -> Self {
        let n = norm(PLANE_NORMAL);
        let reference = if dot(n, [0.0, 0.0, 1.0]).abs() < 0.99 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let x = norm(cross(reference, n));
        let y = norm(cross(n, x));
        Frame { x, y, n }
    }
    fn local(&self, p: [f64; 3]) -> [f64; 3] {
        let r = [
            p[0] - PLANE_ORIGIN[0],
            p[1] - PLANE_ORIGIN[1],
            p[2] - PLANE_ORIGIN[2],
        ];
        [dot(r, self.x), dot(r, self.y), dot(r, self.n)]
    }
}

/// A partial revolve about the in-plane axis `v = v_axis` (direction ±u) of
/// a profile centred on `v = 0`, with kernel-v2's sign conventions
/// (`construct/revolve.rs`): `ŵ = ±(n̂ × â)` toward the profile, sweep
/// velocity at θ = 0 is `m̂ = â × ŵ`.
struct Revolve {
    v_axis: f64,
    /// +1 when the profile lies at `v > v_axis`.
    wdir: f64,
    /// Sign of `m̂ · n̂`: the sweep leaves the plane toward this side of `w`.
    sweep: f64,
    angle: f64,
}

impl Revolve {
    fn new(frame: &Frame, axis_origin: [f64; 3], angle_deg: f64) -> Self {
        let a = norm(AXIS_DIR);
        assert!(dot(a, frame.n).abs() < 1e-9, "axis in plane");
        assert!(dot(a, frame.y).abs() < 1e-9, "axis along sketch x");
        let v_axis = frame.local(axis_origin)[1];
        let wdir = if -v_axis > 0.0 { 1.0 } else { -1.0 };
        let w_hat = [wdir * frame.y[0], wdir * frame.y[1], wdir * frame.y[2]];
        let m = cross(a, w_hat);
        let sweep = dot(m, frame.n).signum();
        Revolve {
            v_axis,
            wdir,
            sweep,
            angle: angle_deg.to_radians(),
        }
    }
    /// `(rho, theta, v_unrotated)` of a frame point: the distance from the
    /// axis, the sweep angle in `[0, 2π)`, and where the point sits in the
    /// profile plane once rotated back to θ = 0.
    fn meridian(&self, v: f64, w: f64) -> (f64, f64, f64) {
        let rho = (v - self.v_axis).hypot(w);
        let theta = (self.sweep * w)
            .atan2(self.wdir * (v - self.v_axis))
            .rem_euclid(TAU);
        (rho, theta, self.v_axis + self.wdir * rho)
    }
}

/// The involute gear disc in closed form, exactly as `waffle_types::gear`
/// lays it out: tooth `k` centred at `k · 2π/N` from sketch +x; radial flank
/// from the root to the base circle at the half-tooth angle
/// `π/(2N) + inv(α)`; involute from the base circle to the addendum with
/// polar offset `inv(t) = t − atan t`, `t = √((r/r_b)² − 1)`; tip arc at the
/// addendum radius; root arc at `max(r_pitch − 1.25 m, r_b/2)`.
struct GearDisc {
    root_r: f64,
    base_r: f64,
    add_r: f64,
    half: f64,
    pitch_ang: f64,
}

impl GearDisc {
    fn new() -> Self {
        let alpha = GEAR_PRESSURE_DEG.to_radians();
        let pitch_r = GEAR_TEETH as f64 * GEAR_MODULE / 2.0;
        let base_r = pitch_r * alpha.cos();
        let add_r = pitch_r + GEAR_MODULE;
        let ded_r = pitch_r - 1.25 * GEAR_MODULE;
        let root_r = ded_r.max(base_r * 0.5);
        let inv_alpha = alpha.tan() - alpha;
        let pitch_ang = TAU / GEAR_TEETH as f64;
        GearDisc {
            root_r,
            base_r,
            add_r,
            half: pitch_ang / 4.0 + inv_alpha,
            pitch_ang,
        }
    }
    fn contains(&self, x: f64, y: f64) -> bool {
        let r = x.hypot(y);
        if r <= self.root_r {
            return true;
        }
        if r > self.add_r {
            return false;
        }
        let phi = y.atan2(x);
        let d = (phi + self.pitch_ang / 2.0).rem_euclid(self.pitch_ang) - self.pitch_ang / 2.0;
        let hw = if r <= self.base_r {
            self.half
        } else {
            let t = ((r / self.base_r).powi(2) - 1.0).sqrt();
            self.half - (t - t.atan())
        };
        d.abs() <= hw
    }
    /// Tooth angular half-width at radius `r` (for the report).
    fn half_width_at(&self, r: f64) -> f64 {
        if r <= self.base_r {
            self.half
        } else {
            let t = ((r / self.base_r).powi(2) - 1.0).sqrt();
            self.half - (t - t.atan())
        }
    }
}

struct Solids {
    ring: Revolve,
    gear: Revolve,
    disc: GearDisc,
}

impl Solids {
    fn new(frame: &Frame) -> Self {
        Solids {
            ring: Revolve::new(frame, AXIS0_ORIGIN, ANGLE0_DEG),
            gear: Revolve::new(frame, AXIS2_ORIGIN, ANGLE2_DEG),
            disc: GearDisc::new(),
        }
    }
    fn in_ring(&self, p: [f64; 3]) -> bool {
        if p[0].abs() > RING_HALF.0 {
            return false;
        }
        let (rho, theta, _) = self.ring.meridian(p[1], p[2]);
        let (lo, hi) = (
            (-RING_HALF.1 - self.ring.v_axis).abs(),
            (RING_HALF.1 - self.ring.v_axis).abs(),
        );
        rho >= lo.min(hi) && rho <= lo.max(hi) && theta <= self.ring.angle
    }
    fn in_box(&self, p: [f64; 3]) -> bool {
        p[0].abs() <= BOX_HALF.0 && p[1].abs() <= BOX_HALF.1 && p[2] >= 0.0 && p[2] <= BOX_DEPTH
    }
    fn in_gear(&self, p: [f64; 3]) -> bool {
        let (_, theta, v_unrot) = self.gear.meridian(p[1], p[2]);
        theta <= self.gear.angle && self.disc.contains(p[0], v_unrot)
    }
    fn member(&self, set: &[&str], p: [f64; 3]) -> bool {
        set.iter().any(|s| match *s {
            "ring" => self.in_ring(p),
            "box" => self.in_box(p),
            "gear" => self.in_gear(p),
            other => panic!("unknown operand {other}"),
        })
    }
}

fn env_list(name: &str, default: &str) -> Vec<String> {
    std::env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn env_f64s(name: &str, default: &str) -> Vec<f64> {
    std::env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

/// The exact-membership ladder. Prints one line per (set, h, phase).
#[test]
#[ignore = "instrument: run by hand with --nocapture (minutes at fine h)"]
fn r0053_exact_ladder() {
    let frame = Frame::new();
    let s = Solids::new(&frame);
    println!(
        "frame: ring axis v0 = {:.4} (wdir {:+}, sweep toward w {:+}); gear axis v2 = {:.4} (wdir {:+}, sweep {:+}); axes offset {:.4}; extrude along +w",
        s.ring.v_axis, s.ring.wdir, s.ring.sweep, s.gear.v_axis, s.gear.wdir, s.gear.sweep,
        (s.ring.v_axis - s.gear.v_axis).abs()
    );
    println!(
        "gear: root {:.4} base {:.4} addendum {:.4}; half-tooth {:.4}° at base, {:.4}° at the tip",
        s.disc.root_r,
        s.disc.base_r,
        s.disc.add_r,
        s.disc.half.to_degrees(),
        s.disc.half_width_at(s.disc.add_r).to_degrees()
    );
    // Joint bounding box of everything, padded.
    let vmin = s.gear.v_axis - s.disc.add_r - (-s.gear.v_axis) - 2.0;
    let lo = [
        -(s.disc.add_r + 2.0),
        vmin.min(-BOX_HALF.1 - 2.0),
        -(s.disc.add_r - s.gear.v_axis) - 2.0,
    ];
    let hi = [
        s.disc.add_r + 2.0,
        (BOX_HALF.1 + 2.0).max(s.disc.add_r + 2.0),
        (s.disc.add_r - s.gear.v_axis).max(BOX_DEPTH) + 2.0,
    ];
    println!(
        "bbox u {:?} v {:?} w {:?}",
        (lo[0], hi[0]),
        (lo[1], hi[1]),
        (lo[2], hi[2])
    );
    let sets = env_list("EXACT_SETS", "ring;box;gear;ring,box;ring,box,gear");
    let hs = env_f64s("EXACT_H", "2,1");
    let phases = env_f64s("EXACT_PHASE", "0.5");
    for set in &sets {
        let names: Vec<&str> = set.split(',').map(str::trim).collect();
        for &h in &hs {
            for &phase in &phases {
                let n = [
                    ((hi[0] - lo[0]) / h).ceil() as usize,
                    ((hi[1] - lo[1]) / h).ceil() as usize,
                    ((hi[2] - lo[2]) / h).ceil() as usize,
                ];
                let t0 = std::time::Instant::now();
                let grid = VoxelGrid::from_fn(n, |i, j, k| {
                    let p = [
                        lo[0] + (i as f64 + phase) * h,
                        lo[1] + (j as f64 + phase) * h,
                        lo[2] + (k as f64 + phase) * h,
                    ];
                    s.member(&names, p)
                });
                let r = grid.readout();
                println!(
                    "{:<16} h={:<5} phase={:<4} n={:?}  chi={:>4}  components={}  cubes={}  volume={:.6e}  ({:.1}s)",
                    set,
                    h,
                    phase,
                    n,
                    r.chi,
                    r.components,
                    r.cubes,
                    r.cubes as f64 * h * h * h,
                    t0.elapsed().as_secs_f64()
                );
            }
        }
    }
}

/// Where the ring meets the gear's teeth: for each tooth, the depth by which
/// the ring solid reaches INTO the tooth zone `[root, addendum]` of the
/// gear's meridian disc, and whether it reaches the root — the measurement
/// §3af's "fourteen tunnels" inference needs and never had.
#[test]
#[ignore = "instrument: run by hand with --nocapture"]
fn r0053_ring_vs_teeth() {
    let frame = Frame::new();
    let s = Solids::new(&frame);
    // Sample the ring solid densely; bin each point that lands in the gear's
    // tooth zone (any sweep angle of the gear within its range) by the
    // nearest tooth and record the min / max radius from the gear centre.
    let nteeth = GEAR_TEETH as usize;
    let mut min_r = vec![f64::INFINITY; nteeth];
    let mut max_r = vec![f64::NEG_INFINITY; nteeth];
    let mut in_groove_min = vec![f64::INFINITY; nteeth];
    let mut in_groove_max = vec![f64::NEG_INFINITY; nteeth];
    let h = 0.25;
    let (lo_r, hi_r) = (
        (-RING_HALF.1 - s.ring.v_axis)
            .abs()
            .min((RING_HALF.1 - s.ring.v_axis).abs()),
        (-RING_HALF.1 - s.ring.v_axis)
            .abs()
            .max((RING_HALF.1 - s.ring.v_axis).abs()),
    );
    let mut u = -RING_HALF.0;
    while u <= RING_HALF.0 {
        let mut rho = lo_r;
        while rho <= hi_r {
            let steps = ((rho * s.ring.angle) / h).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let theta = s.ring.angle * k as f64 / steps as f64;
                // Frame point of the ring at (u, rho, theta).
                let v = s.ring.v_axis + s.ring.wdir * rho * theta.cos();
                let w = s.ring.sweep * rho * theta.sin();
                let (_, th2, v_unrot) = s.gear.meridian(v, w);
                if th2 > s.gear.angle {
                    continue;
                }
                let r = u.hypot(v_unrot);
                if r < s.disc.root_r || r > s.disc.add_r {
                    continue;
                }
                let phi = v_unrot.atan2(u);
                let k_tooth = ((phi / s.disc.pitch_ang).round().rem_euclid(nteeth as f64)) as usize;
                let d = (phi + s.disc.pitch_ang / 2.0).rem_euclid(s.disc.pitch_ang)
                    - s.disc.pitch_ang / 2.0;
                if d.abs() <= s.disc.half_width_at(r) {
                    min_r[k_tooth] = min_r[k_tooth].min(r);
                    max_r[k_tooth] = max_r[k_tooth].max(r);
                } else {
                    in_groove_min[k_tooth] = in_groove_min[k_tooth].min(r);
                    in_groove_max[k_tooth] = in_groove_max[k_tooth].max(r);
                }
            }
            rho += h;
        }
        u += h;
    }
    println!(
        "tooth zone: root {:.3} addendum {:.3}; ring sampled at h = {h}",
        s.disc.root_r, s.disc.add_r
    );
    for k in 0..nteeth {
        let deg = (k as f64 * s.disc.pitch_ang).to_degrees();
        let tooth = if min_r[k].is_finite() {
            format!("IN TOOTH r ∈ [{:.3}, {:.3}]", min_r[k], max_r[k])
        } else {
            "no tooth material".to_string()
        };
        let groove = if in_groove_min[k].is_finite() {
            format!(
                "in groove r ∈ [{:.3}, {:.3}]",
                in_groove_min[k], in_groove_max[k]
            )
        } else {
            "no groove space".to_string()
        };
        println!("tooth {:>2} @ {:>6.2}°: ring {tooth}; {groove}", k, deg);
    }
}
