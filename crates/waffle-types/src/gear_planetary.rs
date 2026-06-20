//! Planetary (epicyclic) gear stage generator.
//!
//! Places a **sun**, **N planets**, and a **ring** gear in one sketch — all
//! sharing module `m` and pressure angle `α` (a meshing requirement) — so the
//! stage is directly extrudable. Builds on [`crate::gear`] (`GearParams` +
//! `generate_gear_profile`); the ring uses `internal: true`.
//!
//! See `docs/planetary_gear_generator_plan.md` for the theory. The pure core
//! here validates the tooth-count / assembly / non-interference constraints,
//! computes each positioned `GearParams` (with the phasing that makes the
//! planets mesh tooth-to-space against the sun), and either blocks with hints
//! or auto-adjusts. The meshing oracle in the test module is the correctness
//! gate for the phasing formula.

use std::f64::consts::{PI, TAU};

use serde::{Deserialize, Serialize};

use crate::gear::GearParams;

/// Minimum sane tooth count for any gear in the stage. Below this, involute
/// undercutting/profile-degeneration makes the gear meaningless.
pub const MIN_TEETH: u32 = 6;

/// Maximum planet count we will consider (placement gets pathological beyond).
pub const MAX_PLANETS: u32 = 12;

/// Parameters for a planetary gear stage.
///
/// `module` and `backlash` are in internal units (meters); the dialog converts
/// from the document display unit before calling. The ring tooth count is
/// derived (`Z_r = Z_s + 2·Z_p`), never a free input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetaryParams {
    pub module: f64,
    #[serde(default = "default_pressure_angle")]
    pub pressure_angle_deg: f64,
    pub sun_teeth: u32,
    pub planet_teeth: u32,
    pub planet_count: u32,
    #[serde(default)]
    pub backlash: f64,
    /// Stage center X (internal units). The sun and ring sit here; planet `k`
    /// sits at `(center_x, center_y) + R_c·(cos ψ_k, sin ψ_k)`. The dialog
    /// seeds this from the click location (already in internal sketch coords).
    #[serde(default)]
    pub center_x: f64,
    /// Stage center Y (internal units). See [`Self::center_x`].
    #[serde(default)]
    pub center_y: f64,
    /// When `true`, snap an invalid planet count to the nearest valid divisor
    /// (and clamp to a non-interfering value) and proceed. When `false`, an
    /// invalid stage is reported via `hints`/error and the caller blocks.
    #[serde(default)]
    pub auto_adjust: bool,
}

fn default_pressure_angle() -> f64 {
    20.0
}

impl Default for PlanetaryParams {
    fn default() -> Self {
        Self {
            module: 0.002,
            pressure_angle_deg: 20.0,
            sun_teeth: 24,
            planet_teeth: 16,
            planet_count: 3,
            backlash: 0.0,
            center_x: 0.0,
            center_y: 0.0,
            auto_adjust: false,
        }
    }
}

/// Result of generating a planetary stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanetaryResult {
    /// The positioned gears: sun first, then `planet_count` planets, then the
    /// ring last.
    pub gears: Vec<GearParams>,
    /// The center of the sun and each planet, in world (sketch) coordinates —
    /// sun first, then `planet_count` planets, in the same order as the
    /// corresponding gears. The ring shares the sun center, so it is NOT
    /// repeated. Used by the UI to drop a sketch `Point` at each gear center.
    pub centers: Vec<(f64, f64)>,
    /// Derived ring tooth count `Z_r = Z_s + 2·Z_p`.
    pub ring_teeth: u32,
    /// Carrier radius `R_c = (Z_s + Z_p)·m/2` (sun-planet center distance).
    pub carrier_radius: f64,
    /// Human-readable validation/advice messages (empty when nothing to note).
    pub hints: Vec<String>,
    /// When `auto_adjust` snapped the input, the adjusted params actually used.
    pub adjusted: Option<PlanetaryParams>,
}

/// Why a planetary stage cannot be generated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanetaryError {
    /// A tooth count is below [`MIN_TEETH`].
    ToothCountTooLow { which: String, value: u32, min: u32 },
    /// `module` is not positive.
    NonPositiveModule,
    /// `planet_count` is out of `[1, MAX_PLANETS]`.
    PlanetCountOutOfRange { value: u32, max: u32 },
    /// Hint mode (`auto_adjust=false`) and the stage is invalid; `hints`
    /// describes what to change. Never produces a silent bad sketch.
    Invalid { hints: Vec<String> },
}

