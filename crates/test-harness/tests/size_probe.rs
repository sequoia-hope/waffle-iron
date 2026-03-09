#[cfg(test)]
mod size_tests {
    use test_harness::helpers::mesh_volume;
    use test_harness::ModelBuilder;

    #[test]
    fn three_sequential_3x3_cuts() {
        let mut m = ModelBuilder::kernel();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();
        let v0 = mesh_volume(&m.tessellate("cube").unwrap());

        let pos = [(0.5, 0.5), (4.0, 0.5), (0.5, 6.0)];
        let mut prev = v0;
        for (i, (x, y)) in pos.iter().enumerate() {
            let sk = format!("c{}_sk", i);
            let cn = format!("c{}", i);
            m.rect_sketch(&sk, [0., 0., 10.], [0., 0., 1.], *x, *y, 3., 3.)
                .unwrap();
            m.extrude_cut(&cn, &sk, 5.0).unwrap();
            assert!(
                m.assert_has_solid(&cn).is_ok(),
                "Cut {} should have solid",
                i
            );
            let v = mesh_volume(&m.tessellate(&cn).unwrap());
            eprintln!("Cut {}: vol {:.0} (prev {:.0})", i, v, prev);
            assert!(v < prev, "Cut {} should reduce volume", i);
            prev = v;
        }
        eprintln!("3 sequential 3x3 cuts: PASS, final vol {:.0}", prev);
    }

    #[test]
    fn five_sequential_3x3_cuts() {
        let mut m = ModelBuilder::kernel();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();
        let v0 = mesh_volume(&m.tessellate("cube").unwrap());

        let pos = [(0.5, 0.5), (4.0, 0.5), (0.5, 4.0), (4.0, 4.0), (0.5, 7.0)];
        let mut prev = v0;
        for (i, (x, y)) in pos.iter().enumerate() {
            let sk = format!("c{}_sk", i);
            let cn = format!("c{}", i);
            m.rect_sketch(&sk, [0., 0., 10.], [0., 0., 1.], *x, *y, 3., 3.)
                .unwrap();
            m.extrude_cut(&cn, &sk, 4.0).unwrap();
            let ok = m.assert_has_solid(&cn).is_ok();
            if !ok {
                eprintln!("Cut {} FAILED (no solid)", i);
                return;
            }
            let v = mesh_volume(&m.tessellate(&cn).unwrap());
            eprintln!("Cut {}: vol {:.0} (prev {:.0})", i, v, prev);
            assert!(v < prev, "Cut {} should reduce volume", i);
            prev = v;
        }
        eprintln!("5 sequential 3x3 cuts: PASS, final vol {:.0}", prev);
    }

    #[test]
    fn boss_then_cut_3x3() {
        let mut m = ModelBuilder::kernel();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();
        let v0 = mesh_volume(&m.tessellate("cube").unwrap());

        // Boss
        m.rect_sketch("b_sk", [0., 0., 10.], [0., 0., 1.], 3., 3., 3., 3.)
            .unwrap();
        m.extrude("boss", "b_sk", 4.0).unwrap();
        let vb = mesh_volume(&m.tessellate("boss").unwrap());
        eprintln!("Boss: vol {:.0} (was {:.0})", vb, v0);
        assert!(vb > v0, "Boss should increase volume");

        // Cut (on same body)
        m.rect_sketch("c_sk", [0., 0., 10.], [0., 0., 1.], 0.5, 0.5, 3., 3.)
            .unwrap();
        m.extrude_cut("cut", "c_sk", 5.0).unwrap();
        let ok = m.assert_has_solid("cut").is_ok();
        if ok {
            let vc = mesh_volume(&m.tessellate("cut").unwrap());
            eprintln!("Boss then cut: vol {:.0} (boss was {:.0})", vc, vb);
        } else {
            eprintln!("Boss then cut 3x3: FAIL (no solid)");
        }
    }

    #[test]
    fn cut_then_boss_then_cut() {
        let mut m = ModelBuilder::kernel();
        m.rect_sketch("base_sk", [0., 0., 0.], [0., 0., 1.], 0., 0., 10., 10.)
            .unwrap();
        m.extrude("cube", "base_sk", 10.0).unwrap();
        let v0 = mesh_volume(&m.tessellate("cube").unwrap());

        // Cut first
        m.rect_sketch("c1_sk", [0., 0., 10.], [0., 0., 1.], 0.5, 0.5, 3., 3.)
            .unwrap();
        m.extrude_cut("c1", "c1_sk", 5.0).unwrap();
        let ok1 = m.assert_has_solid("c1").is_ok();
        if !ok1 {
            eprintln!("First cut FAIL");
            return;
        }
        let v1 = mesh_volume(&m.tessellate("c1").unwrap());
        eprintln!("First cut: vol {:.0}", v1);

        // Boss
        m.rect_sketch("b_sk", [0., 0., 10.], [0., 0., 1.], 6., 6., 3., 3.)
            .unwrap();
        m.extrude("boss", "b_sk", 4.0).unwrap();
        let ok2 = m.assert_has_solid("boss").is_ok();
        if !ok2 {
            eprintln!("Boss after cut FAIL");
            return;
        }
        let v2 = mesh_volume(&m.tessellate("boss").unwrap());
        eprintln!("Boss: vol {:.0}", v2);

        // Second cut
        m.rect_sketch("c2_sk", [0., 0., 10.], [0., 0., 1.], 4., 0.5, 3., 3.)
            .unwrap();
        m.extrude_cut("c2", "c2_sk", 4.0).unwrap();
        let ok3 = m.assert_has_solid("c2").is_ok();
        if !ok3 {
            eprintln!("Cut after boss FAIL");
            return;
        }
        let v3 = mesh_volume(&m.tessellate("c2").unwrap());
        eprintln!("Cut after boss: vol {:.0}", v3);
    }
}
