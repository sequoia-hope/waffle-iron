//! Yang 2025 §4.2.3 — provenance-derived surface incidence.
//!
//! # The paper's sentence
//!
//! §4.2.3 "Approximate parameter coordinates"
//! (`refs/text/yang2025_hybrid_boolean.txt:510-515`, LEFT column):
//!
//! > *"After the mesh intersection step, each intersection curve is represented
//! > as a sequence of vertices. Owing to the implicit point representations used
//! > in [Cherchi et al. 2022], we can directly map each intersection point back
//! > to both NURBS surfaces corresponding to the meshes using barycentric
//! > coordinates **by querying the triangles that intersect at that point**."*
//!
//! The surfaces meeting at an intersection point come from the **triangles
//! incident to it**, resolved through the arrangement's own provenance. They are
//! not re-derived from geometry, and not inferred from which patch a boundary
//! edge happens to belong to.
//!
//! # What this module measures
//!
//! `compute_phase_a` (`stage4_correct.rs:137`) builds its incidence map the
//! other way: for every patch, for every boundary cycle, for every edge of that
//! cycle, it pushes **the patch's single inherited face surface**. Deviation
//! **N10** names that construction as the mis-attribution source and names the
//! paper's route as the durable fix ("consume true mesh-level two-surface
//! provenance from the `LabeledArrangement` producer").
//!
//! Both signals are already present at that call site. The §4.2.3 one is
//! [`TriangleAttributionMap`] — established by `boolean()` step 5 from
//! cherchi's per-triangle `source` through the Stage-1 `tri_face` maps
//! (deviation **N4**, RESOLVED: provenance is the sole production path). So the
//! comparison needs no new plumbing, and this module is **read-only**: it
//! computes the §4.2.3 view and diffs it against the shipped cycle-derived one.
//!
//! # The prediction this exists to test
//!
//! [`flood_fill_patches`](crate::stage5_topology::flood_fill_patches) groups
//! triangles by **identical** attribution (`t_attr != seed_attr` ⇒ skip), so
//! before merging, every triangle of a patch carries that patch's attribution
//! exactly and the two views agree by construction.
//!
//! `merge_same_plane_patches` (PR-YR27 Finding 1a) then merges edge-adjacent
//! same-plane patches and gives the merged patch the **lexicographically
//! smallest member's** attribution. Its members keep their own. So a boundary
//! edge of a merged patch is tagged with a face that the triangles actually
//! incident to it need not carry.
//!
//! **⇒ Divergence should appear at merged same-plane patches and nowhere else.**
//! If the sweep shows that, the §4.2.3 residual is scoped to the coplanar
//! classes. If it shows divergence elsewhere, the prediction is wrong and the
//! mechanism is not what the code reading suggests — which is the reason to
//! measure rather than to build on the reading.
//!
//! # MEASURED 2026-08-08 — the two views AGREE; there is no incidence gap
//!
//! Full corpus, `YANG_S423_INCIDENCE`, 280 of 312 cases reaching Stage 4
//! (the other 32 fail earlier), **1810 `compute_phase_a` invocations,
//! 2 589 874 boundary edges**:
//!
//! | bucket | count |
//! |---|---:|
//! | `agree` | 2 546 094 (98.3 %) |
//! | **`cycle_unsupported`** | **0** |
//! | `disjointish` | 43 214 |
//! | `merge_explained` | **43 214 — identical** |
//! | `prov_richer` (= all 566 "unexplained") | 566 |
//! | `missing_in_prov` | 0 |
//!
//! **`cycle_unsupported = 0` corpus-wide is the decisive number.** That bucket
//! is precisely the mis-attribution deviation N10 posits — the cycle view
//! claiming a face that no triangle actually incident to the edge carries — and
//! it never happens, on any case, on any edge.
//!
//! The prediction above was **half right**: the mechanism is patch merging, but
//! it lands in `disjointish`, not `cycle_unsupported`, because the *outer*
//! triangle's own attribution still agrees while the merged side's does not. And
//! `disjointish == merge_explained` exactly, so every two-way divergence is
//! PR-YR27's merge doing its job by design.
//!
//! The 566 residual are all `ProvenanceRicher` — provenance seeing an incident
//! triangle whose patch cycle does not include that edge. Benign by
//! construction, and **not a discriminator**: 2 SUPPORTED_CORRECT cases (R0063,
//! R0046) carry it and 36 ERROR cases do not.
//!
//! **⇒ The shipped cycle-derived incidence is provenance-equivalent. §4.2.3's
//! "query the triangles that intersect at that point" would return the same face
//! identities Stage 3/4 already receive, so rebuilding the incidence on
//! provenance is a NO-OP and must not be built.**
//!
//! Scope bound: the comparison is over the KEPT submesh, which is the same mesh
//! `compute_phase_a` builds its map from — the right denominator for this
//! question, but it cannot speak for surfaces carried only by discarded
//! triangles. Coverage is 280/312 cases.
//!
//! This module stays as the standing instrument for that invariant
//! (`cycle_unsupported` must remain 0; a non-zero reading is a real regression
//! in patch attribution), not as a step toward a replacement.
//!
//! Compared on face **IDENTITY** (`(InputId, face_idx)`), never on `Surface`
//! geometry: two faces can carry equal surfaces and still be different faces,
//! and an identity question answered with a tolerance is the recurring error
//! this campaign has logged repeatedly.

