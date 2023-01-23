use core::{any::Provider, future::Future};
use std::time::{Duration, Instant};

use crate::{with_ref, ProviderFut, ProviderFutExt};

pub use shutdown::{ShutdownProvider, ShutdownReceiver, ShutdownSender};

#[cfg(feature = "time")]
pub use shutdown::{run_until_signal, SignalOrComplete};

// mod linked_list;
// mod notify;
mod shutdown;

/// Extension trait to provide some well known context values
pub trait WellKnownProviderExt: Future + Sized {
    /// Wraps a [`Future`] so that it should expire within the given duration.
    ///
    /// Note, this doesn't guarantee that the future will stop executing, this is up
    /// to the implementation to respect the timeout.
    /// 
    /// ```
    /// use std::time::Duration;
    /// use context_rs::well_known::{
    ///     WellKnownProviderExt, ShutdownSender, run_until_signal, SignalOrComplete
    /// };
    ///
    /// async fn do_something() {
    ///     loop {
    ///         // pretend this is more interesting
    ///         let work = tokio::time::sleep(Duration::from_secs(1));
    ///         match run_until_signal(std::pin::pin!(work)).await {
    ///             SignalOrComplete::Completed(_) => continue,
    ///             SignalOrComplete::ShutdownSignal(_) => break,
    ///         }
    ///     }
    /// }
    ///
    /// # #[tokio::main] async fn main() {
    /// do_something().with_timeout(Duration::from_secs(5)).await;
    /// # }
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

impl Deadline {
    // Returns the deadline of the current context, if there is one
    pub async fn get() -> Option<Instant> {
        with_ref(|Deadline(deadline)| *deadline).await
    }
}
