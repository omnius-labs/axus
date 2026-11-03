//! Test-only helpers shared by the engine's integration tests.

use std::{future::Future, time::Duration};

use testresult::TestResult;
use tokio::time::Instant;

/// The safety net of a wait in an integration test. It exists to catch a hang, not to assert speed, so it is far
/// larger than any operation the tests perform.
pub(crate) const TEST_WATCHDOG: Duration = Duration::from_secs(120);

/// Waits for a condition under [`TEST_WATCHDOG`] so a test does not decide pass or fail by the speed of the machine.
pub(crate) struct Watchdog;

impl Watchdog {
    /// Waits for a condition and names what was awaited when the watchdog expires.
    pub(crate) async fn wait_for<F, Fut>(label: &str, condition: F) -> TestResult
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = bool>,
    {
        Self::wait_for_with_snapshot(label, condition, || async { String::from("no further state") }).await
    }

    /// Waits for a condition and reports the state built by `snapshot` when the watchdog expires. The snapshot runs
    /// only on timeout, so a test pays nothing for it while it waits.
    pub(crate) async fn wait_for_with_snapshot<F, Fut, S, SFut>(label: &str, mut condition: F, snapshot: S) -> TestResult
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = bool>,
        S: Fn() -> SFut,
        SFut: Future<Output = String>,
    {
        let deadline = Instant::now() + TEST_WATCHDOG;
        loop {
            if condition().await {
                return Ok(());
            }
            if Instant::now() >= deadline {
                panic!("{label} did not hold within {TEST_WATCHDOG:?}: {}", snapshot().await);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
