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

/// Get a value from the current context. Only works when paired with
/// [`provide_ref`](crate::ProviderFutExt::provide_ref)
pub async fn get_value<T: 'static + Clone>() -> Option<T> {
    with_ref(T::clone).await
}

/// Get a value from the current context. Only works when paired with
/// [`provide_ref`](crate::ProviderFutExt::provide_ref)
pub async fn with_ref<T: 'static + ?Sized, F: for<'c> FnOnce(&'c T) -> R, R>(f: F) -> Option<R> {
    WithRefFut(Some(f), core::marker::PhantomData::<(&'static T, R)>).await
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
