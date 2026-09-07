//! 本域版本、形态、连接读取与固定权限名；授权执行属于组合层。
use super::SupplierApiService;
use crate::dto::supplier_api::{RelatedImpactView, SafeReferenceView, SupplierActionBlockerView};
use crate::entity::supplier_api::{
    SupplierApiConnection, SupplierApiConnectionId, SupplierCommandShapeRejection, SupplierConnectionAction,
    SupplierGovernanceBlocker,
};
use crate::repository::SupplierApiExt;
use crate::{Error, Result};
use sha2::{Digest, Sha256};
type SupplierConnectionImpact = <mongodb::Database as SupplierApiExt>::SupplierConnectionImpact;
impl SupplierApiService {
    /// 使用调用方事务读取连接；不存在时保留连接命令的固定错误。
    ///
    /// # Errors
    /// 连接不存在返回 NotFound，仓储失败原样透传。
    pub async fn load_connection(
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
pub fn map_command_shape_rejection(rejection: SupplierCommandShapeRejection) -> Error {
    Error::ValidationError(rejection.to_string())
}

/// 返回固定动作对应的权限名；权限求值由组合层执行。
pub fn action_permission(action: SupplierConnectionAction) -> &'static str {
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

/// 构造原动作阻塞投影，不添加权限或业务判断。
pub fn blocker(
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
pub fn governance_blocker_view(blocker: SupplierGovernanceBlocker) -> SupplierActionBlockerView {
    SupplierActionBlockerView {
        action: blocker.action.as_str().to_string(),
        code: blocker.code.to_string(),
        message: blocker.message,
        destination_workspace_id: blocker.destination_workspace_id.map(str::to_string),
    }
}

/// 只投影绑定与可见状态，不暴露内部引用。
pub fn safe_reference(bound: bool, visible: bool) -> SafeReferenceView {
    SafeReferenceView {
        state: if bound { "BOUND" } else { "MISSING" },
        alias: None,
        version: None,
        visible,
    }
}

/// 把完整权威影响事实映射为原三个计数字段。
pub fn impact_view(impact: SupplierConnectionImpact) -> RelatedImpactView {
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

/// 对每段字节使用 u64 大端长度前缀后计算 SHA-256，供命令与任务共用身份。
pub fn digest(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    hex::encode(digest.finalize())
}
