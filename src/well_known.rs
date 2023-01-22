use core::{any::Provider, future::Future};
use std::time::{Duration, Instant};

use crate::{with_ref, ProviderFut, ProviderFutExt};

pub use shutdown::{ShutdownProvider, ShutdownReceiver, ShutdownSender};

#[cfg(feature = "time")]
pub use shutdown::{run_until_signal, SignalOrComplete};

mod linked_list;
mod notify;
mod shutdown;

/// Extension trait to provide some well known context values
pub trait WellKnownProviderExt: Future + Sized {
    /// Wraps a [`Future`] so that it should expire within the given duration.
    ///
    /// Note, this doesn't guarantee that the future will stop executing, this is up
    /// to the implementation to respect the timeout.
    ///
    /// ```
    ///
    /// ```
    fn with_timeout(self, duration: Duration) -> ProviderFut<Self, Deadline> {
        self.with_deadline(Instant::now() + duration)
    }

    /// Wraps a [`Future`] o that it should expire at the given deadline.
    ///
    /// See [`with_timeout`](WellKnownProviderExt::with_timeout) for more
    fn with_deadline(self, deadline: Instant) -> ProviderFut<Self, Deadline> {
        self.provide(Deadline(deadline))
    }

    /// Wraps a [`Future`] so it can be shutdown externally.
    ///
    /// ```
    /// use context_rs::well_known::{
    ///     WellKnownProviderExt, ShutdownSender, run_until_signal, SignalOrComplete
    /// };
    ///
    /// async fn generator(tx: tokio::sync::mpsc::Sender<i32>) {
    ///     for i in 0.. {
    ///         match run_until_signal(std::pin::pin!(tx.send(i))).await {
    ///             SignalOrComplete::Completed(_) => continue,
    ///             SignalOrComplete::ShutdownSignal(_) => break,
    ///         }
    ///     }
    /// }
    ///
    /// # #[tokio::main] async fn main() {
    /// let shutdown = ShutdownSender::new();
    /// let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    ///
    /// tokio::spawn(generator(tx).with_shutdown_handler(shutdown.clone().receiver()));
    ///
    /// while let Some(x) = rx.recv().await {
    ///     dbg!(x);
    ///     if x == 5 {
    ///         break
    ///     }
    /// }
    ///
    /// // shutdown now that we're done with the rx
    /// shutdown.shutdown()
    /// # }
    /// ```
    fn with_shutdown_handler(
        self,
        handler: ShutdownReceiver,
    ) -> ProviderFut<Self, ShutdownProvider> {
        self.provide(ShutdownProvider::from(handler))
    }
}
impl<F: Future> WellKnownProviderExt for F {}

#[derive(Debug, Clone, Copy)]
pub struct Deadline(pub Instant);

impl Provider for Deadline {
    fn provide<'a>(&'a self, demand: &mut core::any::Demand<'a>) {
        demand.provide_ref(self);
    }
}

#[derive(Debug, PartialEq)]
pub struct Expired;

impl std::error::Error for Expired {}
impl core::fmt::Display for Expired {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("context has reached it's deadline")
    }
}

impl Deadline {
    // Returns the deadline of the current context, if there is one
    pub async fn get() -> Option<Instant> {
        with_ref(|Deadline(deadline)| *deadline).await
    }

    // check if the deadline stored in the context has expired
    // returns OK if no deadline is stored.
    pub async fn expired() -> Result<(), Expired> {
        let not_expired = with_ref(|Deadline(deadline)| deadline > &Instant::now())
            .await
            .unwrap_or(true);
        not_expired.then_some(()).ok_or(Expired)
    }
}
