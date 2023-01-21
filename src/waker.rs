use core::{
    any::Provider,
    task::{RawWaker, RawWakerVTable, Waker},
};

/// This is a `Waker` compatible type that can [`provide`](Provider)
/// values down the async call stack implicitly.
#[derive(Copy, Clone)]
pub struct ProviderWaker<'a, 'b> {
    waker: &'a Waker,
    provider: &'b dyn Provider,
}

impl<'a, 'b> ProviderWaker<'a, 'b> {
    /// Create a new [`ProviderWaker`] made up of the [`Waker`] and [`Provider`] pair.
    pub fn new(waker: &'a Waker, provider: &'b dyn Provider) -> Self {
        Self { waker, provider }
    }
}

static VTABLE: RawWakerVTable = {
    unsafe fn clone(waker: *const ()) -> RawWaker {
        let inner = (*waker.cast::<ProviderWaker>()).waker.clone();
        // SAFETY: technically not because Waker isn't guaranteed to be transparent
        // but this is the best I can do for now.
        core::mem::transmute(inner)
    }
    unsafe fn wake(waker: *const ()) {
        (*waker.cast::<ProviderWaker>()).waker.wake_by_ref()
    }
    unsafe fn wake_by_ref(waker: *const ()) {
        (*waker.cast::<ProviderWaker>()).waker.wake_by_ref()
    }
    unsafe fn drop(_: *const ()) { /* ProviderWaker needs no drop */
    }

    RawWakerVTable::new(clone, wake, wake_by_ref, drop)
};

impl<'a, 'b> ProviderWaker<'a, 'b> {
    /// Turns `&self` into a [`Waker`] and pass it into the provided closure
    ///
    /// Calling `wake()` on this waker does `wake_by_ref` only.
    /// Calling `clone()` on this waker only clones the inner waker, not the provider.
    pub fn use_waker_with<R>(&self, f: impl for<'w> FnOnce(&'w Waker) -> R) -> R {
        // SAFETY: Our ptr+vtable pair satisfies the safety requirements of
        // RawWaker. It does not do allow any interior mutation that would need
        // synchronisation, and we offer no ownership that can be sent between
        // threads.
        let ptr = (self as *const Self).cast();
        let waker = unsafe { &Waker::from_raw(RawWaker::new(ptr, &VTABLE)) };
        f(waker)
    }

    /// Try downcast the [`Waker`] into a [`ProviderWaker`]
    pub fn from_waker_ref(waker: &Waker) -> Option<&Self> {
        if waker.as_raw().vtable() == &VTABLE {
            // SAFETY: For the given waker to be valid under the safety requirements,
            // the data pointer must be able to be an argument of the functions in
            // the vtable. Since the vtable points to our vtable, we are safe
            // to assume this is one of our pointers.
            Some(unsafe { &*waker.as_raw().data().cast::<Self>() })
        } else {
            None
        }
    }
}

impl Provider for ProviderWaker<'_, '_> {
    fn provide<'a>(&'a self, demand: &mut core::any::Demand<'a>) {
        self.provider.provide(demand);
        if let Some(cx) = Self::from_waker_ref(self.waker) {
            cx.provide(demand)
        }
    }
}
