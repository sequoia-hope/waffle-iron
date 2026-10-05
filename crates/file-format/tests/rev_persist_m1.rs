//! M1's persistence half, measured through the REAL loader
//! (`specs/drawings_and_mbd.md` §9, "Implementation notes (M1)").
//!
//! Every assertion here goes `save_document` → `load_document` and reads the
//! value back off the loaded document. That is deliberately not the same
//! thing as the serde round trips in
//! `waffle_types::annotation::tolerance::tests` and
//! `feature_engine` `tests/material_table.rs`: a field that serializes but is
//! dropped on the way back in — a `KnownTabKind` arm that forgets it, a
//! `#[serde(skip)]`, a `DocumentMetadata::from` that defaults it — passes a
//! `to_string` → `from_str` test on the leaf type and loses the data in the
//! file. The leaf tests say the TYPE can be written; these say the DOCUMENT
//! carries it.
//!
//! A note on floats: the M1 notes record that `serde_json`'s parse is not
//! always the exact inverse of its print (`0.021 * 1e-3` prints 17 digits and
//! reads back one ULP away). `file-format` enables serde_json's
//! `float_roundtrip` feature precisely so a load → save cycle never drifts a
//! coordinate, and feature unification applies it to the whole build — so the
//! equalities below are asserted EXACTLY, and `tolerance_magnitudes_are_exact`
//! pins that it really is exact rather than tolerated.

use feature_engine::drawing::{Drawing, DrawingView, NamedView, Projection, Sheet, ViewSource};
use feature_engine::types::{Appearance, FeatureTree, Material};
use file_format::metadata::{Tab, TabKind};
use file_format::{load_document, save_document, DocumentMetadata, LoadError, WaffleDocument};
use serde_json::Map;
use waffle_types::annotation::layout::{
    AnchorGeometry, AnnotationLayout, ToleranceDisplay, ToleranceLayout, ViewLayout,
};
use waffle_types::annotation::tolerance::{
    Characteristic, DatumRef, FitClass, GeometricTolerance, MaterialCondition, Tolerance,
    ToleranceValue, ZoneShape,
};
use waffle_types::annotation::{Annotation, DimensionKind, Measured, Placement2};
use waffle_types::{Anchor, GeomRef, OutputKey, ResolvePolicy, Selector, TopoKind};

const MM: f64 = 1e-3;

// ───────────────────────────────────────────────────────── fixtures

fn pid_ref(pid: u64) -> GeomRef {
    GeomRef {
        kind: TopoKind::Edge,
        anchor: Anchor::FeatureOutput {
            feature_id: uuid::Uuid::nil(),
            output_key: OutputKey::Main,
        },
        selector: Selector::Pid { pid, root_pid: pid },
        policy: ResolvePolicy::Strict,
        scope: None,
    }
}

/// A two-tab document: a Part (so the drawing's `ViewSource` resolves and the
/// loader emits no "view of a tab this document does not have" warning) and a
/// Drawing carrying `annotations` on its one view.
fn doc_with_annotations(annotations: Vec<Annotation>) -> WaffleDocument {
    let part = Tab::part("Part 1", FeatureTree::new());
    let part_id = part.id.clone();
    let mut view = DrawingView::new(
        "Front",
        ViewSource::whole_tab(part_id),
        Projection::Named {
            view: NamedView::Front,
        },
    );
    view.annotations = annotations;
    let mut sheet = Sheet::new("Sheet 1");
    sheet.views = vec![view];
    let drawing = Drawing {
        sheets: vec![sheet],
        ..Drawing::default()
    };
    let drawing_tab = Tab::drawing("Drawing 1", drawing);
    let active_tab = drawing_tab.id.clone();
    WaffleDocument {
        document: DocumentMetadata::new("M1 persistence"),
        sources: Vec::new(),
        tabs: vec![part, drawing_tab],
        active_tab,
        extra: Map::new(),
    }
}

/// Through the real writer and the real loader. Panics with the loader's own
/// message, so a refusal names itself.
fn reopen(doc: &WaffleDocument) -> (WaffleDocument, Vec<String>) {
    let json = save_document(doc);
    let loaded = load_document(&json).unwrap_or_else(|e| panic!("the loader refused: {e}\n{json}"));
    (loaded.document, loaded.warnings)
}

fn annotations_of(doc: &WaffleDocument) -> &[Annotation] {
    let tab = doc
        .tabs
        .iter()
        .find(|t| matches!(t.kind, TabKind::Drawing { .. }))
        .expect("the drawing tab came back as a drawing tab");
    let drawing = tab.drawing_tree().expect("…with its drawing");
    &drawing.sheets[0].views[0].annotations
}

fn tolerance_of(a: &Annotation) -> Option<&Tolerance> {
    match a {
        Annotation::Dimension { tolerance, .. } => tolerance.as_ref(),
        other => panic!("expected a dimension, got {other:?}"),
    }
}

fn dimension_with(tolerance: Tolerance, kind: DimensionKind, pid: u64) -> Annotation {
    Annotation::Dimension {
        kind,
        anchors: vec![pid_ref(pid)],
        value: Measured::Value { value: 0.025 },
        tolerance: Some(tolerance),
        precision: None,
        dual_unit: None,
        dual_precision: None,
        placement: Placement2::new(1.0, 2.0),
    }
}

