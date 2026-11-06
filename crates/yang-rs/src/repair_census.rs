//! Print-only census of the Stage-4/5 repair passes the §4.4.1 per-patch
//! update is meant to subsume (`docs/yang_faithfulness_roadmap.md` F2,
//! "retire the repairs it subsumes, one per corpus run, cheapest proof
//! first"): `YANG_REPAIR_CENSUS=<file>` appends one line per pass that
//! CHANGED the mesh, keyed by `ASSAY_CASE` (subprocess-safe under
//! `ASSAY_JOBS`). A pass that never fires on the corpus is the cheapest
//! retirement. No effect on any result.

/// Append `pass\tcount` for the current case when `count > 0`.
pub(crate) fn log(pass: &str, count: usize) {
    if count == 0 {
        return;
    }
    let Some(path) = std::env::var_os("YANG_REPAIR_CENSUS") else {
        return;
    };
    let case = std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(f, "{case}\t{pass}\t{count}");
    }
}
