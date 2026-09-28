//! P0005 (2026-09-28): the Yang Fig. 11(a) edge split winds its halves
//! COMBINATORIALLY — the parent's cyclic order with the inserted point spliced
//! in — never by a geometric proxy. Spec `yang_n2_stage4_cdt_mesh_updating.md`
//! §5c.15.
//!
//! Anchor: P0005's A-cone chart carries two rim mints 3.045e-3 apart (the
//! flank-plane crossing and a neighbouring face's crossing of the same rim),
//! each with its seeded generator, so the chart has a 3e-3 × 424 sliver quad.
//! Plane 114 crosses the quad's generator at v539 and its diagonal at v563,
//! 2.67e-7 apart. The §4.4.1(a) simple arm dropped D = (539, 562, 563) and
//! split N = (328, 563, 562) at 539 on edge (562, 563); the half (539, 563,
//! 328) is a needle whose area normal is noise against N's, and `orient_tri`
//! inverted it — a fold, `s4-halfedge-pairing` edge (328,539) fwd=2 rev=0.

use crate::stage1_tessellate::orient_tri;
use crate::stage4_correct::split_tri_at_edge;
use crate::tri_area_vector;
use cad_primitives::Point3;

/// The directed edges of a triangle.
fn directed(t: [u32; 3]) -> [(u32, u32); 3] {
    [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]
}

/// Every rotation of the parent, in both windings, split on the same edge:
/// the halves reproduce the parent's directed edges verbatim except the split
/// edge, which becomes the two-segment chain in the SAME direction, plus the
/// internal pair `ins–third` in both directions. Pure combinatorics.
#[test]
fn split_halves_inherit_the_parent_winding() {
    let (a, c, d, b) = (10u32, 11u32, 12u32, 99u32);
    for parent in [
        [a, c, d],
        [c, d, a],
        [d, a, c],
        [c, a, d],
        [a, d, c],
        [d, c, a],
    ] {
        let (t1, t2) = split_tri_at_edge(parent, a, c, b).expect("parent has edge a-c");
        let mut got: Vec<(u32, u32)> = directed(t1).into_iter().chain(directed(t2)).collect();
        got.sort_unstable();
        // Expected: parent edges with the a-c edge (in its parent direction
        // u→v) replaced by u→b, b→v; plus b→d and d→b.
        let pe = directed(parent);
        let (u, v) = pe
            .into_iter()
            .find(|&(p, q)| (p == a && q == c) || (p == c && q == a))
            .expect("parent contains the split edge");
        let mut want: Vec<(u32, u32)> = pe
            .into_iter()
            .filter(|&e| e != (u, v))
            .chain([(u, b), (b, v), (b, d), (d, b)])
            .collect();
        want.sort_unstable();
        assert_eq!(got, want, "parent {parent:?}");
        // The split edge's direction is preserved on the chain.
        assert!(
            directed(t1).contains(&(u, b)) || directed(t2).contains(&(u, b)),
            "parent {parent:?}: u→b missing"
        );
    }
    // A parent without the edge is an invariant violation, reported as None.
    assert!(split_tri_at_edge([a, c, d], a, 77, b).is_none());
}

/// The P0005 needle, at its measured coordinates: the combinatorial split
/// pairs every edge, while the retired geometric proxy inverts the needle
/// half (the mutation check — the proxy's verdict on THIS input is the fold).
#[test]
fn p0005_needle_half_is_wound_like_its_parent_not_by_its_own_normal() {
    // v328 (the rim junction), v563 / v539 (the sliver quad's diagonal and
    // generator crossings, 2.67e-7 apart), v562 (the far apex on the
    // neighbouring hyperbola). Ids as in the P0005 Stage-4 dump.
    let verts = vec![
        Point3::new(626.4513688973267, -561.0349209695822, 499.9999999999999), // 0 = v328
        Point3::new(626.4546920993182, -561.04052529531, 499.99423977320464),  // 1 = v539
        Point3::new(626.4546919972286, -561.0405251231435, 499.99423995016036), // 2 = v563
        Point3::new(688.3330696876077, -598.9160026608051, 427.4443576409045), // 3 = v562
    ];
    let (v328, v539, v563, v562) = (0u32, 1u32, 2u32, 3u32);
    let n = [v328, v563, v562]; // N = (328, 563, 562), the neighbour across (562, 563)
    let d = [v539, v562, v563]; // D = (539, 562, 563), dropped

    // Combinatorial: N's order 328→563→562 with 539 spliced between 563 and
    // 562 gives (563, 539, 328) and (539, 562, 328) — every directed edge
    // of D's former neighbours still finds its partner.
    let (t1, t2) = split_tri_at_edge(n, v562, v563, v539).expect("N has edge 562-563");
    let mut dir: std::collections::BTreeMap<(u32, u32), i32> = Default::default();
    // The surviving ring around the site: D's other neighbours present
    // 539→563 (B's flank triangle (931, 539, 563)) and 562→539 (A's
    // (562, 539, 538)); N's other neighbours present 563→328 and 328→562.
    // Represent them by the reversed edges D and N's other neighbours owe.
    let owed = [(v539, v563), (v562, v539), (v563, v328), (v328, v562)];
    for e in owed {
        *dir.entry(e).or_default() += 1;
    }
    for t in [t1, t2] {
        for (p, q) in directed(t) {
            *dir.entry((p, q)).or_default() += 1;
        }
    }
    for (&(p, q), &fwd) in &dir {
        let rev = dir.get(&(q, p)).copied().unwrap_or(0);
        assert_eq!(fwd, rev, "edge ({p},{q}) fwd={fwd} rev={rev}: not paired");
    }
    // D itself is consistent with the ring it is dropped from (sanity).
    for (p, q) in directed(d) {
        assert!(owed.contains(&(q, p)) || (p, q) == (v562, v563) || (p, q) == (v563, v562));
    }

    // Mutation check: the retired proxy winds the needle half by its OWN area
    // normal against N's, and on this input that inverts it — the half
    // (539, 563, 328) presents 328→539 alongside (562, 328, 539): the fold.
    let n_norm = tri_area_vector(
        verts[n[0] as usize].as_array(),
        verts[n[1] as usize].as_array(),
        verts[n[2] as usize].as_array(),
    );
    let mut geo_needle = [v539, v563, v328];
    orient_tri(&verts, &mut geo_needle, n_norm);
    let combinatorial_needle = if t1.contains(&v563) { t1 } else { t2 };
    let same_cycle =
        |x: [u32; 3], y: [u32; 3]| (0..3).any(|k| [x[k], x[(k + 1) % 3], x[(k + 2) % 3]] == y);
    assert!(
        !same_cycle(geo_needle, combinatorial_needle),
        "the geometric proxy agreed with the combinatorial split on P0005's needle — \
         the mutation check no longer pins the defect: geo={geo_needle:?} comb={combinatorial_needle:?}"
    );
}
