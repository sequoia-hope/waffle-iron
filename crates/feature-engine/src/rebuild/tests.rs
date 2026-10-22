use super::*;

#[cfg(test)]
mod depth_mode_tests {
    use super::*;

    /// The P0012 shape, as arithmetic: a body lying ENTIRELY behind the sketch
    /// plane, whose cut therefore sweeps the other way. Through-all must clear
    /// the body at every authoring scale — the old `extent + 1.0` floored at an
    /// absolute 1 m cleared it at ×1e-3 and missed it by 44 m at ×1e3.
    #[test]
    fn through_all_clears_a_body_behind_the_plane_at_every_scale() {
        for scale in [1e-3, 1.0, 1e3] {
            // Along the sketch normal the body sits at [-50 s, -5 s]; the cut
            // reverses, so along the SWEEP it sits at [5 s, 50 s].
            let swept = Some((5.0 * scale, 50.0 * scale));
            let depth = through_all_depth(swept, scale).expect("a reachable body resolves");
            assert!(
                depth > 50.0 * scale,
                "scale {scale}: depth {depth} must pass the body's far side at {}",
                50.0 * scale
            );
            // The overshoot is RELATIVE: 1e-2 of the sweep's own reach
            // (max(span, far) = 50 s here), so depth/scale is identical at
            // every scale. An absolute margin could not be.
            assert!(
                (depth / scale - 50.5).abs() < 1e-9,
                "scale {scale}: depth/scale {} is not the scale-invariant 50.5",
                depth / scale
            );
        }
    }

    /// Measured along the UNREVERSED normal the same body is behind the plane
    /// and NO depth reaches it. That is a loud stop, never a cutter that
    /// silently removes nothing (P0012's silent wrong).
    #[test]
    fn through_all_refuses_a_body_it_cannot_reach() {
        let err = through_all_depth(Some((-50.0, -5.0)), 1.0)
            .expect_err("a body entirely behind the sweep has no through-all depth");
        let text = format!("{err:?}");
        assert!(
            text.contains("ThroughAll") && text.contains("entirely behind"),
            "expected a loud ThroughAll refusal, got {text}"
        );
    }

    /// A body that straddles the plane is swept from the plane past its far
    /// side only — through-all is one-directional, and the near side is not
    /// counted into the depth.
    #[test]
    fn through_all_measures_the_far_side_not_the_span() {
        let depth = through_all_depth(Some((-4.0, 6.0)), 0.0).unwrap();
        assert!((depth - (6.0 + 10.0 * 1e-2)).abs() < 1e-12, "got {depth}");
    }

    /// Nothing to go through: the authored depth, which is at the model's own
    /// scale — never an absolute 100 m default.
    #[test]
    fn through_all_without_a_body_falls_back_to_the_authored_depth() {
        assert_eq!(through_all_depth(None, 7.5).unwrap(), 7.5);
        assert!(through_all_depth(None, 0.0).is_err());
    }

    /// `UpTo` is measured along the direction the extrude actually sweeps, so
    /// a reference BEHIND the sketch plane resolves for a cut the reversal
    /// turned around (and is still refused when it is behind the sweep).
    #[test]
    fn up_to_is_measured_along_the_sweep() {
        let origin = [0.0, 0.0, 10.0];
        let reference = [0.0, 0.0, 2.0]; // behind the plane along +z
        assert!(up_to_depth(origin, [0.0, 0.0, 1.0], reference).is_err());
        let depth = up_to_depth(origin, [0.0, 0.0, -1.0], reference).unwrap();
        assert!((depth - 8.0).abs() < 1e-12, "got {depth}");
    }

    /// A direction that cannot be normalized is refused, not silently treated
    /// as +z.
    #[test]
    fn a_degenerate_direction_has_no_unit() {
        assert!(unit_or_none([0.0, 0.0, 0.0]).is_none());
        let u = unit_or_none([0.0, 0.0, -4.0]).unwrap();
        assert_eq!(u, [0.0, 0.0, -1.0]);
    }
}

#[cfg(test)]
mod plane_and_sketch_tests {
    use super::*;
    use waffle_types::{GeomRef, ResolvePolicy, Selector};

    /// Helper: compute cross product ref x n manually for verification.
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

    fn length(v: [f64; 3]) -> f64 {
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    }

    /// A normal already unit to f64 rounding (assay R0081's sketch plane,
    /// |n| − 1 = −1.1e-16) must reach the kernel bit-identical: renormalizing
    /// it shifted every tilted frame by one ulp and flipped R0081 to ERROR.
    #[test]
    fn unit_normal_keeps_a_normal_that_is_unit_to_rounding() {
        let n = [
            -0.7519142519864097,
            0.47533675347182963,
            -0.4568149827431001,
        ];
        assert!(
            (length(n) - 1.0).abs() > 0.0,
            "fixture must not be exactly unit"
        );
        let u = unit_normal(n);
        for k in 0..3 {
            assert_eq!(u[k].to_bits(), n[k].to_bits(), "component {k} changed");
        }
    }

    /// The F1 defect class, a 6-decimal rounding of a unit vector, is still
    /// normalized.
    #[test]
    fn unit_normal_normalizes_a_six_decimal_normal() {
        let n = [0.718286, 0.0, 0.695747];
        assert!((length(n) - 1.0).abs() > 1e-7);
        assert!((length(unit_normal(n)) - 1.0).abs() <= 2.0 * f64::EPSILON);
    }

    // -- Branch table tests: one per row --
    // These directly test the JS formula: ref = |n.z| < 0.99 ? Z : X; xAxis = ref x n
    // Old buggy code produced different results for n=[0,0,1].

    #[test]
    fn xy_plane_normal_matches_js() {
        // n=[0,0,1]: |n.z|=1.0 >= 0.99, so ref=[1,0,0]
        // xAxis = [1,0,0] x [0,0,1] = [0*1-0*0, 0*0-1*1, 1*0-0*0] = [0,-1,0]
        let result = tangent_x_from_normal([0.0, 0.0, 1.0]);
        assert!(
            (result[0]).abs() < 1e-10
                && (result[1] - (-1.0)).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "XY plane: expected [0,-1,0], got {:?}",
            result
        );
    }

    #[test]
    fn xy_plane_flipped_normal_matches_js() {
        // n=[0,0,-1]: |n.z|=1.0 >= 0.99, so ref=[1,0,0]
        // xAxis = [1,0,0] x [0,0,-1] = [0*(-1)-0*0, 0*0-1*(-1), 1*0-0*0] = [0,1,0]
        let result = tangent_x_from_normal([0.0, 0.0, -1.0]);
        assert!(
            (result[0]).abs() < 1e-10
                && (result[1] - 1.0).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "XY flipped: expected [0,1,0], got {:?}",
            result
        );
    }