/// Field-for-field, arm-for-arm. `Tolerance` is `PartialEq`, but matching
/// explicitly is what makes the ARM part of the assertion: `assert_eq!` on two
/// values that are both `Basic` would also pass if a `Symmetric` had silently
/// become `Basic` on the way out and back, because the expectation would have
/// been built from the same broken path. Here the expected arm is written out
/// by hand.
fn assert_same_tolerance(got: &Tolerance, want: &Tolerance) {
    match (got, want) {
        (Tolerance::Symmetric { plus_minus: a }, Tolerance::Symmetric { plus_minus: b }) => {
            assert_eq!(a.magnitude, b.magnitude, "Symmetric.plus_minus magnitude");
            assert_eq!(a.dimension, b.dimension, "Symmetric.plus_minus dimension");
        }
        (
            Tolerance::Bilateral {
                plus: pa,
                minus: ma,
            },
            Tolerance::Bilateral {
                plus: pb,
                minus: mb,
            },
        ) => {
            assert_eq!(pa.magnitude, pb.magnitude, "Bilateral.plus magnitude");
            assert_eq!(pa.dimension, pb.dimension, "Bilateral.plus dimension");
            assert_eq!(ma.magnitude, mb.magnitude, "Bilateral.minus magnitude");
            assert_eq!(ma.dimension, mb.dimension, "Bilateral.minus dimension");
        }
        (
            Tolerance::Limits {
                upper: ua,
                lower: la,
            },
            Tolerance::Limits {
                upper: ub,
                lower: lb,
            },
        ) => {
            assert_eq!(ua.magnitude, ub.magnitude, "Limits.upper magnitude");
            assert_eq!(ua.dimension, ub.dimension, "Limits.upper dimension");
            assert_eq!(la.magnitude, lb.magnitude, "Limits.lower magnitude");
            assert_eq!(la.dimension, lb.dimension, "Limits.lower dimension");
        }
        (
            Tolerance::Fit {
                hole: ha,
                shaft: sa,
            },
            Tolerance::Fit {
                hole: hb,
                shaft: sb,
            },
        ) => {
            assert_eq!(ha, hb, "Fit.hole (letter and grade, and present-ness)");
            assert_eq!(sa, sb, "Fit.shaft (letter and grade, and present-ness)");
        }
        (Tolerance::Basic, Tolerance::Basic) => {}
        (got, want) => panic!("the arm changed: wrote {want:?}, read back {got:?}"),
    }
}

/// Every arm of `Tolerance`, each with the fields that distinguish it. The
/// exhaustive match is the point: a sixth arm added to the enum stops this
/// file compiling, so the "every kind" in the test name cannot go stale.
fn every_tolerance_arm() -> Vec<(&'static str, Tolerance, DimensionKind)> {
    let arms = vec![
        (
            "Symmetric",
            Tolerance::Symmetric {
                plus_minus: ToleranceValue::length_meters(0.1 * MM),
            },
            DimensionKind::Distance,
        ),
        (
            "Symmetric (angular)",
            Tolerance::Symmetric {
                plus_minus: ToleranceValue::angle_degrees(0.5),
            },
            DimensionKind::Angle,
        ),
        (
            "Bilateral",
            Tolerance::Bilateral {
                plus: ToleranceValue::length_meters(0.021 * MM),
                minus: ToleranceValue::length_meters(-0.005 * MM),
            },
            DimensionKind::Distance,
        ),
        (
            "Limits",
            Tolerance::Limits {
                upper: ToleranceValue::length_meters(25.021 * MM),
                lower: ToleranceValue::length_meters(25.0 * MM),
            },
            DimensionKind::Distance,
        ),
        (
            "Fit (both classes)",
            Tolerance::Fit {
                hole: Some(FitClass::parse("H7").unwrap()),
                shaft: Some(FitClass::parse("g6").unwrap()),
            },
            DimensionKind::Diameter,
        ),
        (
            // The notes: both classes are OPTIONAL against §9's shape, because
            // the common drawing is a single feature carrying its own class.
            "Fit (hole only)",
            Tolerance::Fit {
                hole: Some(FitClass::parse("H7").unwrap()),
                shaft: None,
            },
            DimensionKind::Diameter,
        ),
        (
            "Fit (shaft only)",
            Tolerance::Fit {
                hole: None,
                shaft: Some(FitClass::parse("g6").unwrap()),
            },
            DimensionKind::Diameter,
        ),
        ("Basic", Tolerance::Basic, DimensionKind::Distance),
    ];
    // Exhaustiveness: five arms, named. Adding one to the enum breaks this
    // match, which is what keeps the list above honest.
    for (_, t, _) in &arms {
        match t {
            Tolerance::Symmetric { .. }
            | Tolerance::Bilateral { .. }
            | Tolerance::Limits { .. }
            | Tolerance::Fit { .. }
            | Tolerance::Basic => {}
        }
    }
    arms
}

// ───────────────────────────────────────── 1. every tolerance arm

#[test]
fn a_toleranced_dimension_of_every_kind_survives_save_and_reopen() {
    let arms = every_tolerance_arm();
    // Covered: all five arms, plus the angular Symmetric and both one-sided
    // fits the notes say are legal.
    assert_eq!(arms.len(), 8, "every arm plus the three edge shapes");

    // One document per arm, so an arm that fails names itself rather than
    // taking the whole sheet down.
    for (name, want, kind) in &arms {
        let doc = doc_with_annotations(vec![dimension_with(want.clone(), *kind, 7)]);
        let (back, warnings) = reopen(&doc);
        let got = annotations_of(&back);
        assert_eq!(got.len(), 1, "{name}: the annotation survived");
        let got = tolerance_of(&got[0])
            .unwrap_or_else(|| panic!("{name}: the tolerance came back as None"));
        assert_same_tolerance(got, want);
        assert!(
            !warnings.iter().any(|w| w.contains("tolerance")),
            "{name}: no warning about the tolerance, got {warnings:?}"
        );
    }

    // And all eight on ONE sheet, in order — a per-annotation round trip
    // cannot see an index mix-up.
    let all: Vec<Annotation> = arms
        .iter()
        .enumerate()
        .map(|(i, (_, t, k))| dimension_with(t.clone(), *k, 100 + i as u64))
        .collect();
    let (back, _) = reopen(&doc_with_annotations(all));
    let got = annotations_of(&back);
    assert_eq!(got.len(), arms.len());
    for (i, (name, want, _)) in arms.iter().enumerate() {
        let t = tolerance_of(&got[i]).unwrap_or_else(|| panic!("{name} at {i} came back as None"));
        assert_same_tolerance(t, want);
    }
}

