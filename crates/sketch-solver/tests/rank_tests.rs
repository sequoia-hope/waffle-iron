use nalgebra::{DMatrix, DVector};
use sketch_solver::core::rank::{analyze_rank};

#[test]
fn test_full_rank_2x2() {
    // 2x2 identity matrix
    let jacobian = DMatrix::from_diagonal(&DVector::from_element(2, 1.0));
    let residual = DVector::from_element(2, 0.0);
    let tolerance = 1e-7;

    let analysis = analyze_rank(&jacobian, &residual, tolerance);

    assert_eq!(analysis.rank, 2);
    assert_eq!(analysis.dof, 0);
    assert!(analysis.conflicting_rows.is_empty());
}

#[test]
fn test_rank_deficient_3x2() {
    // 3x2 matrix with duplicate row (rows 0 and 1 are same)
    // [ 1.0, 0.0 ]
    // [ 1.0, 0.0 ]
    // [ 0.0, 1.0 ]
    let jacobian = DMatrix::from_row_slice(3, 2, &[
        1.0, 0.0,
        1.0, 0.0,
        0.0, 1.0,
    ]);
    // Consistent residual (duplicate row has same residual)
    let residual = DVector::from_row_slice(&[0.5, 0.5, 1.0]);
    let tolerance = 1e-7;

    let analysis = analyze_rank(&jacobian, &residual, tolerance);

    assert_eq!(analysis.rank, 2);
    assert_eq!(analysis.dof, 0);
    assert!(analysis.conflicting_rows.is_empty(), "Consistent duplicate row should not be conflicting");
}

#[test]
fn test_under_constrained_1x2() {
    // 1x2 matrix (1 equation, 2 unknowns)
    let jacobian = DMatrix::from_row_slice(1, 2, &[1.0, 0.0]);
    let residual = DVector::from_element(1, 0.0);
    let tolerance = 1e-7;

    let analysis = analyze_rank(&jacobian, &residual, tolerance);

    assert_eq!(analysis.rank, 1);
    assert_eq!(analysis.dof, 1);
    assert!(analysis.conflicting_rows.is_empty());
}

#[test]
fn test_over_constrained_conflicting_3x2() {
    // 3x2 matrix with contradictory residual
    // [ 1.0, 0.0 ] -> x = 1.0
    // [ 0.0, 1.0 ] -> y = 1.0
    // [ 1.0, 1.0 ] -> x + y = 3.0 (Conflicting!)
    let jacobian = DMatrix::from_row_slice(3, 2, &[
        1.0, 0.0,
        0.0, 1.0,
        1.0, 1.0,
    ]);
    let residual = DVector::from_row_slice(&[1.0, 1.0, 3.0]);
    let tolerance = 1e-7;

    let analysis = analyze_rank(&jacobian, &residual, tolerance);

    assert_eq!(analysis.rank, 2);
    assert_eq!(analysis.dof, 0);
    assert!(!analysis.conflicting_rows.is_empty());
    // All 3 rows are involved in the conflict
    assert_eq!(analysis.conflicting_rows, vec![0, 1, 2]);
}

#[test]
fn test_empty_matrix() {
    let jacobian = DMatrix::from_row_slice(0, 0, &[]);
    let residual = DVector::from_row_slice(&[]);
    let tolerance = 1e-7;

    let analysis = analyze_rank(&jacobian, &residual, tolerance);

    assert_eq!(analysis.rank, 0);
    assert_eq!(analysis.dof, 0);
    assert!(analysis.conflicting_rows.is_empty());
}

#[test]
fn test_near_zero_singular_value() {
    // Near-zero singular value should be treated as zero based on eps threshold
    let jacobian = DMatrix::from_row_slice(2, 2, &[
        1.0, 0.0,
        0.0, 1e-18, // Much smaller than f64::EPSILON * 1.0
    ]);
    let residual = DVector::from_element(2, 0.0);
    let tolerance = 1e-7;

    let analysis = analyze_rank(&jacobian, &residual, tolerance);

    assert_eq!(analysis.rank, 1);
    assert_eq!(analysis.dof, 1);
}
