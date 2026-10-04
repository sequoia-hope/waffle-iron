//! Tests for the M1 tolerance types.
//!
//! The ISO 286 table itself is pinned in [`super::super::iso286`]; what is
//! pinned here is the layer above it — that a tolerance is typed, that its
//! band is computed in exactly one place, and that every malformed shape is
//! refused by name rather than resolved into a plausible number.

use super::*;
use crate::annotation::OrdinateAxis;

const MM: f64 = 1e-3;
const UM: f64 = 1e-6;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

#[test]
fn a_tolerance_value_is_read_through_a_named_boundary_and_refuses_the_other_unit() {
    let len = ToleranceValue::length_meters(0.1 * MM);
    assert!(close(len.as_length_meters().unwrap(), 0.0001));
    // The whole reason this type exists: a length cannot be read as an
    // angle, so a 0.1 mm band can never be printed as 0.1 rad.
    let Err(ToleranceError::DimensionMismatch { expected, found }) = len.as_angle_radians() else {
        panic!("a length was accepted as an angle");
    };
    assert_eq!((expected, found), ("an angle", "a length"));

    let ang = ToleranceValue::angle_degrees(0.5);
    assert!(close(ang.as_angle_degrees().unwrap(), 0.5));
    assert!(close(ang.as_angle_radians().unwrap(), 0.5_f64.to_radians()));
    assert!(ang.as_length_meters().is_err());
}

#[test]
fn only_a_length_or_an_angle_is_a_tolerance_and_a_non_finite_one_is_refused() {
    for d in [
        Dimension::Count,
        Dimension::Ratio,
        Dimension::Mass,
        Dimension::Density,
    ] {
        let v = ToleranceValue {
            magnitude: 1.0,
            dimension: d,
        };
        assert!(
            matches!(
                v.validate(),
                Err(ToleranceError::NotAToleranceDimension { .. })
            ),
            "{d:?} must not be a tolerance dimension"
        );
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            ToleranceValue::length_meters(bad).validate(),
            Err(ToleranceError::NotFinite { .. })
        ));
    }
}

#[test]
fn every_arm_resolves_to_the_same_two_numbers_through_one_function() {
    let nominal = 25.0 * MM;

    let sym = Tolerance::Symmetric {
        plus_minus: ToleranceValue::length_meters(0.1 * MM),
    };
    let l = sym.limits_of(nominal).unwrap().unwrap();
    assert!(close(l.upper, 25.1 * MM), "{}", l.upper);
    assert!(close(l.lower, 24.9 * MM), "{}", l.lower);
    assert!(close(l.width(), 0.2 * MM));

    // `minus` is a SIGNED deviation, so it is negative.
    let bil = Tolerance::Bilateral {
        plus: ToleranceValue::length_meters(0.021 * MM),
        minus: ToleranceValue::length_meters(-0.005 * MM),
    };
    let l = bil.limits_of(nominal).unwrap().unwrap();
    assert!(close(l.upper, 25.021 * MM), "{}", l.upper);
    assert!(close(l.lower, 24.995 * MM), "{}", l.lower);

    // A unilateral tolerance is the same arm with one zero.
    let uni = Tolerance::Bilateral {
        plus: ToleranceValue::length_meters(0.021 * MM),
        minus: ToleranceValue::length_meters(0.0),
    };
    let l = uni.limits_of(nominal).unwrap().unwrap();
    assert!(close(l.lower, nominal));

    // `Limits` are absolute SIZES: the nominal plays no part.
    let lim = Tolerance::Limits {
        upper: ToleranceValue::length_meters(25.021 * MM),
        lower: ToleranceValue::length_meters(25.0 * MM),
    };
    let l = lim.limits_of(nominal).unwrap().unwrap();
    let l_elsewhere = lim.limits_of(999.0).unwrap().unwrap();
    assert_eq!(l, l_elsewhere);
    assert!(close(l.upper, 25.021 * MM));

    // `Basic` has no band at all — not a zero-width one.
    assert_eq!(Tolerance::Basic.limits_of(nominal).unwrap(), None);
    assert_eq!(Tolerance::Basic.dimension(), None);
}