#[test]
fn a_fit_with_neither_class_is_refused_by_name_rather_than_persisted() {
    // The notes: "Both absent is refused by name (`FitWithNoClass`), so the
    // pair cannot degenerate into a tolerance that tolerances nothing."
    // Measured where it matters for persistence: a document can carry the
    // shape (nothing validates a tolerance on load — see
    // `Tolerance::check_for`'s two call sites), and `validate` is what names
    // it. If a future loader starts validating, this test says which way.
    let empty = Tolerance::Fit {
        hole: None,
        shaft: None,
    };
    assert!(
        empty.validate().is_err(),
        "a fit naming no class is refused by name"
    );
    let doc = doc_with_annotations(vec![dimension_with(empty, DimensionKind::Diameter, 9)]);
    let json = save_document(&doc);
    match load_document(&json) {
        Ok(loaded) => {
            // It loaded: then it must have come back AS the refusable shape,
            // not as something that silently tolerances nothing.
            let got = tolerance_of(&annotations_of(&loaded.document)[0])
                .expect("the tolerance is still there");
            assert!(
                got.validate().is_err(),
                "a fit with no class must still be refusable after a reopen, got {got:?}"
            );
        }
        Err(e) => {
            let m = e.to_string();
            assert!(
                m.contains("fit") || m.contains("class"),
                "if the loader refuses it, it names what it refused: {m}"
            );
        }
    }
}

#[test]
fn tolerance_magnitudes_are_exact_through_the_file_and_not_merely_close() {
    // The M1 notes' ULP finding, measured rather than assumed: `file-format`
    // enables serde_json's `float_roundtrip`, so the 17-digit print of
    // `0.021 * 1e-3` parses back to the SAME bits. This test is the one that
    // would go red if that feature were dropped — and then the comparison in
    // `assert_same_tolerance` would have to tolerate an ULP and say so.
    let plus = 0.021 * MM;
    let minus = -0.005 * MM;
    let doc = doc_with_annotations(vec![dimension_with(
        Tolerance::Bilateral {
            plus: ToleranceValue::length_meters(plus),
            minus: ToleranceValue::length_meters(minus),
        },
        DimensionKind::Distance,
        11,
    )]);
    let (back, _) = reopen(&doc);
    let Some(Tolerance::Bilateral { plus: p, minus: m }) =
        tolerance_of(&annotations_of(&back)[0]).cloned()
    else {
        panic!("expected the bilateral arm back");
    };
    assert_eq!(
        p.magnitude.to_bits(),
        plus.to_bits(),
        "bit-exact through the file (float_roundtrip)"
    );
    assert_eq!(m.magnitude.to_bits(), minus.to_bits());
}

#[test]
fn the_values_a_skip_serializing_if_would_eat_survive() {
    // The `Some(false)` lesson of item 4, applied to every other field on
    // M1's surface whose meaningful value is also its type's zero. A
    // `skip_serializing_if` written on the VALUE rather than on the
    // `Option` — `is_zero`, `is_false`, `is_empty` — passes every test built
    // from non-zero fixtures and silently drops exactly these.

    // A ZERO deviation. ASME Y14.5 §2.3.2's unilateral form is `25.00
    // +0.021 / 0`: the zero is the tolerance, not its absence.
    let unilateral = Tolerance::Bilateral {
        plus: ToleranceValue::length_meters(0.021 * MM),
        minus: ToleranceValue::length_meters(0.0),
    };
    let (back, _) = reopen(&doc_with_annotations(vec![dimension_with(
        unilateral.clone(),
        DimensionKind::Distance,
        21,
    )]));
    assert_same_tolerance(
        tolerance_of(&annotations_of(&back)[0]).expect("a zero deviation is still a tolerance"),
        &unilateral,
    );

    // A ZERO ± on a symmetric tolerance — `validate` admits it, so the file
    // must carry it rather than reading back as untoleranced.
    let zero_band = Tolerance::Symmetric {
        plus_minus: ToleranceValue::length_meters(0.0),
    };
    assert!(zero_band.validate().is_ok());
    let (back, _) = reopen(&doc_with_annotations(vec![dimension_with(
        zero_band.clone(),
        DimensionKind::Distance,
        22,
    )]));
    assert_same_tolerance(
        tolerance_of(&annotations_of(&back)[0]).expect("Some(0.0) is not None"),
        &zero_band,
    );

    // PRECISION ZERO — whole millimetres, a real drafting choice, both as a
    // document setting and as a per-annotation override.
    let mut meta = DocumentMetadata::new("zero precision");
    meta.precision = Some(0);
    meta.dual_precision = Some(0);
    meta.inch_denominator = Some(0); // not in the drafting set; stored as-is
    let (back, _) = reopen(&one_part_doc(meta));
    assert_eq!(back.document.precision, Some(0), "precision 0 is not None");
    assert_eq!(back.document.dual_precision, Some(0));
    assert_eq!(
        back.document.inch_denominator,
        Some(0),
        "the notes: the denominator is deliberately NOT validated on the way in — \
         the formatter refuses it loudly, so the reader must not quietly drop it"
    );

    // The per-annotation override trio (M1 adds `dual_precision` beside the
    // existing two). All three at their zero/empty edges.
    let doc = doc_with_annotations(vec![Annotation::Dimension {
        kind: DimensionKind::Distance,
        anchors: vec![pid_ref(23)],
        value: Measured::FromGeometry,
        tolerance: None,
        precision: Some(0),
        dual_unit: Some(String::new()),
        dual_precision: Some(0),
        placement: Placement2::default(),
    }]);
    let (back, _) = reopen(&doc);
    let Annotation::Dimension {
        precision,
        dual_unit,
        dual_precision,
        ..
    } = &annotations_of(&back)[0]
    else {
        panic!("expected a dimension");
    };
    assert_eq!(*precision, Some(0), "per-annotation precision 0");
    assert_eq!(
        dual_unit.as_deref(),
        Some(""),
        "an empty dual unit is a present empty string, not an absent one"
    );
    assert_eq!(*dual_precision, Some(0), "per-annotation dual_precision 0");

    // An explicit RFS modifier. The notes call it "the default, written
    // explicitly", so `Some(Rfs)` and `None` are different statements.
    let frame = GeometricTolerance {
        characteristic: Characteristic::Flatness,
        value: ToleranceValue::length_meters(0.05 * MM),
        modifier: Some(MaterialCondition::Rfs),
        datums: Vec::new(),
        zone: ZoneShape::Width,
    };
    let (back, _) = reopen(&doc_with_annotations(vec![
        Annotation::FeatureControlFrame {
            tolerance: frame.clone(),
            anchor: pid_ref(24),
            placement: Placement2::default(),
        },
    ]));
    let Annotation::FeatureControlFrame { tolerance, .. } = &annotations_of(&back)[0] else {
        panic!("expected a frame");
    };
    assert_eq!(
        tolerance.modifier,
        Some(MaterialCondition::Rfs),
        "an explicitly stated RFS is not an unstated modifier"
    );
    assert_eq!(tolerance.zone, ZoneShape::Width, "the default zone, stored");
    assert!(
        tolerance.datums.is_empty(),
        "a form characteristic's empty datum list stays empty"
    );

    // A zero appearance channel.
    let mut tree = FeatureTree::new();
    tree.upsert_material(Material {
        name: "Matte black".to_string(),
        density_kg_m3: 1200.0,
        appearance: Some(Appearance {
            color: [0.0, 0.0, 0.0],
            metalness: Some(0.0),
            roughness: Some(0.0),
        }),
    })
    .unwrap();
    let part = Tab::part("Part 1", tree.clone());
    let active_tab = part.id.clone();
    let (back, _) = reopen(&WaffleDocument {
        document: DocumentMetadata::new("appearance"),
        sources: Vec::new(),
        tabs: vec![part],
        active_tab,
        extra: Map::new(),
    });
    let TabKind::Part { features, .. } = &back.tabs[0].kind else {
        panic!("expected a part tab");
    };
    assert_eq!(
        features.materials, tree.materials,
        "a black, non-metallic, perfectly smooth material is not a default one"
    );
}

