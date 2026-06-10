//! Half-edge B-Rep arena: entity types, ids, and slot storage.
//!
//! ## Structure
//!
//! The topology follows the half-edge ("winged-edge link") representation
//! described by Stroud 2006 §2.2/§3.3 (he attributes the term "half-edges"
//! to Mäntylä): every undirected edge is represented by exactly two directed
//! half-edges that are mutual twins; half-edges chain into closed loops
//! (`next`/`prev` cycles); loops bound faces (one outer loop, zero or more
//! inner "ring"/hole loops); faces group into shells; shells group into
//! solids.
//!
//! There is no separate `Edge` entity: half-edges pair directly via `twin`,
//! and the edge count is `live half-edges / 2` (an invariant checked by
//! `validate_solid`).
//!
//! ## Invariants (encoded in types where possible)
//!
//! - **Twin pairing**: `twin(twin(h)) == h`, `twin(h) != h`. `twin` is a
//!   plain `HalfEdgeId` (never `Option`) — half-edges are only ever created
//!   in pairs by the Euler operators, so an unpaired half-edge is
//!   unrepresentable through the sanctioned mutation API.
//! - **Loop closure**: `next`/`prev` are total and mutually inverse; the
//!   `next` orbit of any half-edge returns to it and stays in one loop.
//! - **Lone-vertex loops**: the loop created by `mvfs` contains a single
//!   vertex and no half-edges. This is encoded as [`LoopBoundary::Lone`] —
//!   a loop cannot be simultaneously empty and edged.
//! - **Newell invariant** (crate hard rule 2): for every face whose outer
//!   loop has a numerically nonzero Newell normal,
//!   `face.surface.normal ≡ normalize(Newell(outer_loop))`. Faces under
//!   construction whose loop is degenerate (lone vertex, or a path walked
//!   out and back so the Newell sum cancels exactly) carry `surface: None`;
//!   `validate_solid` requires `Some` on every face of a finished solid.
//! - **2-manifoldness** (crate hard rule 3): maintained by the Euler
//!   operators' preconditions and checked by `validate_solid` (single radial
//!   fan per vertex, exactly two half-edges per edge).
//!
//! ## Storage and determinism
//!
//! Plain index arenas: `Vec<Option<T>>` slots. Killed entities become `None`
//! and slots are **never reused** — so two identical construction sequences
//! produce bit-identical arenas (the determinism oracle relies on this).
//! Ids are slot indices; access through the checked getters returns
//! `Err(KernelV2Error::InvalidId)` for dead or out-of-range ids.
//!
//! Fields are `pub` so that higher layers and the validation tests can
//! inspect (and, in tests, deliberately corrupt) the structure. **All
//! production mutation must go through the Euler operators** in
//! [`crate::euler`] — they are the only code that upholds the invariants.

use crate::error::KernelV2Error;
use cad_primitives::Point3;

// ---------------------------------------------------------------------------
// Ids
// ---------------------------------------------------------------------------

macro_rules! define_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);

        impl $name {
            /// Slot index into the owning arena vector.
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }
    };
}

define_id!(
    /// Index of a [`Vertex`] slot in the arena.
    VertexId
);
define_id!(
    /// Index of a [`HalfEdge`] slot in the arena.
    HalfEdgeId
);
define_id!(
    /// Index of a [`Loop`] slot in the arena.
    LoopId
);
define_id!(
    /// Index of a [`Face`] slot in the arena.
    FaceId
);
define_id!(
    /// Index of a [`Shell`] slot in the arena.
    ShellId
);
define_id!(
    /// Index of a [`Solid`] slot in the arena.
    SolidId
);

// ---------------------------------------------------------------------------
// Geometry carried by topology
// ---------------------------------------------------------------------------

/// A direction in 3-space. Local to kernel-v2 until `cad-primitives` grows
/// vector arithmetic; stored unit-length on [`Plane::normal`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitVector3 {
    /// X component.
    pub x: f64,
    /// Y component.
    pub y: f64,
    /// Z component.
    pub z: f64,
}

/// An (unbounded) plane: a point on the plane plus its unit normal.
///
/// The normal of a face's plane is **derived from the polygon walk** of the
/// face's outer loop (normalized Newell normal) — the walk direction is the
/// source of truth (crate hard rule 5); the stored normal is a cache whose
/// agreement is asserted at every operator exit and by `validate_solid`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plane {
    /// A point on the plane (the origin of the loop's representative
    /// half-edge at the time the plane was computed).
    pub point: Point3,
    /// Unit normal, `≡ normalize(Newell(outer_loop))`.
    pub normal: UnitVector3,
}

/// Surface descriptor carried by a face.
///
/// KV1 implements `Plane` only. The enum is `#[non_exhaustive]` so Phase-2
/// curved surfaces (cylinder, sphere, cone, torus) extend it without breaking
/// downstream matches.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Surface {
    /// Planar surface.
    Plane(Plane),
}

// ---------------------------------------------------------------------------
// Topological entities
// ---------------------------------------------------------------------------