impl std::fmt::Display for PlanetaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanetaryError::ToothCountTooLow { which, value, min } => write!(
                f,
                "{which} tooth count {value} is below the minimum of {min}"
            ),
            PlanetaryError::NonPositiveModule => write!(f, "module must be positive"),
            PlanetaryError::PlanetCountOutOfRange { value, max } => {
                write!(f, "planet count {value} must be between 1 and {max}")
            }
            PlanetaryError::Invalid { hints } => {
                write!(f, "invalid planetary stage: {}", hints.join("; "))
            }
        }
    }
}

impl std::error::Error for PlanetaryError {}

/// Derived ring tooth count for an equal-module standard planetary set.
pub fn ring_teeth(sun_teeth: u32, planet_teeth: u32) -> u32 {
    sun_teeth + 2 * planet_teeth
}

/// Carrier radius (sun-planet center distance): `R_c = (Z_s + Z_p)·m/2`.
pub fn carrier_radius(sun_teeth: u32, planet_teeth: u32, module: f64) -> f64 {
    (sun_teeth + planet_teeth) as f64 * module / 2.0
}

/// Divisors of `n` that fall in `[1, MAX_PLANETS]` — the valid planet counts
/// for the assembly condition `(Z_s + Z_r) % N == 0`.
fn valid_planet_counts(sum: u32) -> Vec<u32> {
    (1..=MAX_PLANETS)
        .filter(|&d| sum.is_multiple_of(d))
        .collect()
}

/// The reusable building block: mesh an external `planet_teeth` gear against a
/// sun (centered at the origin, `rotation_offset = 0`) at carrier angle
/// `carrier_angle`. Returns the positioned planet `GearParams`.
///
/// This is the named primitive the plan calls `mesh_external`; a future
/// "two meshed spur gears" element reuses it directly. It computes the
/// sun-planet center distance and the conjugate phasing so the planet presents
/// a tooth-space toward the sun at the pitch point.
///
/// Phasing derivation (all gears share module + α):
/// - sun phase toward the planet: `frac_s = (Z_s · ψ / 2π) mod 1`
///   (0 = tooth center pointing along the carrier ray, 0.5 = space center).
/// - the planet must present a space where the sun presents a tooth, so its
///   own phase toward the sun is `frac_s + 0.5` of a pitch.
/// - the planet's local angle toward the sun is `ψ + π` (pointing back at the
///   origin). A planet tooth `i` sits at `rotation_offset + i·(2π/Z_p)`, so the
///   phase of the planet surface at angle `θ` is `(Z_p·(θ − rotation_offset)/2π)`.
///   Setting that (at `θ = ψ + π`) equal to `frac_s + 0.5` and solving:
///
///   `rotation_offset = (ψ + π) − (2π/Z_p)·(frac_s + 0.5)`.
///
/// The `mod 2π/Z_p` is irrelevant to geometry (teeth are periodic) but kept
/// implicitly by the trig. The meshing oracle proves this sign/offset is right.
pub fn mesh_external(
    sun_teeth: u32,
    module: f64,
    pressure_angle_deg: f64,
    backlash: f64,
    planet_teeth: u32,
    carrier_angle: f64,
    center: (f64, f64),
) -> GearParams {
    let rc = carrier_radius(sun_teeth, planet_teeth, module);
    let zs = sun_teeth as f64;
    let zp = planet_teeth as f64;
    let psi = carrier_angle;

    // Sun phase toward the planet (fraction of an angular pitch).
    let frac_s = (zs * psi / TAU).rem_euclid(1.0);
    // Planet must show a space (tooth↔space) ⇒ +0.5 pitch.
    let target = frac_s + 0.5;
    // Local angle on the planet pointing back at the sun is ψ + π.
    let rotation_offset = (psi + PI) - (TAU / zp) * target;

    GearParams {
        tooth_count: planet_teeth,
        module,
        pressure_angle_deg,
        backlash,
        // Planet sits on the carrier circle around the stage center. The phasing
        // is a pure rotation that does NOT depend on the translation, so meshing
        // is preserved when all gears are offset by the same `center`.
        center_x: center.0 + rc * psi.cos(),
        center_y: center.1 + rc * psi.sin(),
        rotation_offset,
        internal: false,
    }
}

