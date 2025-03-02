use std::error::Error;
use std::future::Future;
use std::time::Duration;

use backoff::ExponentialBackoffBuilder;

const MAX_ELAPSED_TIME_IN_SECS: u64 = 60;

/// Retry with exponential backoff
pub async fn retry_exp<T, E, Fut, F>(f: F) -> Result<T, E>
where
    E: Error,
    Fut: Future<Output = Result<T, backoff::Error<E>>>,
    F: Fn() -> Fut,
{
    let strategy = ExponentialBackoffBuilder::default()
        .with_max_elapsed_time(Some(Duration::from_secs(MAX_ELAPSED_TIME_IN_SECS)))
        .build();
    backoff::future::retry(strategy, || async {
        f().await.map_err(|e| {
            log::warn!("retrying after error: {}", e);
            e
        })
    })
    .await
}