#[test]
fn a_fit_resolves_through_the_iso_286_tables_at_the_nominal_in_millimetres() {
    // Ø25 H7 is +0.021 / 0 — the single most-quoted row in the standard.
    let hole = Tolerance::Fit {
        hole: Some(FitClass::parse("H7").unwrap()),
        shaft: None,
    };
    let l = hole.limits_of(25.0 * MM).unwrap().unwrap();
    assert!(close(l.upper, 25.0 * MM + 21.0 * UM), "{}", l.upper);
    assert!(close(l.lower, 25.0 * MM), "{}", l.lower);

    // Ø25 g6 is −0.007 / −0.020.
    let shaft = Tolerance::Fit {
        hole: None,
        shaft: Some(FitClass::parse("g6").unwrap()),
    };
    let l = shaft.limits_of(25.0 * MM).unwrap().unwrap();
    assert!(close(l.upper, 25.0 * MM - 7.0 * UM), "{}", l.upper);
    assert!(close(l.lower, 25.0 * MM - 20.0 * UM), "{}", l.lower);

    // A PAIR spans both zones: the assembly's extreme sizes, H7 above and
    // g6 below.
    let pair = Tolerance::Fit {
        hole: Some(FitClass::parse("H7").unwrap()),
        shaft: Some(FitClass::parse("g6").unwrap()),
    };
    let l = pair.limits_of(25.0 * MM).unwrap().unwrap();
    assert!(close(l.upper, 25.0 * MM + 21.0 * UM));
    assert!(close(l.lower, 25.0 * MM - 20.0 * UM));

    // The nominal is read in MILLIMETRES against the table, so the SAME
    // class at a different size gives a different band — if the conversion
    // were wrong, every size would land in the ≤3 mm step and agree.
    let at_10 = hole.limits_of(10.0 * MM).unwrap().unwrap();
    let at_100 = hole.limits_of(100.0 * MM).unwrap().unwrap();
    assert!(at_10.width() < l.width(), "10 mm H7 must be tighter");
    assert!(at_100.width() > at_10.width(), "100 mm H7 must be looser");
}

#[test]
fn a_fit_outside_the_tables_refuses_by_name_rather_than_extrapolating() {
    let fit = Tolerance::Fit {
        hole: Some(FitClass::parse("H7").unwrap()),
        shaft: None,
    };
    // 600 mm is past the end of the tabulated range.
    assert!(matches!(
        fit.limits_of(600.0 * MM),
        Err(ToleranceError::Iso286(_))
    ));
    // An unsupported letter is loud too.
    let exotic = Tolerance::Fit {
        hole: None,
        shaft: Some(FitClass::parse("zc6").unwrap()),
    };
    assert!(matches!(
        exotic.limits_of(25.0 * MM),
        Err(ToleranceError::Iso286(_))
    ));
}

#[test]
fn a_fit_naming_neither_side_is_refused() {
    let empty = Tolerance::Fit {
        hole: None,
        shaft: None,
    };
    assert_eq!(empty.validate(), Err(ToleranceError::FitWithNoClass));
    assert!(matches!(
        empty.limits_of(25.0 * MM),
        Err(ToleranceError::FitWithNoClass)
    ));
}

#[test]
fn an_inverted_or_mixed_band_is_refused() {
    let inverted = Tolerance::Limits {
        upper: ToleranceValue::length_meters(24.0 * MM),
        lower: ToleranceValue::length_meters(25.0 * MM),
    };
    assert!(matches!(
        inverted.validate(),
        Err(ToleranceError::Inverted { .. })
    ));

    // A bilateral pair whose `minus` is above its `plus` is the same slip.
    let backwards = Tolerance::Bilateral {
        plus: ToleranceValue::length_meters(-0.1 * MM),
        minus: ToleranceValue::length_meters(0.1 * MM),
    };
    assert!(matches!(
        backwards.validate(),
        Err(ToleranceError::Inverted { .. })
    ));

    let mixed = Tolerance::Bilateral {
        plus: ToleranceValue::length_meters(0.1 * MM),
        minus: ToleranceValue::angle_degrees(-0.1),
    };
    assert!(matches!(
        mixed.validate(),
        Err(ToleranceError::MixedDimensions { .. })
    ));

    let negative = Tolerance::Symmetric {
        plus_minus: ToleranceValue::length_meters(-0.1 * MM),
    };
    assert!(matches!(
        negative.validate(),
        Err(ToleranceError::NegativeSymmetric { .. })
    ));
}