#[test]
fn a_fit_class_persists_in_the_case_it_was_authored_in() {
    // The notes: "A `FitClass` persists as the string a drafter writes" and
    // "the letter's CASE carries no meaning (the role comes from which field
    // of `Fit` the class sits in) and `display_for(role)` prints it in the
    // canonical case, so a shaft class authored as `G6` still prints `g6`."
    // That contract says the FILE keeps `G`, and the printing is a view.
    let authored = FitClass::parse("G6").unwrap();
    assert_eq!(authored.letter, "G", "the fixture is the awkward case");
    let (back, _) = reopen(&doc_with_annotations(vec![dimension_with(
        Tolerance::Fit {
            hole: None,
            shaft: Some(authored.clone()),
        },
        DimensionKind::Diameter,
        31,
    )]));
    let Some(Tolerance::Fit { hole, shaft }) = tolerance_of(&annotations_of(&back)[0]) else {
        panic!("expected the fit arm");
    };
    assert_eq!(*hole, None, "the absent side stays absent");
    let shaft = shaft.as_ref().expect("the shaft class survived");
    assert_eq!(
        (shaft.letter.as_str(), shaft.grade),
        ("G", 6),
        "as authored — the canonical case is `display_for`'s job, not the file's"
    );
}

// ──────────────────────────────── 2. the feature control frame

#[test]
fn a_feature_control_frame_survives_with_its_datum_order() {
    let frame = GeometricTolerance {
        characteristic: Characteristic::Position,
        value: ToleranceValue::length_meters(0.05 * MM),
        modifier: Some(MaterialCondition::Mmc),
        // Primary, secondary, tertiary — precedence, not a set. Three
        // DISTINCT labels with distinct modifiers, so a reorder is visible.
        datums: vec![
            DatumRef {
                label: "A".to_string(),
                modifier: None,
            },
            DatumRef {
                label: "B".to_string(),
                modifier: Some(MaterialCondition::Mmc),
            },
            DatumRef {
                label: "C".to_string(),
                modifier: Some(MaterialCondition::Lmc),
            },
        ],
        zone: ZoneShape::Diametral,
    };
    assert!(frame.validate().is_ok(), "the fixture is a legal frame");

    let doc = doc_with_annotations(vec![Annotation::FeatureControlFrame {
        tolerance: frame.clone(),
        anchor: pid_ref(42),
        placement: Placement2::new(3.0, -4.0),
    }]);
    let (back, warnings) = reopen(&doc);
    let got = annotations_of(&back);
    assert_eq!(got.len(), 1);
    let Annotation::FeatureControlFrame {
        tolerance,
        anchor,
        placement,
    } = &got[0]
    else {
        panic!("the variant changed: {:?}", got[0]);
    };

    assert_eq!(tolerance.characteristic, Characteristic::Position);
    assert_eq!(tolerance.value.magnitude, 0.05 * MM);
    assert_eq!(tolerance.value.dimension, frame.value.dimension);
    assert_eq!(tolerance.modifier, Some(MaterialCondition::Mmc));
    assert_eq!(tolerance.zone, ZoneShape::Diametral);

    // The ORDER. A `HashMap<String, _>` or a set would still carry all three
    // labels and would silently reorder a frame's datum precedence, which is
    // a different inspection.
    let labels: Vec<&str> = tolerance.datums.iter().map(|d| d.label.as_str()).collect();
    assert_eq!(labels, vec!["A", "B", "C"], "primary, secondary, tertiary");
    assert_eq!(
        tolerance.datums, frame.datums,
        "each datum's own modifier travels with its position"
    );
    assert_ne!(
        tolerance.datums,
        {
            let mut r = frame.datums.clone();
            r.reverse();
            r
        },
        "the order is observable — the equality above is not vacuous"
    );

    // And the rest of the variant.
    // `Selector` is deliberately not `PartialEq` (see `Annotation`'s doc
    // comment), so the anchor is compared by its serialized form — which is
    // the form that is persisted and the only one equality would need to
    // agree with.
    assert_eq!(
        serde_json::to_value(anchor).unwrap(),
        serde_json::to_value(pid_ref(42)).unwrap(),
        "the frame's anchor survives"
    );
    assert_eq!((placement.dx, placement.dy), (3.0, -4.0));
    assert!(warnings.is_empty(), "no warning for a frame: {warnings:?}");
}