    #[test]
    fn xz_plane_normal_matches_js() {
        // n=[0,1,0]: |n.z|=0.0 < 0.99, so ref=[0,0,1]
        // xAxis = [0,0,1] x [0,1,0] = [0*0-1*1, 1*0-0*0, 0*1-0*0] = [-1,0,0]
        let result = tangent_x_from_normal([0.0, 1.0, 0.0]);
        assert!(
            (result[0] - (-1.0)).abs() < 1e-10
                && (result[1]).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "XZ plane: expected [-1,0,0], got {:?}",
            result
        );
    }

    #[test]
    fn yz_plane_normal_matches_js() {
        // n=[1,0,0]: |n.z|=0.0 < 0.99, so ref=[0,0,1]
        // xAxis = [0,0,1] x [1,0,0] = [0*0-1*0, 1*1-0*0, 0*0-0*1] = [0,1,0]
        let result = tangent_x_from_normal([1.0, 0.0, 0.0]);
        assert!(
            (result[0]).abs() < 1e-10
                && (result[1] - 1.0).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "YZ plane: expected [0,1,0], got {:?}",
            result
        );
    }

    #[test]
    fn xz_plane_flipped_normal() {
        // n=[0,-1,0]: |n.z|=0.0 < 0.99, so ref=[0,0,1]
        // xAxis = [0,0,1] x [0,-1,0] = [0*0-1*(-1), 1*0-0*0, 0*(-1)-0*0] = [1,0,0]
        let result = tangent_x_from_normal([0.0, -1.0, 0.0]);
        assert!(
            (result[0] - 1.0).abs() < 1e-10
                && (result[1]).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "XZ flipped: expected [1,0,0], got {:?}",
            result
        );
    }

    #[test]
    fn yz_plane_flipped_normal() {
        // n=[-1,0,0]: |n.z|=0.0 < 0.99, so ref=[0,0,1]
        // xAxis = [0,0,1] x [-1,0,0] = [0*0-1*0, 1*(-1)-0*0, 0*0-0*(-1)] = [0,-1,0]
        let result = tangent_x_from_normal([-1.0, 0.0, 0.0]);
        assert!(
            (result[0]).abs() < 1e-10
                && (result[1] - (-1.0)).abs() < 1e-10
                && (result[2]).abs() < 1e-10,
            "YZ flipped: expected [0,-1,0], got {:?}",
            result
        );
    }

    // -- Invariant tests --

    #[test]
    fn perpendicularity_invariant_all_axis_normals() {
        let normals = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        for n in &normals {
            let x = tangent_x_from_normal(*n);
            let d = dot(x, *n);
            assert!(
                d.abs() < 1e-10,
                "Perpendicularity violated for n={:?}: dot={}, x={:?}",
                n,
                d,
                x
            );
            let l = length(x);
            assert!(
                (l - 1.0).abs() < 1e-10,
                "Unit length violated for n={:?}: |x|={}, x={:?}",
                n,
                l,
                x
            );
        }
    }

    #[test]
    fn degenerate_zero_normal_returns_fallback() {
        // Zero-length normal should produce fallback [1,0,0]
        let result = tangent_x_from_normal([0.0, 0.0, 0.0]);
        assert_eq!(
            result,
            [1.0, 0.0, 0.0],
            "Zero normal should fallback to [1,0,0]"
        );
    }

    // -- Cross-validation: verify against manual cross product --

