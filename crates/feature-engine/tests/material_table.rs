//! M1's material table: the invariants, the coupled edits, and why the
//! absence of a material is a refusal rather than a default
//! (`specs/drawings_and_mbd.md` §9).
//!
//! The mass that comes OUT of a material is pinned against real geometry in
//! `crates/wasm-bridge/tests/measurement_expr.rs`
//! (`mass_measures_the_material_and_refuses_without_one`). What is pinned
//! here is the table itself, which needs no kernel: a density that is not a
//! density, two materials with one name, an assignment to a material that is
//! not there, and the three edits that have to move two tables at once.

use feature_engine::types::{Appearance, FeatureTree, Material};

const BODY: &str = "11111111-1111-4111-8111-111111111111/main";
const OTHER: &str = "22222222-2222-4222-8222-222222222222/main";

fn aluminium() -> Material {
    Material::new("Aluminium", 2700.0)
}

#[test]
fn a_density_that_is_not_a_density_is_refused_at_the_boundary() {
    let mut tree = FeatureTree::new();
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let err = tree
            .upsert_material(Material::new("Nonsense", bad))
            .expect_err("a non-positive density is not a density");
        assert!(err.contains("positive"), "{err}");
    }
    // A material with no name is refused too: the name IS the reference, so
    // an unnamed one could never be assigned to anything.
    assert!(tree.upsert_material(Material::new("   ", 2700.0)).is_err());
    assert!(tree.materials.is_empty(), "nothing was added");
    assert!(tree.upsert_material(aluminium()).is_ok());
}

#[test]
fn an_appearance_channel_out_of_range_is_refused_rather_than_clamped() {
    let mut tree = FeatureTree::new();
    let mut m = aluminium();
    m.appearance = Some(Appearance {
        color: [1.2, 0.0, 0.0],
        metalness: None,
        roughness: None,
    });
    // Clamped, 1.2 would silently become 1.0 — a colour nobody authored,
    // and the author never hears about the typo.
    assert!(tree.upsert_material(m.clone()).is_err());
    m.appearance = Some(Appearance {
        color: [1.0, 0.0, 0.0],
        metalness: Some(1.5),
        roughness: None,
    });
    assert!(tree.upsert_material(m.clone()).is_err());
    m.appearance = Some(Appearance {
        color: [1.0, 0.0, 0.0],
        metalness: Some(0.9),
        roughness: Some(0.2),
    });
    assert!(tree.upsert_material(m).is_ok());
}

#[test]
fn an_upsert_replaces_by_name_and_the_second_name_is_refused() {
    let mut tree = FeatureTree::new();
    assert!(tree.upsert_material(aluminium()).unwrap().is_none());
    let replaced = tree
        .upsert_material(Material::new("Aluminium", 2710.0))
        .unwrap()
        .expect("the old entry comes back for undo");
    assert_eq!(replaced.density_kg_m3, 2700.0);
    assert_eq!(tree.materials.len(), 1, "replaced, not appended");
    assert_eq!(tree.material("Aluminium").unwrap().density_kg_m3, 2710.0);

    // A hand-built table with two entries of one name is caught by
    // `check_materials`, which is what the engine's editing boundary runs:
    // the second would be unreachable, because a material is referred to by
    // name.
    tree.materials.push(Material::new("Aluminium", 1.0));
    let err = tree.check_materials().expect_err("two of one name");
    assert!(err.contains("two materials"), "{err}");
}

#[test]
fn a_body_with_no_material_has_no_density_and_that_is_not_an_error() {
    let tree = FeatureTree::new();
    assert_eq!(tree.density_of_body(BODY), Ok(None));
    assert_eq!(tree.material_of_body(BODY).unwrap(), None);
}

#[test]
fn an_assignment_to_a_material_that_is_not_there_is_refused() {
    let mut tree = FeatureTree::new();
    let err = tree
        .set_body_material(BODY, Some("Aluminium"))
        .expect_err("not in the table");
    assert!(err.contains("material table first"), "{err}");
    assert!(tree.body_materials.is_empty());

    tree.upsert_material(aluminium()).unwrap();
    assert_eq!(tree.set_body_material(BODY, Some("Aluminium")), Ok(None));
    assert_eq!(tree.density_of_body(BODY), Ok(Some(2700.0)));
    // Clearing returns what it replaced, for undo.
    assert_eq!(
        tree.set_body_material(BODY, None),
        Ok(Some("Aluminium".to_string()))
    );
    assert_eq!(tree.density_of_body(BODY), Ok(None));
}

#[test]
fn a_dangling_assignment_refuses_by_name_rather_than_defaulting_its_density() {
    // The shape a hand-edited file (or a half-applied edit) can be in. The
    // body points at nothing, and the honest answer is an error naming the
    // material — NOT `DEFAULT_DENSITY_KG_M3`, at which the mass would be
    // numerically the volume.
    let mut tree = FeatureTree::new();
    tree.body_materials
        .insert(BODY.to_string(), "Unobtanium".to_string());
    let err = tree.density_of_body(BODY).expect_err("dangling");
    assert!(err.contains("Unobtanium"), "{err}");
    assert!(err.contains(BODY), "{err}");
    assert_eq!(
        tree.dangling_material_refs(),
        vec![(BODY.to_string(), "Unobtanium".to_string())]
    );
    assert!(tree.check_materials().is_err());
}

