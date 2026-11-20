//! The OpenRouter model list: one read of `GET /api/v1/models` through the provider transport, kept for an hour.
//!
//! The request goes through the same policy as the chat (the built-in approval, HTTPS, every resolved address checked, the
//! connection pinned to those addresses, no proxy, no redirect, the time caps) and carries no key: the endpoint is
//! public. The reply has its own byte cap, and is read by `noct_core::ai::catalog`, which keeps four fields of each entry.
//! One read runs at a time; a read can be cancelled; a failed read is not kept.

use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use noct_core::ai::catalog::{CatalogError, CatalogModel, OPENROUTER_MODELS_PATH, parse_models};
use noct_core::ai::network::{OPENROUTER_HOST, OPENROUTER_PORT};
use noct_core::ai::openrouter::ChatFailure;
use noct_core::limits::model_catalog::{CACHE_SECONDS, MAXIMUM_RESPONSE_BYTES};

use crate::error::CommandError;
use crate::provider_transport::{Limits, Target, fetch_json, verify_openrouter};

/// The URL of the model list.
pub fn openrouter_models_url() -> String {
    if OPENROUTER_PORT == 443 {
        format!("https://{OPENROUTER_HOST}{OPENROUTER_MODELS_PATH}")
    } else {
        format!("https://{OPENROUTER_HOST}:{OPENROUTER_PORT}{OPENROUTER_MODELS_PATH}")
    }
}

/// The limits of the read: the chat's times, and the list's own byte cap.
pub fn limits() -> Limits {
    Limits {
        reply_bytes: MAXIMUM_RESPONSE_BYTES,
        ..Limits::beta_one()
    }
}

/// The failure of a read, as the interface is told it: the network, a reply that is not a list, or a list too large.
pub fn failure_of(failure: ChatFailure) -> CommandError {
    match failure {
        ChatFailure::TooLarge => CommandError::ModelsTooLarge,
        ChatFailure::Malformed | ChatFailure::Truncated => CommandError::ModelsInvalid,
        ChatFailure::Cancelled => CommandError::ModelsCancelled,
        _ => CommandError::ModelsNetwork,
    }
}

/// Reads the list from a checked target. The tests pass a loopback target; the command passes OpenRouter.
pub async fn fetch_from(
    target: &Target,
    limits: &Limits,
) -> Result<Vec<CatalogModel>, CommandError> {
    let body = fetch_json(target, limits).await.map_err(failure_of)?;
    parse_models(&body).map_err(|error| match error {
        CatalogError::Invalid => CommandError::ModelsInvalid,
        CatalogError::TooMany => CommandError::ModelsTooLarge,
    })
}

/// Reads the list from OpenRouter.
pub async fn fetch_openrouter() -> Result<Vec<CatalogModel>, CommandError> {
    let target = verify_openrouter(openrouter_models_url())
        .await
        .map_err(failure_of)?;
    fetch_from(&target, &limits()).await
}

type Abort = Box<dyn Fn() + Send + Sync>;

/// The list kept in memory, and the read that is running.
#[derive(Default)]
pub struct ModelCatalog {
    kept: Mutex<Option<(Instant, Arc<Vec<CatalogModel>>)>>,
    running: Mutex<Option<Abort>>,
}

impl ModelCatalog {
    /// The kept list, when it was read less than an hour before `now`.
    pub fn fresh(&self, now: Instant) -> Option<Arc<Vec<CatalogModel>>> {
        let kept = lock(&self.kept);
        kept.as_ref()
            .filter(|(read_at, _)| {
                now.saturating_duration_since(*read_at) < Duration::from_secs(CACHE_SECONDS)
            })
            .map(|(_, list)| Arc::clone(list))
    }

    /// The list: the kept one when it is fresh, otherwise a new read by `fetch`, which is kept when it succeeds. A second
    /// read while one runs is refused with `ModelsBusy`, so the provider is never asked twice at once.
    pub async fn list<F, Fut>(
        &self,
        now: Instant,
        fetch: F,
    ) -> Result<Arc<Vec<CatalogModel>>, CommandError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Vec<CatalogModel>, CommandError>> + Send + 'static,
    {
        if let Some(list) = self.fresh(now) {
            return Ok(list);
        }
        let task = {
            let mut running = lock(&self.running);
            if running.is_some() {
                return Err(CommandError::ModelsBusy);
            }
            let task = tauri::async_runtime::spawn(fetch());
            let abort = task.inner().abort_handle();
            *running = Some(Box::new(move || abort.abort()));
            task
        };
        // The mark of the running read is cleared however this call ends, even when it is dropped before the read ends, so a
        // lost call never leaves every later read refused as busy.
        let _clear = ClearRunning(&self.running);
        let outcome = task.await;
        let list = Arc::new(outcome.map_err(|_| CommandError::ModelsCancelled)??);
        *lock(&self.kept) = Some((now, Arc::clone(&list)));
        Ok(list)
    }

    /// Stops the read that is running. Returns whether there was one.
    pub fn cancel(&self) -> bool {
        match lock(&self.running).as_ref() {
            Some(abort) => {
                abort();
                true
            }
            None => false,
        }
    }
}

/// Clears the mark of the running read when it goes out of scope.
struct ClearRunning<'a>(&'a Mutex<Option<Abort>>);

impl Drop for ClearRunning<'_> {
    fn drop(&mut self) {
        lock(self.0).take();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
