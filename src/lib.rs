#![feature(waker_getters)]

use std::{
    cell::RefCell,
    future::Future,
    mem::ManuallyDrop,
    pin::Pin,
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use pin_project_lite::pin_project;
use typemap_rev::{TypeMap, TypeMapKey};

struct ContextWaker<'a, 'b> {
    ctx: &'a Waker,
    map: &'b RefCell<TypeMap>,
}

static VTABLE: RawWakerVTable = {
    unsafe fn clone(waker: *const ()) -> RawWaker {
        let inner = (*waker.cast::<ContextWaker>()).ctx.clone();
        // SAFETY: technically not because Waker isn't guaranteed to be transparent
        // but this is the best I can do for now.
        std::mem::transmute(inner)
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
        let waker = unsafe {
            ManuallyDrop::new(Waker::from_raw(RawWaker::new(
                (self as *const Self).cast(),
                &VTABLE,
            )))
        };

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

pub trait WithContextExt: Sized {
    fn with_value<T: TypeMapKey>(self, value: T::Value) -> ContextWrapper<Self, T>;
}

impl<F: Future> WithContextExt for F {
    fn with_value<T: TypeMapKey>(self, value: T::Value) -> ContextWrapper<Self, T> {
        ContextWrapper {
            inner: self,
            map: RefCell::new(TypeMap::new()),
            value: Some(value),
        }
    }
}

pin_project!(
    pub struct ContextWrapper<F, T: TypeMapKey> {
        #[pin]
        inner: F,
        map: RefCell<TypeMap>,
        value: Option<T::Value>,
    }
);

impl<F: Future, T: TypeMapKey> Future for ContextWrapper<F, T> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        let cx2 = ContextWaker {
            ctx: cx.waker(),
            map: this.map,
        };

        // use the existing context, if possible
        let cx2 = ContextWaker::from_waker(cx.waker()).unwrap_or(&cx2);

        // insert the value into the current context
        let mut guard = cx2.map.borrow_mut();
        let old = guard.remove::<T>();
        guard.insert::<T>(this.value.take().unwrap());
        drop(guard);

        // poll the future
        let res = cx2.as_waker(|waker| {
            let mut cx = Context::from_waker(waker);
            this.inner.poll(&mut cx)
        });

        // remove the current value and replace it with the old value
        let mut guard = cx2.map.borrow_mut();
        *this.value = guard.remove::<T>();
        if let Some(old) = old {
            guard.insert::<T>(old);
        }

        res
    }
}

/// Get a value from the current context
pub fn get_value<T: TypeMapKey>() -> impl Future<Output = Option<T::Value>>
where
    T::Value: Clone,
{
    GetValueFut(std::marker::PhantomData::<T>)
}

struct GetValueFut<T>(std::marker::PhantomData<T>);

impl<T: TypeMapKey> Future for GetValueFut<T>
where
    T::Value: Clone,
{
    type Output = Option<T::Value>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(
            ContextWaker::from_waker(cx.waker()).and_then(|cx| cx.map.borrow().get::<T>().cloned()),
        )
    }
}

/// Take a value from the current context
pub fn take_value<T: TypeMapKey>() -> impl Future<Output = Option<T::Value>> {
    TakeValueFut(std::marker::PhantomData::<T>)
}

struct TakeValueFut<T>(std::marker::PhantomData<T>);

impl<T: TypeMapKey> Future for TakeValueFut<T> {
    type Output = Option<T::Value>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(
            ContextWaker::from_waker(cx.waker()).and_then(|cx| cx.map.borrow_mut().remove::<T>()),
        )
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use super::*;

    struct Key1;
    struct Key2;
    struct Key3;

    impl TypeMapKey for Key1 {
        type Value = usize;
    }

    impl TypeMapKey for Key2 {
        type Value = String;
    }

    impl TypeMapKey for Key3 {
        type Value = Vec<u8>;
    }

    #[test]
    fn it_works() {
        let block = async {
            async {
                async {
                    (
                        get_value::<Key1>().await.unwrap(),
                        get_value::<Key2>().await.unwrap(),
                        get_value::<Key3>().await.unwrap(),
                    )
                }
                .with_value::<Key3>(vec![1, 2, 3, 4])
                .await
            }
            .with_value::<Key2>("hello world".to_owned())
            .await
        }
        .with_value::<Key1>(123);

        let (v1, v2, v3) = block.now_or_never().unwrap();

        assert_eq!(v1, 123);
        assert_eq!(v2, "hello world");
        assert_eq!(v3, [1, 2, 3, 4]);
    }
}
