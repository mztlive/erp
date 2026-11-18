use std::sync::Arc;

use application_core::AuditActor;
use config::JdVopConfig;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, supplier_api};
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
    pub(super) authorization: Arc<AuthorizationService>,
    pub(super) settings: Option<JdVopConfig>,
    pub(super) protocol: Result<Arc<dyn AuthorizationHttp>>,
}

impl ConnectionAccess {
    pub(super) async fn require(
        &self,
        id: &str,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> ProcessResult<SupplierApiConnection> {
        self.authorization
            .require(actor, &connection_permission(action)?, AccessTarget::record(id)?, executor)
            .await
            .map_err(ProcessError::from)
            .map_err(conceal_missing)?;

        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(id, executor)
            .await
            .map_err(ProcessError::from)
            .map_err(conceal_missing)?;
        if connection.connector_type != Some(SupplierConnectorType::JdVop) {
            return Err(ProcessError::Forbidden("当前账号不能操作此供应商连接".into()));
        }
        Ok(connection)
    }

    pub(super) fn binding(&self, connection: &SupplierApiConnection) -> Result<()> {
        verify_binding(self.settings.as_ref(), &self.protocol, connection)
    }
}

/// 隐藏对象存在性，设施失败和授权漂移仍保留原分类。
pub(super) fn conceal_missing(error: ProcessError) -> ProcessError {
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

fn connection_permission(action: &str) -> ProcessResult<Permission> {
    match action {
        "detail" => Ok(supplier_api::CONNECTION_DETAIL),
        "authorize" => Ok(supplier_api::JD_VOP_AUTHORIZE),
        _ => Err(ProcessError::ValidationError("未知供应商连接动作".into())),
    }
}

/// 核对当前部署配置与协议绑定，不执行身份访问或远程授权。
/// # 参数
/// settings/protocol 为固定部署能力；connection 为已授权的真实连接。
/// # 返回
/// 配置存在且协议绑定匹配时成功。
/// # 错误
/// 配置缺失或绑定不符时保留脱敏协议错误。
pub(super) fn verify_binding(
    settings: Option<&JdVopConfig>,
    protocol: &Result<Arc<dyn AuthorizationHttp>>,
    connection: &SupplierApiConnection,
) -> Result<()> {
    settings.ok_or(AuthorizationError::Unavailable)?;
    protocol.as_ref().map_err(|error| *error)?.verify_binding(connection)
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
