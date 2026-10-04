//! M1's claim that a material edit is ONE undo record covering a COUPLED
//! edit of both tables (`specs/drawings_and_mbd.md` §9: "Three material edits
//! are COUPLED, and that is why there is one undo record").
//!
//! `tests/material_table.rs` pins the tree-level operations —
//! `remove_material` clears the bodies, `rename_material` carries them,
//! `set_body_material` refuses a material that is not there. None of it goes
//! through `Engine::edit_materials`, which is the only caller the app has
//! (`wasm_bridge::dispatch`), and which is where the two claims that matter
//! live: one undo step for a two-table edit, and a refused edit that leaves
//! the document whole. Before this file `edit_materials` had no test at all
//! (`grep -rn edit_materials crates/ --include=*.rs` finds the definition and
//! one dispatch call site).
//!
//! The body ids here are synthetic. `body_materials` is keyed by
//! `FeatureTree::body_id` (`"{feature_id}/{output_key}"`) and nothing in the
//! material path resolves them against geometry — `set_body_material` checks
//! the material table, not the body list — so the coupling is measurable
//! without a rebuild, and a kernel in the loop would only add noise. The mass
//! that comes out of a material is pinned against real geometry in
//! `crates/wasm-bridge/tests/measurement_expr.rs`.

use feature_engine::types::{FeatureTree, Material};
use feature_engine::Engine;
use waffle_types::kernel::MockKernel;

const BODY_A: &str = "11111111-1111-4111-8111-111111111111/main";
const BODY_B: &str = "22222222-2222-4222-8222-222222222222/main";
const BODY_C: &str = "33333333-3333-4333-8333-333333333333/main";

/// Both tables, as one comparable value. `materials` is a `Vec` (order is
/// part of it) and `body_materials` a `HashMap`, so this is the pair an
/// "exactly as before" assertion has to compare.
type Snapshot = (Vec<Material>, Vec<(String, String)>);

/// One way for an edit to be refused: a closure `edit_materials` is handed.
type Refusal = fn(&mut FeatureTree) -> Result<(), String>;

fn snapshot(tree: &FeatureTree) -> Snapshot {
    let mut assignments: Vec<(String, String)> = tree
        .body_materials
        .iter()
        .map(|(b, m)| (b.clone(), m.clone()))
        .collect();
    assignments.sort();
    (tree.materials.clone(), assignments)
}

/// An engine whose part has two materials and three assigned bodies: TWO
/// bodies share `Aluminium`, so deleting it has to move two rows of one table
/// and one of the other.
fn engine_with_two_materials() -> (Engine, MockKernel) {
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    engine
        .edit_materials(&mut kernel, |tree| -> Result<(), String> {
            tree.upsert_material(Material::new("Aluminium", 2700.0))?;
            tree.upsert_material(Material::new("Steel", 7850.0))?;
            tree.set_body_material(BODY_A, Some("Aluminium"))?;
            tree.set_body_material(BODY_B, Some("Aluminium"))?;
            tree.set_body_material(BODY_C, Some("Steel"))?;
            Ok(())
        })
        .expect("the setup edit is legal");
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
    (engine, kernel)
}

#[test]
fn deleting_a_material_two_bodies_use_undoes_in_one_step() {
    let (mut engine, mut kernel) = engine_with_two_materials();
    let before = snapshot(&engine.tree);
    assert_eq!(before.0.len(), 2);
    assert_eq!(before.1.len(), 3);
    assert_eq!(engine.tree.density_of_body(BODY_A), Ok(Some(2700.0)));
    assert_eq!(engine.tree.density_of_body(BODY_B), Ok(Some(2700.0)));

    // The delete. One `edit_materials` call, which moves BOTH tables: the row
    // leaves `materials` and the two bodies pointing at it leave
    // `body_materials` (leaving them would make each a dangling reference).
    let orphaned = engine
        .edit_materials(&mut kernel, |tree| -> Result<Vec<String>, String> {
            let (_, orphaned) = tree
                .remove_material("Aluminium")
                .ok_or_else(|| "Aluminium was not there".to_string())?;
            Ok(orphaned)
        })
        .expect("deleting a material is legal");
    assert_eq!(
        orphaned,
        vec![BODY_A.to_string(), BODY_B.to_string()],
        "both bodies were cleared, and the edit says which"
    );
    assert!(engine.tree.material("Aluminium").is_none());
    assert_eq!(engine.tree.density_of_body(BODY_A), Ok(None));
    assert_eq!(engine.tree.density_of_body(BODY_B), Ok(None));
    assert_eq!(
        engine.tree.density_of_body(BODY_C),
        Ok(Some(7850.0)),
        "the body made of Steel is untouched"
    );
    assert_ne!(snapshot(&engine.tree), before, "the edit did something");

    // ONE undo. Both halves come back — not the table row alone, which would
    // leave two bodies silently un-materialed.
    engine.undo(&mut kernel).expect("one undo step");
    assert_eq!(
        snapshot(&engine.tree),
        before,
        "one undo restores BOTH tables exactly as they were"
    );
    assert_eq!(engine.tree.density_of_body(BODY_A), Ok(Some(2700.0)));
    assert_eq!(engine.tree.density_of_body(BODY_B), Ok(Some(2700.0)));
    assert_eq!(engine.tree.density_of_body(BODY_C), Ok(Some(7850.0)));
    assert!(engine.tree.check_materials().is_ok());
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);

    // And it really was ONE record: the next undo rewinds the SETUP edit, so
    // the delete did not leave a second half behind on the stack.
    engine.undo(&mut kernel).expect("the setup edit undoes too");
    assert_eq!(
        snapshot(&engine.tree),
        (Vec::new(), Vec::new()),
        "the setup edit was one record as well"
    );
    assert!(!engine.can_undo(), "exactly two records for two edits");

    // Redo walks forward over the same two records.
    engine.redo(&mut kernel).unwrap();
    assert_eq!(snapshot(&engine.tree), before);
    engine.redo(&mut kernel).unwrap();
    assert!(engine.tree.material("Aluminium").is_none());
    assert_eq!(engine.tree.density_of_body(BODY_A), Ok(None));
}