// ────────────────────────────── 3. materials and assignments

#[test]
fn the_material_table_and_the_body_assignments_survive_with_awkward_names() {
    // Three names a key-mangling writer would break: a space, a hyphen, and
    // non-ASCII (both a Latin-1 letter and a character outside it).
    let plain = "Aluminium";
    let spaced = "Aluminium 6061-T6";
    let non_ascii = "Messing – CuZn37 (Ø≤40)";

    let mut tree = FeatureTree::new();
    tree.upsert_material(Material::new(plain, 2700.0)).unwrap();
    tree.upsert_material(Material {
        name: spaced.to_string(),
        density_kg_m3: 2710.0,
        appearance: Some(Appearance {
            color: [0.7, 0.72, 0.75],
            metalness: Some(0.9),
            roughness: Some(0.35),
        }),
    })
    .unwrap();
    tree.upsert_material(Material::new(non_ascii, 8470.0))
        .unwrap();

    let body_a = "11111111-1111-4111-8111-111111111111/main";
    let body_b = "22222222-2222-4222-8222-222222222222/main";
    let body_c = "33333333-3333-4333-8333-333333333333/main";
    tree.set_body_material(body_a, Some(plain)).unwrap();
    tree.set_body_material(body_b, Some(spaced)).unwrap();
    tree.set_body_material(body_c, Some(non_ascii)).unwrap();
    // A fourth body with NO material: its absence must survive too, or a
    // reader would have to guess.
    let body_d = "44444444-4444-4444-8444-444444444444/main";

    let part = Tab::part("Part 1", tree.clone());
    let active_tab = part.id.clone();
    let doc = WaffleDocument {
        document: DocumentMetadata::new("materials"),
        sources: Vec::new(),
        tabs: vec![part],
        active_tab,
        extra: Map::new(),
    };

    let (back, warnings) = reopen(&doc);
    let TabKind::Part { features, .. } = &back.tabs[0].kind else {
        panic!("expected a part tab");
    };

    // The table, row for row and in order.
    assert_eq!(features.materials, tree.materials, "the table, in order");
    let names: Vec<&str> = features.materials.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec![plain, spaced, non_ascii]);
    assert_eq!(
        features.material(spaced).map(|m| m.density_kg_m3),
        Some(2710.0),
        "a name with a space is the key it is"
    );
    assert_eq!(
        features.material(non_ascii).map(|m| m.density_kg_m3),
        Some(8470.0),
        "a non-ASCII name is the key it is"
    );
    assert_eq!(
        features.material(spaced).and_then(|m| m.appearance.clone()),
        Some(Appearance {
            color: [0.7, 0.72, 0.75],
            metalness: Some(0.9),
            roughness: Some(0.35),
        }),
        "the appearance rides along"
    );

    // The assignments, resolved — which is the only thing that proves the
    // two halves still point at each other after the file.
    assert_eq!(features.body_materials, tree.body_materials);
    assert_eq!(features.density_of_body(body_a), Ok(Some(2700.0)));
    assert_eq!(features.density_of_body(body_b), Ok(Some(2710.0)));
    assert_eq!(features.density_of_body(body_c), Ok(Some(8470.0)));
    assert_eq!(
        features.density_of_body(body_d),
        Ok(None),
        "an unassigned body stays unassigned, not defaulted"
    );
    assert!(
        features.dangling_material_refs().is_empty(),
        "no assignment lost its material: {:?}",
        features.dangling_material_refs()
    );
    assert!(features.check_materials().is_ok());
    assert!(
        !warnings.iter().any(|w| w.contains("material")),
        "no warning about materials: {warnings:?}"
    );

    // A second trip is a fixed point.
    let (twice, _) = reopen(&back);
    let TabKind::Part { features: f2, .. } = &twice.tabs[0].kind else {
        panic!("expected a part tab");
    };
    assert_eq!(f2.materials, features.materials);
    assert_eq!(f2.body_materials, features.body_materials);
}

// ──────────────────────────────── 4. the six display settings

/// The six, each with a DISTINCT value, so a serializer that crossed two of
/// them over would be caught: the three numeric ones differ (3 / 5 / 32) and
/// the two flags differ (`true` / `false`).
fn all_six() -> DocumentMetadata {
    let mut m = DocumentMetadata::new("six settings");
    m.precision = Some(3);
    m.dual_unit = Some("in".to_string());
    m.dual_precision = Some(5);
    m.inch_fraction = Some(true);
    m.inch_denominator = Some(32);
    m.fit_band = Some(false);
    m
}

fn one_part_doc(document: DocumentMetadata) -> WaffleDocument {
    let part = Tab::part("Part 1", FeatureTree::new());
    let active_tab = part.id.clone();
    WaffleDocument {
        document,
        sources: Vec::new(),
        tabs: vec![part],
        active_tab,
        extra: Map::new(),
    }
}

#[test]
fn all_six_display_settings_survive_each_asserted_on_its_own() {
    let (back, warnings) = reopen(&one_part_doc(all_six()));
    let d = &back.document;
    assert_eq!(d.precision, Some(3), "precision");
    assert_eq!(d.dual_unit.as_deref(), Some("in"), "dual_unit");
    assert_eq!(d.dual_precision, Some(5), "dual_precision");
    assert_eq!(d.inch_fraction, Some(true), "inch_fraction");
    assert_eq!(d.inch_denominator, Some(32), "inch_denominator");
    assert_eq!(d.fit_band, Some(false), "fit_band");
    assert!(
        !warnings
            .iter()
            .any(|w| w.contains("precision") || w.contains("inch") || w.contains("fit")),
        "no warning about the display settings: {warnings:?}"
    );

    // The file itself, so a settings-shaped field that happened to be
    // reconstructed from somewhere else would still fail.
    let json: serde_json::Value = serde_json::from_str(&save_document(&back)).unwrap();
    assert_eq!(json["document"]["precision"], 3);
    assert_eq!(json["document"]["dual_unit"], "in");
    assert_eq!(json["document"]["dual_precision"], 5);
    assert_eq!(json["document"]["inch_fraction"], true);
    assert_eq!(json["document"]["inch_denominator"], 32);
    assert_eq!(json["document"]["fit_band"], false);
}

