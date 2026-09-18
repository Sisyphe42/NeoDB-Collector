//! Runtime adapter only; not yet composed into preview jobs or HTTP routes.
use collector_core::{access::PortFuture, catalog_resolver::ResolutionControl};
use collector_domain::ApiError;
use std::time::Duration;
use tokio::time::{Instant, sleep_until, timeout_at};
use tokio_util::sync::CancellationToken;

pub struct TokioResolutionControl {
    epoch: Instant,
    cancelled: CancellationToken,
}

fn cancelled() -> ApiError {
    ApiError::new(
        "catalog_resolution_cancelled",
        "Catalog resolution was cancelled.",
    )
}
fn expired() -> ApiError {
    ApiError::new(
        "catalog_resolution_timeout",
        "Catalog resolution exceeded its time budget.",
    )
}

impl TokioResolutionControl {
    /// The job owner retains a token clone and its standard `drop_guard()`.
    pub fn new(cancelled: CancellationToken) -> Self {
        Self {
            epoch: Instant::now(),
            cancelled,
        }
    }

    fn check(&self, deadline: Duration) -> Result<(), ApiError> {
        if self.cancelled.is_cancelled() {
            Err(cancelled())
        } else if self.now() >= deadline {
            Err(expired())
        } else {
            Ok(())
        }
    }
}

impl ResolutionControl for TokioResolutionControl {
    fn now(&self) -> Duration {
        self.epoch.elapsed()
    }

    fn wait_until(&self, at: Duration) -> PortFuture<'_, ()> {
        Box::pin(async move {
            let instant = self.epoch.checked_add(at).ok_or_else(expired)?;
            tokio::select! {
                biased;
                _ = self.cancelled.cancelled() => Err(cancelled()),
                _ = sleep_until(instant) => Ok(()),
            }
        })
    }

    fn run<'a, T: Send + 'a>(
        &'a self,
        deadline: Duration,
        work: PortFuture<'a, T>,
    ) -> PortFuture<'a, T> {
        Box::pin(async move {
            // timeout_at accepts immediately-ready work even after its deadline.
            // Our policy is stricter, so retain explicit pre/post checks.
            self.check(deadline)?;
            let instant = self.epoch.checked_add(deadline).ok_or_else(expired)?;
            let result = tokio::select! {
                biased;
                _ = self.cancelled.cancelled() => Err(cancelled()),
                result = timeout_at(instant, work) => result.map_err(|_| expired())?,
            };
            self.check(deadline)?;
            result
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::pending,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    fn setup() -> (
        TokioResolutionControl,
        CancellationToken,
        tokio_util::sync::DropGuard,
    ) {
        let token = CancellationToken::new();
        let guard = token.clone().drop_guard();
        (TokioResolutionControl::new(token.clone()), token, guard)
    }

    #[tokio::test(start_paused = true)]
    async fn timer_cancellation_and_dropped_guard_are_fail_closed() {
        let (control, token, _guard) = setup();
        control.wait_until(Duration::from_secs(15)).await.unwrap();
        assert_eq!(control.now(), Duration::from_secs(15));
        token.cancel();
        assert_eq!(
            control.wait_until(Duration::ZERO).await.unwrap_err().code,
            "catalog_resolution_cancelled"
        );
        let (control, _token, guard) = setup();
        drop(guard);
        assert!(control.wait_until(Duration::from_secs(120)).await.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_wakes_registered_waiter() {
        let (control, token, _guard) = setup();
        let waiter =
            tokio::spawn(async move { control.wait_until(Duration::from_secs(120)).await });
        tokio::task::yield_now().await;
        token.cancel();
        assert_eq!(
            waiter.await.unwrap().unwrap_err().code,
            "catalog_resolution_cancelled"
        );
    }

    struct OnDrop(Arc<AtomicBool>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_and_cancellation_drop_inflight_work() {
        for cancel in [false, true] {
            let (control, token, _guard) = setup();
            let dropped = Arc::new(AtomicBool::new(false));
            let marker = OnDrop(dropped.clone());
            let task = tokio::spawn(async move {
                control
                    .run(
                        Duration::from_secs(120),
                        Box::pin(async move {
                            let _marker = marker;
                            pending::<Result<(), ApiError>>().await
                        }),
                    )
                    .await
            });
            tokio::task::yield_now().await;
            if cancel {
                token.cancel();
            } else {
                tokio::time::advance(Duration::from_secs(120)).await;
            }
            let code = task.await.unwrap().unwrap_err().code;
            assert_eq!(
                code,
                if cancel {
                    "catalog_resolution_cancelled"
                } else {
                    "catalog_resolution_timeout"
                }
            );
            assert!(dropped.load(Ordering::SeqCst));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_and_deadline_win_over_ready_work() {
        let (control, token, _guard) = setup();
        let ran = AtomicBool::new(false);
        let work = || {
            Box::pin(async {
                ran.store(true, Ordering::SeqCst);
                Ok(())
            }) as PortFuture<'_, ()>
        };
        assert_eq!(
            control.run(Duration::ZERO, work()).await.unwrap_err().code,
            "catalog_resolution_timeout"
        );
        assert!(!ran.load(Ordering::SeqCst));
        token.cancel();
        assert_eq!(
            control
                .run(Duration::from_secs(120), work())
                .await
                .unwrap_err()
                .code,
            "catalog_resolution_cancelled"
        );
        assert!(!ran.load(Ordering::SeqCst));
        let (control, token, _guard) = setup();
        let result = control
            .run(
                Duration::from_secs(120),
                Box::pin(async {
                    token.cancel();
                    Ok(())
                }),
            )
            .await;
        assert_eq!(result.unwrap_err().code, "catalog_resolution_cancelled");
        let (control, _token, _guard) = setup();
        assert_eq!(
            control
                .run(Duration::from_secs(120), Box::pin(async { Ok(42) }))
                .await
                .unwrap(),
            42
        );
    }
}
