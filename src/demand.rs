pub use core::future::Future;
use core::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use crate::waker::ProviderWaker;

fn request_ref_from_context<'c, T: 'static + ?Sized>(cx: &Context<'c>) -> Option<&'c T> {
    ProviderWaker::from_waker_ref(cx.waker()).and_then(|cx| core::any::request_ref(cx))
}

fn request_value_from_context<T: 'static>(cx: &Context<'_>) -> Option<T> {
    ProviderWaker::from_waker_ref(cx.waker()).and_then(|cx| core::any::request_value(cx))
}

/// Get a value from the current context. Only works when paired with
/// [`provide_ref`](crate::ProviderFutExt::provide_ref)
pub fn get_value<T: 'static + Clone>() -> impl Future<Output = Option<T>> {
    GetValueFut(core::marker::PhantomData::<T>)
}

/// [`Future`] returned by [`get_value`]
struct GetValueFut<T>(core::marker::PhantomData<T>);
impl<T> Unpin for GetValueFut<T> {}

impl<T: Clone + 'static> Future for GetValueFut<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(request_ref_from_context(cx).cloned())
    }
}

/// Take a value from the current context. Only works when paired with
/// [`provide_value`](crate::ProviderFutExt::provide_value)
pub fn take_value<T: 'static>() -> impl Future<Output = Option<T>> {
    TakeValueFut(core::marker::PhantomData::<T>)
}

/// [`Future`] returned by [`take_value`]
struct TakeValueFut<T>(core::marker::PhantomData<T>);
impl<T> Unpin for TakeValueFut<T> {}

impl<T: 'static> Future for TakeValueFut<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(request_value_from_context(cx))
    }
}

/// Get a value from the current context. Only works when paired with
/// [`provide_ref`](crate::ProviderFutExt::provide_ref)
pub fn with_ref<T: 'static + ?Sized, F: for<'c> FnOnce(&'c T) -> R, R>(
    f: F,
) -> impl Future<Output = Option<R>> {
    WithRefFut(Some(f), core::marker::PhantomData::<(&'static T, R)>)
}

/// [`Future`] returned by [`with_ref`]
struct WithRefFut<T: 'static + ?Sized, F: for<'c> FnOnce(&'c T) -> R, R>(
    Option<F>,
    PhantomData<(&'static T, R)>,
);

impl<T: 'static + ?Sized, F: for<'c> FnOnce(&'c T) -> R, R> Unpin for WithRefFut<T, F, R> {}

impl<T: 'static + ?Sized, F: for<'c> FnOnce(&'c T) -> R, R> Future for WithRefFut<T, F, R> {
    type Output = Option<R>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let f = self
            .0
            .take()
            .expect("futures should not be polled after completion");
        Poll::Ready(request_ref_from_context(cx).map(f))
    }
}
