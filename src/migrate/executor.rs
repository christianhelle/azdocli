//! Rate-limit-aware request helpers.
//!
//! Wraps async ADO operations with exponential backoff retry on transient
//! errors (HTTP 429/503/network timeouts). Concurrency is bounded by a
//! semaphore.
//!
//! For SDK calls, the `retry` helper handles fallible closures returning
//! `anyhow::Result<T>`. Heuristics inspect the error string for the
//! signature of throttling responses, since the SDK does not expose
//! structured rate-limit errors.

#![allow(dead_code)]

use anyhow::Result;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

#[derive(Debug, Clone)]
pub struct Executor {
    semaphore: Arc<Semaphore>,
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Executor {
    pub fn new(concurrency: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
            max_retries: 5,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
        }
    }

    /// Acquire a concurrency permit; held for the duration of the
    /// returned guard.
    pub async fn permit(&self) -> tokio::sync::OwnedSemaphorePermit {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore not closed")
    }

    /// Retry an async fallible operation with exponential backoff.
    pub async fn retry<F, Fut, T>(&self, mut op: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let mut attempt = 0u32;
        loop {
            match op().await {
                Ok(v) => return Ok(v),
                Err(e) => {
                    attempt += 1;
                    if attempt > self.max_retries || !is_transient(&e) {
                        return Err(e);
                    }
                    let delay = backoff(self.base_delay, self.max_delay, attempt);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
}

fn backoff(base: Duration, max: Duration, attempt: u32) -> Duration {
    let exp = base.as_millis().saturating_mul(1u128 << attempt.min(10));
    let capped = exp.min(max.as_millis());
    Duration::from_millis(capped as u64)
}

/// Best-effort transient-error detection. The Azure DevOps SDK surfaces
/// errors as `anyhow::Error`/`azure_core::Error`; we inspect the string
/// for 429/503/timeout/connection markers.
fn is_transient(e: &anyhow::Error) -> bool {
    let s = format!("{:#}", e).to_lowercase();
    s.contains("429")
        || s.contains("503")
        || s.contains("502")
        || s.contains("504")
        || s.contains("timed out")
        || s.contains("timeout")
        || s.contains("temporarily unavailable")
        || s.contains("connection reset")
        || s.contains("connection closed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn is_transient_accepts_throttling_and_server_errors() {
        for message in [
            "HTTP status 429 Too Many Requests",
            "request failed with status 503 Service Unavailable",
            "502 Bad Gateway",
            "504 Gateway Timeout",
        ] {
            assert!(
                is_transient(&anyhow!(message)),
                "{message} should be transient"
            );
        }
    }

    #[test]
    fn is_transient_accepts_network_timeout_markers() {
        for message in [
            "operation timed out while sending request",
            "network timeout",
            "connection reset by peer",
            "connection closed before message completed",
            "service temporarily unavailable",
        ] {
            assert!(
                is_transient(&anyhow!(message)),
                "{message} should be transient"
            );
        }
    }

    #[test]
    fn is_transient_rejects_client_and_auth_errors() {
        for message in [
            "400 Bad Request",
            "401 Unauthorized",
            "403 Forbidden",
            "404 Not Found",
        ] {
            assert!(
                !is_transient(&anyhow!(message)),
                "{message} should not be transient"
            );
        }
    }
}
