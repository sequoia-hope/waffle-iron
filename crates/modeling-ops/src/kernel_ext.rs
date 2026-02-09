use kernel_fork::{Kernel, KernelIntrospect};

/// Combined trait for operations that need both mutable Kernel access
/// and read-only KernelIntrospect access on the same object.
///
/// This avoids the borrow-checker issue of needing &mut and & on the same value.
pub trait KernelBundle: Kernel + KernelIntrospect {
    fn as_introspect(&self) -> &dyn KernelIntrospect;
}

// Blanket implementation for any type that implements both traits
impl<T: Kernel + KernelIntrospect> KernelBundle for T {
    fn as_introspect(&self) -> &dyn KernelIntrospect {
        self
    }
}
