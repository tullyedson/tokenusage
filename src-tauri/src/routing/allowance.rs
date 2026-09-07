//! Best-effort account observations. They never delay or gate a model request.
use super::{
    engine::{InferenceContext, RouteAccount},
    metrics::{AllowanceObservation, AllowanceSnapshot},
    reports::AllowanceUpdate,
};
use crate::credentials::ISecretStore;
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

pub struct AllowanceProbe {
    pub before: AllowanceSnapshot,
    pub account: RouteAccount,
    pub client: reqwest::Client,
    pub secrets: Arc<dyn ISecretStore>,
    pub clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    pub cancelled: CancellationToken,
    pub slots: Arc<Semaphore>,
    pub update: AllowanceUpdate,
}
impl AllowanceProbe {
    pub fn start(self) {
        let Ok(permit) = self.slots.clone().try_acquire_owned() else {
            self.update.set(AllowanceObservation::unavailable());
            return;
        };
        tokio::spawn(async move {
            let _permit = permit;
            let context = InferenceContext {
                account_id: &self.account.id,
                config: &self.account.config,
                secrets: self.secrets.as_ref(),
                client: &self.client,
                now: (self.clock)(),
                session_id: None,
            };
            let mut after = tokio::select! {
                biased;
                _ = self.cancelled.cancelled() => None,
                value = tokio::time::timeout(Duration::from_secs(2), self.account.provider.observe_allowance(&context)) => value.ok().flatten(),
            };
            if self.cancelled.is_cancelled() {
                after = None;
            }
            if let Some(after) = &mut after {
                after.checked_at = (self.clock)();
            }
            let result = after
                .as_ref()
                .map_or_else(AllowanceObservation::unavailable, |after| {
                    AllowanceObservation::compare(&self.before, after)
                });
            self.update.set(result);
        });
    }
}

/// Ownership ends with the submitted response/stream. Only numeric observations
/// and connection dependencies move into the bounded post-response read.
pub struct ProbeOnDrop(pub Option<AllowanceProbe>);
impl Drop for ProbeOnDrop {
    fn drop(&mut self) {
        if let Some(probe) = self.0.take() {
            probe.start();
        }
    }
}

#[cfg(test)]
mod tests;
