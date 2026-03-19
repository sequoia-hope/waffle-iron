use sketch_solver::core::constraint::ConstraintEq;
use sketch_solver::core::lm::lm_solve;
use sketch_solver::core::types::{PointIdx, ScaleType, SolveOptions};

struct TestPinPoint {
    idx: PointIdx,
    target: (f64, f64),
}

impl ConstraintEq for TestPinPoint {
    fn num_equations(&self) -> usize {
        2
    }
    fn scale_types(&self) -> &[ScaleType] {
        &[ScaleType::Distance, ScaleType::Distance]
    }
    fn residuals(&self, params: &[f64], out: &mut [f64]) {
        let p = self.idx.read(params);
        out[0] = p.x - self.target.0;
        out[1] = p.y - self.target.1;
    }
    fn jacobian(&self, _params: &[f64], eq_offset: usize, out: &mut Vec<(usize, usize, f64)>) {
        out.push((eq_offset, self.idx.x(), 1.0));
        out.push((eq_offset + 1, self.idx.y(), 1.0));
    }
}

struct TestDistancePP {
    p1: PointIdx,
    p2: PointIdx,
    d: f64,
}

impl ConstraintEq for TestDistancePP {
    fn num_equations(&self) -> usize {
        1
    }
    fn scale_types(&self) -> &[ScaleType] {
        &[ScaleType::Distance]
    }
    fn residuals(&self, params: &[f64], out: &mut [f64]) {
        let p1 = self.p1.read(params);
        let p2 = self.p2.read(params);
        let dist = ((p1.x - p2.x).powi(2) + (p1.y - p2.y).powi(2)).sqrt();
        out[0] = dist - self.d;
    }
    fn jacobian(&self, params: &[f64], eq_offset: usize, out: &mut Vec<(usize, usize, f64)>) {
        let p1 = self.p1.read(params);
        let p2 = self.p2.read(params);
        let dx = p1.x - p2.x;
        let dy = p1.y - p2.y;
        let dist = (dx.powi(2) + dy.powi(2)).sqrt();
        if dist > 1e-12 {
            out.push((eq_offset, self.p1.x(), dx / dist));
            out.push((eq_offset, self.p1.y(), dy / dist));
            out.push((eq_offset, self.p2.x(), -dx / dist));
            out.push((eq_offset, self.p2.y(), -dy / dist));
        }
    }
}

enum TestConstraint {
    Pin(TestPinPoint),
    Dist(TestDistancePP),
}

impl ConstraintEq for TestConstraint {
    fn num_equations(&self) -> usize {
        match self {
            Self::Pin(c) => c.num_equations(),
            Self::Dist(c) => c.num_equations(),
        }
    }
    fn scale_types(&self) -> &[ScaleType] {
        match self {
            Self::Pin(c) => c.scale_types(),
            Self::Dist(c) => c.scale_types(),
        }
    }
    fn residuals(&self, params: &[f64], out: &mut [f64]) {
        match self {
            Self::Pin(c) => c.residuals(params, out),
            Self::Dist(c) => c.residuals(params, out),
        }
    }
    fn jacobian(&self, params: &[f64], eq_offset: usize, out: &mut Vec<(usize, usize, f64)>) {
        match self {
            Self::Pin(c) => c.jacobian(params, eq_offset, out),
            Self::Dist(c) => c.jacobian(params, eq_offset, out),
        }
    }
}

#[test]
fn fully_constrained_point() {
    let x0 = vec![1.0, 2.0];
    let x_anchor = vec![1.0, 2.0];
    let constraints = vec![TestConstraint::Pin(TestPinPoint {
        idx: PointIdx(0),
        target: (3.0, 4.0),
    })];
    let eq_scale_types = vec![ScaleType::Distance, ScaleType::Distance];
    let options = SolveOptions {
        spring_mu: 1e-10, // Very weak so it doesn't pull p.x away from 3.0 much
        ..Default::default()
    };

    let result = lm_solve(&x0, &x_anchor, &constraints, &eq_scale_types, 2, &options);
    assert!(result.converged);
    assert!(result.iterations < 5);
    assert!((result.params[0] - 3.0).abs() < 1e-6);
    assert!((result.params[1] - 4.0).abs() < 1e-6);
}