/// A vertex: geometry only. Incident half-edges are derived by scanning
/// (validation is O(n) anyway), which removes a whole class of stale-pointer
/// invariants from the mutation paths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    /// Position (meters, like everything in Waffle Iron).
    pub point: Point3,
}

/// A directed half-edge. See module docs for the invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HalfEdge {
    /// The oppositely-directed partner sharing the same undirected edge.
    pub twin: HalfEdgeId,
    /// Next half-edge around the owning loop.
    pub next: HalfEdgeId,
    /// Previous half-edge around the owning loop (`prev(next(h)) == h`).
    pub prev: HalfEdgeId,
    /// Vertex this half-edge leaves from. Its destination is
    /// `origin(next)` (== `origin(twin)`).
    pub origin: VertexId,
    /// Loop this half-edge belongs to.
    pub loop_id: LoopId,
}

/// What bounds a loop: either a single isolated vertex (the state created by
/// `mvfs`, before any edge exists) or a closed cycle of half-edges identified
/// by a representative member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopBoundary {
    /// Loop consists of one vertex and no edges (Stroud 2006 §F.8 creates
    /// this state; §F.10 calls the inner-loop analog a vertex hole-loop).
    Lone(VertexId),
    /// Loop is a closed half-edge cycle; the id is any member of the cycle.
    Edges(HalfEdgeId),
}

/// Whether a loop is a face's perimeter or a hole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopKind {
    /// The face's perimeter loop.
    Outer,
    /// An inner loop (ring / hole-loop). Counts toward `R` in the
    /// Euler–Poincaré formula and must wind opposite to the outer loop.
    Inner,
}

/// A closed boundary cycle belonging to a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loop {
    /// Owning face.
    pub face: FaceId,
    /// The boundary content.
    pub boundary: LoopBoundary,
    /// Outer (perimeter) or inner (ring).
    pub kind: LoopKind,
}

/// A face: one outer loop, zero or more inner loops, a surface descriptor.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// Surface geometry. `None` only while the face is under construction
    /// and its outer loop is degenerate (see module docs); `validate_solid`
    /// rejects finished solids containing `None`.
    pub surface: Option<Surface>,
    /// The perimeter loop.
    pub outer_loop: LoopId,
    /// Rings (hole-loops), in creation order.
    pub inner_loops: Vec<LoopId>,
    /// Owning shell.
    pub shell: ShellId,
}

/// A connected set of faces forming one closed surface of a solid.
#[derive(Debug, Clone, PartialEq)]
pub struct Shell {
    /// Owning solid.
    pub solid: SolidId,
    /// Member faces, in creation order.
    pub faces: Vec<FaceId>,
    /// Genus bookkeeping: incremented by `kfmrh` (through-hole creation).
    /// Stroud 2006 §F.9 notes genus "is not specifically represented in the
    /// object datastructure" and must otherwise be derived; we track it
    /// explicitly so the Euler–Poincaré formula is checkable at every step.
    pub genus: u32,
}

/// A solid: one or more shells.
#[derive(Debug, Clone, PartialEq)]
pub struct Solid {
    /// Member shells (first is the peripheral shell), in creation order.
    pub shells: Vec<ShellId>,
}

// ---------------------------------------------------------------------------
// Arena
// ---------------------------------------------------------------------------

/// Slot-arena storage for the whole B-Rep. See module docs for the storage
/// and determinism contract.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BrepArena {
    /// Vertex slots (`None` == killed).
    pub vertices: Vec<Option<Vertex>>,
    /// Half-edge slots.
    pub half_edges: Vec<Option<HalfEdge>>,
    /// Loop slots.
    pub loops: Vec<Option<Loop>>,
    /// Face slots.
    pub faces: Vec<Option<Face>>,
    /// Shell slots.
    pub shells: Vec<Option<Shell>>,
    /// Solid slots.
    pub solids: Vec<Option<Solid>>,
}

macro_rules! checked_getters {
    ($get:ident, $get_mut:ident, $vec:ident, $ty:ty, $id:ty, $kind:literal) => {
        /// Checked read access; `Err(InvalidId)` for dead or out-of-range ids.
        pub fn $get(&self, id: $id) -> Result<&$ty, KernelV2Error> {
            self.$vec
                .get(id.index())
                .and_then(|slot| slot.as_ref())
                .ok_or(KernelV2Error::InvalidId { kind: $kind })
        }

        /// Checked write access; `Err(InvalidId)` for dead or out-of-range ids.
        pub fn $get_mut(&mut self, id: $id) -> Result<&mut $ty, KernelV2Error> {
            self.$vec
                .get_mut(id.index())
                .and_then(|slot| slot.as_mut())
                .ok_or(KernelV2Error::InvalidId { kind: $kind })
        }
    };
}

impl BrepArena {
    /// Empty arena.
    pub fn new() -> Self {
        Self::default()
    }

    checked_getters!(vertex, vertex_mut, vertices, Vertex, VertexId, "vertex");
    checked_getters!(
        half_edge,
        half_edge_mut,
        half_edges,
        HalfEdge,
        HalfEdgeId,
        "half_edge"
    );
    checked_getters!(loop_, loop_mut, loops, Loop, LoopId, "loop");
    checked_getters!(face, face_mut, faces, Face, FaceId, "face");
    checked_getters!(shell, shell_mut, shells, Shell, ShellId, "shell");
    checked_getters!(solid, solid_mut, solids, Solid, SolidId, "solid");