#[test]
fn each_display_setting_travels_alone_and_the_other_five_come_back_absent() {
    // A document that sets all six and checks all six passes even if two of
    // them are crossed over in the serializer. Six documents, each setting
    // exactly ONE, cannot: a crossover shows up as the wrong field carrying
    // the value and the intended one coming back None.
    type Probe = (
        &'static str,
        fn(&mut DocumentMetadata),
        fn(&DocumentMetadata) -> bool,
    );
    let probes: Vec<Probe> = vec![
        (
            "precision",
            |m| m.precision = Some(4),
            |m| m.precision == Some(4),
        ),
        (
            "dual_unit",
            |m| m.dual_unit = Some("mm".to_string()),
            |m| m.dual_unit.as_deref() == Some("mm"),
        ),
        (
            "dual_precision",
            |m| m.dual_precision = Some(4),
            |m| m.dual_precision == Some(4),
        ),
        (
            "inch_fraction",
            |m| m.inch_fraction = Some(true),
            |m| m.inch_fraction == Some(true),
        ),
        (
            "inch_denominator",
            |m| m.inch_denominator = Some(64),
            |m| m.inch_denominator == Some(64),
        ),
        (
            "fit_band",
            |m| m.fit_band = Some(true),
            |m| m.fit_band == Some(true),
        ),
    ];

    for (name, set, check) in &probes {
        let mut meta = DocumentMetadata::new("one setting");
        set(&mut meta);
        let (back, _) = reopen(&one_part_doc(meta));
        let d = &back.document;
        assert!(check(d), "{name} did not survive: {d:?}");

        // The other five are ABSENT — None, not a defaulted number. A
        // formatter has its own defaults, so "unset" and "explicitly the
        // default" are different states and the file must distinguish them.
        let others: Vec<(&str, bool)> = vec![
            ("precision", *name == "precision" || d.precision.is_none()),
            ("dual_unit", *name == "dual_unit" || d.dual_unit.is_none()),
            (
                "dual_precision",
                *name == "dual_precision" || d.dual_precision.is_none(),
            ),
            (
                "inch_fraction",
                *name == "inch_fraction" || d.inch_fraction.is_none(),
            ),
            (
                "inch_denominator",
                *name == "inch_denominator" || d.inch_denominator.is_none(),
            ),
            ("fit_band", *name == "fit_band" || d.fit_band.is_none()),
        ];
        for (other, ok) in others {
            assert!(
                ok,
                "with only {name} set, {other} must come back absent, got {d:?}"
            );
        }

        // And the bytes carry only the one key.
        let json: serde_json::Value = serde_json::from_str(&save_document(&back)).unwrap();
        let doc_obj = json["document"].as_object().unwrap();
        for key in [
            "precision",
            "dual_unit",
            "dual_precision",
            "inch_fraction",
            "inch_denominator",
            "fit_band",
        ] {
            assert_eq!(
                doc_obj.contains_key(key),
                key == *name,
                "with only {name} set, the file must carry {key}: {}",
                key == *name
            );
        }
    }
}

#[test]
fn an_explicit_false_is_not_an_absent_setting() {
    // "Explicitly decimal" and "unset" are different states for a formatter
    // that has its own default: `inch_fraction: Some(false)` must NOT collapse
    // to `None`, which is what a `skip_serializing_if = "is_false"`-shaped
    // mistake would do. Same for `fit_band`.
    let mut meta = DocumentMetadata::new("explicit false");
    meta.inch_fraction = Some(false);
    meta.fit_band = Some(false);
    let (back, _) = reopen(&one_part_doc(meta));
    assert_eq!(
        back.document.inch_fraction,
        Some(false),
        "Some(false) is not None"
    );
    assert_eq!(
        back.document.fit_band,
        Some(false),
        "Some(false) is not None"
    );

    let json: serde_json::Value = serde_json::from_str(&save_document(&back)).unwrap();
    let doc_obj = json["document"].as_object().unwrap();
    assert_eq!(
        doc_obj.get("inch_fraction"),
        Some(&serde_json::json!(false))
    );
    assert_eq!(doc_obj.get("fit_band"), Some(&serde_json::json!(false)));

    // And the three states are distinguishable end to end.
    let mut unset = DocumentMetadata::new("unset");
    unset.precision = Some(2); // something, so the document is not empty
    let (unset_back, _) = reopen(&one_part_doc(unset));
    assert_eq!(unset_back.document.inch_fraction, None);
    let mut on = DocumentMetadata::new("on");
    on.inch_fraction = Some(true);
    let (on_back, _) = reopen(&one_part_doc(on));
    assert_eq!(on_back.document.inch_fraction, Some(true));
}

// ───────── 5½. the persisted layout cache, which is what a kernel-less
//              reader actually draws

