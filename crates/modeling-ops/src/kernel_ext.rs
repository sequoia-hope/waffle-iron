use waffle_types::kernel::{Kernel, KernelIntrospect, KernelMeasure};

/// Combined trait for operations that need both mutable Kernel access
/// and read-only KernelIntrospect access on the same object.
///
/// This avoids the borrow-checker issue of needing &mut and & on the same value.
///
/// [`KernelMeasure`] (Q1 of `specs/agent_mechanical_design.md` §4.1) joins the
/// bundle so a consumer holding a `&mut dyn KernelBundle` — every bridge tool
/// does — can ask a geometric question without a second handle to the same
/// kernel. Its methods default to `NotSupported`, so a kernel that cannot
/// measure still satisfies the bundle.
pub trait KernelBundle: Kernel + KernelIntrospect + KernelMeasure {
    fn as_introspect(&self) -> &dyn KernelIntrospect;

    /// The same kernel as a measurer (`&`-access, like [`Self::as_introspect`]).
    fn as_measure(&self) -> &dyn KernelMeasure;
}

// Blanket implementation for any type that implements all three traits
impl<T: Kernel + KernelIntrospect + KernelMeasure> KernelBundle for T {
    fn as_introspect(&self) -> &dyn KernelIntrospect {
        self
    }

    fn as_measure(&self) -> &dyn KernelMeasure {
        self
    }
}