    /// Number of live vertices in the whole arena.
    pub fn num_vertices(&self) -> usize {
        self.vertices.iter().flatten().count()
    }

    /// Number of live half-edges in the whole arena.
    pub fn num_half_edges(&self) -> usize {
        self.half_edges.iter().flatten().count()
    }

    /// Number of live undirected edges (`half-edges / 2`).
    pub fn num_edges(&self) -> usize {
        self.num_half_edges() / 2
    }

    /// Number of live faces in the whole arena.
    pub fn num_faces(&self) -> usize {
        self.faces.iter().flatten().count()
    }

    /// Number of live inner loops (rings / hole-loops) in the whole arena.
    pub fn num_rings(&self) -> usize {
        self.loops
            .iter()
            .flatten()
            .filter(|l| l.kind == LoopKind::Inner)
            .count()
    }

    /// Walk the half-edge cycle of a loop, starting at its representative.
    ///
    /// Returns the empty vector for a [`LoopBoundary::Lone`] loop. Returns
    /// `Err(LoopNotClosed)` if the `next` orbit fails to return to the
    /// representative within the number of live half-edges (a corrupted
    /// arena), or `Err(InvalidId)` if the walk hits a dead slot.
    pub fn loop_half_edges(&self, loop_id: LoopId) -> Result<Vec<HalfEdgeId>, KernelV2Error> {
        let lp = self.loop_(loop_id)?;
        let start = match lp.boundary {
            LoopBoundary::Lone(_) => return Ok(Vec::new()),
            LoopBoundary::Edges(h) => h,
        };
        let mut out = Vec::new();
        let mut cur = start;
        let budget = self.num_half_edges();
        loop {
            out.push(cur);
            cur = self.half_edge(cur)?.next;
            if cur == start {
                return Ok(out);
            }
            if out.len() > budget {
                return Err(KernelV2Error::LoopNotClosed { loop_id });
            }
        }
    }

    /// Origin points of a loop's half-edge cycle, in walk order.
    pub fn loop_points(&self, loop_id: LoopId) -> Result<Vec<Point3>, KernelV2Error> {
        self.loop_half_edges(loop_id)?
            .into_iter()
            .map(|h| Ok(self.vertex(self.half_edge(h)?.origin)?.point))
            .collect()
    }

    /// Euler–Poincaré element counts for one solid (entities reachable from
    /// its shells).
    pub fn euler_counts(&self, solid: SolidId) -> Result<EulerCounts, KernelV2Error> {
        let solid_ref = self.solid(solid)?;
        let mut vertices = std::collections::BTreeSet::new();
        let mut half_edges: usize = 0;
        let mut faces: usize = 0;
        let mut rings: usize = 0;
        let mut genus: u32 = 0;
        let shells = solid_ref.shells.len();
        for &sh in &solid_ref.shells {
            let shell = self.shell(sh)?;
            genus += shell.genus;
            for &f in &shell.faces {
                faces += 1;
                let face = self.face(f)?;
                let mut loops = vec![face.outer_loop];
                loops.extend(face.inner_loops.iter().copied());
                for lid in loops {
                    let lp = self.loop_(lid)?;
                    if lp.kind == LoopKind::Inner {
                        rings += 1;
                    }
                    match lp.boundary {
                        LoopBoundary::Lone(v) => {
                            vertices.insert(v);
                        }
                        LoopBoundary::Edges(_) => {
                            for h in self.loop_half_edges(lid)? {
                                half_edges += 1;
                                vertices.insert(self.half_edge(h)?.origin);
                            }
                        }
                    }
                }
            }
        }
        Ok(EulerCounts {
            v: vertices.len() as i64,
            e: (half_edges / 2) as i64,
            f: faces as i64,
            r: rings as i64,
            s: shells as i64,
            g: genus as i64,
        })
    }
}

/// Element counts entering the Euler–Poincaré formula
/// `V − E + F − R = 2(S − G)` (Stroud 2006 §4, rule 4, with
/// v/e/f/h/b/g spelled V/E/F/R/S/G here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EulerCounts {
    /// Vertices.
    pub v: i64,
    /// Edges (half-edge pairs).
    pub e: i64,
    /// Faces.
    pub f: i64,
    /// Rings (inner / hole-loops; Stroud's `h`).
    pub r: i64,
    /// Shells (Stroud's multiplicity `b`).
    pub s: i64,
    /// Genus.
    pub g: i64,
}

impl EulerCounts {
    /// Left-hand side `V − E + F − R`.
    pub fn lhs(&self) -> i64 {
        self.v - self.e + self.f - self.r
    }

    /// Right-hand side `2(S − G)`.
    pub fn rhs(&self) -> i64 {
        2 * (self.s - self.g)
    }

    /// Whether the Euler–Poincaré formula holds.
    pub fn holds(&self) -> bool {
        self.lhs() == self.rhs()
    }
}
