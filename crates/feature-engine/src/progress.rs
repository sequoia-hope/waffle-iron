//! Rebuild progress frames (`specs/b4_balanced_union.md` §2.3).
//!
//! A long feature (today: `UnionAll`, one pairwise union at a time) reports
//! its steps through a process-wide sink. The engine is single-threaded in
//! every host (the WASM worker, the native test binaries), so the sink is a
//! thread-local: the host installs one closure per thread and every
//! `report` on that thread reaches it. No sink installed ⇒ `report` is a
//! no-op, so library code reports unconditionally.

use std::cell::RefCell;

use uuid::Uuid;

/// One progress step of a feature under rebuild.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProgressEvent {
    /// The feature reporting.
    pub feature_id: Uuid,
    /// Its user-visible name.
    pub feature_name: String,
    /// Steps completed so far (for `UnionAll`: pairwise unions RUN).
    pub done: usize,
    /// Work left, in the feature's own unit (for `UnionAll`: bodies not yet
    /// folded). `0` on the final frame.
    pub remaining: usize,
    /// A short human-readable line ("union 3 of ≤ 7").
    pub label: String,
}

type Sink = Box<dyn Fn(&ProgressEvent)>;

thread_local! {
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
}

/// Install the sink for this thread (replacing any previous one).
pub fn install(sink: Sink) {
    SINK.with(|s| *s.borrow_mut() = Some(sink));
}

/// Remove this thread's sink.
pub fn clear() {
    SINK.with(|s| *s.borrow_mut() = None);
}

/// Deliver an event to this thread's sink, if any.
pub fn report(event: &ProgressEvent) {
    SINK.with(|s| {
        if let Some(sink) = s.borrow().as_ref() {
            sink(event);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn report_reaches_the_installed_sink_and_is_silent_without_one() {
        let ev = ProgressEvent {
            feature_id: Uuid::nil(),
            feature_name: "u".into(),
            done: 1,
            remaining: 2,
            label: "x".into(),
        };
        report(&ev); // no sink: no-op
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s2 = Rc::clone(&seen);
        install(Box::new(move |e| s2.borrow_mut().push(e.clone())));
        report(&ev);
        clear();
        report(&ev);
        assert_eq!(seen.borrow().as_slice(), &[ev]);
    }
}