#[test]
fn the_persisted_view_layout_carries_the_resolved_tolerance_and_the_frame() {
    // A `DrawingView.cache` is persisted so "a reader with no kernel can
    // still draw the sheet complete". The M1 notes list "the resolved
    // `ToleranceLayout` on a persisted `AnnotationLayout`" among the additive
    // half — which means a reader that drops it draws the dimension with no
    // band and no sign that there was one. Measured through the file.
    let fit = Tolerance::Fit {
        hole: Some(FitClass::parse("H7").unwrap()),
        shaft: None,
    };
    let resolved = ToleranceLayout::resolve(&fit, DimensionKind::Diameter, 0.025)
        .expect("Ø25 H7 is tabulated");
    assert!(
        resolved.deviations.is_some() && resolved.limits.is_some(),
        "the fixture really carries numbers: {resolved:?}"
    );

    let frame = GeometricTolerance {
        characteristic: Characteristic::Perpendicularity,
        value: ToleranceValue::length_meters(0.02 * MM),
        modifier: Some(MaterialCondition::Lmc),
        datums: vec![DatumRef::new("B"), DatumRef::new("A")],
        zone: ZoneShape::Spherical,
    };

    let layout = ViewLayout {
        annotations: vec![
            AnnotationLayout::Dimension {
                kind: DimensionKind::Diameter,
                anchors: vec![AnchorGeometry::Point { at: [0.0, 0.0] }],
                value: 0.025,
                tolerance: Some(resolved.clone()),
                precision: Some(3),
                dual_unit: Some("in".to_string()),
                dual_precision: Some(4),
                placement: Placement2::new(0.5, 0.5),
            },
            AnnotationLayout::FeatureControlFrame {
                tolerance: frame.clone(),
                anchor: AnchorGeometry::Point { at: [1.0, 2.0] },
                placement: Placement2::default(),
            },
        ],
        ..ViewLayout::default()
    };

    let part = Tab::part("Part 1", FeatureTree::new());
    let part_id = part.id.clone();
    let mut view = DrawingView::new(
        "Front",
        ViewSource::whole_tab(part_id),
        Projection::Named {
            view: NamedView::Front,
        },
    );
    view.cache = Some(layout.clone());
    view.cache_key = Some("m1-review".to_string());
    let mut sheet = Sheet::new("Sheet 1");
    sheet.views = vec![view];
    let drawing_tab = Tab::drawing(
        "Drawing 1",
        Drawing {
            sheets: vec![sheet],
            ..Drawing::default()
        },
    );
    let active_tab = drawing_tab.id.clone();
    let doc = WaffleDocument {
        document: DocumentMetadata::new("layout cache"),
        sources: Vec::new(),
        tabs: vec![part, drawing_tab],
        active_tab,
        extra: Map::new(),
    };

    let (back, _) = reopen(&doc);
    let tab = back
        .tabs
        .iter()
        .find(|t| matches!(t.kind, TabKind::Drawing { .. }))
        .unwrap();
    let view = &tab.drawing_tree().unwrap().sheets[0].views[0];
    let cache = view.cache.as_ref().expect("the cache survived");
    assert_eq!(view.cache_key.as_deref(), Some("m1-review"));
    assert_eq!(cache.annotations.len(), 2);

    let AnnotationLayout::Dimension {
        tolerance,
        precision,
        dual_precision,
        ..
    } = &cache.annotations[0]
    else {
        panic!(
            "the dimension layout changed arm: {:?}",
            cache.annotations[0]
        );
    };
    let t = tolerance.as_ref().expect("the resolved band survived");
    assert_eq!(t.display, resolved.display, "the display form");
    assert!(
        matches!(&t.display, ToleranceDisplay::Fit { hole: Some(h), shaft: None } if h == "H7"),
        "the class text, in its canonical case: {:?}",
        t.display
    );
    assert_eq!(t.deviations, resolved.deviations, "the resolved deviations");
    assert_eq!(t.limits, resolved.limits, "the resolved limits");
    assert_eq!(*precision, Some(3));
    assert_eq!(*dual_precision, Some(4));

    let AnnotationLayout::FeatureControlFrame { tolerance, .. } = &cache.annotations[1] else {
        panic!("the frame layout changed arm: {:?}", cache.annotations[1]);
    };
    assert_eq!(tolerance, &frame, "the frame crosses verbatim");
    let labels: Vec<&str> = tolerance.datums.iter().map(|d| d.label.as_str()).collect();
    assert_eq!(labels, vec!["B", "A"], "and in the order it was authored");

    // `save_document_verified` is the production door (v4 §4 invariant 8),
    // and `SaveVerifier` is the one the host autosaves through. Neither may
    // refuse an M1 document.
    file_format::save_document_verified(&back).expect("the verified writer accepts it");
    let mut verifier = file_format::SaveVerifier::default();
    verifier
        .save(&back)
        .expect("the cached verifier accepts it too");
    verifier.save(&back).expect("…and again, off its own cache");
}

#[test]
fn the_single_tree_door_keeps_the_material_tables() {
    // `save_project` / `load_project` is the other public door (the assay
    // generators and every single-tree consumer). It round-trips a
    // `FeatureTree`, so both material tables have to come back — they live on
    // the tree, not on the document metadata.
    let mut tree = FeatureTree::new();
    tree.upsert_material(Material::new("Brass CuZn37", 8470.0))
        .unwrap();
    let body = "55555555-5555-4555-8555-555555555555/main";
    tree.set_body_material(body, Some("Brass CuZn37")).unwrap();

    let meta = file_format::ProjectMetadata::new("single tree");
    let json = file_format::save_project(&tree, &meta);
    let (back, _) = file_format::load_project(&json).expect("it reloads");
    assert_eq!(back.materials, tree.materials);
    assert_eq!(back.body_materials, tree.body_materials);
    assert_eq!(back.density_of_body(body), Ok(Some(8470.0)));
}

// ─────────────────── 6. a pre-M1 document opens quietly at the defaults

/// A v`version` document with no M1 field anywhere: no display settings, no
/// material table, no tolerance, no feature control frame.
fn pre_m1_document(version: u32) -> String {
    format!(
        r#"{{
      "format": "waffle-iron",
      "version": {version},
      "min_reader_version": {version},
      "document": {{ "id": "00000000-0000-4000-8000-00000000000{version:x}",
        "name": "Pre-M1",
        "created": "2026-10-01T00:00:00.000Z",
        "modified": "2026-10-01T00:00:00.000Z",
        "display_unit": "mm" }},
      "sources": [],
      "tabs": [
        {{ "id": "11111111-1111-4111-8111-111111111111", "name": "Part 1",
           "kind": {{ "type": "Part", "features": {{
             "features": [], "active_index": null }} }} }}
      ],
      "active_tab": "11111111-1111-4111-8111-111111111111"
    }}"#
    )
}

