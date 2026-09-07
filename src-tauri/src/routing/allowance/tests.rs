use super::*;
use crate::{
    credentials::ISecretStore,
    model::ProviderConfig,
    routing::{
        engine::{IInferenceProvider, InferenceDefinition, PreparedRequest, RouteFailure},
        metadata::InferenceModel,
        metrics::AllowanceStatus,
        reports::{RequestStatus, RoutingReports},
    },
};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use zeroize::Zeroizing;

struct NoSecrets;
impl ISecretStore for NoSecrets {
    fn get(&self, _: &str, _: &str) -> Result<Option<Zeroizing<String>>, String> {
        Ok(None)
    }
    fn set(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
        panic!("Reporting must not write credentials")
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), String> {
        panic!("Reporting must not delete credentials")
    }
}
struct WaitingProvider(Arc<AtomicUsize>);
#[async_trait]
impl IInferenceProvider for WaitingProvider {
    fn definition(&self) -> InferenceDefinition {
        InferenceDefinition {
            description: "Fictional observation fixture",
        }
    }
    fn validate(&self, _: &ProviderConfig) -> Result<(), String> {
        Ok(())
    }
    async fn models(&self, _: &InferenceContext<'_>) -> Result<Vec<InferenceModel>, String> {
        panic!("Reporting must not discover models")
    }
    async fn prepare(
        &self,
        _: &InferenceContext<'_>,
        _: &Value,
        _: &str,
    ) -> Result<PreparedRequest, RouteFailure> {
        panic!("Reporting must not submit inference")
    }
    async fn observe_allowance(&self, _: &InferenceContext<'_>) -> Option<AllowanceSnapshot> {
        self.0.fetch_add(1, Ordering::SeqCst);
        std::future::pending().await
    }
}
fn probe(
    reports: &Arc<RoutingReports>,
    slots: Arc<Semaphore>,
    cancelled: CancellationToken,
    calls: Arc<AtomicUsize>,
) -> AllowanceProbe {
    let trace = reports.begin("fixture".into(), false);
    let update = trace.allowance_update();
    trace.finish(RequestStatus::Completed, Some(200), "Completed.");
    AllowanceProbe {
        before: AllowanceSnapshot {
            checked_at: 100,
            windows: vec![],
        },
        account: RouteAccount {
            id: "fixture".into(),
            config: Default::default(),
            provider: Arc::new(WaitingProvider(calls)),
            serial: Arc::new(tokio::sync::Mutex::new(())),
        },
        client: reqwest::Client::new(),
        secrets: Arc::new(NoSecrets),
        clock: Arc::new(|| 100),
        cancelled,
        slots,
        update,
    }
}
#[tokio::test]
async fn full_probe_capacity_and_cancellation_do_not_start_an_account_read() {
    let reports = Arc::new(RoutingReports::default());
    let calls = Arc::new(AtomicUsize::new(0));
    probe(
        &reports,
        Arc::new(Semaphore::new(0)),
        CancellationToken::new(),
        calls.clone(),
    )
    .start();
    assert!(
        reports.snapshot().recent[0]
            .metrics
            .allowance
            .as_ref()
            .unwrap()
            .status
            == AllowanceStatus::Unavailable
    );
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let slots = Arc::new(Semaphore::new(1));
    probe(&reports, slots.clone(), cancelled, calls.clone()).start();
    tokio::time::timeout(Duration::from_secs(1), async {
        while slots.available_permits() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(
        reports.snapshot().recent[0]
            .metrics
            .allowance
            .as_ref()
            .unwrap()
            .status
            == AllowanceStatus::Unavailable
    );
}
#[tokio::test]
async fn an_unresponsive_observation_times_out_and_releases_its_slot_without_changing_call_duration(
) {
    let reports = Arc::new(RoutingReports::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let slots = Arc::new(Semaphore::new(1));
    let probe = probe(
        &reports,
        slots.clone(),
        CancellationToken::new(),
        calls.clone(),
    );
    let duration = reports.snapshot().recent[0].duration_ms;
    probe.start();
    tokio::time::timeout(Duration::from_secs(4), async {
        while slots.available_permits() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let row = &reports.snapshot().recent[0];
    assert_eq!(row.duration_ms, duration);
    assert_eq!(row.status, RequestStatus::Completed);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(row.metrics.allowance.as_ref().unwrap().status == AllowanceStatus::Unavailable);
}
