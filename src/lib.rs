#![feature(waker_getters, provide_any)]
#![cfg_attr(not(test), no_std)]

use core::{
    any::Provider,
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use pin_project_lite::pin_project;

struct ContextWaker<'a, 'b> {
    ctx: &'a Waker,
    value: &'b dyn Provider,
}

static VTABLE: RawWakerVTable = {
    unsafe fn clone(waker: *const ()) -> RawWaker {
        let inner = (*waker.cast::<ContextWaker>()).ctx.clone();
        // SAFETY: technically not because Waker isn't guaranteed to be transparent
        // but this is the best I can do for now.
        core::mem::transmute(inner)
    }
    unsafe fn wake(waker: *const ()) {
        (*waker.cast::<ContextWaker>()).ctx.wake_by_ref()
    }
    unsafe fn wake_by_ref(waker: *const ()) {
        (*waker.cast::<ContextWaker>()).ctx.wake_by_ref()
    }
    unsafe fn drop(_: *const ()) {}

    RawWakerVTable::new(clone, wake, wake_by_ref, drop)
};

impl<'a, 'b> ContextWaker<'a, 'b> {
    fn as_waker<R>(&self, f: impl for<'w> FnOnce(&'w Waker) -> R) -> R {
        let waker =
            unsafe { Waker::from_raw(RawWaker::new((self as *const Self).cast(), &VTABLE)) };

        f(&waker)
    }

    fn from_waker(waker: &Waker) -> Option<&Self> {
        if waker.as_raw().vtable() == &VTABLE {
            // SAFETY: dunno yet, maybe
            Some(unsafe { &*waker.as_raw().data().cast::<Self>() })
        } else {
            None
        }
    }
}

impl Provider for ContextWaker<'_, '_> {
    fn provide<'a>(&'a self, demand: &mut core::any::Demand<'a>) {
        self.value.provide(demand);
        if let Some(cx) = Self::from_waker(self.ctx) {
            cx.provide(demand)
        }
    }
}

pub trait WithContextExt: Sized {
    fn with_value<T>(self, value: T) -> ContextWrapper<Self, ProvideOwned<T>>;
    fn with_ref<T: ?Sized>(self, value: &T) -> ContextWrapper<Self, ProvideRef<'_, T>>;
}

impl<F: Future> WithContextExt for F {
    fn with_value<T>(self, value: T) -> ContextWrapper<Self, ProvideOwned<T>> {
        ContextWrapper {
            inner: self,
            value: ProvideOwned(Cell::new(Some(value))),
        }
    }
    fn with_ref<T: ?Sized>(self, value: &T) -> ContextWrapper<Self, ProvideRef<'_, T>> {
        ContextWrapper {
            inner: self,
            value: ProvideRef(value),
        }
    }
}

pin_project!(
    pub struct ContextWrapper<F, T> {
        #[pin]
        inner: F,
        value: T,
    }
);

#[doc(hidden)]
pub struct ProvideRef<'a, T: ?Sized>(&'a T);
#[doc(hidden)]
pub struct ProvideOwned<T>(Cell<Option<T>>);

impl<T: 'static + ?Sized> Provider for ProvideRef<'_, T> {
    fn provide<'a>(&'a self, demand: &mut core::any::Demand<'a>) {
        if demand.would_be_satisfied_by_ref_of::<T>() {
            demand.provide_ref(self.0);
        }
    }
}
impl<T: 'static> Provider for ProvideOwned<T> {
    fn provide<'a>(&'a self, demand: &mut core::any::Demand<'a>) {
        if demand.would_be_satisfied_by_value_of::<T>() {
            if let Some(x) = self.0.take() {
                demand.provide_value(x);
            }
        }
    }
}

impl<F: Future, P: Provider> Future for ContextWrapper<F, P> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        ContextWaker {
            ctx: cx.waker(),
            value: this.value,
        }
        .as_waker(|waker| {
            let mut cx = Context::from_waker(waker);
            this.inner.poll(&mut cx)
        })
    }
}

/// Clone out a value from the current context
pub fn get_value<T: 'static + Clone>() -> impl Future<Output = Option<T>> {
    GetValueFut(core::marker::PhantomData::<T>)
}

struct GetValueFut<T>(core::marker::PhantomData<T>);

impl<T: Clone + 'static> Future for GetValueFut<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(
            ContextWaker::from_waker(cx.waker())
                .and_then(|cx| core::any::request_ref(cx))
                .cloned(),
        )
    }
}

/// Take a value from the current context
pub fn take_value<T: 'static>() -> impl Future<Output = Option<T>> {
    TakeValueFut(core::marker::PhantomData::<T>)
}

struct TakeValueFut<T>(core::marker::PhantomData<T>);

impl<T: 'static> Future for TakeValueFut<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(
            ContextWaker::from_waker(cx.waker()).and_then(|cx| core::any::request_value(cx)),
        )
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use super::*;

    #[test]
    fn it_works() {
        let test = async {
            (
                get_value::<usize>().await.unwrap(),
                get_value::<usize>().await.unwrap(),
                take_value::<String>().await.unwrap(),
                take_value::<String>().await.unwrap(),
                take_value::<Vec<u8>>().await.unwrap(),
                take_value::<Vec<u8>>().await,
            )
        };

        let (v0, v1, v2, v3, v4, v5) = test
            .with_value(vec![1_u8, 2, 3, 4])
            .with_value("hello world".to_owned())
            .with_value("goodbye world".to_owned())
            .with_ref(&123_usize)
            .now_or_never()
            .unwrap();

        // get_value should not remove from context and should be
        // callable again
        assert_eq!(v0, 123);
        assert_eq!(v1, 123);

        // take_value should also work, returning the chain of owned values
        assert_eq!(v2, "hello world");
        assert_eq!(v3, "goodbye world");

        // take_value should return the None if there's no more values in the chain
        assert_eq!(v4, [1, 2, 3, 4]);
        assert_eq!(v5, None);
    }
}