use crate::brep::{InputId, TriangleAttribution, TriangleAttributionMap};
use cherchi_rs::Mesh;
use std::collections::{BTreeMap, BTreeSet};

/// Undirected mesh edge key: `(min, max)` vertex indices.
pub(crate) type EdgeKey = (u32, u32);

/// Per undirected edge, the set of B-Rep faces incident to it.
pub(crate) type IncidenceIds = BTreeMap<EdgeKey, BTreeSet<TriangleAttribution>>;

fn key(s: u32, e: u32) -> EdgeKey {
    if s < e {
        (s, e)
    } else {
        (e, s)
    }
}

/// §4.2.3: for every undirected mesh edge, the set of B-Rep faces carried by
/// the triangles incident to it — "querying the triangles that intersect at
/// that point", read through the per-triangle provenance map.
///
/// Triangles with no attribution (`None`) contribute nothing; they cannot name
/// a face, and guessing one is exactly what N4's retirement removed.
pub(crate) fn provenance_edge_incidence(
    mesh: &Mesh,
    attribution: &TriangleAttributionMap,
) -> IncidenceIds {
    let mut out: IncidenceIds = BTreeMap::new();
    for (t, tri) in mesh.tris.iter().enumerate() {
        if t >= attribution.len() {
            break;
        }
        let Some(attr) = attribution.lookup(t as u32) else {
            continue;
        };
        for i in 0..3 {
            let k = key(tri[i], tri[(i + 1) % 3]);
            out.entry(k).or_default().insert(attr);
        }
    }
    out
}

/// The shipped cycle-derived incidence, re-expressed on face IDENTITY so it is
/// comparable with [`provenance_edge_incidence`].
///
/// Mirrors `compute_phase_a`'s loop exactly: per patch, per cycle, per edge,
/// push the patch's own `(input, face_idx)`.
pub(crate) fn cycle_edge_incidence<'a, I>(infos: I) -> IncidenceIds
where
    I: IntoIterator<Item = (InputId, u32, &'a [Vec<(u32, u32)>])>,
{
    let mut out: IncidenceIds = BTreeMap::new();
    for (input, face_idx, cycles) in infos {
        let attr = TriangleAttribution {
            input,
            face: face_idx,
        };
        for cycle in cycles {
            for &(s, e) in cycle {
                out.entry(key(s, e)).or_default().insert(attr);
            }
        }
    }
    out
}

/// How a single boundary edge's two views relate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EdgeVerdict {
    /// Identical face sets.
    Agree,
    /// Provenance names every face the cycle view does, and more. Benign: the
    /// cycle view only ever pushes the patches whose CYCLE the edge is on,
    /// while provenance sees every incident triangle.
    ProvenanceRicher,
    /// **The mis-tag.** The cycle view claims a face that no triangle actually
    /// incident to the edge carries.
    CycleClaimsUnsupportedFace,
    /// Neither set contains the other.
    Disjointish,
}