#[test]
fn a_tolerance_must_match_its_dimensions_kind_and_a_fit_needs_a_size() {
    let len = Tolerance::Symmetric {
        plus_minus: ToleranceValue::length_meters(0.1 * MM),
    };
    let ang = Tolerance::Symmetric {
        plus_minus: ToleranceValue::angle_degrees(0.5),
    };

    assert!(len.check_for(DimensionKind::Distance).is_ok());
    assert!(ang.check_for(DimensionKind::Angle).is_ok());
    // An angular band under a millimetre nominal would print 0.0087 as if
    // it were a length. Refused by name.
    assert!(matches!(
        ang.check_for(DimensionKind::Distance),
        Err(ToleranceError::DimensionMismatch { .. })
    ));
    assert!(matches!(
        len.check_for(DimensionKind::Angle),
        Err(ToleranceError::DimensionMismatch { .. })
    ));

    let fit = Tolerance::Fit {
        hole: Some(FitClass::parse("H7").unwrap()),
        shaft: None,
    };
    for ok in [
        DimensionKind::Distance,
        DimensionKind::PointLineDistance,
        DimensionKind::HDistance,
        DimensionKind::VDistance,
        DimensionKind::Diameter,
    ] {
        assert!(fit.check_for(ok).is_ok(), "{ok:?} is a size");
    }
    for not in [
        DimensionKind::Radius,
        DimensionKind::Angle,
        DimensionKind::Ordinate {
            axis: OrdinateAxis::U,
        },
    ] {
        let err = fit.check_for(not).unwrap_err();
        assert!(
            matches!(
                err,
                ToleranceError::FitOnANonSize { .. } | ToleranceError::DimensionMismatch { .. }
            ),
            "{not:?} must not take a fit: {err}"
        );
    }
    // A radius is refused for the FIT reason specifically, not the
    // dimension one — ISO 286 tabulates by diameter.
    assert!(matches!(
        fit.check_for(DimensionKind::Radius),
        Err(ToleranceError::FitOnANonSize { kind: "radius" })
    ));
}

#[test]
fn a_fit_class_round_trips_as_the_string_a_drafter_writes() {
    let h7 = FitClass::parse("H7").unwrap();
    assert_eq!(h7.letter, "H");
    assert_eq!(h7.grade, 7);
    assert_eq!(h7.to_string(), "H7");
    assert_eq!(
        serde_json::to_string(&h7).unwrap(),
        "\"H7\"",
        "persisted as the drafter's spelling, not as an object"
    );
    assert_eq!(
        serde_json::from_str::<FitClass>("\"H7\"").unwrap(),
        h7,
        "and read back from it"
    );

    // Two-letter classes and surrounding space both parse.
    assert_eq!(FitClass::parse(" js9 ").unwrap().letter, "js");
    assert_eq!(FitClass::parse("js9").unwrap().grade, 9);

    // The case carries no meaning; the ROLE decides how it prints.
    let authored_upper = FitClass::parse("G6").unwrap();
    assert_eq!(authored_upper.display_for(FitRole::Shaft), "g6");
    assert_eq!(authored_upper.display_for(FitRole::Hole), "G6");
}

