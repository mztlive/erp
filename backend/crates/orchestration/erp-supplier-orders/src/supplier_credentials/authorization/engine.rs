use std::sync::Arc;
use std::time::Duration;

use application_core::AuditActor;
use config::JdVopConfig;
use erp_integration::entity::supplier_credential::{
    SupplierCredential, SupplierCredentialAuthorizationRequest, SupplierCredentialRefreshClaim,
    SupplierCredentialRefreshState,
};
use erp_supply::ports::jd_vop::authorization::{
    AuthorizationGrant, AuthorizationHttp, AuthorizationParameters,
};
use id_generator::next_id;
use tokio::time::timeout;

use super::super::crypto::CredentialCrypto;
use super::super::{Clock, SystemClock};
use super::access::ConnectionAccess;
use super::dto::{JdVopAuthorizationRequest, JdVopAuthorizationStatus};
use super::status::operation_projection;
use super::store::{AuthorizationStore, CommandContext, MongoAuthorizationStore};
use super::{AuthorizationError, JdVopAuthorizationTasks, Result};

#[derive(Clone)]
pub(super) struct AuthorizationEngine {
    pub(super) crypto: CredentialCrypto,
    pub(super) settings: JdVopConfig,
    pub(super) store: Arc<dyn AuthorizationStore>,
    pub(super) http: Arc<dyn AuthorizationHttp>,
    pub(super) clock: Arc<dyn Clock>,
    pub(super) tasks: Arc<JdVopAuthorizationTasks>,
    owner_id: String,
}

impl AuthorizationEngine {
    pub(super) fn new(
        access: ConnectionAccess,
        settings: &JdVopConfig,
        tasks: Arc<JdVopAuthorizationTasks>,
        http: Arc<dyn AuthorizationHttp>,
    ) -> Result<Self> {
        let crypto = CredentialCrypto::new(settings).map_err(|_| AuthorizationError::Unavailable)?;
        http.ensure_available()?;
        let store = MongoAuthorizationStore {
            access,
            credential_id: crypto.id.clone(),
            binding: crypto.binding.clone(),
        };
        Ok(Self {
            crypto,
            settings: settings.clone(),
            store: Arc::new(store),
            http,
            clock: Arc::new(SystemClock),
            tasks,
            owner_id: next_id(),
        })
    }

    #[cfg(test)]
    pub(super) async fn status(&self, operation_id: Option<&str>) -> JdVopAuthorizationStatus {
        self.project_status(self.store.load().await, operation_id)
    }

    pub(super) fn project_status(
        &self,
        record: Result<Option<SupplierCredential>>,
        operation_id: Option<&str>,
    ) -> JdVopAuthorizationStatus {
        match record {
            Ok(Some(record)) => operation_projection(&record, &self.crypto, self.clock.now(), operation_id),
            Ok(None) => JdVopAuthorizationStatus::empty(
                "missing",
                true,
                "尚未保存京东授权，请使用企业采购账号完成授权",
            ),
            Err(AuthorizationError::BindingMismatch) => JdVopAuthorizationStatus::empty(
                "unconfigured",
                false,
                "已保存账号与当前京东配置不一致，请联系管理员核验",
            ),
            Err(_) => JdVopAuthorizationStatus::empty(
                "unreadable",
                false,
                "京东授权状态暂时无法读取，请稍后查询或联系管理员",
            ),
        }
    }

    #[cfg(test)]
    pub(super) async fn authorize(
        &self,
        id: &str,
        request: JdVopAuthorizationRequest,
        actor: &AuditActor,
    ) -> Result<JdVopAuthorizationStatus> {
        let current = self.store.load().await?;
        let (command, replayed) = self.prepare_authorization(&request, actor, current.as_ref())?;
        if let Some(status) = replayed {
            return Ok(status);
        }
        self.authorize_prepared(id, request, actor, command).await
    }

    pub(super) fn prepare_authorization(
        &self,
        request: &JdVopAuthorizationRequest,
        actor: &AuditActor,
        current: Option<&SupplierCredential>,
    ) -> Result<(SupplierCredentialAuthorizationRequest, Option<JdVopAuthorizationStatus>)> {
        let command = self.command(request, current, actor)?;
        let replayed = current.filter(|record| {
            record.authorization_history.iter().any(|event| event.operation_id == request.operation_id)
        });
        let status = if let Some(existing) = replayed {
            let mut existing = existing.clone();
            existing.claim_authorization(&command).map_err(|_| AuthorizationError::Conflict)?;
            Some(operation_projection(&existing, &self.crypto, self.clock.now(), Some(&request.operation_id)))
        } else {
            None
        };
        Ok((command, status))
    }

    pub(super) async fn authorize_prepared(
        &self,
        id: &str,
        request: JdVopAuthorizationRequest,
        actor: &AuditActor,
        mut command: SupplierCredentialAuthorizationRequest,
    ) -> Result<JdVopAuthorizationStatus> {
        let permit = self.tasks.permit()?;
        let app_key = self.settings.app_key.clone();
        let http = self.http.clone();
        let parameters = tokio::task::spawn_blocking(move || http.prepare(&request, &app_key))
            .await
            .map_err(|_| AuthorizationError::InvalidInput)??;
        command.claim.now = self.clock.now();
        let context = CommandContext { connection_id: id.into(), actor: actor.clone() };
        let operation_id = command.claim.operation_id.clone();
        let (record, changed) = self.store.reserve(command, context.clone()).await?;
        let status = operation_projection(&record, &self.crypto, self.clock.now(), Some(&operation_id));
        if changed {
            let engine = self.clone();
            tokio::spawn(async move {
                let _permit = permit;
                // 调用者关闭页面或HTTP超时不取消保存；超600秒仍保持已持久化未知阶段。
                let _ = timeout(Duration::from_secs(600), engine.run(record, parameters, context)).await;
            });
        }
        Ok(status)
    }

