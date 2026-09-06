use database::SupplierApiExt;
use entities::supplier_api::{
    BusinessCapabilityConfirmation, SupplierApiCapability, SupplierApiConnection,
    SupplierCommandShapeRejection, SupplierConnectionAction, SupplierGovernanceBlocker,
    SupplierHealthCheckRun,
};
use entities::Permission;
use erp_core::ids::SupplierApiConnectionId;
use sha2::{Digest, Sha256};

use crate::errors::{Error, Result};
use crate::iam::subject;
use application_core::AuditActor;

use super::super::dto::{RelatedImpactView, SafeReferenceView, SupplierActionBlockerView};
use super::super::SupplierApiService;

type SupplierConnectionImpact = <mongodb::Database as SupplierApiExt>::SupplierConnectionImpact;

pub(super) struct GovernanceContext {
    pub(super) capabilities: Vec<SupplierApiCapability>,
    pub(super) confirmations: Vec<BusinessCapabilityConfirmation>,
    pub(super) health_runs: Vec<SupplierHealthCheckRun>,
    pub(super) impact: SupplierConnectionImpact,
}

impl SupplierApiService {
    pub(super) async fn governance_context(
        &self,
        connection: &SupplierApiConnection,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<GovernanceContext> {
        let data = self
            .db
            .supplier_api()
            .governance_data(&SupplierApiConnectionId::new(&connection.base.id), 50, executor)
            .await?;
        Ok(GovernanceContext {
            capabilities: data.capabilities,
            confirmations: data.confirmations,
            health_runs: data.health_runs,
            impact: data.impact,
        })
    }

    pub(super) async fn ensure_action_permission(
        &self,
        actor: &AuditActor,
        action: SupplierConnectionAction,
    ) -> Result<()> {
        let permission = action_permission(action);
        self.ensure_permission(actor, permission).await
    }

    pub(super) async fn has_action_permission(
        &self,
        actor: &AuditActor,
        action: SupplierConnectionAction,
    ) -> Result<bool> {
        self.has_permission(actor, action_permission(action)).await
    }

    pub(super) async fn ensure_permission(&self, actor: &AuditActor, permission: &str) -> Result<()> {
        if self.has_permission(actor, permission).await? {
            return Ok(());
        }
        Err(Error::Forbidden("当前角色不能执行该连接治理动作".to_string()))
    }

    pub(super) async fn has_permission(&self, actor: &AuditActor, permission: &str) -> Result<bool> {
        let Some(rbac) = self.rbac.as_ref() else {
            return Ok(false);
        };
        let permission = Permission::parse(permission)?;
        rbac.enforce(&subject(actor.kind(), actor.id()), &permission)
            .await
    }

    pub(super) async fn load_connection(
        &self,
        id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<SupplierApiConnection> {
        self.db
            .supplier_api()
            .connection(&SupplierApiConnectionId::new(id), executor)
            .await?
            .ok_or_else(|| Error::NotFound("连接不存在".to_string()))
    }
}

/// 将命令形态拒绝映射为参数校验错误（保持历史必填校验语义）。
///
/// # 参数
/// * `rejection` - 命令形态校验拒绝原因
///
/// # 返回
/// 一律映射为 `ValidationError`（含新增的多余字段拒绝）。
pub(crate) fn map_command_shape_rejection(rejection: SupplierCommandShapeRejection) -> Error {
    Error::ValidationError(rejection.to_string())
}

fn action_permission(action: SupplierConnectionAction) -> &'static str {
    match action {
        SupplierConnectionAction::UpdateBusinessProfile => "supplier_api_connection:update_business_profile",
        SupplierConnectionAction::BindEndpointReference => "supplier_api_connection:bind_endpoint_reference",
        SupplierConnectionAction::BindCredentialReference => {
            "supplier_api_connection:manage_credential_reference"
        }
        SupplierConnectionAction::RunHealthCheck => "supplier_api_connection:health_check",
        SupplierConnectionAction::Enable => "supplier_api_connection:enable",
        SupplierConnectionAction::Disable => "supplier_api_connection:disable",
        SupplierConnectionAction::StartCatalogSync => "supplier_api_connection:catalog_sync",
    }
}

pub(super) fn blocker(
    action: &str,
    code: &str,
    message: &str,
    destination: Option<&str>,
) -> SupplierActionBlockerView {
    SupplierActionBlockerView {
        action: action.to_string(),
        code: code.to_string(),
        message: message.to_string(),
        destination_workspace_id: destination.map(str::to_string),
    }
}

/// 将实体层治理阻塞原因转换为服务响应视图。
pub(super) fn governance_blocker_view(blocker: SupplierGovernanceBlocker) -> SupplierActionBlockerView {
    SupplierActionBlockerView {
        action: blocker.action.as_str().to_string(),
        code: blocker.code.to_string(),
        message: blocker.message,
        destination_workspace_id: blocker.destination_workspace_id.map(str::to_string),
    }
}

pub(super) fn safe_reference(bound: bool, visible: bool) -> SafeReferenceView {
    SafeReferenceView {
        state: if bound { "BOUND" } else { "MISSING" },
        alias: None,
        version: None,
        visible,
    }
}

pub(super) fn impact_view(impact: SupplierConnectionImpact) -> RelatedImpactView {
    RelatedImpactView {
        active_offerings: impact.active_offerings,
        open_supplier_orders: impact.open_supplier_orders,
        active_sync_jobs: impact.active_sync_jobs,
    }
}

pub(super) fn ensure_version(actual: u64, expected: u64) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError(
        "数据已被其他请求修改，请刷新后重试".to_string(),
    ))
}

pub(super) fn digest(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    hex::encode(digest.finalize())
}