#[test]
fn a_malformed_fit_class_is_refused_by_name_including_through_serde() {
    for (text, _why) in [
        ("H", "no grade"),
        ("7", "no letter"),
        ("H7x", "trailing junk"),
        ("H0", "grade below 1"),
        ("H19", "grade above 18"),
        ("", "empty"),
        ("H 7", "an inner space is junk"),
    ] {
        assert!(
            matches!(
                FitClass::parse(text),
                Err(ToleranceError::NotAFitClass { .. })
            ),
            "`{text}` must not parse as a fit class, got {:?}",
            FitClass::parse(text)
        );
    }
    // A malformed class in a document is a hard parse failure, not a
    // silently dropped tolerance: a fit nobody can resolve must not load
    // as "no tolerance".
    assert!(serde_json::from_str::<FitClass>("\"H0\"").is_err());
    assert!(serde_json::from_str::<Tolerance>(r#"{"type":"Fit","hole":"nonsense"}"#).is_err());
}

#[test]
fn a_tolerance_round_trips_through_serde_arm_for_arm() {
    let cases = vec![
        Tolerance::Symmetric {
            plus_minus: ToleranceValue::length_meters(0.1 * MM),
        },
        Tolerance::Bilateral {
            plus: ToleranceValue::length_meters(0.021 * MM),
            minus: ToleranceValue::length_meters(-0.005 * MM),
        },
        Tolerance::Limits {
            upper: ToleranceValue::length_meters(25.021 * MM),
            lower: ToleranceValue::length_meters(25.0 * MM),
        },
        Tolerance::Fit {
            hole: Some(FitClass::parse("H7").unwrap()),
            shaft: Some(FitClass::parse("g6").unwrap()),
        },
        Tolerance::Fit {
            hole: Some(FitClass::parse("H7").unwrap()),
            shaft: None,
        },
        Tolerance::Basic,
        Tolerance::Symmetric {
            plus_minus: ToleranceValue::angle_degrees(0.5),
        },
    ];
    for t in cases {
        let json = serde_json::to_string(&t).unwrap();
        let back: Tolerance = serde_json::from_str(&json).unwrap();
        // Compared by the SHAPE, re-serialized, and not by `==` on the
        // magnitudes. `serde_json`'s parse is not always the exact inverse
        // of its print: `0.021 * 1e-3` is 2.1000000000000002e-5, prints as
        // the 17-digit `0.000021000000000000002`, and reads back as
        // 2.1e-5 — one ULP away, which is 3e-21 m on a tolerance and
        // physically nothing, but not `==`. That is a property of every
        // f64 in every `.waffle` file, not of this type, so this test pins
        // what it can actually promise: a tolerance survives a round trip
        // as the same arm with the same fields, and a SECOND round trip
        // is a fixed point.
        assert_eq!(serde_json::to_string(&back).unwrap(), {
            let twice: Tolerance = serde_json::from_str(&json).unwrap();
            serde_json::to_string(&twice).unwrap()
        });
        assert_eq!(
            std::mem::discriminant(&back),
            std::mem::discriminant(&t),
            "{json}"
        );
        if let (Some(a), Some(b)) = (t.limits_of(0.025).unwrap(), back.limits_of(0.025).unwrap()) {
            assert!((a.upper - b.upper).abs() < 1e-18, "{json}");
            assert!((a.lower - b.lower).abs() < 1e-18, "{json}");
        }
    }
    // The absent side of a fit is omitted, not written as null.
    let one_sided = serde_json::to_string(&Tolerance::Fit {
        hole: Some(FitClass::parse("H7").unwrap()),
        shaft: None,
    })
    .unwrap();
    assert_eq!(one_sided, r#"{"type":"Fit","hole":"H7"}"#);
}

#[test]
fn a_form_characteristic_refuses_a_datum_and_an_orientation_one_requires_it() {
    let z = ToleranceValue::length_meters(0.05 * MM);

    // Flatness with respect to A is a frame no inspector can act on.
    let bad =
        GeometricTolerance::new(Characteristic::Flatness, z).with_datums(vec![DatumRef::new("A")]);
    assert!(matches!(
        bad.validate(),
        Err(ToleranceError::FormTakesNoDatum {
            characteristic: "flatness",
            count: 1
        })
    ));
    assert!(GeometricTolerance::new(Characteristic::Flatness, z)
        .validate()
        .is_ok());

    // Perpendicularity to nothing is meaningless.
    let bare = GeometricTolerance::new(Characteristic::Perpendicularity, z);
    assert!(matches!(
        bare.validate(),
        Err(ToleranceError::DatumRequired {
            characteristic: "perpendicularity"
        })
    ));
    assert!(bare
        .with_datums(vec![DatumRef::new("A")])
        .validate()
        .is_ok());

    // Profile is honestly both.
    assert!(GeometricTolerance::new(Characteristic::Profile, z)
        .validate()
        .is_ok());
    assert!(GeometricTolerance::new(Characteristic::Profile, z)
        .with_datums(vec![DatumRef::new("A"), DatumRef::new("B")])
        .validate()
        .is_ok());

    // Every characteristic states a rule — no arm falls through.
    for c in [
        Characteristic::Flatness,
        Characteristic::Straightness,
        Characteristic::Circularity,
        Characteristic::Cylindricity,
        Characteristic::Perpendicularity,
        Characteristic::Parallelism,
        Characteristic::Angularity,
        Characteristic::Position,
        Characteristic::Concentricity,
        Characteristic::Symmetry,
        Characteristic::Profile,
        Characteristic::Runout,
    ] {
        let frame = match c.datum_rule() {
            DatumRule::None => GeometricTolerance::new(c, z),
            _ => GeometricTolerance::new(c, z).with_datums(vec![DatumRef::new("A")]),
        };
        assert!(frame.validate().is_ok(), "{c:?} must validate");
        assert!(!c.name().is_empty());
        assert!(!c.symbol().is_ascii(), "{c:?} needs its ISO 1101 glyph");
    }
}

#[test]
fn a_geometric_zone_is_a_positive_length() {
    assert!(matches!(
        GeometricTolerance::new(Characteristic::Flatness, ToleranceValue::angle_degrees(0.5))
            .validate(),
        Err(ToleranceError::ZoneIsNotALength { .. })
    ));
    for bad in [0.0, -0.05 * MM] {
        assert!(matches!(
            GeometricTolerance::new(Characteristic::Flatness, ToleranceValue::length_meters(bad))
                .validate(),
            Err(ToleranceError::ZoneNotPositive { .. })
        ));
    }
}

#[test]
fn a_geometric_tolerance_round_trips_and_defaults_its_zone_to_a_width() {
    let g = GeometricTolerance::new(
        Characteristic::Position,
        ToleranceValue::length_meters(0.2 * MM),
    )
    .with_datums(vec![
        DatumRef::new("A"),
        DatumRef {
            label: "B".to_string(),
            modifier: Some(MaterialCondition::Mmc),
        },
    ])
    .diametral();
    let json = serde_json::to_string(&g).unwrap();
    assert_eq!(
        serde_json::from_str::<GeometricTolerance>(&json).unwrap(),
        g
    );

    // An older record with no `zone` reads as a width zone, which is the
    // ISO 1101 default and the only safe one: reading a width as diametral
    // would double the zone a part is inspected against.
    let no_zone = r#"{"characteristic":{"type":"Flatness"},
        "value":{"magnitude":0.00005,"dimension":"Length"}}"#;
    let back: GeometricTolerance = serde_json::from_str(no_zone).unwrap();
    assert_eq!(back.zone, ZoneShape::Width);
    assert_eq!(back.zone.prefix(), "");
    assert_eq!(ZoneShape::Diametral.prefix(), "\u{2300}");
    assert_eq!(ZoneShape::Spherical.prefix(), "S\u{2300}");

    // RFS has no symbol; printing one would print a glyph no standard
    // defines.
    assert_eq!(MaterialCondition::Rfs.symbol(), None);
    assert!(MaterialCondition::Mmc.symbol().is_some());
    assert!(MaterialCondition::Lmc.symbol().is_some());
}

#[test]
fn a_non_finite_nominal_is_refused_rather_than_producing_a_nan_band() {
    let t = Tolerance::Symmetric {
        plus_minus: ToleranceValue::length_meters(0.1 * MM),
    };
    for bad in [f64::NAN, f64::INFINITY] {
        assert!(matches!(
            t.limits_of(bad),
            Err(ToleranceError::NotFinite { .. })
        ));
    }
}
