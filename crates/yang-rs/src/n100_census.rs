//! Print-only census for the possible silent paths of ledger N100 (items 1,
//! 2, 4, 5): `YANG_N100_CENSUS=<file>` appends one line per firing, keyed by
//! `ASSAY_CASE` (subprocess-safe under `ASSAY_JOBS`, whose children's stderr
//! the assay runner discards). No effect on any result.

/// True when the census is on — callers guard any extra measurement with it.
pub(crate) fn on() -> bool {
    std::env::var_os("YANG_N100_CENSUS").is_some()
}

/// Append `item\tdetail` for the current case.
pub(crate) fn log(item: u8, detail: &str) {
    let Some(path) = std::env::var_os("YANG_N100_CENSUS") else {
        return;
    };
    let case = std::env::var("ASSAY_CASE").unwrap_or_else(|_| "-".into());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(f, "{case}\titem{item}\t{detail}");
    }
}
