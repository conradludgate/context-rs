use core::{
    any::Provider,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project_lite::pin_project;

use crate::waker::ProviderWaker;

/// Extension trait that aims to simplify the providing of values
/// in the async context.
pub trait ProviderFutExt: Future + Sized {
    /// Wraps a [`Future`] so it can provide values into the async context.
    fn provide<P: Provider>(self, provider: P) -> ProviderFut<Self, P> {
        ProviderFut {
            inner: self,
            provider,
        }
    }

    /// Wraps a [`Future`] so it can provide this ref into the async context.
    ///
    /// Can be cloned from the context using [`take_value`](crate::get_value)
    fn provide_ref<T: 'static + ?Sized>(self, value: &T) -> ProviderFut<Self, ProvideRef<'_, T>> {
        self.provide(ProvideRef(value))
    }
}

impl<F: Future> ProviderFutExt for F {}

/// [`Provider`] used by [`provide_ref`](ProviderFutExt::provide_ref).
pub struct ProvideRef<'a, T: ?Sized>(&'a T);

impl<T: 'static + ?Sized> Provider for ProvideRef<'_, T> {
    fn provide<'a>(&'a self, demand: &mut core::any::Demand<'a>) {
        if demand.would_be_satisfied_by_ref_of::<T>() {
            demand.provide_ref(self.0);
        }
    }
}

pin_project!(
    /// [`Future`] returned by methods on [`ProviderFutExt`].
    pub struct ProviderFut<F, T> {
        #[pin]
        inner: F,
        provider: T,
    }
);

impl<F: Future, P: Provider> Future for ProviderFut<F, P> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        ProviderWaker::new(cx.waker(), this.provider).use_waker_with(|waker| {
            let mut cx = Context::from_waker(waker);
            this.inner.poll(&mut cx)
        })
    }
}
