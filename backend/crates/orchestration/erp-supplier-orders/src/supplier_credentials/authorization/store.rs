use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_integration::Error as IntegrationError;
use erp_integration::entity::supplier_credential::{
    SupplierCredential, SupplierCredentialAuthorizationRequest, SupplierCredentialBinding,
    SupplierCredentialPayload,
};
use erp_integration::indexes::supplier_credential::ensure;
use erp_integration::repository::SupplierCredentialExt;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::access::ConnectionAccess;
use super::{AuthorizationError, Result};
use crate::{Error as ProcessError, Result as ProcessResult};

#[derive(Clone)]
pub(super) struct CommandContext {
    pub(super) connection_id: String,
    pub(super) actor: AuditActor,
}

#[async_trait]
pub(super) trait AuthorizationStore: Send + Sync {
    async fn load(&self) -> Result<Option<SupplierCredential>>;
    async fn reserve(
        &self,
        request: SupplierCredentialAuthorizationRequest,
        context: CommandContext,
    ) -> Result<(SupplierCredential, bool)>;
    async fn sent(
        &self,
        record: SupplierCredential,
        now: Instant,
        context: CommandContext,
    ) -> Result<Option<SupplierCredential>>;
    async fn exchange(
        &self,
        record: SupplierCredential,
        now: Instant,
        context: CommandContext,
    ) -> Result<Option<SupplierCredential>>;
    async fn reject(
        &self,
        record: SupplierCredential,
        now: Instant,
        context: CommandContext,
    ) -> Result<Option<SupplierCredential>>;
    async fn complete(
        &self,
        record: SupplierCredential,
        payload: SupplierCredentialPayload,
        uid: Option<String>,
        now: Instant,
        context: CommandContext,
    ) -> Result<Option<SupplierCredential>>;
}

#[derive(Clone)]
pub(super) struct MongoAuthorizationStore {
    pub(super) access: ConnectionAccess,
    pub(super) credential_id: String,
    pub(super) binding: SupplierCredentialBinding,
}

enum Mutation {
    Sent(SupplierCredential, Instant),
    Exchange(SupplierCredential, Instant),
    Reject(SupplierCredential, Instant),
    Complete(SupplierCredential, SupplierCredentialPayload, Option<String>, Instant),
}

impl MongoAuthorizationStore {
    async fn mutate(
        &self,
        mutation: Mutation,
        context: CommandContext,
    ) -> Result<Option<SupplierCredential>> {
        let this = self.clone();
        let result: ProcessResult<_> = self
            .access
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    if matches!(mutation, Mutation::Sent(..) | Mutation::Exchange(..)) {
                        let connection = this
                            .access
                            .require(&context.connection_id, &context.actor, "authorize", executor)
                            .await
                            .map_err(|_| ProcessError::Forbidden("当前账号不能继续授权此连接".into()))?;
                        this.access
                            .binding(&connection)
                            .map_err(|_| ProcessError::ConflictError("京东连接配置已变化".into()))?;
                    }
                    let result = this.apply(mutation, executor).await?;
                    if result.is_some() {
                        audit(&this.access, &this.credential_id, &context, executor).await?;
                    }
                    Ok(result)
                })
            })
            .await;
        result.map_err(|_| AuthorizationError::Unknown)
    }

    async fn apply(
        &self,
        mutation: Mutation,
        executor: &mut dyn Executor,
    ) -> ProcessResult<Option<SupplierCredential>> {
        let repo = self.access.db.supplier_credentials();
        Ok(match mutation {
            Mutation::Sent(record, now) => repo.mark_authorization_sent(&record, now, executor).await?,
            Mutation::Exchange(record, now) => {
                repo.mark_authorization_exchange_sent(&record, now, executor).await?
            },
            Mutation::Reject(record, now) => repo.reject_authorization(&record, now, executor).await?,
            Mutation::Complete(record, payload, uid, now) => {
                repo.complete_authorization(&record, payload, uid, now, executor).await?
            },
        })
    }
}

async fn audit(
    access: &ConnectionAccess,
    credential_id: &str,
    context: &CommandContext,
    executor: &mut dyn Executor,
) -> ProcessResult<()> {
    let log = context.actor.clone().resource_log(
        "supplier_credential.authorize",
        "supplier_credential",
        credential_id.to_string(),
    )?;
    persist_log(&access.db, &log, executor).await?;
    Ok(())
}

#[async_trait]
impl AuthorizationStore for MongoAuthorizationStore {
    async fn load(&self) -> Result<Option<SupplierCredential>> {
        self.access
            .db
            .supplier_credentials()
            .load(&self.credential_id, &self.binding, &mut NoTransaction)
            .await
            .map_err(|error| match error {
                IntegrationError::ConflictError(_) => AuthorizationError::BindingMismatch,
                _ => AuthorizationError::Unavailable,
            })
    }

    async fn reserve(
        &self,
        request: SupplierCredentialAuthorizationRequest,
        context: CommandContext,
    ) -> Result<(SupplierCredential, bool)> {
        ensure(&self.access.db).await.map_err(|_| AuthorizationError::Unavailable)?;
        let this = self.clone();
        let result: ProcessResult<_> = self
            .access
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let connection = this
                        .access
                        .require(&context.connection_id, &context.actor, "authorize", executor)
                        .await
                        .map_err(|_| ProcessError::Forbidden("当前账号不能授权此连接".into()))?;
                    this.access
                        .binding(&connection)
                        .map_err(|_| ProcessError::ConflictError("京东连接配置已变化".into()))?;
                    let result = this
                        .access
                        .db
                        .supplier_credentials()
                        .reserve_authorization(&this.credential_id, &this.binding, request, executor)
                        .await?;
                    if result.1 {
                        audit(&this.access, &this.credential_id, &context, executor).await?;
                    }
                    Ok(result)
                })
            })
            .await;
        result.map_err(|_| AuthorizationError::Conflict)
    }

    async fn sent(
        &self,
        record: SupplierCredential,
        now: Instant,
        context: CommandContext,
    ) -> Result<Option<SupplierCredential>> {
        self.mutate(Mutation::Sent(record, now), context).await
    }
    async fn exchange(
        &self,
        record: SupplierCredential,
        now: Instant,
        context: CommandContext,
    ) -> Result<Option<SupplierCredential>> {
        self.mutate(Mutation::Exchange(record, now), context).await
    }
    async fn reject(
        &self,
        record: SupplierCredential,
        now: Instant,
        context: CommandContext,
    ) -> Result<Option<SupplierCredential>> {
        self.mutate(Mutation::Reject(record, now), context).await
    }
    async fn complete(
        &self,
        record: SupplierCredential,
        payload: SupplierCredentialPayload,
        uid: Option<String>,
        now: Instant,
        context: CommandContext,
    ) -> Result<Option<SupplierCredential>> {
        self.mutate(Mutation::Complete(record, payload, uid, now), context).await
    }
}
