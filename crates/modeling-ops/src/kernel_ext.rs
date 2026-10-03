use waffle_types::kernel::{Kernel, KernelIntrospect, KernelMeasure, KernelProjection};

/// Combined trait for operations that need both mutable Kernel access
/// and read-only KernelIntrospect access on the same object.
///
/// This avoids the borrow-checker issue of needing &mut and & on the same value.
///
/// `KernelProjection` (`specs/drawings_and_mbd.md` §5.1) joined the bundle
/// with D1a: a drawing view is produced from the same kernel a feature tree
/// was built on, and a consumer that holds a `&mut dyn KernelBundle` is the
/// only thing that has it. Every projection method has a typed
/// `NotSupported` default, so this is additive for any implementor.
///
/// [`KernelMeasure`] (Q1 of `specs/agent_mechanical_design.md` §4.1) joined it
/// for the same reason and on the same terms: a consumer holding a
/// `&mut dyn KernelBundle` — every bridge tool does — can ask a geometric
/// question without a second handle to the same kernel, and the methods
/// default to `NotSupported`, so a kernel that cannot measure still satisfies
/// the bundle.
pub trait KernelBundle: Kernel + KernelIntrospect + KernelProjection + KernelMeasure {
    fn as_introspect(&self) -> &dyn KernelIntrospect;

    /// The same kernel as a measurer (`&`-access, like [`Self::as_introspect`]).
    fn as_measure(&self) -> &dyn KernelMeasure;
}

// Blanket implementation for any type that implements all four
impl<T: Kernel + KernelIntrospect + KernelProjection + KernelMeasure> KernelBundle for T {
    fn as_introspect(&self) -> &dyn KernelIntrospect {
        self
    }

    fn as_measure(&self) -> &dyn KernelMeasure {
        self
    }
}
