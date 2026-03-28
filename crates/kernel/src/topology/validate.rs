//! Topology validation — Euler-Poincaré checker, manifold edge checker.

use super::arena::TopoArena;

/// Check the Euler-Poincaré formula: V - E + F = 2S (for genus-0 with no inner loops).
///
/// For the general case: V - E + F = 2(S - G) + R
/// where S = shells, G = genus, R = inner loops (rings).
/// For a simple solid (S=1, G=0, R=0): V - E + F = 2.
pub fn check_euler_poincare(arena: &TopoArena) -> Result<(), String> {
    let v = arena.vertex_count() as i64;
    let e = arena.edge_count() as i64;
    let f = arena.face_count() as i64;
    let s = arena.shell_count() as i64;

    // Count inner loops (rings)
    let r: i64 = arena
        .faces
        .iter()
        .map(|face| face.inner_loops.len() as i64)
        .sum();

    // V - E + F - R = 2S (for genus-0)
    let lhs = v - e + f - r;
    let rhs = 2 * s;

    if lhs == rhs {
        Ok(())
    } else {
        Err(format!(
            "Euler-Poincaré violated: V({}) - E({}) + F({}) - R({}) = {} ≠ 2S({})",
            v, e, f, r, lhs, rhs
        ))
    }
}

/// Check that every edge has exactly two distinct faces (manifold condition).
/// Returns Ok if manifold, Err with description of violation.
pub fn check_manifold_edges(arena: &TopoArena) -> Result<(), String> {
    for (i, edge) in arena.edges.iter().enumerate() {
        let he_a = edge.half_edge;
        let he_b = arena.half_edges[he_a.0].twin;

        let loop_a = arena.half_edges[he_a.0].loop_;
        let loop_b = arena.half_edges[he_b.0].loop_;

        let face_a = arena.loops[loop_a.0].face;
        let face_b = arena.loops[loop_b.0].face;

        if face_a == face_b {
            return Err(format!(
                "Non-manifold: edge {} has both half-edges in face {:?}",
                i, face_a
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::euler_ops::*;

    #[test]
    fn mvfs_satisfies_euler() {
        let mut arena = TopoArena::new();
        mvfs(&mut arena, [0.0, 0.0, 0.0]);
        // V=1, E=0, F=1, S=1, R=0 → 1-0+1-0=2=2*1 ✓
        assert_eq!(arena.vertex_count(), 1);
        assert_eq!(arena.edge_count(), 0);
        assert_eq!(arena.face_count(), 1);
        assert_eq!(arena.shell_count(), 1);
        assert!(check_euler_poincare(&arena).is_ok());
    }

    #[test]
    fn triangle_satisfies_euler() {
        let mut arena = TopoArena::new();
        let (_, _, face, v0) = mvfs(&mut arena, [0.0, 0.0, 0.0]);
        let loop_ = arena.faces[face.0].outer_loop;
        let (_, v1) = mev(&mut arena, v0, loop_, [1.0, 0.0, 0.0]);
        let (_, v2) = mev(&mut arena, v1, loop_, [0.5, 1.0, 0.0]);
        let _ = mef(&mut arena, v2, v0, loop_);

        // V=3, E=3, F=2, S=1, R=0 → 3-3+2=2=2*1 ✓
        assert_eq!(arena.vertex_count(), 3);
        assert_eq!(arena.edge_count(), 3);
        assert_eq!(arena.face_count(), 2);
        assert_eq!(arena.shell_count(), 1);
        assert!(check_euler_poincare(&arena).is_ok());
        assert!(check_manifold_edges(&arena).is_ok());
    }
}
