use waffle_types::kernel::{Kernel, KernelIntrospect, KernelProjection};

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
pub trait KernelBundle: Kernel + KernelIntrospect + KernelProjection {
    fn as_introspect(&self) -> &dyn KernelIntrospect;
}

// Blanket implementation for any type that implements all three
impl<T: Kernel + KernelIntrospect + KernelProjection> KernelBundle for T {
    fn as_introspect(&self) -> &dyn KernelIntrospect {
        self
    }
}