    #[test]
    fn result_matches_manual_cross_product() {
        let normals: [[f64; 3]; 4] = [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ];
        for n in &normals {
            let ref_vec = if n[2].abs() < 0.99 {
                [0.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            let cx = cross(ref_vec, *n);
            let l = length(cx);
            let expected = if l < 1e-12 {
                [1.0, 0.0, 0.0]
            } else {
                [cx[0] / l, cx[1] / l, cx[2] / l]
            };
            let result = tangent_x_from_normal(*n);
            for i in 0..3 {
                assert!(
                    (result[i] - expected[i]).abs() < 1e-10,
                    "Mismatch for n={:?}: result={:?}, expected={:?}",
                    n,
                    result,
                    expected
                );
            }
        }
    }

    // -- Adversarial: mutation-detecting tests --

    #[test]
    fn flipping_cross_product_order_changes_sign() {
        // If someone changes ref x n to n x ref, the sign flips.
        // For n=[0,0,1], ref=[1,0,0]:
        //   ref x n = [0,-1,0]
        //   n x ref = [0,+1,0]
        // Our function must return [0,-1,0], not [0,+1,0].
        let result = tangent_x_from_normal([0.0, 0.0, 1.0]);
        assert!(
            result[1] < 0.0,
            "xAxis.y must be negative for n=[0,0,1]; got {:?} (would be positive if cross order reversed)",
            result
        );
    }

    #[test]
    fn near_threshold_normal() {
        // n=[0, 0.14, 0.99]: |n.z| = 0.99, exactly at threshold boundary
        // With < 0.99, this picks ref=X (since 0.99 is NOT < 0.99)
        let n = [0.0, 0.14, 0.99];
        let result = tangent_x_from_normal(n);
        // Must be perpendicular regardless of branch
        let d = dot(result, n);
        assert!(
            d.abs() < 1e-6,
            "Near-threshold: perpendicularity failed, dot={}",
            d
        );
        let l = length(result);
        assert!(
            (l - 1.0).abs() < 1e-6,
            "Near-threshold: unit length failed, |x|={}",
            l
        );

        // At exactly 0.99, |n.z| < 0.99 is FALSE, so ref=[1,0,0]
        let ref_vec = [1.0, 0.0, 0.0]; // because 0.99 is NOT < 0.99
        let expected_cross = cross(ref_vec, n);
        let el = length(expected_cross);
        let expected = [
            expected_cross[0] / el,
            expected_cross[1] / el,
            expected_cross[2] / el,
        ];
        for i in 0..3 {
            assert!(
                (result[i] - expected[i]).abs() < 1e-6,
                "Near-threshold: component {} mismatch: got {}, expected {}",
                i,
                result[i],
                expected[i]
            );
        }
    }

    #[test]
    fn oblique_45_degree_normal() {
        // 45-degree oblique: n = normalize([1,1,1])
        let s = 1.0 / (3.0f64).sqrt();
        let n = [s, s, s];
        let result = tangent_x_from_normal(n);

        // |n.z| = 1/sqrt(3) ~ 0.577 < 0.99, so ref=[0,0,1]
        let d = dot(result, n);
        assert!(d.abs() < 1e-10, "45deg: perpendicularity failed, dot={}", d);
        let l = length(result);
        assert!(
            (l - 1.0).abs() < 1e-10,
            "45deg: unit length failed, |x|={}",
            l
        );

        // Verify against manual cross: [0,0,1] x [s,s,s] = [-s, s, 0] normalized
        let cx = cross([0.0, 0.0, 1.0], n);
        let cl = length(cx);
        let expected = [cx[0] / cl, cx[1] / cl, cx[2] / cl];
        for i in 0..3 {
            assert!(
                (result[i] - expected[i]).abs() < 1e-10,
                "45deg: component {} mismatch",
                i
            );
        }
    }

    #[test]
    fn changing_threshold_to_half_breaks_near_threshold() {
        // If threshold were changed to 0.5 instead of 0.99,
        // then n=[0, 0.14, 0.99] would use ref=Z instead of ref=X.
        // This test verifies the actual function uses the 0.99 threshold
        // by checking that n=[0,0,0.98] (just under threshold) uses ref=Z.
        let n_under = [0.0, 0.0, 0.98];
        let result_under = tangent_x_from_normal(n_under);
        // |n.z|=0.98 < 0.99, so ref=[0,0,1]
        // cross([0,0,1], [0,0,0.98]) = [0*0.98-1*0, 1*0-0*0.98, 0*0-0*0] = [0,0,0]
        // This is degenerate (nearly parallel), should fallback to [1,0,0]
        // Actually: cross is near-zero, so fallback applies
        assert_eq!(
            result_under,
            [1.0, 0.0, 0.0],
            "n=[0,0,0.98] should hit Z branch and produce degenerate cross -> fallback"
        );

        // n=[0,0,0.995] is above threshold, uses ref=X
        let n_over = [0.0, 0.0, 0.995];
        let result_over = tangent_x_from_normal(n_over);
        // |n.z|=0.995 >= 0.99, so ref=[1,0,0]
        // cross([1,0,0], [0,0,0.995]) = [0*0.995-0*0, 0*0-1*0.995, 1*0-0*0] = [0,-0.995,0]
        // Normalized: [0,-1,0]
        assert!(
            result_over[1] < -0.99,
            "n=[0,0,0.995] should use X branch; got {:?}",
            result_over
        );
    }

    // -- DatumPlane tests --

    use crate::types::{DatumPlaneParams, PlaneDefinition};

    fn make_datum_plane_feature(name: &str, definition: PlaneDefinition) -> Feature {
        Feature {
            id: Uuid::new_v4(),
            name: name.to_string(),
            operation: Operation::DatumPlane {
                params: DatumPlaneParams {
                    name: name.to_string(),
                    definition,
                },
            },
            suppressed: false,
            references: Vec::new(),
        }
    }

    #[test]
    fn test_datum_plane_point_normal() {
        let feature = make_datum_plane_feature(
            "Custom Plane",
            PlaneDefinition::PointNormal {
                origin: [1.0, 2.0, 3.0],
                origin_expr: None,
                normal: [0.0, 1.0, 0.0],
            },
        );
        let tree = FeatureTree {
            features: vec![feature.clone()],
            active_index: None,
            ..Default::default()
        };
        let results = HashMap::new();
        let result = execute_feature(
            &feature,
            &mut waffle_types::kernel::MockKernel::new(),
            &results,
            &tree,
            &std::collections::HashSet::new(),
            &SourceStore::new(),
            None,
        );
        assert!(result.is_ok(), "PointNormal datum plane should succeed");
        assert!(
            result.unwrap().outputs.is_empty(),
            "DatumPlane produces no outputs"
        );
    }

    #[test]
    fn test_datum_plane_offset_from_builtin() {
        let front_id: Uuid = FRONT_PLANE_ID.parse().unwrap();
        let def = PlaneDefinition::Offset {
            base_plane_id: front_id,
            distance: 10.0,
            distance_expr: None,
        };
        let feature = make_datum_plane_feature("Offset from Front", def.clone());
        let tree = FeatureTree {
            features: vec![feature.clone()],
            active_index: None,
            ..Default::default()
        };
        let results = HashMap::new();

        // Verify execution succeeds
        let result = execute_feature(
            &feature,
            &mut waffle_types::kernel::MockKernel::new(),
            &results,
            &tree,
            &std::collections::HashSet::new(),
            &SourceStore::new(),
            None,
        );
        assert!(result.is_ok(), "Offset from built-in should succeed");

        // Verify resolution
        let mk = waffle_types::kernel::MockKernel::new();
        let (origin, normal) =
            resolve_plane_definition(&def, &tree, &results, mk.as_introspect()).unwrap();
        assert!(
            (origin[2] - 10.0).abs() < 1e-10,
            "Origin Z should be offset by 10"
        );
        assert!((normal[2] - 1.0).abs() < 1e-10, "Normal should be [0,0,1]");
    }

    /// Build a box solid (rectangle sketch → extrude) and return the rebuild
    /// state plus the extrude feature id, so face-base datum tests have real
    /// MockKernel faces to resolve against.
    fn make_box_rebuild() -> (FeatureTree, Uuid, RebuildState) {
        use waffle_types::SketchEntity;
        let sketch_id = Uuid::new_v4();
        let sketch = make_deserialized_sketch(
            sketch_id,
            vec![
                SketchEntity::Point {
                    id: 1,
                    x: 0.0,
                    y: 0.0,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 2,
                    x: 0.01,
                    y: 0.0,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 3,
                    x: 0.01,
                    y: 0.005,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 4,
                    x: 0.0,
                    y: 0.005,
                    construction: false,
                },
                SketchEntity::Line {
                    id: 5,
                    start_id: 1,
                    end_id: 2,
                    construction: false,
                },
                SketchEntity::Line {
                    id: 6,
                    start_id: 2,
                    end_id: 3,
                    construction: false,
                },
                SketchEntity::Line {
                    id: 7,
                    start_id: 3,
                    end_id: 4,
                    construction: false,
                },
                SketchEntity::Line {
                    id: 8,
                    start_id: 4,
                    end_id: 1,
                    construction: false,
                },
            ],
        );
        let (tree, extrude_id) = make_sketch_extrude_tree(sketch);
        let mut kb = waffle_types::kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(
            &tree,
            &mut kb,
            0,
            &Changed::All,
            &existing,
            &[],
            Carried::default(),
            &SourceStore::new(),
            None,
        );
        (tree, extrude_id, state)
    }

    #[test]
    fn test_offset_from_face_matches_equivalent_offset_from_plane() {
        // A face-base offset datum plane must resolve to (base-face plane
        // origin + distance*normal), and must produce the SAME plane as an
        // explicit PointNormal offset from that same base face plane. This is
        // the dual-resolver agreement contract in plane space: same normal,
        // and the offset origin lies the correct signed distance off the face
        // along its normal.
        let (tree, extrude_id, state) = make_box_rebuild();
        assert!(
            !state.errors.iter().any(|(id, _)| *id == extrude_id),
            "box extrude should succeed: {:?}",
            state.errors
        );

        // Build the kernel fresh and re-resolve so we hold a live introspect.
        let mut kb = waffle_types::kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(
            &tree,
            &mut kb,
            0,
            &Changed::All,
            &existing,
            &[],
            Carried::default(),
            &SourceStore::new(),
            None,
        );
        let extrude_result = state
            .feature_results
            .get(&extrude_id)
            .expect("extrude result");

        // Pick a planar face from the extrude provenance.
        let face = extrude_result
            .provenance
            .created
            .iter()
            .find(|e| {
                e.kind == TopoKind::Face && e.signature.surface_type.as_deref() == Some("planar")
            })
            .expect("extrude should create at least one planar face");

        // GeomRef pinned to that exact face by its own signature.
        let face_ref = GeomRef {
            kind: TopoKind::Face,
            anchor: waffle_types::Anchor::FeatureOutput {
                feature_id: extrude_id,
                output_key: OutputKey::Main,
            },
            selector: Selector::Signature {
                signature: face.signature.clone(),
            },
            policy: ResolvePolicy::Strict,
            scope: None,
        };

        let introspect = kb.as_introspect();
        let distance = 0.02_f64;

        // Resolve the base face plane directly (the "must match" reference).
        let (base_origin, base_normal) =
            resolve_face_plane(&face_ref, &state.feature_results, introspect).unwrap();

        // (a) offset-from-face
        let off_face = PlaneDefinition::OffsetFromFace {
            distance_expr: None,
            base: face_ref.clone(),
            distance,
        };
        let (face_origin, face_n) =
            resolve_plane_definition(&off_face, &tree, &state.feature_results, introspect).unwrap();

        // (b) equivalent explicit PointNormal offset from the same base plane.
        let expected_origin = [
            base_origin[0] + base_normal[0] * distance,
            base_origin[1] + base_normal[1] * distance,
            base_origin[2] + base_normal[2] * distance,
        ];

        // Normals identical.
        for k in 0..3 {
            assert!(
                (face_n[k] - base_normal[k]).abs() < 1e-12,
                "normal mismatch axis {}: {} vs {}",
                k,
                face_n[k],
                base_normal[k]
            );
        }
        // Origin = base origin + distance*normal.
        for k in 0..3 {
            assert!(
                (face_origin[k] - expected_origin[k]).abs() < 1e-12,
                "origin mismatch axis {}: {} vs {}",
                k,
                face_origin[k],
                expected_origin[k]
            );
        }

        // The resolved offset origin lies exactly `distance` off the face
        // along the normal: (origin - base_origin)·normal == distance.
        let signed = (face_origin[0] - base_origin[0]) * base_normal[0]
            + (face_origin[1] - base_origin[1]) * base_normal[1]
            + (face_origin[2] - base_origin[2]) * base_normal[2];
        assert!(
            (signed - distance).abs() < 1e-12,
            "signed offset should equal distance: {} vs {}",
            signed,
            distance
        );

        // A negative distance flips to the back side (no separate variant).
        let off_back = PlaneDefinition::OffsetFromFace {
            distance_expr: None,
            base: face_ref,
            distance: -distance,
        };
        let (back_origin, _) =
            resolve_plane_definition(&off_back, &tree, &state.feature_results, introspect).unwrap();
        let signed_back = (back_origin[0] - base_origin[0]) * base_normal[0]
            + (back_origin[1] - base_origin[1]) * base_normal[1]
            + (back_origin[2] - base_origin[2]) * base_normal[2];
        assert!(
            (signed_back + distance).abs() < 1e-12,
            "negative distance should offset to the back side"
        );
    }

    #[test]
    fn test_offset_from_non_planar_face_is_resolution_failed() {
        // A non-planar base face must be a loud ResolutionFailed, never a
        // guessed plane. We fillet the box to introduce a cylindrical face
        // (MockKernel models a fillet face as "cylindrical"), then attempt to
        // base an offset datum plane on it.
        use waffle_types::kernel::Kernel;

        let (_tree, extrude_id, state) = make_box_rebuild();
        let extrude_result = state
            .feature_results
            .get(&extrude_id)
            .expect("extrude result");
        let handle = extrude_result
            .outputs
            .first()
            .map(|(_, body)| body.handle.clone())
            .expect("extrude main output");

        // Re-run on a fresh kernel so the handle is live in `kb`.
        let mut kb = waffle_types::kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(
            &_tree,
            &mut kb,
            0,
            &Changed::All,
            &existing,
            &[],
            Carried::default(),
            &SourceStore::new(),
            None,
        );
        let handle = state
            .feature_results
            .get(&extrude_id)
            .and_then(|r| r.outputs.first())
            .map(|(_, body)| body.handle.clone())
            .unwrap_or(handle);

        let edges = kb.as_introspect().list_edges(&handle);
        let edge0 = *edges.first().expect("box has edges");
        let filleted = kb.fillet_edges(&handle, &[edge0], 0.001).unwrap();

        let intro = kb.as_introspect();
        let faces = intro.list_faces(&filleted);
        let cyl_face = faces
            .iter()
            .copied()
            .find(|&f| {
                intro
                    .compute_signature(f, TopoKind::Face)
                    .surface_type
                    .as_deref()
                    == Some("cylindrical")
            })
            .expect("fillet should create a cylindrical face");

        // Expose that real kernel face through a synthetic OpResult so
        // resolve_with_fallback can find it by signature.
        let feat_id = Uuid::new_v4();
        let cyl_sig = intro.compute_signature(cyl_face, TopoKind::Face);
        let mut results = HashMap::new();
        results.insert(
            feat_id,
            modeling_ops::OpResult {
                outputs: vec![],
                provenance: modeling_ops::Provenance {
                    created: vec![modeling_ops::EntityRecord {
                        kernel_id: cyl_face,
                        kind: TopoKind::Face,
                        signature: cyl_sig.clone(),
                    }],
                    deleted: vec![],
                    modified: vec![],
                    role_assignments: vec![],
                },
                diagnostics: modeling_ops::Diagnostics::default(),
            },
        );

        let cyl_ref = GeomRef {
            kind: TopoKind::Face,
            anchor: waffle_types::Anchor::FeatureOutput {
                feature_id: feat_id,
                output_key: OutputKey::Main,
            },
            selector: Selector::Signature { signature: cyl_sig },
            policy: ResolvePolicy::Strict,
            scope: None,
        };

        let err = resolve_face_plane(&cyl_ref, &results, kb.as_introspect()).unwrap_err();
        assert!(
            matches!(err, EngineError::ResolutionFailed { .. }),
            "non-planar base must be ResolutionFailed, got: {:?}",
            err
        );
        let msg = err.to_string();
        assert!(
            msg.contains("not planar"),
            "error should be loud about non-planar base, got: {}",
            msg
        );
    }

    #[test]
    fn test_datum_plane_zero_normal_error() {
        let feature = make_datum_plane_feature(
            "Bad Plane",
            PlaneDefinition::PointNormal {
                origin: [0.0, 0.0, 0.0],
                origin_expr: None,
                normal: [0.0, 0.0, 0.0],
            },
        );
        let tree = FeatureTree {
            features: vec![feature.clone()],
            active_index: None,
            ..Default::default()
        };
        let results = HashMap::new();
        let result = execute_feature(
            &feature,
            &mut waffle_types::kernel::MockKernel::new(),
            &results,
            &tree,
            &std::collections::HashSet::new(),
            &SourceStore::new(),
            None,
        );
        assert!(result.is_err(), "Zero normal should fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("zero-length"),
            "Error should mention zero-length: {}",
            err
        );
    }

    #[test]
    fn test_datum_plane_missing_base_error() {
        let missing_id = Uuid::new_v4();
        let feature = make_datum_plane_feature(
            "Offset from Missing",
            PlaneDefinition::Offset {
                base_plane_id: missing_id,
                distance: 5.0,
                distance_expr: None,
            },
        );
        let tree = FeatureTree {
            features: vec![feature.clone()],
            active_index: None,
            ..Default::default()
        };
        let results = HashMap::new();
        let result = execute_feature(
            &feature,
            &mut waffle_types::kernel::MockKernel::new(),
            &results,
            &tree,
            &std::collections::HashSet::new(),
            &SourceStore::new(),
            None,
        );
        assert!(result.is_err(), "Missing base plane should fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found"),
            "Error should mention not found: {}",
            err
        );
    }

    // -- Deserialized sketch (empty solved_profiles/solved_positions) tests --
    // These simulate loading a .waffle file where solved_profiles and solved_positions
    // are empty because they are #[serde(default, skip_serializing)].

    /// Helper: create a Sketch with empty derived data (simulating deserialization).
    fn make_deserialized_sketch(id: Uuid, entities: Vec<waffle_types::SketchEntity>) -> Sketch {
        Sketch {
            id,
            plane: waffle_types::GeomRef {
                kind: waffle_types::TopoKind::Face,
                anchor: waffle_types::Anchor::Datum {
                    datum_id: FRONT_PLANE_ID.parse().unwrap(),
                },
                selector: waffle_types::Selector::Role {
                    role: waffle_types::Role::EndCapPositive,
                    index: 0,
                },
                policy: waffle_types::ResolvePolicy::Strict,
                scope: None,
            },
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: None,
            entities,
            constraints: vec![],
            solve_status: waffle_types::SolveStatus::FullyConstrained,
            // Simulating deserialization: these are empty because skip_serializing
            solved_positions: HashMap::new(),
            solved_profiles: vec![],
            projected: vec![],
            plane_face: None,
        }
    }

    /// Helper: create a sketch Feature + extrude Feature pair for testing.
    fn make_sketch_extrude_tree(sketch: Sketch) -> (FeatureTree, Uuid) {
        let sketch_id = sketch.id;
        let extrude_id = Uuid::new_v4();
        let tree = FeatureTree {
            features: vec![
                Feature {
                    id: sketch_id,
                    name: "Sketch1".to_string(),
                    operation: Operation::Sketch { sketch },
                    suppressed: false,
                    references: vec![],
                },
                Feature {
                    id: extrude_id,
                    name: "Extrude1".to_string(),
                    operation: Operation::Extrude {
                        params: crate::types::ExtrudeParams {
                            combine: None,
                            targets: None,
                            sketch_id,
                            profile_index: 0,
                            profile_entity_ids: None,
                            depth: 0.01,
                            depth_expr: None,
                            direction: None,
                            symmetric: false,
                            cut: false,
                            merge: false,
                            target_body: None,
                            depth_mode: crate::types::DepthMode::Blind,
                            second_direction: None,
                            region: None,
                            regions: Vec::new(),
                        },
                    },
                    suppressed: false,
                    references: vec![],
                },
            ],
            active_index: None,
            ..Default::default()
        };
        (tree, extrude_id)
    }

    #[test]
    fn rebuild_recomputes_profiles_for_circle_sketch() {
        // A circle sketch loaded from a .waffle file has empty solved_profiles
        // and solved_positions (they are skip_serializing). Rebuild must
        // recompute them before attempting to extrude.
        use waffle_types::SketchEntity;

        let sketch_id = Uuid::new_v4();
        let sketch = make_deserialized_sketch(
            sketch_id,
            vec![
                SketchEntity::Point {
                    id: 1,
                    x: 0.0,
                    y: 0.0,
                    construction: false,
                },
                SketchEntity::Circle {
                    id: 2,
                    center_id: 1,
                    radius: 0.005, // 5mm in meters
                    construction: false,
                },
            ],
        );

        let (tree, extrude_id) = make_sketch_extrude_tree(sketch);

        let mut kb = waffle_types::kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(
            &tree,
            &mut kb,
            0,
            &Changed::All,
            &existing,
            &[],
            Carried::default(),
            &SourceStore::new(),
            None,
        );

        // The extrude should succeed — profiles should have been recomputed from entities.
        // Currently this fails because solved_profiles is empty after deserialization.
        let extrude_failed = state.errors.iter().any(|(id, _)| *id == extrude_id);
        assert!(
            !extrude_failed,
            "Extrude should succeed after profile recomputation, but got errors: {:?}",
            state.errors
        );
        assert!(
            state.feature_results.contains_key(&extrude_id),
            "Extrude feature should have a result"
        );
    }

    #[test]
    fn rebuild_recomputes_profiles_for_rectangle_sketch() {
        // A rectangle sketch (4 points, 4 lines) loaded from a .waffle file.
        // solved_profiles and solved_positions are empty after deserialization.
        use waffle_types::SketchEntity;

        let sketch_id = Uuid::new_v4();
        let sketch = make_deserialized_sketch(
            sketch_id,
            vec![
                // Four corner points of a 10mm x 5mm rectangle
                SketchEntity::Point {
                    id: 1,
                    x: 0.0,
                    y: 0.0,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 2,
                    x: 0.01,
                    y: 0.0,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 3,
                    x: 0.01,
                    y: 0.005,
                    construction: false,
                },
                SketchEntity::Point {
                    id: 4,
                    x: 0.0,
                    y: 0.005,
                    construction: false,
                },
                // Four lines forming the rectangle
                SketchEntity::Line {
                    id: 5,
                    start_id: 1,
                    end_id: 2,
                    construction: false,
                },
                SketchEntity::Line {
                    id: 6,
                    start_id: 2,
                    end_id: 3,
                    construction: false,
                },
                SketchEntity::Line {
                    id: 7,
                    start_id: 3,
                    end_id: 4,
                    construction: false,
                },
                SketchEntity::Line {
                    id: 8,
                    start_id: 4,
                    end_id: 1,
                    construction: false,
                },
            ],
        );

        let (tree, extrude_id) = make_sketch_extrude_tree(sketch);

        let mut kb = waffle_types::kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(
            &tree,
            &mut kb,
            0,
            &Changed::All,
            &existing,
            &[],
            Carried::default(),
            &SourceStore::new(),
            None,
        );

        // The extrude should succeed — profiles should have been recomputed.
        // Currently this fails because solved_profiles is empty after deserialization.
        let extrude_failed = state.errors.iter().any(|(id, _)| *id == extrude_id);
        assert!(
            !extrude_failed,
            "Extrude should succeed after profile recomputation, but got errors: {:?}",
            state.errors
        );
        assert!(
            state.feature_results.contains_key(&extrude_id),
            "Extrude feature should have a result"
        );
    }

    #[test]
    fn rebuild_gear_sketch_works_without_precomputed_profiles() {
        // Gear sketches get expand_gears() called during rebuild, which populates
        // solved_profiles and solved_positions. This test confirms that baseline
        // behavior still works even when starting from empty derived data.
        use waffle_types::SketchEntity;

        let sketch_id = Uuid::new_v4();
        let sketch = make_deserialized_sketch(
            sketch_id,
            vec![SketchEntity::Gear {
                id: 1,
                params: waffle_types::GearParams {
                    tooth_count: 8,
                    module: 0.01,
                    ..Default::default()
                },
                construction: false,
            }],
        );

        let (tree, extrude_id) = make_sketch_extrude_tree(sketch);

        let mut kb = waffle_types::kernel::MockKernel::new();
        let existing = HashMap::new();
        let state = rebuild(
            &tree,
            &mut kb,
            0,
            &Changed::All,
            &existing,
            &[],
            Carried::default(),
            &SourceStore::new(),
            None,
        );

        // Gear sketches should work because expand_gears() populates profiles.
        let extrude_failed = state.errors.iter().any(|(id, _)| *id == extrude_id);
        assert!(
            !extrude_failed,
            "Gear extrude should succeed via expand_gears(), but got errors: {:?}",
            state.errors
        );
        assert!(
            state.feature_results.contains_key(&extrude_id),
            "Gear extrude feature should have a result"
        );
    }

    #[test]
    fn test_datum_plane_offset_chain() {
        // Create a user DatumPlane, then offset from it
        let first_plane = make_datum_plane_feature(
            "Plane A",
            PlaneDefinition::PointNormal {
                origin: [0.0, 0.0, 5.0],
                origin_expr: None,
                normal: [0.0, 0.0, 1.0],
            },
        );
        let second_plane_def = PlaneDefinition::Offset {
            base_plane_id: first_plane.id,
            distance: 7.0,
            distance_expr: None,
        };
        let second_plane = Feature {
            id: Uuid::new_v4(),
            name: "Plane B".to_string(),
            operation: Operation::DatumPlane {
                params: DatumPlaneParams {
                    name: "Plane B".to_string(),
                    definition: second_plane_def.clone(),
                },
            },
            suppressed: false,
            references: Vec::new(),
        };

        let tree = FeatureTree {
            features: vec![first_plane.clone(), second_plane.clone()],
            active_index: None,
            ..Default::default()
        };

        // Build results for first plane
        let mut results = HashMap::new();
        let r1 = execute_feature(
            &first_plane,
            &mut waffle_types::kernel::MockKernel::new(),
            &results,
            &tree,
            &std::collections::HashSet::new(),
            &SourceStore::new(),
            None,
        )
        .unwrap();
        results.insert(first_plane.id, r1);

        // Resolve the second plane's definition
        let mk = waffle_types::kernel::MockKernel::new();
        let (origin, normal) =
            resolve_plane_definition(&second_plane_def, &tree, &results, mk.as_introspect())
                .unwrap();
        assert!(
            (origin[2] - 12.0).abs() < 1e-10,
            "Z should be 5+7=12, got {}",
            origin[2]
        );
        assert!(
            (normal[2] - 1.0).abs() < 1e-10,
            "Normal should still be [0,0,1]"
        );
    }

    /// Projection integration (incr 4): a sketch point bound to a box vertex
    /// reprojects to that vertex, and tracks it when an upstream edit moves it.
    #[test]
    fn reproject_tracks_moved_source_vertex() {
        use modeling_ops::{BodyOutput, Diagnostics, Provenance};
        use waffle_types::kernel::{Kernel, KernelSolidHandle, MockKernel};
        use waffle_types::{
            Anchor, ClosedProfile, GeomRef, ProjectedEntity, ProjectedKind, ProjectedSource,
            ResolvePolicy, SolveStatus,
        };

        fn box_kernel(corners: [(f64, f64); 4], height: f64) -> (MockKernel, KernelSolidHandle) {
            let mut kernel = MockKernel::new();
            let profile = ClosedProfile {
                entity_ids: vec![1, 2, 3, 4],
                is_outer: true,
                vertex_ids: vec![],
                circle: None,
                spline_segments: vec![],
                arc_segments: vec![],
            };
            let mut positions = HashMap::new();
            for (i, c) in corners.iter().enumerate() {
                positions.insert((i + 1) as u32, *c);
            }
            let face_ids = kernel
                .make_faces_from_profiles(
                    &[profile],
                    [0.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0],
                    [1.0, 0.0, 0.0],
                    &positions,
                )
                .unwrap();
            let handle = kernel
                .extrude_face(face_ids[0], [0.0, 0.0, 1.0], height)
                .unwrap();
            (kernel, handle)
        }

        let fid = Uuid::new_v4();
        let results_with = |handle: KernelSolidHandle| {
            let mut m: HashMap<Uuid, OpResult> = HashMap::new();
            m.insert(
                fid,
                OpResult {
                    outputs: vec![(
                        OutputKey::Main,
                        BodyOutput {
                            handle,
                            mesh: None,
                            edges: None,
                        },
                    )],
                    provenance: Provenance {
                        created: vec![],
                        deleted: vec![],
                        modified: vec![],
                        role_assignments: vec![],
                    },
                    diagnostics: Diagnostics::default(),
                },
            );
            m
        };

        // GeomRef picking the top vertex over footprint corner (2, 3).
        let gref = GeomRef {
            kind: TopoKind::Vertex,
            anchor: Anchor::FeatureOutput {
                feature_id: fid,
                output_key: OutputKey::Main,
            },
            selector: Selector::Position {
                x: 2.0,
                y: 3.0,
                z: 5.0,
            },
            policy: ResolvePolicy::BestEffort,
            scope: None,
        };
        let make_sketch = || Sketch {
            id: Uuid::new_v4(),
            plane: gref.clone(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: None,
            entities: vec![SketchEntity::Point {
                id: 100,
                x: 0.0,
                y: 0.0,
                construction: true,
            }],
            constraints: vec![],
            solve_status: SolveStatus::UnderConstrained { dof: 0 },
            solved_positions: HashMap::new(),
            solved_profiles: vec![],
            plane_face: None,
            projected: vec![ProjectedEntity {
                point_id: 100,
                source: ProjectedSource {
                    geom_ref: gref.clone(),
                    kind: ProjectedKind::Vertex,
                },
            }],
        };
        let point_xy = |s: &Sketch| -> (f64, f64) {
            for e in &s.entities {
                if let SketchEntity::Point { id, x, y, .. } = e {
                    if *id == 100 {
                        return (*x, *y);
                    }
                }
            }
            panic!("point 100 not found");
        };
        let basis = SketchPlaneBasis::from_origin_normal([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);

        // Independently find the nearest vertex to the pick (MockKernel builds a
        // square box of side sqrt(area), so we read actual vertex positions
        // rather than assume the footprint).
        let nearest_pos = |k: &MockKernel, h: &KernelSolidHandle| -> [f64; 3] {
            let pick = [2.0, 3.0, 5.0];
            k.list_vertices(h)
                .into_iter()
                .filter_map(|v| k.compute_signature(v, TopoKind::Vertex).centroid)
                .min_by(|a, b| {
                    let da = (a[0] - pick[0]).powi(2)
                        + (a[1] - pick[1]).powi(2)
                        + (a[2] - pick[2]).powi(2);
                    let db = (b[0] - pick[0]).powi(2)
                        + (b[1] - pick[1]).powi(2)
                        + (b[2] - pick[2]).powi(2);
                    da.partial_cmp(&db).unwrap()
                })
                .unwrap()
        };

        // Box 1.
        let (k1, h1) = box_kernel([(0.0, 0.0), (2.0, 0.0), (2.0, 3.0), (0.0, 3.0)], 5.0);
        let mut s1 = make_sketch();
        reproject_sketch(&mut s1, &results_with(h1.clone()), k1.as_introspect(), None);
        let (eu1, ev1) = basis.world_to_local(nearest_pos(&k1, &h1));
        let p1 = point_xy(&s1);
        assert!(
            (p1.0 - eu1).abs() < 1e-6 && (p1.1 - ev1).abs() < 1e-6,
            "box1 projection {p1:?} != {:?}",
            (eu1, ev1)
        );

        // Box 2: a larger footprint moves the resolved vertex; the same pick
        // resolves to the (moved) nearest vertex, so the projected point follows.
        let (k2, h2) = box_kernel([(0.0, 0.0), (2.5, 0.0), (2.5, 3.2), (0.0, 3.2)], 5.0);
        let mut s2 = make_sketch();
        reproject_sketch(&mut s2, &results_with(h2.clone()), k2.as_introspect(), None);
        let (eu2, ev2) = basis.world_to_local(nearest_pos(&k2, &h2));
        let p2 = point_xy(&s2);
        assert!(
            (p2.0 - eu2).abs() < 1e-6 && (p2.1 - ev2).abs() < 1e-6,
            "box2 projection {p2:?} != {:?}",
            (eu2, ev2)
        );

        // Parametric: the projected point MOVED because the source moved.
        assert!(
            (p1.0 - p2.0).abs() > 1e-3 || (p1.1 - p2.1).abs() > 1e-3,
            "projected point must track the upstream edit ({p1:?} -> {p2:?})"
        );
    }

    /// Adversarial (incr 7): a projected binding whose source cannot be resolved
    /// (the source feature has no result — deleted/dangling) leaves the bound
    /// point at its last position rather than failing or moving to garbage.
    #[test]
    fn reproject_dangling_source_keeps_last_position() {
        use waffle_types::kernel::MockKernel;
        use waffle_types::{
            Anchor, GeomRef, ProjectedEntity, ProjectedKind, ProjectedSource, ResolvePolicy,
            SolveStatus,
        };

        let gref = GeomRef {
            kind: TopoKind::Vertex,
            anchor: Anchor::FeatureOutput {
                feature_id: Uuid::new_v4(),
                output_key: OutputKey::Main,
            },
            selector: Selector::Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            policy: ResolvePolicy::BestEffort,
            scope: None,
        };
        let mut sketch = Sketch {
            id: Uuid::new_v4(),
            plane: gref.clone(),
            plane_origin: [0.0, 0.0, 0.0],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: None,
            entities: vec![SketchEntity::Point {
                id: 100,
                x: 7.0,
                y: -4.0,
                construction: true,
            }],
            constraints: vec![],
            solve_status: SolveStatus::UnderConstrained { dof: 0 },
            solved_positions: HashMap::new(),
            solved_profiles: vec![],
            plane_face: None,
            projected: vec![ProjectedEntity {
                point_id: 100,
                source: ProjectedSource {
                    geom_ref: gref,
                    kind: ProjectedKind::Vertex,
                },
            }],
        };

        // Empty feature_results → the source feature is unresolvable.
        let empty: HashMap<Uuid, OpResult> = HashMap::new();
        let kernel = MockKernel::new();
        reproject_sketch(&mut sketch, &empty, kernel.as_introspect(), None);

        // The point keeps its last (x, y) — nothing moved, no panic.
        match &sketch.entities[0] {
            SketchEntity::Point { x, y, .. } => {
                assert!((*x - 7.0).abs() < 1e-12 && (*y + 4.0).abs() < 1e-12);
            }
            _ => panic!("expected point"),
        }
    }
}

#[cfg(test)]
mod profile_addressing_tests {
    //! `resolve_profile_index` (v4 §2.9): the returned INDEX is what the
    //! integration tests cannot observe through the MockKernel.
    use super::resolve_profile_index;
    use crate::types::EngineError;
    use uuid::Uuid;
    use waffle_types::{
        Anchor, ClosedProfile, GeomRef, ResolvePolicy, Selector, Sketch, SolveStatus, TopoKind,
    };

    fn profile(ids: &[u32]) -> ClosedProfile {
        ClosedProfile {
            entity_ids: ids.to_vec(),
            is_outer: true,
            vertex_ids: vec![],
            circle: None,
            spline_segments: vec![],
            arc_segments: vec![],
        }
    }

    fn sketch(profiles: Vec<ClosedProfile>) -> Sketch {
        Sketch {
            id: Uuid::new_v4(),
            plane: GeomRef {
                kind: TopoKind::Face,
                anchor: Anchor::Datum {
                    datum_id: Uuid::new_v4(),
                },
                selector: Selector::Role {
                    role: waffle_types::roles::Role::EndCapPositive,
                    index: 0,
                },
                policy: ResolvePolicy::Strict,
                scope: None,
            },
            plane_origin: [0.0; 3],
            plane_normal: [0.0, 0.0, 1.0],
            plane_x_axis: None,
            entities: vec![],
            constraints: vec![],
            solve_status: SolveStatus::FullyConstrained,
            solved_positions: Default::default(),
            projected: vec![],
            plane_face: None,
            solved_profiles: profiles,
        }
    }

    #[test]
    fn resolves_the_index_of_the_loop_with_that_set_regardless_of_order() {
        let s = sketch(vec![
            profile(&[1, 2, 3]),
            profile(&[7]),
            profile(&[4, 5, 6, 8]),
        ]);
        assert_eq!(
            resolve_profile_index(&s, 0, Some(&[8, 6, 4, 5]), &mut Vec::new()).unwrap(),
            2
        );
        assert_eq!(
            resolve_profile_index(&s, 0, Some(&[7]), &mut Vec::new()).unwrap(),
            1
        );
        assert_eq!(
            resolve_profile_index(&s, 99, Some(&[3, 1, 2]), &mut Vec::new()).unwrap(),
            0
        );
        // Duplicates in the request collapse to the set.
        assert_eq!(
            resolve_profile_index(&s, 0, Some(&[7, 7]), &mut Vec::new()).unwrap(),
            1
        );
    }

    #[test]
    fn index_path_is_unchanged() {
        let s = sketch(vec![profile(&[1, 2, 3]), profile(&[7])]);
        assert_eq!(
            resolve_profile_index(&s, 1, None, &mut Vec::new()).unwrap(),
            1
        );
        assert!(matches!(
            resolve_profile_index(&s, 2, None, &mut Vec::new()),
            Err(EngineError::ProfileOutOfRange { index: 2, count: 2 })
        ));
        let empty = sketch(vec![]);
        assert!(matches!(
            resolve_profile_index(&empty, 0, None, &mut Vec::new()),
            Err(EngineError::ProfileOutOfRange { index: 0, count: 0 })
        ));
    }

    #[test]
    fn missing_and_ambiguous_sets_are_typed_errors() {
        let s = sketch(vec![profile(&[1, 2, 3]), profile(&[3, 2, 1])]);
        // A set with an entity no loop bounds names no loop. (A SUBSET of a
        // loop does name one — see the growth tests below — so the "missing"
        // case has to name something genuinely absent.)
        assert!(matches!(
            resolve_profile_index(&s, 0, Some(&[1, 99]), &mut Vec::new()),
            Err(EngineError::ProfileNotFound { ref entity_ids, count: 2 }) if entity_ids == &[1, 99]
        ));
        assert!(matches!(
            resolve_profile_index(&s, 0, Some(&[1, 2, 3]), &mut Vec::new()),
            Err(EngineError::ProfileAmbiguous { matches: 2, .. })
        ));
        // Two loops both CONTAIN {1,2}: equally ambiguous, and never a silent
        // pick of the first.
        assert!(matches!(
            resolve_profile_index(&s, 0, Some(&[1, 2]), &mut Vec::new()),
            Err(EngineError::ProfileAmbiguous { matches: 2, .. })
        ));
        // An empty set names no loop (a profile always has ≥1 entity) — and it
        // is a subset of every loop, so it must be refused before the
        // containment pass rather than by it.
        assert!(matches!(
            resolve_profile_index(&s, 0, Some(&[]), &mut Vec::new()),
            Err(EngineError::ProfileNotFound { .. })
        ));
    }

    /// A loop that GREW keeps the reference: the fillet case, measured through
    /// the agent's tools (`specs/agent_mechanical_design.md` §10.3).
    #[test]
    fn a_loop_that_grew_still_answers_to_the_stored_set() {
        // The rectangle {5,6,7,8} after a fillet at the corner between 6 and 7:
        // the arc 12 joins the loop, and a bore circle sits inside it.
        let s = sketch(vec![profile(&[5, 6, 12, 7, 8]), profile(&[10])]);
        let mut warnings = Vec::new();
        assert_eq!(
            resolve_profile_index(&s, 0, Some(&[5, 6, 7, 8]), &mut warnings).unwrap(),
            0
        );
        // And it SAYS it re-pointed, naming both sets: the same answer reached
        // by containment rather than by equality is not the same claim.
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains("[5, 6, 7, 8]") && warnings[0].contains("[5, 6, 12, 7, 8]"),
            "{}",
            warnings[0]
        );
        // The order the caller stored is irrelevant, as for an equal set.
        assert_eq!(
            resolve_profile_index(&s, 0, Some(&[8, 7, 6, 5]), &mut Vec::new()).unwrap(),
            0
        );
        // An exact set still wins outright, even when another loop contains it
        // — and that match is silent, because nothing drifted.
        let s2 = sketch(vec![profile(&[5, 6, 12, 7, 8]), profile(&[5, 6, 7, 8])]);
        let mut quiet = Vec::new();
        assert_eq!(
            resolve_profile_index(&s2, 0, Some(&[5, 6, 7, 8]), &mut quiet).unwrap(),
            1
        );
        assert!(quiet.is_empty(), "{quiet:?}");
    }

    /// Growth is an identity; SHRINKAGE is not. An entity the author named that
    /// no longer bounds anything means the loop they picked is gone — what is
    /// left may bound a larger region nobody asked for — so it stays loud.
    #[test]
    fn a_loop_that_lost_a_named_entity_is_still_not_found() {
        // 7 was trimmed away and the remains joined the outer boundary.
        let s = sketch(vec![profile(&[5, 6, 8, 20, 21])]);
        assert!(matches!(
            resolve_profile_index(&s, 0, Some(&[5, 6, 7, 8]), &mut Vec::new()),
            Err(EngineError::ProfileNotFound { count: 1, .. })
        ));
    }
}