/// Validate the basic per-gear sanity (tooth counts, module, planet count).
fn check_basics(p: &PlanetaryParams) -> Result<(), PlanetaryError> {
    // Reject non-positive AND NaN modules (NaN fails every comparison).
    if p.module <= 0.0 || p.module.is_nan() {
        return Err(PlanetaryError::NonPositiveModule);
    }
    if p.planet_count == 0 || p.planet_count > MAX_PLANETS {
        return Err(PlanetaryError::PlanetCountOutOfRange {
            value: p.planet_count,
            max: MAX_PLANETS,
        });
    }
    if p.sun_teeth < MIN_TEETH {
        return Err(PlanetaryError::ToothCountTooLow {
            which: "sun".to_string(),
            value: p.sun_teeth,
            min: MIN_TEETH,
        });
    }
    if p.planet_teeth < MIN_TEETH {
        return Err(PlanetaryError::ToothCountTooLow {
            which: "planet".to_string(),
            value: p.planet_teeth,
            min: MIN_TEETH,
        });
    }
    Ok(())
}

/// Collect assembly + non-interference hints for a (basics-valid) stage.
/// Returns the hints (empty ⇒ valid) plus a suggested valid planet count for
/// auto-adjust.
fn collect_hints(p: &PlanetaryParams) -> (Vec<String>, Option<u32>) {
    let zr = ring_teeth(p.sun_teeth, p.planet_teeth);
    let sum = p.sun_teeth + zr; // assembly: (Z_s + Z_r) divisible by N
    let rc = carrier_radius(p.sun_teeth, p.planet_teeth, p.module);
    let r_p = p.planet_teeth as f64 * p.module / 2.0;

    let mut hints = Vec::new();
    let valid_ns = valid_planet_counts(sum);

    // Assembly condition.
    let assembly_ok = sum.is_multiple_of(p.planet_count);
    if !assembly_ok {
        hints.push(format!(
            "For {} equally-spaced planets, (Z_s + Z_r) = {} must be divisible by N. \
             Valid planet counts: {:?}. Or change sun/planet teeth.",
            p.planet_count, sum, valid_ns
        ));
    }

    // Non-interference: adjacent planet tip circles must clear:
    //   r_p + module < R_c · sin(π/N).
    // Find the largest planet count that still clears (for the advisory).
    let clears = |n: u32| -> bool { r_p + p.module < rc * (PI / n as f64).sin() };
    let interference_ok = clears(p.planet_count);
    if !interference_ok {
        let max_n = (1..=p.planet_count).rev().find(|&n| clears(n)).unwrap_or(1);
        hints.push(format!(
            "{} planets of {} teeth collide (tip circles overlap); \
             reduce planet count to {} or fewer, or increase sun teeth.",
            p.planet_count, p.planet_teeth, max_n
        ));
    }

    // Suggest a valid planet count: nearest assembly divisor that also clears.
    let suggestion = if assembly_ok && interference_ok {
        None
    } else {
        valid_ns
            .iter()
            .copied()
            .filter(|&n| clears(n))
            .min_by_key(|&n| (n as i64 - p.planet_count as i64).abs())
    };

    (hints, suggestion)
}