#[test]
fn two_points_distance() {
    // p1 pinned at (0,0), p2 starts at (10,0), dist(p1,p2) = 5
    // p2 should move to (5,0)
    let x0 = vec![0.0, 0.0, 10.0, 0.0];
    let x_anchor = vec![0.0, 0.0, 10.0, 0.0];
    let constraints = vec![
        TestConstraint::Pin(TestPinPoint {
            idx: PointIdx(0),
            target: (0.0, 0.0),
        }),
        TestConstraint::Dist(TestDistancePP {
            p1: PointIdx(0),
            p2: PointIdx(2),
            d: 5.0,
        }),
    ];
    let eq_scale_types = vec![
        ScaleType::Distance,
        ScaleType::Distance,
        ScaleType::Distance,
    ];
    let options = SolveOptions {
        spring_mu: 1e-10,
        ..Default::default()
    };

    let result = lm_solve(&x0, &x_anchor, &constraints, &eq_scale_types, 3, &options);
    assert!(result.converged);
    let dist = (result.params[2].powi(2) + result.params[3].powi(2)).sqrt();
    assert!((dist - 5.0).abs() < 1e-6);
}

#[test]
fn under_constrained_stabilized_by_springs() {
    // 1 point horizontal pin only. y should stay at x0.y=2.0 due to springs.
    let x0 = vec![1.0, 2.0];
    let x_anchor = vec![1.0, 2.0];

    struct HorizPin {
        idx: PointIdx,
        target_x: f64,
    }
    impl ConstraintEq for HorizPin {
        fn num_equations(&self) -> usize {
            1
        }
        fn scale_types(&self) -> &[ScaleType] {
            &[ScaleType::Distance]
        }
        fn residuals(&self, params: &[f64], out: &mut [f64]) {
            out[0] = params[self.idx.x()] - self.target_x;
        }
        fn jacobian(&self, _params: &[f64], eq_offset: usize, out: &mut Vec<(usize, usize, f64)>) {
            out.push((eq_offset, self.idx.x(), 1.0));
        }
    }

    let constraints = vec![HorizPin {
        idx: PointIdx(0),
        target_x: 3.0,
    }];
    let eq_scale_types = vec![ScaleType::Distance];
    let options = SolveOptions {
        spring_mu: 1e-4, // strong enough to notice in few iters
        ..Default::default()
    };

    let result = lm_solve(&x0, &x_anchor, &constraints, &eq_scale_types, 1, &options);
    assert!(result.converged);
    // x should be near 3.0, but slightly pulled towards 1.0 by the spring (mu=1e-4)
    // Equilibrium: (x-3) + 1e-4(x-1) = 0 => x \approx 3 - 2e-4
    assert!((result.params[0] - 3.0).abs() < 5e-4);
    // y should stay at 2.0 exactly (it's the minimum of the spring cost, no other forces)
    assert!((result.params[1] - 2.0).abs() < 1e-6);
}

#[test]
fn cold_start_distant_guess() {
    let x0 = vec![100.0, 100.0];
    let x_anchor = vec![100.0, 100.0];
    let constraints = vec![TestConstraint::Pin(TestPinPoint {
        idx: PointIdx(0),
        target: (0.0, 0.0),
    })];
    let eq_scale_types = vec![ScaleType::Distance, ScaleType::Distance];
    let options = SolveOptions {
        spring_mu: 1e-10,
        lambda_init: 1.0,
        ..Default::default()
    };

    let result = lm_solve(&x0, &x_anchor, &constraints, &eq_scale_types, 2, &options);
    assert!(result.converged);
    assert!(result.iterations < 50);
    assert!((result.params[0]).abs() < 1e-6);
    assert!((result.params[1]).abs() < 1e-6);
}
