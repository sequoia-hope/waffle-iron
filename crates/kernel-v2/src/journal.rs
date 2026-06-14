//! Operation journal (KV13 F2) — per-operation evolution of persistent face
//! identities ([`crate::arena::Pid`]).
//!
//! Each kernel operation that produces a solid appends an [`Evolution`]
//! recording how its OUTPUT entities descend from its INPUT entities — the
//! Parasolid "operation journal" / OCCT GENERATED·MODIFIED·DELETED model.
//! Lineage (F3) is the transitive closure of `modified` edges back to a
//! `generated` origin; `FaceOrigin` walks it to the introducing feature.
//!
//! F2 records the **boolean**: every output face descends (via yang's
//! per-face attribution) from an operand face, recorded as a `modified` edge
//! `(operand_pid → output_pid)`. Constructor (extrude/revolve/…) evolutions —
//! where every output face is `generated` — are added when F3 needs them.

use crate::arena::Pid;
use cad_primitives::BoolOp;

/// How an output entity descends from an input entity (Parasolid-style
/// evolution kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvoKind {
    /// The output face IS the input face (carried through unchanged).
    Same,
    /// The output face is a trimmed sub-region of the input face.
    Trimmed,
    /// The input face split into several output faces (this is one of them).
    Split,
    /// Several input faces merged into one output face (e.g. coplanar union).
    Merge,
}

/// The operation that produced an [`Evolution`]. Carries only kernel-level
/// identity; the owning feature id is attached at the feature-engine layer
/// (F3/F5), which knows the feature tree the kernel does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpTag {
    /// A boolean of two operand solids.
    Boolean(BoolOp),
}

/// One operation's effect on persistent FACE identities (KV13 F2;
/// edges/vertices follow in F1b/F4d).
#[derive(Debug, Clone, PartialEq)]
pub struct Evolution {
    /// The producing operation.
    pub op: OpTag,
    /// Output faces with no input ancestor (a genuinely new surface).
    pub generated: Vec<Pid>,
    /// `(input_pid, output_pid, kind)` lineage edges.
    pub modified: Vec<(Pid, Pid, EvoKind)>,
    /// Input faces consumed by the operation (no output descends from them).
    pub deleted: Vec<Pid>,
}