    fn command(
        &self,
        input: &JdVopAuthorizationRequest,
        current: Option<&SupplierCredential>,
        actor: &AuditActor,
    ) -> Result<SupplierCredentialAuthorizationRequest> {
        let event = current.and_then(|record| {
            record.authorization_history.iter().find(|event| event.operation_id == input.operation_id)
        });
        let unknown = current.is_some_and(|record| {
            matches!(
                record.refresh,
                SupplierCredentialRefreshState::SentUnknown { .. }
                    | SupplierCredentialRefreshState::AuthorizationSentUnknown { .. }
            )
        });
        if event.is_none() && unknown && input.evidence_reference.is_none() {
            return Err(AuthorizationError::InvalidInput);
        }
        let new_authorization = event.map(|event| event.new_authorization).unwrap_or_else(|| {
            current.is_some_and(|record| {
                !(record.encrypted_payload.is_empty()
                    && matches!(record.refresh, SupplierCredentialRefreshState::AuthorizationRejected { .. }))
            })
        });
        let fingerprint = self
            .crypto
            .identity_fingerprint("username", &input.username)
            .map_err(|_| AuthorizationError::Unavailable)?;
        if event.is_none()
            && current.is_some_and(|record| {
                record
                    .account_identity_fingerprint
                    .as_ref()
                    .or(record.authorization_candidate_identity_fingerprint.as_ref())
                    .is_some_and(|original| original != &fingerprint)
            })
        {
            return Err(AuthorizationError::IdentityMismatch);
        }
        Ok(SupplierCredentialAuthorizationRequest {
            expected_version: input.expected_version,
            claim: SupplierCredentialRefreshClaim {
                owner_id: self.owner_id.clone(),
                operation_id: input.operation_id.clone(),
                now: self.clock.now(),
                lease_seconds: self.settings.timeout_seconds * 2 + 15,
            },
            new_authorization,
            actor_id: actor.id().into(),
            evidence_reference: input
                .evidence_reference
                .clone()
                .unwrap_or_else(|| format!("interactive-authorization:{}", input.operation_id)),
            account_identity_fingerprint: fingerprint,
        })
    }

    async fn run(
        &self,
        record: SupplierCredential,
        parameters: AuthorizationParameters,
        context: CommandContext,
    ) -> Result<()> {
        let sent = self
            .store
            .sent(record, self.clock.now(), context.clone())
            .await?
            .ok_or(AuthorizationError::Conflict)?;
        self.send_budget(&sent)?;
        let code =
            match timeout(Duration::from_secs(self.settings.timeout_seconds), self.http.code(parameters))
                .await
            {
                Ok(Ok(code)) => code,
                Ok(Err(AuthorizationError::Rejected)) => {
                    self.store
                        .reject(sent, self.clock.now(), context)
                        .await?
                        .ok_or(AuthorizationError::Conflict)?;
                    return Ok(());
                },
                _ => return Err(AuthorizationError::Unknown),
            };
        let exchange = self
            .store
            .exchange(sent, self.clock.now(), context.clone())
            .await?
            .ok_or(AuthorizationError::Conflict)?;
        self.send_budget(&exchange)?;
        let parameters =
            self.http.exchange_parameters(&self.settings.app_key, &self.settings.app_secret, &code);
        drop(code);
        let grant =
            timeout(Duration::from_secs(self.settings.timeout_seconds), self.http.exchange(parameters))
                .await
                .map_err(|_| AuthorizationError::Unknown)??;
        self.save_grant(exchange, grant, context).await
    }

    /// 将已取得授权的身份、令牌和到期时间加密并保存到原执行记录。
    async fn save_grant(
        &self,
        exchange: SupplierCredential,
        grant: AuthorizationGrant,
        context: CommandContext,
    ) -> Result<()> {
        let now = self.clock.now();
        let uid = grant
            .identity
            .as_ref()
            .map(|value| self.crypto.identity_fingerprint("oauth_identity", value))
            .transpose()
            .map_err(|_| AuthorizationError::Unknown)?;
        let (access, refresh, expires) =
            grant.tokens.into_credential_parts(now).map_err(|_| AuthorizationError::Unknown)?;
        let payload = self
            .crypto
            .replacement(access, refresh, expires, now)
            .map_err(|_| AuthorizationError::Unknown)?;
        self.store
            .complete(exchange, payload, uid, now, context)
            .await?
            .ok_or(AuthorizationError::Conflict)?;
        Ok(())
    }

    fn send_budget(&self, record: &SupplierCredential) -> Result<()> {
        let timeout =
            i64::try_from(self.settings.timeout_seconds).map_err(|_| AuthorizationError::Unknown)?;
        let finish = self
            .clock
            .now()
            .unix_secs()
            .checked_add(timeout)
            .and_then(|value| value.checked_add(1))
            .ok_or(AuthorizationError::Unknown)?;
        if record.refresh.deadline().is_none_or(|deadline| deadline.unix_secs() < finish) {
            return Err(AuthorizationError::Unknown);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
