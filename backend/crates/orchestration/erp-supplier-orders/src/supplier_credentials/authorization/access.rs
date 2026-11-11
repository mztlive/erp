use std::sync::Arc;

use application_core::AuditActor;
use config::JdVopConfig;
use erp_identity::{
    AccessSubject, AccessTarget, AuthorizationRegistry, AuthorizationService, Permission, ResourceTargets,
    SharedRbacService,
};
use erp_supply::entity::supplier_api::{SupplierApiConnection, SupplierConnectorType};
use erp_supply::ports::jd_vop::authorization::AuthorizationHttp;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
use persistence_core::Executor;

use super::{AuthorizationError, Result};
use crate::{Error as ProcessError, Result as ProcessResult};

#[derive(Clone)]
pub(super) struct ConnectionAccess {
    pub(super) db: Database,
    pub(super) rbac: SharedRbacService,
    pub(super) settings: Option<JdVopConfig>,
    pub(super) protocol: Result<Arc<dyn AuthorizationHttp>>,
}

impl ConnectionAccess {
    pub(super) async fn permission(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> ProcessResult<bool> {
        let permission = Permission::parse(format!("supplier_api_connection:{action}"))?;
        Ok(AuthorizationService::new(self.db.clone(), self.rbac.clone(), AuthorizationRegistry::default())
            .has_permission(actor, &permission, executor)
            .await?)
    }

    pub(super) async fn require(
        &self,
        id: &str,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> ProcessResult<SupplierApiConnection> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(id, executor)
            .await
            .map_err(ProcessError::from)
            .map_err(conceal_missing)?;
        super::policy::authorization(&self.db, self.rbac.clone(), &connection)?
            .require(
                actor,
                &Permission::parse(format!("supplier_api_connection:{action}"))?,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await
            .map_err(ProcessError::from)
            .map_err(conceal_missing)?;
        if connection.connector_type != Some(SupplierConnectorType::JdVop) || connection.base.is_deleted() {
            return Err(ProcessError::Forbidden("当前账号不能操作此供应商连接".into()));
        }
        Ok(connection)
    }

    pub(super) fn binding(&self, connection: &SupplierApiConnection) -> Result<()> {
        self.settings.as_ref().ok_or(AuthorizationError::Unavailable)?;
        self.protocol.as_ref().map_err(|error| *error)?.verify_binding(connection)
    }
}

/// 隐藏对象存在性，设施失败和授权漂移仍保留原分类。
fn conceal_missing(error: ProcessError) -> ProcessError {
    match error {
        ProcessError::NotFound(_) => ProcessError::Forbidden("当前账号不能操作此供应商连接".into()),
        other => other,
    }
}

impl From<ProcessError> for AuthorizationError {
    /// 将内部授权失败转为固定脱敏分类，不把设施失败伪装成无权。
    fn from(error: ProcessError) -> Self {
        match error {
            ProcessError::Forbidden(_) | ProcessError::NotFound(_) | ProcessError::Unauthenticated(_) => {
                Self::Forbidden
            },
            ProcessError::ConflictError(_)
            | ProcessError::ReceiptDuplicate(_)
            | ProcessError::TransientTransaction(_) => Self::Conflict,
            ProcessError::OutcomeUnknown(_) => Self::Unknown,
            _ => Self::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_failures_keep_denial_drift_and_unavailability_distinct() {
        assert!(matches!(
            conceal_missing(ProcessError::NotFound("connection".into())),
            ProcessError::Forbidden(_)
        ));
        assert_eq!(
            AuthorizationError::from(ProcessError::Forbidden("denied".into())),
            AuthorizationError::Forbidden
        );
        assert_eq!(
            AuthorizationError::from(ProcessError::ConflictError("DATA_SCOPE_CHANGED".into())),
            AuthorizationError::Conflict
        );
        assert_eq!(
            AuthorizationError::from(ProcessError::Internal("identity unavailable".into())),
            AuthorizationError::Unavailable
        );
        assert_eq!(
            AuthorizationError::from(ProcessError::Rbac("policy unavailable".into())),
            AuthorizationError::Unavailable
        );
    }
}
