#![feature(waker_getters, provide_any)]
#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod demand;
mod provider;
mod waker;

pub use demand::{get_value, with_ref};
pub use provider::{ProvideRef, ProviderFut, ProviderFutExt};
pub use waker::ProviderWaker;

#[cfg(feature = "std")]
pub mod well_known;

#[cfg(test)]
mod tests {
    use core::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
    };
    use futures_util::FutureExt;

    use super::*;

    #[test]
    fn ctx_with_ref() {
        let val = async {
            // works once
            let upper = with_ref(|s: &str| s.to_uppercase()).await.unwrap();
            // works again
            let len = with_ref(|s: &str| s.len()).await.unwrap();
            format!("{upper}{len}")
        }
        .provide_ref("foo")
        .now_or_never()
        .unwrap();

        assert_eq!(val, "FOO3");
    }

    #[test]
    fn ctx_get_value() {
        let val = async {
            // works once
            let num1 = get_value::<i32>().await.unwrap();
            // works again
            let num2 = get_value::<i32>().await.unwrap();
            num1 + num2
        }
        .provide_ref(&123)
        .now_or_never()
        .unwrap();

        assert_eq!(val, 246);
    }

    #[tokio::test]
    async fn waker_works() {
        async {
            yield_now().await;
            yield_clone_now().await;
        }
        .provide_ref(&())
        .await;
    }

    pub async fn yield_now() {
        /// Yield implementation
        struct YieldNow {
            yielded: bool,
        }

        impl Future for YieldNow {
            type Output = ();

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
                if self.yielded {
                    return Poll::Ready(());
                }
                self.yielded = true;
                cx.waker().wake_by_ref();

                Poll::Pending
            }
        }

        YieldNow { yielded: false }.await
    }

    pub async fn yield_clone_now() {
        /// Yield implementation
        struct YieldNow {
            yielded: bool,
        }

        impl Future for YieldNow {
            type Output = ();

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
                if self.yielded {
                    return Poll::Ready(());
                }
                self.yielded = true;
                cx.waker().clone().wake();

                Poll::Pending
            }
        }

        YieldNow { yielded: false }.await
    }
}