/// Generate a planetary gear stage.
///
/// On success returns the positioned gears (sun, N planets, ring) plus the
/// derived radii. In hint mode (`auto_adjust = false`) an invalid stage returns
/// `Err(PlanetaryError::Invalid { hints })` — never a silent bad sketch. In
/// auto-adjust mode an invalid planet count is snapped to the nearest valid,
/// non-interfering divisor and recorded in `adjusted`.
pub fn generate_planetary(params: &PlanetaryParams) -> Result<PlanetaryResult, PlanetaryError> {
    check_basics(params)?;

    let (hints, suggestion) = collect_hints(params);

    // Resolve the effective params (possibly auto-adjusted).
    let (effective, adjusted) = if hints.is_empty() {
        (params.clone(), None)
    } else if params.auto_adjust {
        match suggestion {
            Some(n) => {
                let mut snapped = params.clone();
                snapped.planet_count = n;
                // Recheck — snapping N fixes both assembly and interference by
                // construction (we filtered on `clears`).
                let (rem, _) = collect_hints(&snapped);
                if !rem.is_empty() {
                    return Err(PlanetaryError::Invalid { hints: rem });
                }
                (snapped.clone(), Some(snapped))
            }
            None => {
                // No valid planet count exists for these tooth counts.
                return Err(PlanetaryError::Invalid { hints });
            }
        }
    } else {
        return Err(PlanetaryError::Invalid { hints });
    };

    let zr = ring_teeth(effective.sun_teeth, effective.planet_teeth);
    let rc = carrier_radius(
        effective.sun_teeth,
        effective.planet_teeth,
        effective.module,
    );
    // Each mesh gets B = B/2 (gear A) + B/2 (gear B); see plan §Backlash.
    let half_backlash = effective.backlash / 2.0;

    let (cx, cy) = (effective.center_x, effective.center_y);

    let mut gears = Vec::with_capacity(effective.planet_count as usize + 2);
    // Sun + each planet center (ring shares the sun center, so not repeated).
    let mut centers = Vec::with_capacity(effective.planet_count as usize + 1);

    // Sun: at the stage center, no rotation.
    gears.push(GearParams {
        tooth_count: effective.sun_teeth,
        module: effective.module,
        pressure_angle_deg: effective.pressure_angle_deg,
        backlash: half_backlash,
        center_x: cx,
        center_y: cy,
        rotation_offset: 0.0,
        internal: false,
    });
    centers.push((cx, cy));

    // Planets: equally spaced at ψ_k = 2π·k/N, meshed against the sun, all
    // offset by the stage center.
    for k in 0..effective.planet_count {
        let psi = TAU * k as f64 / effective.planet_count as f64;
        let planet = mesh_external(
            effective.sun_teeth,
            effective.module,
            effective.pressure_angle_deg,
            half_backlash,
            effective.planet_teeth,
            psi,
            (cx, cy),
        );
        centers.push((planet.center_x, planet.center_y));
        gears.push(planet);
    }

    // Ring: internal, at the stage center, no rotation. Conjugate consistency
    // with the sun is guaranteed by Z_r = Z_s + 2·Z_p + the assembly condition.
    gears.push(GearParams {
        tooth_count: zr,
        module: effective.module,
        pressure_angle_deg: effective.pressure_angle_deg,
        backlash: half_backlash,
        center_x: cx,
        center_y: cy,
        rotation_offset: 0.0,
        internal: true,
    });

    let mut out_hints = Vec::new();
    if let Some(adj) = &adjusted {
        out_hints.push(format!(
            "Auto-adjusted planet count to {} (nearest valid divisor of Z_s + Z_r).",
            adj.planet_count
        ));
    }

    Ok(PlanetaryResult {
        gears,
        centers,
        ring_teeth: zr,
        carrier_radius: rc,
        hints: out_hints,
        adjusted,
    })
}

