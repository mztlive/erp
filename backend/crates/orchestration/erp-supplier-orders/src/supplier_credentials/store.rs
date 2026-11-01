//! Mongo 拥有仓储适配：每次关键状态写入与资源审计使用同一个执行器；HTTP 不进入事务。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_integration::entity::supplier_credential::{
    SupplierCredential, SupplierCredentialBinding, SupplierCredentialPayload,
    SupplierCredentialReauthorization, SupplierCredentialRefreshClaim,
};
use erp_integration::indexes::supplier_credential::ensure;
use erp_integration::repository::SupplierCredentialExt;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::jd_vop::token::token_error;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use crate::Result;

#[async_trait]
pub(super) trait CredentialStore: Send + Sync {
    async fn load(&self) -> ConnectorResult<Option<SupplierCredential>>;
    async fn import(
        &self,
        payload: SupplierCredentialPayload,
        expected: Option<u64>,
        recovery: Option<SupplierCredentialReauthorization>,
        actor: AuditActor,
        now: Instant,
    ) -> ConnectorResult<SupplierCredential>;
    async fn claim(
        &self,
        version: u64,
        claim: SupplierCredentialRefreshClaim,
    ) -> ConnectorResult<Option<SupplierCredential>>;
    async fn sent(
        &self,
        claimed: SupplierCredential,
        now: Instant,
    ) -> ConnectorResult<Option<SupplierCredential>>;
    async fn complete(
        &self,
        sent: SupplierCredential,
        payload: SupplierCredentialPayload,
        now: Instant,
    ) -> ConnectorResult<Option<SupplierCredential>>;
    async fn invalidate(
        &self,
        sent: SupplierCredential,
        now: Instant,
    ) -> ConnectorResult<Option<SupplierCredential>>;
}

enum Mutation {
    Import(SupplierCredentialPayload, Option<u64>, Option<SupplierCredentialReauthorization>, Instant),
    Claim(u64, SupplierCredentialRefreshClaim),
    Sent(SupplierCredential, Instant),
    Complete(SupplierCredential, SupplierCredentialPayload, Instant),
    Invalidate(SupplierCredential, Instant),
}

#[derive(Clone)]
pub(super) struct MongoCredentialStore {
    db: Database,
    id: String,
    binding: SupplierCredentialBinding,
}

impl MongoCredentialStore {
    pub(super) fn new(db: Database, id: String, binding: SupplierCredentialBinding) -> Self {
        Self { db, id, binding }
    }

    async fn mutate(
        &self,
        mutation: Mutation,
        actor: AuditActor,
    ) -> ConnectorResult<Option<SupplierCredential>> {
        let action = if matches!(mutation, Mutation::Import(..)) {
            "supplier_credential.import"
        } else {
            "supplier_credential.refresh"
        };
        let log =
            actor.resource_log(action, "supplier_credential", self.id.clone()).map_err(|_| token_error())?;
        let context = self.clone();
        let result: Result<_> = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let result = context.apply(mutation, executor).await?;
                    if result.is_some() {
                        persist_log(&context.db, &log, executor).await?;
                    }
                    Ok(result)
                })
            })
            .await;
        result.map_err(|_| token_error())
    }

    async fn apply(
        &self,
        mutation: Mutation,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCredential>> {
        let repository = self.db.supplier_credentials();
        let result = match mutation {
            Mutation::Import(payload, expected, recovery, now) => match recovery {
                Some(recovery) => Some(
                    repository.reauthorize(&self.id, &self.binding, payload, recovery, now, executor).await?,
                ),
                None => {
                    Some(repository.import(&self.id, &self.binding, payload, expected, now, executor).await?)
                },
            },
            Mutation::Claim(version, claim) => {
                repository.claim_refresh(&self.id, &self.binding, version, claim, executor).await?
            },
            Mutation::Sent(claimed, now) => repository.mark_refresh_sent(&claimed, now, executor).await?,
            Mutation::Complete(sent, payload, now) => {
                repository.complete_refresh(&sent, payload, now, executor).await?
            },
            Mutation::Invalidate(sent, now) => repository.invalidate_refresh(&sent, now, executor).await?,
        };
        Ok(result)
    }
}

fn refresh_actor() -> AuditActor {
    AuditActor::new("system:jd-vop-refresh".into(), "system:jd-vop-refresh".into(), AccountKind::Admin)
}

#[async_trait]
impl CredentialStore for MongoCredentialStore {
    async fn load(&self) -> ConnectorResult<Option<SupplierCredential>> {
        self.db
            .supplier_credentials()
            .load(&self.id, &self.binding, &mut NoTransaction)
            .await
            .map_err(|_| token_error())
    }
    async fn import(
        &self,
        payload: SupplierCredentialPayload,
        expected: Option<u64>,
        recovery: Option<SupplierCredentialReauthorization>,
        actor: AuditActor,
        now: Instant,
    ) -> ConnectorResult<SupplierCredential> {
        // CLI 可能先于新版 Web 启动；正式写入必须先证明本集合唯一索引存在。
        ensure(&self.db).await.map_err(|_| token_error())?;
        self.mutate(Mutation::Import(payload, expected, recovery, now), actor).await?.ok_or_else(token_error)
    }
    async fn claim(
        &self,
        version: u64,
        claim: SupplierCredentialRefreshClaim,
    ) -> ConnectorResult<Option<SupplierCredential>> {
        self.mutate(Mutation::Claim(version, claim), refresh_actor()).await
    }
    async fn sent(
        &self,
        claimed: SupplierCredential,
        now: Instant,
    ) -> ConnectorResult<Option<SupplierCredential>> {
        self.mutate(Mutation::Sent(claimed, now), refresh_actor()).await
    }
    async fn complete(
        &self,
        sent: SupplierCredential,
        payload: SupplierCredentialPayload,
        now: Instant,
    ) -> ConnectorResult<Option<SupplierCredential>> {
        self.mutate(Mutation::Complete(sent, payload, now), refresh_actor()).await
    }
    async fn invalidate(
        &self,
        sent: SupplierCredential,
        now: Instant,
    ) -> ConnectorResult<Option<SupplierCredential>> {
        self.mutate(Mutation::Invalidate(sent, now), refresh_actor()).await
    }
}
