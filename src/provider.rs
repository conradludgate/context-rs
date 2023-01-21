use core::{
    any::Provider,
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project_lite::pin_project;

use crate::waker::ProviderWaker;

pub trait ProviderFutExt: Sized {
    /// Wraps a [`Future`] so it can provide values into the async context.
    fn provide<P: Provider>(self, provider: P) -> ProviderFut<Self, P>;

    /// Wraps a [`Future`] so it can provide this value into the async context.
    ///
    /// Can be extracted from the context using [`take_value`](crate::take_value)
    fn provide_value<T: 'static>(self, value: T) -> ProviderFut<Self, ProvideValue<T>> {
        self.provide(ProvideValue(Cell::new(Some(value))))
    }

    /// Wraps a [`Future`] so it can provide this ref into the async context.
    ///
    /// Can be cloned from the context using [`take_value`](crate::get_value)
    fn provide_ref<T: 'static + ?Sized>(self, value: &T) -> ProviderFut<Self, ProvideRef<'_, T>> {
        self.provide(ProvideRef(value))
    }
}

impl<F: Future> ProviderFutExt for F {
    fn provide<P>(self, provider: P) -> ProviderFut<Self, P> {
        ProviderFut {
            inner: self,
            provider,
        }
    }
}

pin_project!(
    pub struct ProviderFut<F, T> {
        #[pin]
        inner: F,
        provider: T,
    }
);

pub struct ProvideRef<'a, T: ?Sized>(&'a T);
pub struct ProvideValue<T>(Cell<Option<T>>);

impl<T: 'static + ?Sized> Provider for ProvideRef<'_, T> {
    fn provide<'a>(&'a self, demand: &mut core::any::Demand<'a>) {
        if demand.would_be_satisfied_by_ref_of::<T>() {
            demand.provide_ref(self.0);
        }
    }
}

impl<T: 'static> Provider for ProvideValue<T> {
    fn provide<'a>(&'a self, demand: &mut core::any::Demand<'a>) {
        if demand.would_be_satisfied_by_value_of::<T>() {
            if let Some(x) = self.0.take() {
                demand.provide_value(x);
            }
        }
    }
}

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
