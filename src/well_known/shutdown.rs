use crate::get_value;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::{any::Provider, sync::atomic::AtomicBool};
use pin_project_lite::pin_project;
use std::sync::Arc;
use tokio::sync::futures::Notified;
use tokio::sync::Notify;

#[derive(Debug, Default)]
struct ShutdownInner {
    notifier: Notify,
    shutdown: AtomicBool,
}

pub struct ShutdownProvider(Option<Arc<ShutdownInner>>);

impl From<ShutdownReceiver> for ShutdownProvider {
    fn from(value: ShutdownReceiver) -> Self {
        Self(value.0)
    }
}

impl Provider for ShutdownProvider {
    fn provide<'a>(&'a self, demand: &mut core::any::Demand<'a>) {
        if let Some(inner) = self.0.as_ref() {
            demand.provide_ref(inner);
        }
    }
}

#[derive(Clone, Default)]
pub struct ShutdownSender(Arc<ShutdownInner>);

#[derive(Clone)]
pub struct ShutdownReceiver(Option<Arc<ShutdownInner>>);

impl ShutdownSender {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn receiver(self) -> ShutdownReceiver {
        ShutdownReceiver(Some(self.0))
    }

    /// Sends the shutdown signal to all the current and future waiters
    pub fn shutdown(&self) {
        self.0
            .shutdown
            .store(true, std::sync::atomic::Ordering::Release);
        self.0.notifier.notify_waiters();
    }
}

impl ShutdownReceiver {
    pub async fn from_context() -> Self {
        Self(get_value().await)
    }

    /// Waits for the shutdown signal
    pub async fn wait_for_signal(&self) {
        if let Some(x) = &self.0 {
            ShutdownSignal {
                shutdown: &x.shutdown,
                notified: x.notifier.notified(),
            }
            .await
        }
    }
}

pin_project!(
    struct ShutdownSignal<'a> {
        shutdown: &'a AtomicBool,
        #[pin]
        notified: Notified<'a>,
    }
);

impl Future for ShutdownSignal<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if this.shutdown.load(core::sync::atomic::Ordering::Acquire) {
            Poll::Ready(())
        } else {
            this.notified.poll(cx)
        }
    }
}

#[cfg(feature = "time")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub(crate) mod time {
    use super::{ShutdownReceiver, ShutdownSignal};
    use core::future::Future;
    use core::pin::Pin;
    use core::task::{Context, Poll};
    use pin_project_lite::pin_project;

    #[derive(Debug)]
    pub enum SignalOrComplete<F: Future> {
        ShutdownSignal(F),
        Completed(F::Output),
    }

    impl<F: Future> SignalOrComplete<F> {
        pub fn completed(self) -> Option<F::Output> {
            match self {
                SignalOrComplete::ShutdownSignal(_) => None,
                SignalOrComplete::Completed(f) => Some(f),
            }
        }
    }

    pin_project!(
        struct SignalOrCompleteFut<F, A, B> {
            inner: Option<F>,
            #[pin]
            a: Option<A>,
            #[pin]
            b: Option<B>,
        }
    );

    impl<F, A, B> Future for SignalOrCompleteFut<F, A, B>
    where
        F: Future + Unpin,
        A: Future<Output = ()>,
        B: Future<Output = ()>,
    {
        type Output = SignalOrComplete<F>;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.project();
            let mut f = this.inner.take().expect("cannot poll Select twice");

            if let Poll::Ready(f) = Pin::new(&mut f).poll(cx) {
                return Poll::Ready(SignalOrComplete::Completed(f));
            }
            if let Some(a) = this.a.as_pin_mut() {
                if a.poll(cx).is_ready() {
                    return Poll::Ready(SignalOrComplete::ShutdownSignal(f));
                }
            }
            if let Some(b) = this.b.as_pin_mut() {
                if b.poll(cx).is_ready() {
                    return Poll::Ready(SignalOrComplete::ShutdownSignal(f));
                }
            }

            *this.inner = Some(f);
            Poll::Pending
        }
    }

    /// Runs the provided future until either [`shutdown`](super::ShutdownSender::shutdown)
    /// is called on the [registered shutdown handler](crate::well_known::WellKnownProviderExt::with_shutdown_handler),
    /// or until the [`deadline`](crate::well_known::WellKnownProviderExt::with_deadline) expires.
    /// The unfinished future is returned in case it is not cancel safe and you need to complete it
    ///
    /// ```
    /// use std::time::Duration;
    /// use context_rs::well_known::{
    ///     WellKnownProviderExt, ShutdownSender, SignalOrComplete, run_until_signal
    /// };
    ///
    /// # #[tokio::main] async fn main() {
    /// async fn important_work() -> Option<i32> {
    ///     let mut sum = 0;
    ///     for i in 0..6 {
    ///         // internally, we respect any shutdown signals
    ///         // and the deadline by using `run_until_stopped`
    ///         sum += run_until_signal(std::pin::pin!(expensive_work())).await.completed()?;
    ///     }
    ///     Some(sum)
    /// }
    ///
    /// async fn expensive_work() -> i32 {
    ///     tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    ///     7
    /// }
    ///
    /// let shutdown = ShutdownSender::new();
    ///
    /// // spawn off some important work
    /// let work = important_work()
    ///     .with_shutdown_handler(shutdown.clone().receiver())
    ///     .with_timeout(Duration::from_secs(5));
    ///
    /// // should return None if it takes longer than 5 seconds,
    /// // or if `shutdown.shutdown()` is called
    /// let res = work.await;
    /// # assert_eq!(res, None);
    /// # }
    /// ```
    pub async fn run_until_signal<F: Future + Unpin>(f: F) -> SignalOrComplete<F> {
        use crate::well_known::Deadline;

        let deadline = Deadline::get().await;
        let shutdown = ShutdownReceiver::from_context().await.0;
        let res = SignalOrCompleteFut {
            inner: Some(f),
            a: deadline.map(|deadline| tokio::time::sleep_until(deadline.into())),
            b: shutdown.as_deref().map(|shutdown| ShutdownSignal {
                shutdown: &shutdown.shutdown,
                notified: shutdown.notifier.notified(),
            }),
        }
        .await;

        // temporaries are a pain sometimes...
        #[allow(clippy::let_and_return)]
        res
    }
}