/// Diff of the two views over the CYCLE view's key set (the boundary edges —
/// the only ones `compute_phase_a` publishes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IncidenceDiff {
    pub(crate) boundary_edges: usize,
    pub(crate) agree: usize,
    pub(crate) prov_richer: usize,
    pub(crate) cycle_unsupported: usize,
    pub(crate) disjointish: usize,
    /// Boundary edges absent from the provenance view entirely (no attributed
    /// incident triangle). A producer fault if non-zero, so it is counted
    /// separately rather than folded into a verdict.
    pub(crate) missing_in_prov: usize,
    /// The `CycleClaimsUnsupportedFace` edges, for anchoring.
    pub(crate) unsupported_samples: Vec<(EdgeKey, Vec<TriangleAttribution>)>,
    /// The `Disjointish` edges, for anchoring: `(edge, cycle-only, prov-only)`.
    /// Sampled separately because a two-way divergence names a different
    /// mechanism than a one-way one, and the 2026-08-08 C0043 smoke test
    /// landed here rather than in `unsupported_samples`.
    pub(crate) disjoint_samples: Vec<(
        EdgeKey,
        Vec<TriangleAttribution>,
        Vec<TriangleAttribution>,
    )>,
    /// Divergent edges on an IMPURE (merged) patch's cycle — by design.
    pub(crate) divergent_merge_explained: usize,
    /// **The load-bearing number.** Divergent edges NOT explained by patch
    /// merging: the cycle-derived incidence and the §4.2.3 provenance view
    /// disagree about which faces meet at an edge, with no design reason.
    pub(crate) divergent_unexplained: usize,
    /// Samples of the unexplained set: `(edge, verdict, cycle-only, prov-only)`.
    pub(crate) unexplained_samples: Vec<(
        EdgeKey,
        EdgeVerdict,
        Vec<TriangleAttribution>,
        Vec<TriangleAttribution>,
    )>,
}

/// Classify one edge's two face sets.
pub(crate) fn classify_edge(
    cycle: &BTreeSet<TriangleAttribution>,
    prov: &BTreeSet<TriangleAttribution>,
) -> EdgeVerdict {
    if cycle == prov {
        EdgeVerdict::Agree
    } else if cycle.is_subset(prov) {
        EdgeVerdict::ProvenanceRicher
    } else if prov.is_subset(cycle) {
        EdgeVerdict::CycleClaimsUnsupportedFace
    } else {
        EdgeVerdict::Disjointish
    }
}

/// A patch is IMPURE when it holds a triangle whose own attribution differs
/// from the patch's — which `flood_fill_patches` can never produce (it groups
/// by identical attribution) and `merge_same_plane_patches` always does (the
/// merged patch takes the lexicographically smallest member's attribution).
///
/// So "impure" is exactly "was merged", detected from the data rather than by
/// threading a flag out of the merge.
pub(crate) fn patch_is_impure(
    tri_indices: &[u32],
    attribution: &TriangleAttributionMap,
    patch_attr: TriangleAttribution,
) -> bool {
    tri_indices.iter().any(|&t| {
        (t as usize) < attribution.len() && attribution.lookup(t) != Some(patch_attr)
    })
}

