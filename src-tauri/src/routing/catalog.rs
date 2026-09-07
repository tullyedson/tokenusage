use super::{
    config::{ModelPool, PoolMember},
    engine::{InferenceContext, RouteAccount},
    metadata::{InferenceModel, ModelLimits},
};
use crate::{credentials::ISecretStore, model::ProviderConfig};
use futures_util::{stream, FutureExt, StreamExt};
use serde::Serialize;
use std::collections::BTreeMap;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogReport {
    pub account_id: String,
    pub models: Vec<InferenceModel>,
    pub checked_at: Option<i64>,
    pub error: Option<String>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailablePool {
    #[serde(flatten)]
    pub pool: ModelPool,
    pub automatic: bool,
    pub available: bool,
    pub limits: ModelLimits,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelLibrary {
    pub catalogs: Vec<CatalogReport>,
    pub pools: Vec<AvailablePool>,
}
#[derive(Clone)]
struct Cached {
    config: ProviderConfig,
    report: CatalogReport,
    attempted_at: i64,
}
#[derive(Default)]
pub struct ModelCatalog {
    cache: Mutex<BTreeMap<String, Cached>>,
    refresh: Mutex<()>,
}
pub struct CatalogContext<'a> {
    pub accounts: &'a [RouteAccount],
    pub secrets: &'a dyn ISecretStore,
    pub client: &'a reqwest::Client,
    pub cancelled: &'a CancellationToken,
    pub now: i64,
}

pub fn account_issue(account: &RouteAccount) -> Option<String> {
    if !account.config.enabled {
        return Some("Account is disabled. Enable it in Settings.".into());
    }
    if !account.config.routing.enabled {
        return Some("This account is excluded from model pools. Enable it in Settings.".into());
    }
    account.provider.validate(&account.config).err()
}

impl ModelCatalog {
    /// Reporting uses fresh cached bounds only and never adds discovery I/O to
    /// a completion. A missing, changed, failed or stale catalog stays unknown.
    pub async fn cached(&self, accounts: &[RouteAccount], now: i64) -> Vec<CatalogReport> {
        let cache = self.cache.lock().await;
        accounts
            .iter()
            .filter_map(|account| {
                cache
                    .get(&account.id)
                    .filter(|old| {
                        old.config == account.config
                            && old.report.error.is_none()
                            && (0..300).contains(&now.saturating_sub(old.attempted_at))
                    })
                    .map(|old| old.report.clone())
            })
            .collect()
    }

    pub async fn read(
        &self,
        ctx: CatalogContext<'_>,
        force: bool,
    ) -> Result<Vec<CatalogReport>, String> {
        let _refresh = tokio::select! {
            _ = ctx.cancelled.cancelled() => return Err("Account settings changed. Refresh models again.".into()),
            lock = self.refresh.lock() => lock,
        };
        let previous = self.cache.lock().await.clone();
        let due = ctx
            .accounts
            .iter()
            .filter(|account| account_issue(account).is_none())
            .filter(|account| {
                force
                    || previous.get(&account.id).is_none_or(|old| {
                        old.config != account.config
                            || ctx.now - old.attempted_at
                                >= if old.report.error.is_some() { 30 } else { 300 }
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        let reads = due
            .into_iter()
            .map(|account| {
                async move {
                    let context = InferenceContext {
                        account_id: &account.id,
                        config: &account.config,
                        secrets: ctx.secrets,
                        client: ctx.client,
                        now: ctx.now,
                        session_id: None,
                    };
                    let result = tokio::time::timeout(
                        std::time::Duration::from_secs(15),
                        account.provider.models(&context),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        Err("Model discovery timed out. Check the provider connection.".into())
                    });
                    (account, result)
                }
                .boxed()
            })
            .collect::<Vec<_>>();
        let reads = stream::iter(reads).buffer_unordered(8).collect::<Vec<_>>();
        let results = tokio::select! {
            _ = ctx.cancelled.cancelled() => return Err("Account settings changed. Refresh models again.".into()),
            result = reads => result,
        };
        let mut cache = self.cache.lock().await;
        if ctx.cancelled.is_cancelled() {
            return Err("Account settings changed. Refresh models again.".into());
        }
        cache.retain(|id, _| ctx.accounts.iter().any(|account| &account.id == id));
        for (account, result) in results {
            let mut report = CatalogReport {
                account_id: account.id.clone(),
                models: vec![],
                checked_at: None,
                error: None,
            };
            match result {
                Ok(mut models) => {
                    models.retain(|model| super::config::valid_model(&model.id));
                    for model in &mut models {
                        model.limits = model.limits.bounded();
                    }
                    models.sort_by(|a, b| a.id.cmp(&b.id));
                    // Duplicate IDs must not let a larger advertised bound win.
                    models.dedup_by(|a, b| {
                        if a.id != b.id {
                            return false;
                        }
                        b.limits = ModelLimits::intersection(&[a.limits, b.limits]);
                        true
                    });
                    models.truncate(4096);
                    report.models = models;
                    report.checked_at = Some(ctx.now);
                }
                Err(error) => {
                    if let Some(old) = previous
                        .get(&account.id)
                        .filter(|old| old.config == account.config)
                    {
                        report.models = old.report.models.clone();
                        report.checked_at = old.report.checked_at;
                    }
                    report.error = Some(error);
                }
            }
            cache.insert(
                account.id.clone(),
                Cached {
                    config: account.config.clone(),
                    report,
                    attempted_at: ctx.now,
                },
            );
        }
        Ok(ctx
            .accounts
            .iter()
            .map(|account| {
                if let Some(error) = account_issue(account) {
                    CatalogReport {
                        account_id: account.id.clone(),
                        models: vec![],
                        checked_at: None,
                        error: Some(error),
                    }
                } else {
                    cache
                        .get(&account.id)
                        .filter(|cached| cached.config == account.config)
                        .map(|cached| cached.report.clone())
                        .unwrap_or(CatalogReport {
                            account_id: account.id.clone(),
                            models: vec![],
                            checked_at: None,
                            error: Some("Models have not been discovered yet.".into()),
                        })
                }
            })
            .collect())
    }
}

pub fn library(
    accounts: &[RouteAccount],
    catalogs: Vec<CatalogReport>,
    custom: &[ModelPool],
) -> ModelLibrary {
    let mut pools = BTreeMap::<String, AvailablePool>::new();
    for catalog in &catalogs {
        for model in &catalog.models {
            let pool = pools
                .entry(model.id.clone())
                .or_insert_with(|| AvailablePool {
                    pool: ModelPool {
                        mode: Default::default(),
                        name: model.id.clone(),
                        members: vec![],
                    },
                    automatic: true,
                    available: true,
                    limits: ModelLimits::default(),
                });
            pool.pool.members.push(PoolMember {
                account_id: catalog.account_id.clone(),
                model: model.id.clone(),
            });
        }
    }
    for pool in custom {
        let available = pool.members.iter().any(|member| {
            accounts
                .iter()
                .any(|account| account.id == member.account_id && account_issue(account).is_none())
        });
        pools.insert(
            pool.name.clone(),
            AvailablePool {
                pool: pool.clone(),
                automatic: false,
                available,
                limits: ModelLimits::default(),
            },
        );
    }
    for pool in pools.values_mut() {
        let limits = pool
            .pool
            .members
            .iter()
            .filter(|member| {
                accounts.iter().any(|account| {
                    account.id == member.account_id && account_issue(account).is_none()
                })
            })
            .map(|member| {
                catalogs
                    .iter()
                    .find(|catalog| {
                        catalog.account_id == member.account_id && catalog.error.is_none()
                    })
                    .and_then(|catalog| {
                        catalog.models.iter().find(|model| model.id == member.model)
                    })
                    .map(|model| model.limits)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        pool.limits = ModelLimits::intersection(&limits);
    }
    ModelLibrary {
        catalogs,
        pools: pools.into_values().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::engine::{
        IInferenceProvider, InferenceDefinition, PreparedRequest, RouteFailure,
    };
    use async_trait::async_trait;
    use serde_json::Value;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };
    struct Adapter {
        failed: AtomicBool,
        reads: AtomicUsize,
    }
    #[async_trait]
    impl IInferenceProvider for Adapter {
        fn definition(&self) -> InferenceDefinition {
            InferenceDefinition {
                description: "Fixture",
            }
        }
        fn validate(&self, _: &ProviderConfig) -> Result<(), String> {
            Ok(())
        }
        async fn models(&self, _: &InferenceContext<'_>) -> Result<Vec<InferenceModel>, String> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.failed.load(Ordering::SeqCst) {
                Err("Fixture catalog offline".into())
            } else {
                Ok(vec![
                    "glm-flash".into(),
                    "glm-flash".into(),
                    "qwen".into(),
                    "invalid name".into(),
                ])
            }
        }
        async fn prepare(
            &self,
            _: &InferenceContext<'_>,
            _: &Value,
            _: &str,
        ) -> Result<PreparedRequest, RouteFailure> {
            unreachable!("Catalog reads must never generate")
        }
    }
    struct NoSecrets;
    impl ISecretStore for NoSecrets {
        fn get(&self, _: &str, _: &str) -> Result<Option<zeroize::Zeroizing<String>>, String> {
            Ok(None)
        }
        fn set(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
            unreachable!()
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), String> {
            unreachable!()
        }
    }
    #[tokio::test]
    async fn discovery_caches_names_but_never_reuses_them_across_changed_accounts() {
        let adapter = Arc::new(Adapter {
            failed: AtomicBool::new(false),
            reads: AtomicUsize::new(0),
        });
        let accounts = vec![RouteAccount {
            id: "first".into(),
            config: ProviderConfig {
                enabled: true,
                ..Default::default()
            },
            provider: adapter.clone(),
            serial: Arc::new(Mutex::new(())),
        }];
        let catalog = ModelCatalog::default();
        let client = reqwest::Client::new();
        let cancelled = CancellationToken::new();
        let read = |accounts, now| {
            catalog.read(
                CatalogContext {
                    accounts,
                    now,
                    secrets: &NoSecrets,
                    client: &client,
                    cancelled: &cancelled,
                },
                false,
            )
        };
        let reports = read(&accounts, 1000).await.unwrap();
        assert_eq!(
            reports[0]
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["glm-flash", "qwen"]
        );
        assert_eq!(library(&accounts, reports, &[]).pools.len(), 2);
        read(&accounts, 1001).await.unwrap();
        assert_eq!(adapter.reads.load(Ordering::SeqCst), 1);
        adapter.failed.store(true, Ordering::SeqCst);
        let stale = read(&accounts, 1301).await.unwrap();
        assert!(stale[0].error.is_some());
        assert_eq!(stale[0].models.len(), 2);
        assert_eq!(stale[0].checked_at, Some(1000));
        let mut accounts = accounts.clone();
        accounts[0].config.revision += 1;
        assert!(read(&accounts, 1302).await.unwrap()[0].models.is_empty());
        let mut accounts = accounts.clone();
        accounts[0].config.enabled = false;
        let count = adapter.reads.load(Ordering::SeqCst);
        assert!(read(&accounts, 1400).await.unwrap()[0].models.is_empty());
        assert_eq!(adapter.reads.load(Ordering::SeqCst), count);
    }
    #[test]
    fn custom_pool_overrides_automatic_members_and_keeps_the_exact_order() {
        let reports = ["first", "second"]
            .into_iter()
            .map(|id| CatalogReport {
                account_id: id.into(),
                models: vec!["glm".into()],
                checked_at: None,
                error: None,
            })
            .collect::<Vec<_>>();
        let automatic = library(&[], reports.clone(), &[]);
        assert_eq!(automatic.pools[0].pool.members.len(), 2);
        let custom = ModelPool {
            mode: Default::default(),
            name: "glm".into(),
            members: vec![
                PoolMember {
                    account_id: "second".into(),
                    model: "deepseek".into(),
                },
                PoolMember {
                    account_id: "first".into(),
                    model: "glm".into(),
                },
            ],
        };
        let result = library(&[], reports, std::slice::from_ref(&custom));
        assert_eq!(result.pools.len(), 1);
        assert!(!result.pools[0].automatic);
        assert_eq!(result.pools[0].pool, custom);
    }
    #[test]
    fn chain_limits_include_smaller_missing_and_stale_members_but_exclude_disabled_accounts() {
        let mut accounts = ["first", "second"]
            .into_iter()
            .map(|id| RouteAccount {
                id: id.into(),
                config: ProviderConfig {
                    enabled: true,
                    ..Default::default()
                },
                provider: Arc::new(Adapter {
                    failed: AtomicBool::new(false),
                    reads: AtomicUsize::new(0),
                }),
                serial: Arc::new(Mutex::new(())),
            })
            .collect::<Vec<_>>();
        let mut reports = accounts
            .iter()
            .zip([1_000_000, 32768])
            .map(|(account, context)| CatalogReport {
                account_id: account.id.clone(),
                checked_at: Some(1000),
                error: None,
                models: vec![InferenceModel {
                    id: "model".into(),
                    limits: ModelLimits {
                        context: Some(context),
                        output: Some(8192),
                        input: None,
                    },
                }],
            })
            .collect::<Vec<_>>();
        let pool = ModelPool {
            mode: crate::routing::config::RouteMode::LoadDistribution,
            name: "arbitrary-chain".into(),
            members: accounts
                .iter()
                .map(|account| PoolMember {
                    account_id: account.id.clone(),
                    model: "model".into(),
                })
                .collect(),
        };
        let limit = |accounts: &[RouteAccount], reports: &[CatalogReport]| {
            library(accounts, reports.to_vec(), std::slice::from_ref(&pool))
                .pools
                .into_iter()
                .find(|p| p.pool.name == pool.name)
                .unwrap()
                .limits
                .context
        };
        assert_eq!(limit(&accounts, &reports), Some(32768));
        reports[1].error = Some("Catalog unavailable".into());
        assert_eq!(limit(&accounts, &reports), None);
        reports[1].error = None;
        reports[1].models.clear();
        assert_eq!(limit(&accounts, &reports), None);
        accounts[1].config.routing.enabled = false;
        assert_eq!(limit(&accounts, &reports), Some(1_000_000));
    }
}