/// Generate a lightweight live-preview for a planetary stage: one flat polyline
/// per positioned gear (sun, N planets, ring), each via
/// [`crate::gear::generate_gear_preview_polyline`]. Mirrors the single-gear
/// preview but returns a polyline-per-gear so the UI can draw the whole stage
/// as the user drags params / moves the placement center.
///
/// Returns an empty `Vec` when the params are invalid (so the UI simply clears
/// the preview) — it does not surface validation errors. In hint mode an
/// invalid planet count yields no preview; auto-adjust mode previews the
/// snapped stage.
pub fn generate_planetary_preview(params: &PlanetaryParams) -> Vec<Vec<(f64, f64)>> {
    match generate_planetary(params) {
        Ok(result) => result
            .gears
            .iter()
            .map(crate::gear::generate_gear_preview_polyline)
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gear::generate_gear_profile;

    fn valid(sun: u32, planet: u32, n: u32) -> PlanetaryParams {
        PlanetaryParams {
            module: 0.002,
            pressure_angle_deg: 20.0,
            sun_teeth: sun,
            planet_teeth: planet,
            planet_count: n,
            backlash: 0.0,
            center_x: 0.0,
            center_y: 0.0,
            auto_adjust: false,
        }
    }

    #[test]
    fn ring_teeth_formula() {
        assert_eq!(ring_teeth(24, 16), 56);
        assert_eq!(ring_teeth(20, 20), 60);
        assert_eq!(ring_teeth(30, 15), 60);
    }

    #[test]
    fn carrier_radius_formula() {
        // R_c = (Z_s + Z_p)·m/2
        assert!((carrier_radius(24, 16, 0.002) - 0.04).abs() < 1e-12);
    }

    #[test]
    fn assembly_condition_accepts_valid() {
        // 24/16 → Z_r=56, sum=80, divisible by 4 and 2, not by 3.
        let p = valid(24, 16, 4);
        let r = generate_planetary(&p).expect("4 planets valid for 24/16");
        assert_eq!(r.ring_teeth, 56);
        assert_eq!(r.gears.len(), 4 + 2);
    }

    #[test]
    fn assembly_condition_rejects_invalid_in_hint_mode() {
        // sum=80 not divisible by 3 → invalid; hint mode → loud error.
        let p = valid(24, 16, 3);
        let err = generate_planetary(&p).expect_err("3 planets invalid for 24/16, hint mode");
        match err {
            PlanetaryError::Invalid { hints } => {
                assert!(hints.iter().any(|h| h.contains("divisible")));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn auto_adjust_snaps_planet_count() {
        let mut p = valid(24, 16, 3);
        p.auto_adjust = true;
        let r = generate_planetary(&p).expect("auto-adjust should find a valid N");
        let adj = r.adjusted.expect("should record adjustment");
        // sum=80; valid Ns (≤12) are 1,2,4,5,8,10. Nearest to 3 = 2 or 4.
        assert!(
            [2, 4, 5].contains(&adj.planet_count),
            "got {}",
            adj.planet_count
        );
        assert_eq!(r.gears.len(), adj.planet_count as usize + 2);
    }

    #[test]
    fn tooth_count_too_low_errors() {
        let p = valid(4, 16, 3);
        assert!(matches!(
            generate_planetary(&p),
            Err(PlanetaryError::ToothCountTooLow { .. })
        ));
    }

    #[test]
    fn non_positive_module_errors() {
        let mut p = valid(24, 16, 4);
        p.module = 0.0;
        assert!(matches!(
            generate_planetary(&p),
            Err(PlanetaryError::NonPositiveModule)
        ));
    }

    #[test]
    fn too_many_planets_interfere() {
        // Many planets of a large planet gear → tip circles collide.
        // 12/30/N: r_p large vs R_c small. sum = Z_s+Z_r = 12 + 72 = 84.
        let p = valid(12, 30, 6); // 84 % 6 == 0, but big planets collide
        let err = generate_planetary(&p).expect_err("big planets should interfere");
        match err {
            PlanetaryError::Invalid { hints } => {
                assert!(hints.iter().any(|h| h.contains("collide")));
            }
            other => panic!("expected Invalid (collision), got {other:?}"),
        }
    }

    #[test]
    fn planet_centers_on_carrier_circle() {
        let p = valid(20, 20, 4);
        let r = generate_planetary(&p).unwrap();
        let rc = r.carrier_radius;
        // gears[0] = sun, gears[1..N+1] = planets, gears[last] = ring.
        for k in 0..4 {
            let g = &r.gears[1 + k];
            let d = (g.center_x * g.center_x + g.center_y * g.center_y).sqrt();
            assert!((d - rc).abs() < 1e-12, "planet {k} off carrier circle");
        }
        // sun and ring centered.
        assert!(r.gears[0].center_x.abs() < 1e-15 && r.gears[0].center_y.abs() < 1e-15);
        let ring = r.gears.last().unwrap();
        assert!(ring.internal && ring.center_x.abs() < 1e-15);
    }

    #[test]
    fn backlash_split_half_to_each_gear() {
        let mut p = valid(24, 16, 4);
        p.backlash = 0.001;
        let r = generate_planetary(&p).unwrap();
        for g in &r.gears {
            assert!((g.backlash - 0.0005).abs() < 1e-15);
        }
    }

    // ---- The meshing ORACLE (the correctness gate for the phasing formula) ----

    /// Sample a gear's boundary vertex loop in WORLD coordinates via the
    /// production profile generator.
    fn flank_points(g: &GearParams) -> Vec<(f64, f64)> {
        let prof = generate_gear_profile(g);
        let pos = &prof.positions;
        prof.profiles[0]
            .vertex_ids
            .iter()
            .filter_map(|id| pos.get(id).copied())
            .collect()
    }

    /// Minimum distance between two polylines' vertices (a coarse
    /// interpenetration probe).
    fn min_vertex_gap(a: &[(f64, f64)], b: &[(f64, f64)]) -> f64 {
        let mut best = f64::INFINITY;
        for &(ax, ay) in a {
            for &(bx, by) in b {
                let d = ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt();
                if d < best {
                    best = d;
                }
            }
        }
        best
    }

    /// Assert two gears mesh: (a) boundaries don't interpenetrate (positive
    /// gap), and (b) the gap is backlash-scale, not a half-pitch phase error.
    /// The boundary is a coarse 12-sample-per-flank polyline, so this is a
    /// floor/ceiling sanity check, not exact metrology — but it is enough to
    /// catch a wrong phasing sign or offset (tooth-on-tooth → ~0 gap; off by
    /// half a pitch → gap ≥ one angular-pitch arc).
    fn assert_mesh(a: &GearParams, b: &GearParams, label: &str) {
        let pa = flank_points(a);
        let pb = flank_points(b);
        assert!(!pa.is_empty() && !pb.is_empty(), "{label}: empty profiles");
        let gap = min_vertex_gap(&pa, &pb);
        // (a) no interpenetration: a correct mesh leaves a real gap.
        assert!(
            gap > 1e-9,
            "{label}: gears interpenetrate (min vertex gap {gap:.3e} ≤ 0). \
             Phasing formula is wrong — tooth hits tooth instead of space."
        );
        // (b) in-slot: gap must be below one angular-pitch arc of the smaller
        // gear; a half-pitch phase error would push a tooth into the next slot.
        let r_pitch_a = a.tooth_count as f64 * a.module / 2.0;
        let r_pitch_b = b.tooth_count as f64 * b.module / 2.0;
        let pitch_arc = TAU * r_pitch_a.min(r_pitch_b) / (a.tooth_count.min(b.tooth_count) as f64);
        assert!(
            gap < pitch_arc,
            "{label}: min gap {gap:.3e} ≥ one angular-pitch arc {pitch_arc:.3e} — \
             planet is phased into the wrong slot (off by ~half a pitch)."
        );
    }

    /// Build a stage and assert every sun-planet and planet-ring mesh meshes.
    fn run_oracle(sun: u32, planet: u32, n: u32, backlash: f64) {
        let mut p = valid(sun, planet, n);
        p.backlash = backlash;
        let r = generate_planetary(&p)
            .unwrap_or_else(|e| panic!("oracle combo {sun}/{planet}/{n} should be valid: {e}"));
        let sun_g = &r.gears[0];
        let ring_g = r.gears.last().unwrap();
        for k in 0..n as usize {
            let planet_g = &r.gears[1 + k];
            assert_mesh(
                sun_g,
                planet_g,
                &format!("{sun}/{planet}/{n} sun-planet{k}"),
            );
            assert_mesh(
                planet_g,
                ring_g,
                &format!("{sun}/{planet}/{n} planet{k}-ring"),
            );
        }
    }

    #[test]
    fn meshing_oracle_24_16_3_via_autoadjust() {
        // 24/16/3 is assembly-invalid; auto-adjust to a valid N, then mesh-check.
        let mut p = valid(24, 16, 3);
        p.auto_adjust = true;
        let r = generate_planetary(&p).unwrap();
        let n = r.adjusted.unwrap().planet_count;
        run_oracle(24, 16, n, 0.0);
    }

    #[test]
    fn meshing_oracle_20_20_4() {
        run_oracle(20, 20, 4, 0.0);
    }

    #[test]
    fn meshing_oracle_30_15_3() {
        // Z_r = 60, sum = 90, divisible by 3. Valid.
        run_oracle(30, 15, 3, 0.0);
    }

    #[test]
    fn meshing_oracle_with_backlash() {
        // Backlash must keep the mesh valid (gap grows but stays in-slot).
        run_oracle(20, 20, 4, 0.001);
    }

    #[test]
    fn backlash_widens_the_mesh_gap() {
        // (b) Backlash correctness: more backlash ⇒ a larger pitch-line gap.
        let mk = |b: f64| {
            let mut p = valid(20, 20, 4);
            p.backlash = b;
            let r = generate_planetary(&p).unwrap();
            let sun = r.gears[0].clone();
            let planet = r.gears[1].clone();
            let pa = flank_points(&sun);
            let pb = flank_points(&planet);
            min_vertex_gap(&pa, &pb)
        };
        let g0 = mk(0.0);
        let g1 = mk(0.002);
        assert!(
            g1 > g0,
            "backlash should widen the mesh gap: g(0)={g0:.4e}, g(B)={g1:.4e}"
        );
    }

    // ---- Center placement ----

    #[test]
    fn nonzero_center_offsets_every_gear_and_emits_centers() {
        let base = generate_planetary(&valid(20, 20, 4)).unwrap();

        let mut p = valid(20, 20, 4);
        p.center_x = 0.123;
        p.center_y = -0.456;
        let off = generate_planetary(&p).unwrap();

        // Same gear count and identical relative layout: each gear's center is
        // exactly shifted by (cx, cy); rotation/teeth/internal unchanged.
        assert_eq!(off.gears.len(), base.gears.len());
        for (g, b) in off.gears.iter().zip(base.gears.iter()) {
            assert!((g.center_x - (b.center_x + 0.123)).abs() < 1e-12);
            assert!((g.center_y - (b.center_y - 0.456)).abs() < 1e-12);
            assert_eq!(g.tooth_count, b.tooth_count);
            assert_eq!(g.internal, b.internal);
            assert!((g.rotation_offset - b.rotation_offset).abs() < 1e-15);
        }

        // Center list: sun + N planets (N+1), ring NOT repeated.
        assert_eq!(off.centers.len(), p.planet_count as usize + 1);
        // centers[0] is the sun center == (cx, cy).
        assert!((off.centers[0].0 - 0.123).abs() < 1e-12);
        assert!((off.centers[0].1 - (-0.456)).abs() < 1e-12);
        // Each subsequent center matches its planet gear's center.
        for (k, c) in off.centers.iter().enumerate().skip(1) {
            let g = &off.gears[k]; // gears[1..=N] are the planets
            assert!((c.0 - g.center_x).abs() < 1e-12 && (c.1 - g.center_y).abs() < 1e-12);
        }
    }

    #[test]
    fn offset_stage_still_meshes() {
        // Offsetting every gear by the same translation must NOT change meshing
        // (the meshes are relative). Run the oracle on an offset stage.
        let mut p = valid(20, 20, 4);
        p.center_x = 0.5;
        p.center_y = -0.25;
        let r = generate_planetary(&p).unwrap();
        let sun_g = &r.gears[0];
        let ring_g = r.gears.last().unwrap();
        for k in 0..p.planet_count as usize {
            let planet_g = &r.gears[1 + k];
            assert_mesh(sun_g, planet_g, &format!("offset sun-planet{k}"));
            assert_mesh(planet_g, ring_g, &format!("offset planet{k}-ring"));
        }
    }

    // ---- Preview ----

    #[test]
    fn preview_returns_polyline_per_gear() {
        let r = generate_planetary_preview(&valid(24, 16, 4));
        // N + 2 gears (sun + 4 planets + ring).
        assert_eq!(r.len(), 4 + 2);
        for poly in &r {
            assert!(poly.len() > 2, "each preview polyline should be non-empty");
        }
    }

    #[test]
    fn preview_reflects_center_offset() {
        let p0 = valid(24, 16, 4);
        let mut p1 = p0.clone();
        p1.center_x = 1.0;
        p1.center_y = 2.0;
        let a = generate_planetary_preview(&p0);
        let b = generate_planetary_preview(&p1);
        assert_eq!(a.len(), b.len());
        // The sun preview (gear 0) first point shifts by exactly (1, 2).
        assert!((b[0][0].0 - (a[0][0].0 + 1.0)).abs() < 1e-9);
        assert!((b[0][0].1 - (a[0][0].1 + 2.0)).abs() < 1e-9);
    }

    #[test]
    fn preview_empty_on_invalid() {
        // 24/16/3 is assembly-invalid in hint mode → no preview.
        let r = generate_planetary_preview(&valid(24, 16, 3));
        assert!(r.is_empty());
    }
}