/// Diff the two views. Read-only; changes nothing.
///
/// `explained` is the set of boundary edges belonging to an IMPURE (merged)
/// patch's cycle. Divergence there is BY DESIGN — PR-YR27's merge deliberately
/// re-identifies a connected same-plane region as one output face, so its
/// boundary carries the merged identity while the incident triangles keep their
/// pre-merge ones. The load-bearing number is the divergence OUTSIDE that set.
pub(crate) fn diff_incidence(
    cycle: &IncidenceIds,
    prov: &IncidenceIds,
    explained: &BTreeSet<EdgeKey>,
) -> IncidenceDiff {
    let mut d = IncidenceDiff {
        boundary_edges: cycle.len(),
        ..Default::default()
    };
    for (k, cset) in cycle {
        let Some(pset) = prov.get(k) else {
            d.missing_in_prov += 1;
            continue;
        };
        let v = classify_edge(cset, pset);
        if v != EdgeVerdict::Agree {
            if explained.contains(k) {
                d.divergent_merge_explained += 1;
            } else {
                d.divergent_unexplained += 1;
                if d.unexplained_samples.len() < 8 {
                    d.unexplained_samples.push((
                        *k,
                        v,
                        cset.difference(pset).copied().collect(),
                        pset.difference(cset).copied().collect(),
                    ));
                }
            }
        }
        match v {
            EdgeVerdict::Agree => d.agree += 1,
            EdgeVerdict::ProvenanceRicher => d.prov_richer += 1,
            EdgeVerdict::Disjointish => {
                d.disjointish += 1;
                if d.disjoint_samples.len() < 8 {
                    d.disjoint_samples.push((
                        *k,
                        cset.difference(pset).copied().collect(),
                        pset.difference(cset).copied().collect(),
                    ));
                }
            }
            EdgeVerdict::CycleClaimsUnsupportedFace => {
                d.cycle_unsupported += 1;
                if d.unsupported_samples.len() < 8 {
                    d.unsupported_samples
                        .push((*k, cset.difference(pset).copied().collect()));
                }
            }
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_primitives::Point3;

    fn attr(input: InputId, face: u32) -> TriangleAttribution {
        TriangleAttribution { input, face }
    }

    fn map(v: Vec<Option<TriangleAttribution>>) -> TriangleAttributionMap {
        TriangleAttributionMap { attributions: v }
    }

    /// Two triangles sharing edge (1,2); one on A/face0, one on A/face1.
    fn two_tri_mesh() -> Mesh {
        Mesh::new(
            vec![
                Point3::new(0.0, 0.0, 0.0),
                Point3::new(1.0, 0.0, 0.0),
                Point3::new(0.0, 1.0, 0.0),
                Point3::new(1.0, 1.0, 0.0),
            ],
            vec![[0, 1, 2], [1, 3, 2]],
        )
    }

    #[test]
    fn provenance_incidence_collects_both_sides_of_a_shared_edge() {
        let mesh = two_tri_mesh();
        let a = map(vec![
            Some(attr(InputId::A, 0)),
            Some(attr(InputId::A, 1)),
        ]);
        let inc = provenance_edge_incidence(&mesh, &a);
        // The shared edge (1,2) sees BOTH faces.
        assert_eq!(
            inc[&(1, 2)],
            BTreeSet::from([attr(InputId::A, 0), attr(InputId::A, 1)])
        );
        // A private edge sees only its own triangle's face.
        assert_eq!(inc[&(0, 1)], BTreeSet::from([attr(InputId::A, 0)]));
    }

    #[test]
    fn unattributed_triangles_contribute_nothing() {
        let mesh = two_tri_mesh();
        let a = map(vec![Some(attr(InputId::A, 0)), None]);
        let inc = provenance_edge_incidence(&mesh, &a);
        assert_eq!(inc[&(1, 2)], BTreeSet::from([attr(InputId::A, 0)]));
        // Edge (1,3) belongs only to the unattributed triangle ⇒ absent.
        assert!(!inc.contains_key(&(1, 3)));
    }

    #[test]
    fn a_shorter_attribution_map_does_not_panic() {
        let mesh = two_tri_mesh();
        let a = map(vec![Some(attr(InputId::A, 0))]); // 1 entry, 2 tris
        let inc = provenance_edge_incidence(&mesh, &a);
        assert_eq!(inc[&(0, 1)], BTreeSet::from([attr(InputId::A, 0)]));
    }

    #[test]
    fn cycle_view_mirrors_the_phase_a_push() {
        let cycles: Vec<Vec<(u32, u32)>> = vec![vec![(0, 1), (1, 2), (2, 0)]];
        let inc = cycle_edge_incidence([(InputId::B, 7u32, cycles.as_slice())]);
        assert_eq!(inc.len(), 3);
        assert_eq!(inc[&(0, 1)], BTreeSet::from([attr(InputId::B, 7)]));
        // Keyed undirected: (2,0) canonicalizes to (0,2).
        assert!(inc.contains_key(&(0, 2)));
    }

    #[test]
    fn identical_views_agree() {
        let c = BTreeSet::from([attr(InputId::A, 0), attr(InputId::B, 1)]);
        assert_eq!(classify_edge(&c, &c.clone()), EdgeVerdict::Agree);
    }

    #[test]
    fn provenance_seeing_more_is_benign() {
        let c = BTreeSet::from([attr(InputId::A, 0)]);
        let p = BTreeSet::from([attr(InputId::A, 0), attr(InputId::B, 1)]);
        assert_eq!(classify_edge(&c, &p), EdgeVerdict::ProvenanceRicher);
    }

    /// The merged-patch shape: the cycle view pushes face 0 (the
    /// lexicographically smallest member), but the triangles actually incident
    /// to the edge carry face 1.
    #[test]
    fn cycle_claiming_a_face_no_incident_triangle_carries_is_the_mistag() {
        let c = BTreeSet::from([attr(InputId::A, 0), attr(InputId::A, 1)]);
        let p = BTreeSet::from([attr(InputId::A, 1)]);
        assert_eq!(
            classify_edge(&c, &p),
            EdgeVerdict::CycleClaimsUnsupportedFace
        );
    }

    #[test]
    fn crossing_sets_are_disjointish() {
        let c = BTreeSet::from([attr(InputId::A, 0)]);
        let p = BTreeSet::from([attr(InputId::B, 0)]);
        assert_eq!(classify_edge(&c, &p), EdgeVerdict::Disjointish);
    }

    #[test]
    fn diff_counts_each_bucket_and_samples_the_mistags() {
        let cycle: IncidenceIds = BTreeMap::from([
            ((0, 1), BTreeSet::from([attr(InputId::A, 0)])),
            (
                (1, 2),
                BTreeSet::from([attr(InputId::A, 0), attr(InputId::A, 1)]),
            ),
            ((2, 3), BTreeSet::from([attr(InputId::A, 5)])),
        ]);
        let prov: IncidenceIds = BTreeMap::from([
            ((0, 1), BTreeSet::from([attr(InputId::A, 0)])),
            ((1, 2), BTreeSet::from([attr(InputId::A, 1)])),
            // (2,3) absent ⇒ missing_in_prov
        ]);
        let d = diff_incidence(&cycle, &prov, &BTreeSet::new());
        assert!(d.disjoint_samples.is_empty(), "no two-way divergence here");
        assert_eq!(d.boundary_edges, 3);
        assert_eq!(d.agree, 1);
        assert_eq!(d.cycle_unsupported, 1);
        assert_eq!(d.missing_in_prov, 1);
        assert_eq!(d.unsupported_samples.len(), 1);
        assert_eq!(d.unsupported_samples[0].0, (1, 2));
        assert_eq!(d.unsupported_samples[0].1, vec![attr(InputId::A, 0)]);
    }

    /// The load-bearing invariant: with NO patch merging, the two views agree
    /// on every boundary edge — so any divergence the sweep reports is real
    /// signal, not a modelling artifact of this probe.
    #[test]
    fn unmerged_patches_agree_by_construction() {
        let mesh = two_tri_mesh();
        // Each triangle is its own patch (distinct attributions), so each
        // patch's cycle carries exactly its own face.
        let a = map(vec![
            Some(attr(InputId::A, 0)),
            Some(attr(InputId::A, 1)),
        ]);
        let prov = provenance_edge_incidence(&mesh, &a);
        let c0: Vec<Vec<(u32, u32)>> = vec![vec![(0, 1), (1, 2), (2, 0)]];
        let c1: Vec<Vec<(u32, u32)>> = vec![vec![(1, 3), (3, 2), (2, 1)]];
        let cycle = cycle_edge_incidence([
            (InputId::A, 0u32, c0.as_slice()),
            (InputId::A, 1u32, c1.as_slice()),
        ]);
        let d = diff_incidence(&cycle, &prov, &BTreeSet::new());
        assert_eq!(d.cycle_unsupported, 0, "no merge ⇒ no unsupported claim");
        assert_eq!(d.disjointish, 0);
        assert_eq!(d.missing_in_prov, 0);
        assert_eq!(d.agree + d.prov_richer, d.boundary_edges);
    }
}