#[test]
fn renaming_a_material_also_undoes_in_one_step() {
    // The other coupled edit §9 names: a rename rewrites every body that
    // points at the material, so restoring the table row alone would leave
    // them spelling a name the table no longer has.
    let (mut engine, mut kernel) = engine_with_two_materials();
    let before = snapshot(&engine.tree);

    let moved = engine
        .edit_materials(&mut kernel, |tree| -> Result<Vec<String>, String> {
            tree.rename_material("Aluminium", "6061-T6")
        })
        .expect("a rename is legal");
    assert_eq!(moved, vec![BODY_A.to_string(), BODY_B.to_string()]);
    assert_eq!(engine.tree.density_of_body(BODY_A), Ok(Some(2700.0)));
    assert!(engine.tree.dangling_material_refs().is_empty());

    engine.undo(&mut kernel).expect("one undo step");
    assert_eq!(
        snapshot(&engine.tree),
        before,
        "one undo restores the row AND every body that pointed at it"
    );
}

#[test]
fn a_refused_edit_leaves_neither_table_moved_and_records_nothing() {
    let (mut engine, mut kernel) = engine_with_two_materials();
    let before = snapshot(&engine.tree);

    // Four ways to be refused. The first is the closure's own error (an
    // assignment to a material that is not in the table); the other three
    // leave the tables in a state `check_materials` refuses, reaching the
    // engine's post-edit validation rather than the operation's own guard —
    // which is the path that has to roll BOTH halves back.
    let refusals: Vec<(&str, Refusal)> = vec![
        ("assign a material that is not in the table", |tree| {
            tree.set_body_material(BODY_A, Some("Unobtanium"))?;
            Ok(())
        }),
        ("an edit that half-succeeds before failing", |tree| {
            // Steel is deleted (both tables move), and THEN the edit
            // fails. Without the rollback the document would keep the
            // delete, which is the "half-changed" §9 forbids.
            tree.remove_material("Steel");
            tree.set_body_material(BODY_A, Some("Unobtanium"))?;
            Ok(())
        }),
        ("a duplicate material name", |tree| {
            // Pushed directly: `upsert_material` replaces by name, so the
            // only way to reach two rows of one name is the hand-built shape
            // `check_materials` exists to catch.
            tree.materials.push(Material::new("Steel", 1.0));
            Ok(())
        }),
        ("a dangling assignment", |tree| {
            tree.body_materials
                .insert(BODY_A.to_string(), "Unobtanium".to_string());
            Ok(())
        }),
    ];

    for (what, edit) in refusals {
        let err = engine
            .edit_materials(&mut kernel, edit)
            .expect_err("refused: {what}");
        assert!(!err.is_empty(), "{what}: the refusal names something");
        assert_eq!(
            snapshot(&engine.tree),
            before,
            "{what}: NEITHER table moved — a refused edit cannot leave the \
             document half-changed (the refusal said: {err})"
        );
        assert!(engine.tree.check_materials().is_ok(), "{what}");
    }

    // Nothing was recorded for any of the four: the only undo record is still
    // the setup edit, and undoing it empties both tables.
    engine.undo(&mut kernel).expect("the setup edit");
    assert_eq!(snapshot(&engine.tree), (Vec::new(), Vec::new()));
    assert!(
        !engine.can_undo(),
        "a refused edit must not push an undo record"
    );
}

#[test]
fn a_refused_edit_on_a_fresh_engine_records_nothing_at_all() {
    // The sharpest form of "records nothing": an engine with no history. If a
    // refused edit pushed a no-op record, `can_undo` would be true here.
    let mut kernel = MockKernel::new();
    let mut engine = Engine::new();
    let err = engine
        .edit_materials(&mut kernel, |tree| -> Result<(), String> {
            tree.set_body_material(BODY_A, Some("Aluminium"))?;
            Ok(())
        })
        .expect_err("there is no Aluminium to assign");
    assert!(err.contains("material table first"), "{err}");
    assert!(engine.tree.materials.is_empty());
    assert!(engine.tree.body_materials.is_empty());
    assert!(
        !engine.can_undo(),
        "a refused edit leaves no record to undo"
    );
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
}