#[test]
fn a_pre_m1_document_opens_at_the_defaults_with_no_spurious_warning() {
    for version in [11u32, 12] {
        let json = pre_m1_document(version);
        let loaded = load_document(&json)
            .unwrap_or_else(|e| panic!("a v{version} document must still open: {e}"));

        // All six display settings absent — not defaulted to a number, which
        // would make the document claim a precision its author never chose.
        let d = &loaded.document.document;
        assert_eq!(d.precision, None, "v{version} precision");
        assert_eq!(d.dual_unit, None, "v{version} dual_unit");
        assert_eq!(d.dual_precision, None, "v{version} dual_precision");
        assert_eq!(d.inch_fraction, None, "v{version} inch_fraction");
        assert_eq!(d.inch_denominator, None, "v{version} inch_denominator");
        assert_eq!(d.fit_band, None, "v{version} fit_band");
        // The pre-M1 field it DOES carry still arrives, so the document was
        // really read rather than replaced by a default.
        assert_eq!(d.display_unit.as_deref(), Some("mm"));

        // Both material tables empty.
        let TabKind::Part { features, .. } = &loaded.document.tabs[0].kind else {
            panic!("expected a part tab");
        };
        assert!(features.materials.is_empty(), "v{version} materials");
        assert!(
            features.body_materials.is_empty(),
            "v{version} body_materials"
        );

        // And NOTHING in the warnings about any of it. A warning that fires on
        // every pre-M1 file trains its reader to ignore warnings.
        for w in &loaded.warnings {
            let lower = w.to_lowercase();
            for word in [
                "material",
                "toleran",
                "precision",
                "dual",
                "inch",
                "fit",
                "density",
            ] {
                assert!(
                    !lower.contains(word),
                    "v{version} warned about `{word}` on a pre-M1 file: {w}"
                );
            }
        }

        // Re-saved, it is a v14 file that still says nothing about M1.
        let again: serde_json::Value = serde_json::from_str(&save_document(&loaded.document))
            .expect("a migrated document re-saves");
        assert_eq!(again["version"], file_format::FORMAT_VERSION);
        let doc_obj = again["document"].as_object().unwrap();
        for key in [
            "precision",
            "dual_unit",
            "dual_precision",
            "inch_fraction",
            "inch_denominator",
            "fit_band",
        ] {
            assert!(
                !doc_obj.contains_key(key),
                "v{version}: a pre-M1 document must not grow a {key} key on save"
            );
        }
        let features_obj = again["tabs"][0]["kind"]["features"].as_object().unwrap();
        assert!(!features_obj.contains_key("materials"));
        assert!(!features_obj.contains_key("body_materials"));
    }
}

// ───────────────────────────────────── the floor itself, after the merge

/// The floor is coherent, and the "one ahead" arm is written RELATIVE to it.
///
/// M1 was assigned v14 at dispatch, and that is still the version its own
/// wire breaks are documented under — but the floor the branch SHIPS is v15,
/// D4e's, which merged after M1 and bumped it again. So this test asserts the
/// constants against `FORMAT_VERSION + 1` rather than against a literal 15:
/// pinning the next increment's number here would make a test that has
/// nothing to do with it go red on the next bump, which is the opposite of
/// what a floor-coherence test is for. The literals that DO belong to M1
/// (version 14 in the wire-break fixtures of `format_tests.rs`) stay where
/// they are, because those measure a pre-M1 reader's refusal and are about
/// v14 specifically.
#[test]
fn the_format_floor_is_coherent_and_refuses_one_version_ahead() {
    // Both constants move together, and the reader floor is never ahead of
    // the writer. The absolute value is deliberately NOT asserted — see above.
    assert_eq!(
        file_format::MIN_READER_VERSION,
        file_format::FORMAT_VERSION,
        "every bump since v5 has moved both; a reader floor behind the writer \
         would let this build write files it cannot read back"
    );
    // A COMPILE-TIME assertion, the idiom `save.rs` already uses for the
    // pair's own coherence: both operands are constants, so a runtime
    // `assert!` here is a constant-value assertion clippy rightly refuses.
    // This way the floor going backwards fails the BUILD rather than a test.
    const _: () = assert!(
        file_format::FORMAT_VERSION >= 15,
        "the floor is at least v15 — M1's own v14, then D4e's v15 on top"
    );

    let here = file_format::FORMAT_VERSION;
    let ahead_v = here + 1;

    // A document from one version ahead is refused CLEANLY — by the envelope
    // check, naming both numbers, rather than by a serde error from inside a
    // tab it should never have started reading.
    for ahead in [
        (ahead_v, ahead_v),
        // `version` alone, and `min_reader_version` alone, each trip it: the
        // check takes the max.
        (ahead_v, 0),
        (here, ahead_v),
    ] {
        let json = format!(
            r#"{{
          "format": "waffle-iron",
          "version": {},
          "min_reader_version": {},
          "document": {{ "id": "00000000-0000-4000-8000-000000000001", "name": "Future",
            "created": "2026-10-04T00:00:00.000Z", "modified": "2026-10-04T00:00:00.000Z" }},
          "sources": [],
          "tabs": [ {{ "id": "t1", "name": "Part 1",
            "kind": {{ "type": "Part", "features": {{ "features": [], "active_index": null }} }} }} ],
          "active_tab": "t1"
        }}"#,
            ahead.0, ahead.1
        );
        match load_document(&json) {
            Err(LoadError::FutureVersion {
                file_version,
                supported_version,
            }) => {
                assert_eq!(file_version, ahead_v, "{ahead:?}");
                assert_eq!(supported_version, here, "{ahead:?}");
            }
            other => panic!("v{ahead:?} must be a clean FutureVersion, got {other:?}"),
        }
    }

    // And what this build WRITES loads back, which is the other half of
    // coherence: a floor that refuses its own output would pass every
    // assertion above.
    assert!(load_document(&save_document(&one_part_doc(all_six()))).is_ok());
}
