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

/// The lineage of a face's persistent id (KV13 F3): the **root** pid — where
/// the geometry was introduced (a face with no incoming `modified` edge, i.e.
/// produced directly by a constructor, not derived through a boolean) — and
/// the ops the geometry passed through on its way to the queried pid,
/// **newest-first**.
///
/// `created_by` (the FEATURE that introduced the root) is resolved at the
/// feature-engine layer (F5), which maps a root `Pid` → its creating feature;
/// the kernel knows only the `Pid` lineage, not the feature tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceLineage {
    /// The pid where this geometry was introduced (no incoming edge).
    pub root: Pid,
    /// Ops traversed from the queried pid back to `root`, newest-first.
    pub through: Vec<OpTag>,
}

/// Walk the journal's `modified` edges from `pid` back to its root. Each output
/// pid has at most one incoming edge under F2 (a merge — multiple inputs to one
/// output — is F4c; this takes the first found and stops, documented). The
/// search is newest-first, so a queried output pid resolves through the most
/// recent operation first. Bounded by the journal length (the journal is a DAG
/// — edges go older-pid → newer-pid — but the budget guards against corruption).
pub fn face_lineage(journal: &[Evolution], pid: Pid) -> FaceLineage {
    let mut cur = pid;
    let mut through: Vec<OpTag> = Vec::new();
    let mut budget = journal.len() + 1;
    loop {
        let mut found: Option<(Pid, OpTag)> = None;
        'search: for ev in journal.iter().rev() {
            for &(inp, outp, _) in &ev.modified {
                if outp == cur {
                    found = Some((inp, ev.op));
                    break 'search;
                }
            }
        }
        match found {
            Some((inp, op)) => {
                through.push(op);
                cur = inp;
            }
            None => break,
        }
        budget -= 1;
        if budget == 0 {
            break; // corruption guard; a well-formed journal terminates above
        }
    }
    FaceLineage { root: cur, through }
}

/// Inverse lineage (KV13 F3): every pid that descends from `root` via the
/// journal's `modified` edges (transitively). The caller intersects with the
/// current solid's live face pids to get "the faces a given origin produced."
pub fn descendants(journal: &[Evolution], root: Pid) -> Vec<Pid> {
    use std::collections::BTreeSet;
    let mut seen: BTreeSet<Pid> = BTreeSet::new();
    seen.insert(root);
    let mut frontier = vec![root];
    let mut out = Vec::new();
    while let Some(p) = frontier.pop() {
        for ev in journal {
            for &(inp, outp, _) in &ev.modified {
                if inp == p && seen.insert(outp) {
                    out.push(outp);
                    frontier.push(outp);
                }
            }
        }
    }
    out
}