#[test]
fn a_material_whose_density_went_bad_takes_its_bodies_down_loudly() {
    // A document can only reach this by being hand-edited, and when it does
    // the body must not be measured against a density the type system says
    // is impossible.
    let mut tree = FeatureTree::new();
    tree.upsert_material(aluminium()).unwrap();
    tree.set_body_material(BODY, Some("Aluminium")).unwrap();
    tree.materials[0].density_kg_m3 = 0.0;
    let err = tree.density_of_body(BODY).expect_err("a zero density");
    assert!(err.contains("positive"), "{err}");
}

#[test]
fn deleting_a_material_clears_every_body_that_pointed_at_it() {
    // Leaving the assignments would turn each into the dangling reference
    // above, so deleting one material would break the mass of bodies the
    // author never touched.
    let mut tree = FeatureTree::new();
    tree.upsert_material(aluminium()).unwrap();
    tree.upsert_material(Material::new("Steel", 7850.0))
        .unwrap();
    tree.set_body_material(BODY, Some("Aluminium")).unwrap();
    tree.set_body_material(OTHER, Some("Steel")).unwrap();

    let (removed, orphaned) = tree.remove_material("Aluminium").expect("it was there");
    assert_eq!(removed.density_kg_m3, 2700.0);
    assert_eq!(orphaned, vec![BODY.to_string()]);
    assert_eq!(
        tree.density_of_body(BODY),
        Ok(None),
        "cleared, not dangling"
    );
    assert_eq!(
        tree.density_of_body(OTHER),
        Ok(Some(7850.0)),
        "the other body is untouched"
    );
    assert!(tree.dangling_material_refs().is_empty());
    assert!(tree.remove_material("Aluminium").is_none(), "already gone");
}

#[test]
fn renaming_a_material_carries_every_body_made_of_it() {
    let mut tree = FeatureTree::new();
    tree.upsert_material(aluminium()).unwrap();
    tree.upsert_material(Material::new("Steel", 7850.0))
        .unwrap();
    tree.set_body_material(BODY, Some("Aluminium")).unwrap();
    tree.set_body_material(OTHER, Some("Steel")).unwrap();

    let moved = tree.rename_material("Aluminium", "6061-T6").unwrap();
    assert_eq!(moved, vec![BODY.to_string()]);
    assert_eq!(tree.density_of_body(BODY), Ok(Some(2700.0)));
    assert!(tree.material("Aluminium").is_none());
    assert!(
        tree.dangling_material_refs().is_empty(),
        "both halves moved"
    );

    // Onto a name already taken: refused, and nothing moves.
    let err = tree.rename_material("6061-T6", "Steel").expect_err("taken");
    assert!(err.contains("already has a material"), "{err}");
    assert_eq!(tree.density_of_body(BODY), Ok(Some(2700.0)));
    // A name that is not there, and a no-op rename.
    assert!(tree.rename_material("Brass", "Bronze").is_err());
    assert_eq!(tree.rename_material("Steel", "Steel"), Ok(Vec::new()));
    assert!(tree.rename_material("Steel", "  ").is_err());
}

#[test]
fn both_tables_are_absent_from_the_json_until_something_is_in_them() {
    // Additive both ways: a document with no material writes no key, so its
    // bytes are unchanged from before M1.
    let tree = FeatureTree::new();
    let json = serde_json::to_string(&tree).unwrap();
    assert!(!json.contains("materials"), "{json}");
    assert!(!json.contains("body_materials"), "{json}");

    // ...and a document from before M1 reads with both empty.
    let old = r#"{"features":[],"active_index":null}"#;
    let back: FeatureTree = serde_json::from_str(old).unwrap();
    assert!(back.materials.is_empty());
    assert!(back.body_materials.is_empty());
}

#[test]
fn the_tables_round_trip_through_serde_with_their_appearance() {
    let mut tree = FeatureTree::new();
    let mut m = aluminium();
    m.appearance = Some(Appearance {
        color: [0.7, 0.72, 0.75],
        metalness: Some(0.9),
        roughness: Some(0.35),
    });
    tree.upsert_material(m.clone()).unwrap();
    tree.set_body_material(BODY, Some("Aluminium")).unwrap();

    let json = serde_json::to_string(&tree).unwrap();
    let back: FeatureTree = serde_json::from_str(&json).unwrap();
    assert_eq!(back.materials, vec![m]);
    assert_eq!(back.body_materials, tree.body_materials);
    assert_eq!(back.density_of_body(BODY), Ok(Some(2700.0)));
    // An absent appearance is omitted rather than written as null.
    let plain = {
        let mut t = FeatureTree::new();
        t.upsert_material(aluminium()).unwrap();
        serde_json::to_string(&t).unwrap()
    };
    assert!(!plain.contains("appearance"), "{plain}");
}
