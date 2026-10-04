//! Adversarial review of the UNITS half of increment M1
//! (`specs/drawings_and_mbd.md` §9 + its "Implementation notes (M1)").
//!
//! Every claim M1's notes make about units is MEASURED here rather than
//! reasoned about, and each test is written to fail on the plausible wrong
//! implementation rather than merely to exercise the right one:
//!
//! 1. the four mass suffixes and their factors, against the international
//!    avoirdupois definition (`1 lb = 0.45359237 kg` EXACTLY) — a rounded
//!    `0.4536` is 17 ppm light, 7.6 g on a 1000 lb weldment, so the bits are
//!    compared and not an epsilon;
//! 2. `mass(body)` carries a MASS and is refused next to a length by a typed
//!    error naming BOTH sides;
//! 3. the exponent arithmetic is general — mass/mass is dimensionless,
//!    mass·mass is a `mass^2` no boundary accepts, density·volume IS a mass —
//!    and both new named boundaries refuse the wrong dimension BY NAME
//!    instead of returning a number;
//! 4. the named-boundary discipline: the magnitude a caller reads is the one
//!    the accessor's NAME promises, on both `Quantity` and `ToleranceValue`;
//! 5. the move down a crate: ONE `Dimension` type (proved by type identity,
//!    not by spelling) and ONE serialized spelling, in the same style as the
//!    pre-M1 variants.
//!
//! Plus the degrees-vs-radians trap M1's notes single out: a tolerance
//! magnitude is RADIANS (matching `annotation::measure`) while every
//! feature-tree angle is DEGREES. A stray `to_radians`/`to_degrees` anywhere
//! on that path is a factor of 57.3, and `a_factor_of_180_over_pi_…` below is
//! sized to catch it at every hop.

use std::collections::HashMap;

use feature_engine::expr::{
    self, evaluate_measured, is_reserved_word, unit_by_name, untagged_env, Dim, Dimension,
    ExprError, MeasureCall, MeasureRefusal, Measurer, Quantity,
};
use feature_engine::params::apply_parameters;
use feature_engine::types::{DesignParameter, FeatureTree};
use waffle_types::annotation::layout::ToleranceLayout;
use waffle_types::annotation::tolerance::{Tolerance, ToleranceValue};
use waffle_types::annotation::DimensionKind;

// ---------------------------------------------------------------- helpers

/// A measurer with no kernel behind it: every function answers a fixed
/// working-space number, which is all these tests need (the dimension is
/// attached by the evaluator from `MEASUREMENTS`, not by the measurer).
struct Fixed;

impl Measurer for Fixed {
    fn measure(&self, call: &MeasureCall<'_>) -> Result<f64, MeasureRefusal> {
        match call.function {
            // 2.7 g — a 10 mm aluminium cube. KILOGRAMS: the working space
            // for a mass, per `expr::dim::Quantity::as_mass_kilograms`.
            "mass" => Ok(0.0027),
            // 1000 mm³ — the same cube, in the mm-space of a length axis.
            "volume" => Ok(1000.0),
            "area" => Ok(100.0),
            _ => Ok(10.0),
        }
    }
}

fn q(src: &str) -> Result<Quantity, ExprError> {
    let empty: HashMap<String, f64> = HashMap::new();
    evaluate_measured(src, &untagged_env(&empty), &Fixed)
}

