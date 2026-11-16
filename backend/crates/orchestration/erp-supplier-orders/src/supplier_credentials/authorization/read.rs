//! 本地凭证及回放资格只在连接授权的同一快照中读取。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::CompiledAccess;
use erp_integration::Error as IntegrationError;
use erp_integration::entity::supplier_credential::{
    SupplierCredential, SupplierCredentialAuthorizationRequest,
};
use erp_integration::repository::SupplierCredentialExt;
use erp_permissions::{Permission, supplier_api};
use erp_supply::entity::supplier_api::{SupplierApiConnection, SupplierConnectorType};
use erp_supply::repository::supplier_api::authorized_connection;
use persistence_core::{Error as PersistenceError, Executor};

use super::access::{ConnectionAccess, conceal_missing};
use super::engine::AuthorizationEngine;
use super::{AuthorizationError, JdVopAuthorizationRequest, JdVopAuthorizationStatus, Result};
use crate::Error as ProcessError;

/// 读取事务保留原脱敏授权错误，同时支持事务设施错误转换。
pub(super) struct TransactionError(pub(super) AuthorizationError);

impl From<AuthorizationError> for TransactionError {
    fn from(error: AuthorizationError) -> Self {
        Self(error)
    }
}

impl From<PersistenceError> for TransactionError {
    fn from(error: PersistenceError) -> Self {
        Self(AuthorizationError::from(ProcessError::from(error)))
    }
}

pub(super) async fn status(
    access: &ConnectionAccess,
    engine: Result<&AuthorizationEngine>,
    id: &str,
    actor: &AuditActor,
    operation_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<JdVopAuthorizationStatus> {
    require_snapshot(executor)?;
    let (compiled, connection) =
        connection(access, id, actor, &supplier_api::CONNECTION_DETAIL, executor).await?;
    let result = status_projection(access, engine, &connection, actor, operation_id, executor).await?;
    access.authorization.validate(&compiled, executor).await.map_err(ProcessError::from)?;
    Ok(result)
}

async fn status_projection(
    access: &ConnectionAccess,
    engine: Result<&AuthorizationEngine>,
    connection: &SupplierApiConnection,
    actor: &AuditActor,
    operation_id: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<JdVopAuthorizationStatus> {
    let Ok(engine) = engine else {
        return Ok(JdVopAuthorizationStatus::empty(
            "unconfigured",
            false,
            "京东授权配置未就绪，请联系管理员配置应用资料",
        ));
    };
    if access.binding(connection).is_err() {
        return Ok(JdVopAuthorizationStatus::empty(
            "unconfigured",
            false,
            "京东连接与已部署账号配置不一致，请联系管理员核验",
        ));
    }
    let record = load_record(access, engine, executor).await;
    let mut status = engine.project_status(record, operation_id);
    let permitted = access
        .authorization
        .has_permission(actor, &supplier_api::JD_VOP_AUTHORIZE, executor)
        .await
        .map_err(ProcessError::from)?;
    status.can_authorize &= permitted;
    status.can_configure_tokens = permitted
        && status.state != "unconfigured"
        && (status.state != "unreadable" || status.version.is_some())
        && status.recovery_after.is_none_or(|deadline| deadline <= Instant::now().unix_secs());
    Ok(status)
}

pub(super) async fn authorization_command(
    access: &ConnectionAccess,
    engine: Result<&AuthorizationEngine>,
    id: &str,
    actor: &AuditActor,
    request: &JdVopAuthorizationRequest,
    executor: &mut dyn Executor,
) -> Result<(SupplierCredentialAuthorizationRequest, Option<JdVopAuthorizationStatus>)> {
    require_snapshot(executor)?;
    let (compiled, connection) =
        connection(access, id, actor, &supplier_api::JD_VOP_AUTHORIZE, executor).await?;
    access.binding(&connection)?;
    let engine = engine?;
    let record = load_record(access, engine, executor).await?;
    let prepared = engine.prepare_authorization(request, actor, record.as_ref())?;
    access.authorization.validate(&compiled, executor).await.map_err(ProcessError::from)?;
    Ok(prepared)
}

async fn connection(
    access: &ConnectionAccess,
    id: &str,
    actor: &AuditActor,
    permission: &Permission,
    executor: &mut dyn Executor,
) -> Result<(CompiledAccess, SupplierApiConnection)> {
    let compiled =
        access.authorization.query(actor, permission, executor).await.map_err(ProcessError::from)?;
    let connection = authorized_connection(&access.db, id, &compiled, executor)
        .await
        .map_err(ProcessError::from)
        .map_err(conceal_missing)?;
    if connection.connector_type != Some(SupplierConnectorType::JdVop) {
        return Err(AuthorizationError::Forbidden);
    }
    Ok((compiled, connection))
}

async fn load_record(
    access: &ConnectionAccess,
    engine: &AuthorizationEngine,
    executor: &mut dyn Executor,
) -> Result<Option<SupplierCredential>> {
    access.db.supplier_credentials().load(&engine.crypto.id, &engine.crypto.binding, executor).await.map_err(
        |error| match error {
            IntegrationError::ConflictError(_) => AuthorizationError::BindingMismatch,
            _ => AuthorizationError::Unavailable,
        },
    )
}

fn require_snapshot(executor: &dyn Executor) -> Result<()> {
    if executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        Ok(())
    } else {
        Err(AuthorizationError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn status_and_command_replay_reject_before_credential_reads_without_snapshot_or_qualification() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("status");
        let access = ConnectionAccess {
            db,
            authorization: unavailable_authorization(),
            settings: None,
            protocol: Err(AuthorizationError::Unavailable),
        };
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let request = serde_json::from_value(serde_json::json!({
            "operation_id":"operation", "expected_version":null, "username":"buyer", "password":"password",
            "redirect_uri":"callback", "private_key":"key", "evidence_reference":null
        }))
        .unwrap();
        for executor in [
            &mut ReadExecutor::default() as &mut dyn Executor,
            &mut SnapshotExecutor::default() as &mut dyn Executor,
        ] {
            assert!(matches!(
                status(&access, Err(AuthorizationError::Unavailable), "connection", &actor, None, executor)
                    .await,
                Err(AuthorizationError::Unavailable)
            ));
            assert!(matches!(
                authorization_command(
                    &access,
                    Err(AuthorizationError::Unavailable),
                    "connection",
                    &actor,
                    &request,
                    executor
                )
                .await,
                Err(AuthorizationError::Unavailable)
            ));
        }
    }
}