fn ok(src: &str) -> Quantity {
    q(src).unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// The two sides a `DimensionMismatch` names, or a panic saying what came
/// back instead — a test that accepted a number here would be the defect.
fn mismatch(got: Result<f64, ExprError>, what: &str) -> (String, String) {
    match got {
        Err(ExprError::DimensionMismatch {
            expected, found, ..
        }) => (expected, found),
        Ok(v) => panic!("{what} returned {v} instead of refusing"),
        Err(other) => panic!("{what} refused with {other:?}, not a DimensionMismatch"),
    }
}

// ------------------------------------------------- claim 1: the suffixes

/// The four mass suffixes, and their factors against the DEFINITIONS rather
/// than against themselves.
///
/// `1 lb = 0.45359237 kg` is exact by the 1959 international yard-and-pound
/// agreement, and `1 oz = 1/16 lb` exactly. Dividing by 16 is exact in binary
/// floating point, so the two literals must be BIT-identical to `lb` and
/// `lb / 16` — an epsilon comparison would pass a `0.4536` that is 17 ppm
/// light on every quoted part mass, which is the defect this test exists to
/// refuse.
#[test]
fn rev_units_the_mass_suffix_factors_are_the_exact_avoirdupois_definitions() {
    let f = |name: &str| {
        unit_by_name(name)
            .unwrap_or_else(|| panic!("no `{name}` unit"))
            .factor
    };

    // SI: kg is the working space, so it is the identity; a gram is 1/1000.
    assert_eq!(f("kg"), 1.0);
    assert_eq!(f("g"), 1e-3);

    // Avoirdupois, to the bit.
    let lb = f("lb");
    let oz = f("oz");
    assert_eq!(lb, 0.45359237_f64, "1 lb is 0.45359237 kg EXACTLY");
    assert_eq!(oz, 0.45359237_f64 / 16.0, "1 oz is 1/16 lb EXACTLY");
    assert_eq!(oz, 0.028349523125_f64, "and that is 0.028349523125 kg");
    assert_eq!(16.0 * oz, lb, "16 oz is a pound, to the bit");

    // The rounded forms a careless transcription produces are NOT what is
    // here; the drift one of them costs is measured just below.
    for rounded in [0.4536_f64, 0.454, 0.45, 0.45359] {
        assert_ne!(lb, rounded, "a rounded pound would be a wrong part mass");
    }
    // Measured, not asserted from memory: the `0.4536` transcription is
    // 17 ppm light, which is 7.6 g on a 1000 lb (453.6 kg) weldment and
    // 0.17 g on a 10 kg part — small, and on the wrong side of a quoted
    // mass either way.
    let drift = (1000.0 * lb - 1000.0 * 0.4536_f64).abs();
    assert!(
        (0.0076..0.0077).contains(&drift),
        "the 0.4536 transcription is {drift} kg light over 1000 lb"
    );

    // Every one of them commits a MASS, and nothing else does.
    for name in ["kg", "g", "lb", "oz"] {
        assert_eq!(unit_by_name(name).unwrap().dim, Dim::MASS, "{name}");
    }
    for name in ["mm", "cm", "m", "in", "ft"] {
        assert_eq!(unit_by_name(name).unwrap().dim, Dim::LENGTH, "{name}");
    }

    // ...and the factors reach the evaluator, not just the table.
    assert_eq!(ok("1kg").as_mass_kilograms().unwrap(), 1.0);
    assert_eq!(ok("250g").as_mass_kilograms().unwrap(), 0.25);
    assert_eq!(ok("1lb").as_mass_kilograms().unwrap(), 0.45359237);
    assert_eq!(ok("16oz").as_mass_kilograms().unwrap(), 0.45359237);
    assert_eq!(ok("1lb + 16oz").as_mass_kilograms().unwrap(), 0.90718474);
}

/// `t` is deliberately NOT a tonne, and that is measurable in three places:
/// the table, the reserved-word set, and the parser.
#[test]
fn rev_units_the_tonne_is_not_a_unit_and_t_stays_a_parameter_name() {
    assert!(unit_by_name("t").is_none(), "`t` must not be a unit");
    assert!(!is_reserved_word("t"));
    assert!(expr::validate_name("t").is_ok());
    for spelling in ["tonne", "ton", "T", "mg", "ug", "st", "cwt"] {
        assert!(unit_by_name(spelling).is_none(), "{spelling}");
    }

    // A number followed by `t` is a loud parse error, not a silent tonne and
    // not a silent multiplication.
    let err = q("1t").unwrap_err();
    let text = err.to_string();
    assert!(text.contains("not a unit"), "{text}");

    // And `t` keeps working as the thickness parameter the notes protect.
    let vars: HashMap<String, f64> = [("t".to_string(), 3.0)].into_iter().collect();
    let got = expr::evaluate("2 * t", &vars).unwrap();
    assert_eq!(got, 6.0);
}

/// A mass suffix is a reserved word, and the refusal of a parameter spelling
/// one is LOUD at every boundary — neither the unit nor the parameter wins
/// quietly.
#[test]
fn rev_units_a_mass_suffix_cannot_be_a_parameter_name_and_the_refusal_is_loud() {
    for name in ["kg", "g", "lb", "oz"] {
        assert!(is_reserved_word(name), "{name} must be reserved");
        let why = expr::validate_name(name).unwrap_err();
        assert!(why.contains("reserved"), "{name}: {why}");
    }

    // An environment that HAS a `g` does not make `g` a value: the parser
    // decides it, before any environment is consulted, so there is no
    // silent shadowing in either direction.
    let vars: HashMap<String, f64> = [("g".to_string(), 9.81)].into_iter().collect();
    let err = expr::evaluate("g * 2", &vars).unwrap_err();
    let text = err.to_string();
    assert!(
        text.contains("cannot be used as a value"),
        "a parameter named `g` must not quietly win: {text}"
    );
    // ...and it is not read as 1 gram either.
    assert!(expr::evaluate("g * 2", &vars).is_err());

    // The parameter TABLE refuses the row by name rather than carrying it.
    let mut tree = FeatureTree::new();
    tree.parameters.push(DesignParameter::new("g", "9.81"));
    tree.parameters.push(DesignParameter::new("ok_name", "5"));
    let outcome = apply_parameters(&mut tree);
    let row = &tree.parameters[0];
    let why = row
        .error
        .as_deref()
        .expect("a parameter named `g` must carry an error");
    assert!(why.contains("reserved"), "{why}");
    assert!(
        outcome.errors.iter().any(|(_, m)| m.contains("reserved")),
        "and the outcome must report it: {:?}",
        outcome.errors
    );
    assert!(tree.parameters[1].error.is_none(), "the sound row survives");
}

// --------------------------------------------- claim 2: mass(body)'s kind

/// `mass(body)` yields a MASS, and adding it to a length is a typed
/// dimension mismatch that names BOTH sides — not a coerced number.
#[test]
fn rev_units_mass_of_a_body_is_a_mass_and_refuses_to_be_added_to_a_length() {
    let m = ok("mass(cube)");
    assert_eq!(m.dim(), Dim::MASS);
    assert_eq!(m.dimension(), Some(Dimension::Mass));
    assert_eq!(m.dimension_label(), "mass");
    assert_eq!(m.as_mass_kilograms().unwrap(), 0.0027);

    // The headline: a mass next to a length is refused, naming both.
    let (expected, found) = match q("mass(cube) + 10mm") {
        Err(ExprError::DimensionMismatch {
            expected, found, ..
        }) => (expected, found),
        Ok(v) => panic!("a mass was added to a length, giving {}", v.value),
        Err(other) => panic!("refused with {other:?}, not a DimensionMismatch"),
    };
    assert_eq!((expected.as_str(), found.as_str()), ("a mass", "a length"));

    // The message a human reads names both sides too, so the typed error is
    // not the only place the information exists.
    let text = q("mass(cube) + 10mm").unwrap_err().to_string();
    assert!(text.contains("mass") && text.contains("length"), "{text}");

    // Both orders, and subtraction, min/max and `%` — every operator that
    // unifies rather than composes.
    for src in [
        "10mm + mass(cube)",
        "mass(cube) - 10mm",
        "min(mass(cube), 10mm)",
        "max(10mm, mass(cube))",
        "mass(cube) % 10mm",
        "mass(cube) + 1deg",
    ] {
        assert!(
            matches!(q(src), Err(ExprError::DimensionMismatch { .. })),
            "{src} must be refused"
        );
    }

    // ...and a mass reaching a length, angle, count or ratio FIELD is
    // refused by name, which is the other half of the boundary.
    assert_eq!(
        mismatch(m.as_length_meters(), "a mass as a length"),
        ("a length".to_string(), "a mass".to_string())
    );
    assert_eq!(
        mismatch(m.as_angle_degrees(), "a mass as an angle"),
        ("an angle".to_string(), "a mass".to_string())
    );
    assert_eq!(
        mismatch(m.as_angle_radians(), "a mass as an angle"),
        ("an angle".to_string(), "a mass".to_string())
    );
    assert_eq!(
        mismatch(m.as_ratio(), "a mass as a ratio"),
        ("a ratio (a plain number)".to_string(), "a mass".to_string())
    );
    assert_eq!(
        mismatch(m.as_count(), "a mass as a count"),
        ("a count (a plain number)".to_string(), "a mass".to_string())
    );

    // A mass plus a mass is a mass; a bare number still adopts.
    assert_eq!(ok("mass(cube) + 1kg").as_mass_kilograms().unwrap(), 1.0027);
    assert_eq!(ok("mass(cube) * 2").as_mass_kilograms().unwrap(), 0.0054);
}

// ------------------------------ claim 3: the arithmetic is general

/// The mass axis is an EXPONENT, so every combination follows from the
/// arithmetic — no special case computes a density, and nothing special-cases
/// a mass back out of one.
#[test]
fn rev_units_the_mass_exponent_arithmetic_is_general_not_special_cased() {
    // mass / mass is COMMITTED dimensionless — a ratio, not a plain number,
    // so it is refused as a length exactly like `25deg / 1deg` is.
    let r = ok("mass(cube) / mass(cube)");
    assert_eq!(r.dim(), Dim::NONE);
    assert_eq!(r.dimension(), Some(Dimension::Ratio));
    assert_eq!(r.as_ratio().unwrap(), 1.0);
    assert!(r.as_length_meters().is_err());
    assert!(r.as_mass_kilograms().is_err());

    // mass * mass is representable (the exponents allow it) and nameable,
    // but no boundary accepts it: that is what "refused where it should be"
    // means for a composite.
    let m2 = ok("mass(cube) * mass(cube)");
    assert_eq!(m2.dim(), Dim::MASS.scaled(2).unwrap());
    assert_eq!(m2.dim().mass, 2);
    assert_eq!(m2.dimension(), None);
    assert_eq!(m2.dimension_label(), "mass^2");
    assert_eq!(
        mismatch(m2.as_mass_kilograms(), "a mass^2 as a mass"),
        ("a mass".to_string(), "mass^2".to_string())
    );
    assert_eq!(ok("mass(cube) ^ 2").dim(), m2.dim());
    // ...and sqrt walks it back, because halving an even exponent is legal.
    assert_eq!(ok("sqrt(mass(cube) * mass(cube))").dim(), Dim::MASS);
    // A square root of a mass is not a dimension this system can name.
    assert!(q("sqrt(mass(cube))").is_err());
    // mass^-1 and mass^3 are representable too; the algebra is not a flag.
    assert_eq!(ok("1 / mass(cube)").dim().mass, -1);
    assert_eq!(ok("mass(cube) ^ 3").dim().mass, 3);

    // A DENSITY is composed, never a suffix: there is no `kg/m^3` token.
    assert!(unit_by_name("kg/m^3").is_none());
    let rho = ok("2.7g / 1cm^3");
    assert_eq!(rho.dim(), Dim::DENSITY);
    assert_eq!(
        rho.dim(),
        Dim {
            length: -3,
            angle: 0,
            mass: 1
        }
    );
    assert_eq!(rho.dimension(), Some(Dimension::Density));
    let kg_m3 = rho.as_density_kg_m3().unwrap();
    assert!((kg_m3 - 2700.0).abs() < 1e-9, "{kg_m3}");
    // The same density spelled three other ways reads the same number.
    for src in ["2700kg / 1m^3", "2700000g / 1m^3", "0.0027g / 1mm^3"] {
        let same = ok(src).as_density_kg_m3().unwrap();
        assert!((same - 2700.0).abs() < 1e-6, "{src} gave {same}");
    }

    // density * volume IS a mass, by the exponents AND by the number: a
    // 10 mm aluminium cube is 2.7 g.
    let cube = ok("(2.7g / 1cm^3) * (10mm * 10mm * 10mm)");
    assert_eq!(cube.dim(), Dim::MASS);
    assert!((cube.as_mass_kilograms().unwrap() - 0.0027).abs() < 1e-15);
    // ...and through the MEASURED volume, which is the real path.
    let measured = ok("(2.7g / 1cm^3) * volume(cube)");
    assert_eq!(measured.dim(), Dim::MASS);
    assert!(
        (measured.as_mass_kilograms().unwrap() - 0.0027).abs() < 1e-15,
        "{}",
        measured.value
    );
    // mass / volume is a density again.
    assert_eq!(ok("mass(cube) / volume(cube)").dim(), Dim::DENSITY);
    let back = ok("mass(cube) / volume(cube)").as_density_kg_m3().unwrap();
    assert!((back - 2700.0).abs() < 1e-9, "{back}");

    // Both new boundaries refuse the wrong dimension BY NAME rather than
    // returning a number — in both directions and from a length too.
    assert_eq!(
        mismatch(rho.as_mass_kilograms(), "a density as a mass"),
        ("a mass".to_string(), "a density".to_string())
    );
    assert_eq!(
        mismatch(ok("1kg").as_density_kg_m3(), "a mass as a density"),
        ("a density".to_string(), "a mass".to_string())
    );
    assert_eq!(
        mismatch(ok("1mm").as_mass_kilograms(), "a length as a mass"),
        ("a mass".to_string(), "a length".to_string())
    );
    assert_eq!(
        mismatch(ok("1deg").as_density_kg_m3(), "an angle as a density"),
        ("a density".to_string(), "an angle".to_string())
    );
    // A length^-3 is NOT a density: the mass exponent has to be there.
    let inv_vol = ok("1 / (1cm * 1cm * 1cm)");
    assert_eq!(inv_vol.dim().mass, 0);
    assert!(inv_vol.as_density_kg_m3().is_err());
}

/// REGRESSION PIN (M1 review defect): a bare number cannot be a DENSITY.
///
/// Measured before the fix: a `DesignParameter` with `unit: Density` and
/// expression `"2700"` — which is what a drafter or an agent writes, because
/// the material table, the `material_set` tool and the `Dimension::Density`
/// schema all quote density as `kg/m³` — evaluated with **no error** and read
/// back through `as_density_kg_m3()` as **2 700 000 000 000**. The working
/// space for a density is kg/mm³ (mass axis kg, length axis mm, which is what
/// makes `density × volume` a mass), so a bare 2700 silently meant 2700
/// kg/mm³: a factor of 10⁹, with the right name and the right printed unit
/// beside it.
///
/// Every OTHER dimension's working space is a unit a person authors in, so
/// adopting is right there — and this test pins that asymmetry from both
/// sides, so the carve-out cannot spread to `Mass` and cannot be removed from
/// `Density`.
#[test]
fn rev_units_a_bare_number_cannot_adopt_a_density_but_can_adopt_a_mass() {
    use feature_engine::expr::Span;

    // The authored shape: a table row declaring a density as a bare number.
    let mut tree = FeatureTree::new();
    tree.parameters
        .push(DesignParameter::new("rho", "2700").with_unit(Dimension::Density));
    tree.parameters
        .push(DesignParameter::new("rho_ok", "2700kg / 1m^3").with_unit(Dimension::Density));
    tree.parameters
        .push(DesignParameter::new("part_kg", "2.5").with_unit(Dimension::Mass));
    tree.parameters
        .push(DesignParameter::new("w", "25").with_unit(Dimension::Length));
    let outcome = apply_parameters(&mut tree);

    let bare = &tree.parameters[0];
    let why = bare
        .error
        .as_deref()
        .expect("a bare `2700` declared a density must be refused, not accepted as 2.7e12");
    assert!(
        why.contains("density") && why.contains("kg/mm") && why.contains("2700kg / 1m^3"),
        "the refusal must name the spelling that works: {why}"
    );
    assert!(
        outcome.errors.iter().any(|(id, _)| *id == bare.id),
        "and the outcome must carry it: {:?}",
        outcome.errors
    );

    // The composed spelling is accepted, and reads 2700 kg/m³.
    let env = feature_engine::params::cached_env(&tree.parameters);
    let good = env.get("rho_ok").copied().expect("rho_ok in the env");
    assert_eq!(good.dim(), Dim::DENSITY);
    let kg_m3 = good.as_density_kg_m3().unwrap();
    assert!((kg_m3 - 2700.0).abs() < 1e-9, "{kg_m3}");
    assert!(tree.parameters[1].error.is_none());

    // The asymmetry: a bare MASS and a bare LENGTH still adopt, because kg
    // and mm ARE the units their authors write.
    assert!(
        tree.parameters[2].error.is_none(),
        "{:?}",
        tree.parameters[2].error
    );
    assert!(
        tree.parameters[3].error.is_none(),
        "{:?}",
        tree.parameters[3].error
    );
    let kg = env.get("part_kg").copied().expect("part_kg in the env");
    assert_eq!(kg.as_mass_kilograms().unwrap(), 2.5);
    let w = env.get("w").copied().expect("w in the env");
    assert_eq!(w.as_length_meters().unwrap(), 0.025);

    // Directly on the boundary, for both entry points the bridge uses:
    // `retag` (a declared unit, an instance override) and `check` (the
    // `expression_evaluate` preview's `dimension` argument).
    let untagged = Quantity::untagged(2700.0);
    assert!(untagged.retag(Dimension::Density, Span::new(0, 4)).is_err());
    assert!(untagged.check(Dimension::Density).is_err());
    assert!(untagged.as_density_kg_m3().is_err());
    assert!(untagged.accept(Dimension::Density).is_err());
    // ...and every other dimension still adopts a bare number.
    for want in [
        Dimension::Length,
        Dimension::Angle,
        Dimension::Count,
        Dimension::Ratio,
        Dimension::Mass,
    ] {
        assert!(
            untagged.check(want).is_ok(),
            "a bare number must still adopt {want:?}"
        );
        assert!(untagged.retag(want, Span::new(0, 4)).is_ok(), "{want:?}");
    }
    // A COMMITTED density is of course still accepted — the refusal is about
    // the missing units, not about the dimension.
    let rho = ok("2.7g / 1cm^3");
    assert!(rho.check(Dimension::Density).is_ok());
    assert!(rho.retag(Dimension::Density, Span::new(0, 4)).is_ok());
}

// ----------------------------- claim 4: the named boundary discipline

/// The magnitude a caller gets is the one the accessor's NAME promises, on
/// both dimensioned carriers M1 touches.
///
/// The failure this guards is a caller reaching past the accessor to the raw
/// field: `Quantity.value` for a length is MILLIMETRES (×1000 off metres) and
/// `ToleranceValue.magnitude` for an angle is RADIANS (×57.3 off degrees), so
/// each raw read is pinned here as a DIFFERENT number from the named one.
#[test]
fn rev_units_a_raw_magnitude_is_not_what_the_named_boundary_returns() {
    // `Quantity`: working space mm, boundary metres. A raw read is 1000×.
    let len = ok("25mm");
    assert_eq!(len.value, 25.0, "the working-space magnitude is mm");
    assert_eq!(len.as_length_meters().unwrap(), 0.025);
    assert_ne!(len.value, len.as_length_meters().unwrap());

    // `Quantity` angle: working space DEGREES, and both readings exist.
    let ang = ok("90deg");
    assert_eq!(ang.value, 90.0);
    assert_eq!(ang.as_angle_degrees().unwrap(), 90.0);
    assert!((ang.as_angle_radians().unwrap() - std::f64::consts::FRAC_PI_2).abs() < 1e-15);

    // `Quantity` mass: the one axis whose working space IS the model unit,
    // so the identity is correct and deliberate rather than a missing
    // conversion — pinned so a later "fix" that adds one fails here.
    let kg = ok("250g");
    assert_eq!(kg.value, 0.25);
    assert_eq!(kg.as_mass_kilograms().unwrap(), 0.25);
    assert_eq!(kg.value, kg.as_mass_kilograms().unwrap());

    // `Quantity` density: working space kg/mm³, boundary kg/m³ — 1e9 apart.
    let rho = ok("2.7g / 1cm^3");
    assert!((rho.value - 2.7e-6).abs() < 1e-18, "{}", rho.value);
    assert!((rho.as_density_kg_m3().unwrap() / rho.value - 1e9).abs() < 1.0);

    // `ToleranceValue`: metres and RADIANS, and the raw field is the radian
    // one — so a caller that reads `magnitude` thinking degrees is 57× out,
    // which is why `as_angle_degrees` exists.
    let t = ToleranceValue::angle_degrees(0.5);
    assert!((t.magnitude() - 0.5_f64.to_radians()).abs() < 1e-18);
    assert!((t.as_angle_radians().unwrap() - 0.5_f64.to_radians()).abs() < 1e-18);
    assert!((t.as_angle_degrees().unwrap() - 0.5).abs() < 1e-12);
    assert_ne!(
        t.magnitude(),
        0.5,
        "the constructor must convert, not store"
    );

    // A length tolerance is metres, and the two readings agree because
    // metres IS the model unit there.
    let l = ToleranceValue::length_meters(0.1e-3);
    assert_eq!(l.as_length_meters().unwrap(), 1e-4);
    assert_eq!(l.magnitude(), 1e-4);

    // Each refuses the other dimension by name rather than converting.
    assert!(t.as_length_meters().is_err());
    assert!(l.as_angle_radians().is_err());
    let why = t.as_length_meters().unwrap_err().to_string();
    assert!(why.contains("length") && why.contains("angle"), "{why}");
    // A mass is not a tolerance at all, and `validate` says which.
    let bogus = ToleranceValue {
        magnitude: 1.0,
        dimension: Dimension::Mass,
    };
    let why = bogus.validate().unwrap_err().to_string();
    assert!(why.contains("mass"), "{why}");
}

// ------------------------ claim 5: one type, one serialized spelling

/// There is exactly ONE `Dimension` type and ONE serialized spelling.
///
/// Type IDENTITY is what is proved, not a matching spelling: `same_type`
/// takes `waffle_types::dimension::Dimension` and is handed
/// `feature_engine::expr::Dimension`, so a second definition that happened to
/// serialize the same would not compile.
#[test]
fn rev_units_one_dimension_type_and_one_serialized_spelling() {
    fn same_type(d: waffle_types::dimension::Dimension) -> waffle_types::dimension::Dimension {
        d
    }
    assert_eq!(
        same_type(Dimension::Mass),
        waffle_types::dimension::Dimension::Mass
    );
    assert_eq!(same_type(Dimension::Density), Dimension::Density);
    // `Dim` too — the exponent vector the tolerance types never see but the
    // re-export names.
    let d: waffle_types::dimension::Dim = Dim::MASS;
    assert_eq!(d, waffle_types::dimension::Dim::MASS);

    // The two new variants serialize in the SAME STYLE as the pre-M1 ones:
    // a bare PascalCase string, with a lower-case alias accepted.
    for (variant, pascal, lower) in [
        (Dimension::Length, "\"Length\"", "\"length\""),
        (Dimension::Angle, "\"Angle\"", "\"angle\""),
        (Dimension::Count, "\"Count\"", "\"count\""),
        (Dimension::Ratio, "\"Ratio\"", "\"ratio\""),
        (Dimension::Mass, "\"Mass\"", "\"mass\""),
        (Dimension::Density, "\"Density\"", "\"density\""),
    ] {
        assert_eq!(serde_json::to_string(&variant).unwrap(), pascal);
        assert_eq!(
            serde_json::from_str::<Dimension>(pascal).unwrap(),
            variant,
            "{pascal}"
        );
        assert_eq!(
            serde_json::from_str::<Dimension>(lower).unwrap(),
            variant,
            "{lower}"
        );
        // No second spelling: an all-caps or kebab form is NOT accepted, so
        // a document cannot read back a different unit than it wrote.
        let caps = pascal.to_uppercase();
        if caps != pascal {
            assert!(
                serde_json::from_str::<Dimension>(&caps).is_err(),
                "{caps} must not deserialize"
            );
        }
    }
    // An unknown variant fails LOUDLY — which is why the format floor moved.
    assert!(serde_json::from_str::<Dimension>("\"Tonne\"").is_err());

    // The round trip through the field that actually persists it.
    let p = DesignParameter::new("rho", "2700kg / 1m^3").with_unit(Dimension::Density);
    let json = serde_json::to_value(&p).unwrap();
    assert_eq!(json["unit"], serde_json::json!("Density"));
    let back: DesignParameter = serde_json::from_value(json).unwrap();
    assert_eq!(back.unit, Some(Dimension::Density));

    // And the `dim()`/`label()` mapping is single-valued in both directions
    // for every named dimension M1 added.
    assert_eq!(Dimension::Mass.dim(), Dim::MASS);
    assert_eq!(Dimension::Density.dim(), Dim::DENSITY);
    assert_eq!(Dimension::Mass.label(), "a mass");
    assert_eq!(Dimension::Density.label(), "a density");
    assert_eq!(Dim::MASS.label(), "a mass");
    assert_eq!(Dim::DENSITY.label(), "a density");
}

// ------------------------------- the degrees-vs-radians trap

/// A factor of 180/π anywhere on the angular-tolerance path.
///
/// The path has four hops and two unit conventions, which is why it is worth
/// a test of its own: the MCP boundary takes DEGREES
/// (`ToleranceValue::angle_degrees`), a tolerance magnitude is RADIANS, an
/// expression's angular working space is DEGREES
/// (`Quantity::as_angle_radians` converts), and
/// `waffle_types::annotation::measure` produces RADIANS for
/// `DimensionKind::Angle`. Each assertion below is sized so a 57.3× slip
/// fails it, and so does a 1/57.3× one.
#[test]
fn rev_units_a_factor_of_180_over_pi_on_the_angular_tolerance_path_is_caught() {
    const HALF_DEG: f64 = 0.5_f64 * std::f64::consts::PI / 180.0;

    // Hop 1 — the degree-thinking constructor converts exactly once.
    let pm = ToleranceValue::angle_degrees(0.5);
    assert!((pm.as_angle_radians().unwrap() - HALF_DEG).abs() < 1e-18);
    assert!((pm.as_angle_degrees().unwrap() - 0.5).abs() < 1e-12);
    // A missing conversion would leave 0.5; a doubled one 1.5e-4.
    assert!((pm.magnitude() - 0.5).abs() > 1e-3);
    assert!((pm.magnitude() - HALF_DEG / 57.295).abs() > 1e-6);
    // The radian constructor does NOT convert.
    assert_eq!(
        ToleranceValue::angle_radians(HALF_DEG).magnitude(),
        HALF_DEG
    );

    // Hop 2 — `limits_of` works in the nominal's own unit, so a 30° nominal
    // expressed in radians gets a band that reads 30.5° / 29.5°.
    let nominal = 30_f64.to_radians();
    let sym = Tolerance::Symmetric { plus_minus: pm };
    let limits = sym.limits_of(nominal).unwrap().unwrap();
    assert!(
        (limits.upper.to_degrees() - 30.5).abs() < 1e-9,
        "{limits:?}"
    );
    assert!(
        (limits.lower.to_degrees() - 29.5).abs() < 1e-9,
        "{limits:?}"
    );
    assert!((limits.width().to_degrees() - 1.0).abs() < 1e-9);
    // A 57.3× slip would make the band a radian wide, i.e. 57°.
    assert!(
        limits.width() < 0.02,
        "{} rad is not a ±0.5° band",
        limits.width()
    );

    // Hop 3 — the layout record a renderer reads, through the one resolver.
    let layout = ToleranceLayout::resolve(&sym, DimensionKind::Angle, nominal).unwrap();
    let dev = layout.deviations.expect("a band");
    assert!((dev[0] - HALF_DEG).abs() < 1e-15, "{dev:?}");
    assert!((dev[1] + HALF_DEG).abs() < 1e-15, "{dev:?}");
    let lim = layout.limits.expect("limits");
    assert!((lim[0].to_degrees() - 30.5).abs() < 1e-9, "{lim:?}");
    assert!((lim[1].to_degrees() - 29.5).abs() < 1e-9, "{lim:?}");
    // An angular tolerance on a LINEAR dimension is refused rather than
    // silently printing 0.0087 as if it were a length.
    assert!(ToleranceLayout::resolve(&sym, DimensionKind::Distance, 0.025).is_err());
    // ...and a length tolerance on an angular one, the other way.
    let linear = Tolerance::Symmetric {
        plus_minus: ToleranceValue::length_meters(1e-4),
    };
    assert!(ToleranceLayout::resolve(&linear, DimensionKind::Angle, nominal).is_err());

    // Hop 4 — the expression side. An angle's working space is degrees, and
    // `as_angle_radians` is the conversion `ExprDimensions::value_of` uses
    // for `DimensionKind::Angle`. The two carriers must AGREE, which is the
    // property a stray conversion anywhere on the path breaks.
    let half = ok("0.5deg");
    assert_eq!(half.value, 0.5, "degrees in the working space");
    assert_eq!(half.as_angle_degrees().unwrap(), 0.5);
    assert!((half.as_angle_radians().unwrap() - HALF_DEG).abs() < 1e-18);
    assert_eq!(
        half.as_angle_radians().unwrap(),
        pm.as_angle_radians().unwrap(),
        "the expression path and the tolerance path must produce the SAME radians"
    );
    // The `rad` suffix comes back to the same place from the other side.
    let from_rad = ok("0.008726646259971648rad");
    assert!((from_rad.as_angle_degrees().unwrap() - 0.5).abs() < 1e-12);
    assert!((from_rad.as_angle_radians().unwrap() - HALF_DEG).abs() < 1e-15);
    // A length in an angle field, and an angle in a length field, both
    // refuse — so the conversion can never run on the wrong number.
    assert!(ok("0.5mm").as_angle_radians().is_err());
    assert!(ok("0.5deg").as_length_meters().is_err());
}
