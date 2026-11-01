//! 本域版本、形态、连接读取与固定权限名；授权执行属于组合层。
use sha2::{Digest, Sha256};

use super::SupplierApiService;
use crate::dto::supplier_api::{RelatedImpactView, SafeReferenceView, SupplierActionBlockerView};
use crate::entity::supplier_api::{
    SupplierApiConnection, SupplierApiConnectionId, SupplierCommandShapeRejection, SupplierConnectionAction,
    SupplierGovernanceBlocker,
};
use crate::repository::SupplierApiExt;
use crate::{Error, Result};
type SupplierConnectionImpact = <mongodb::Database as SupplierApiExt>::SupplierConnectionImpact;
impl SupplierApiService {
    /// 使用调用方事务读取连接；不存在时保留连接命令的固定错误。
    ///
    /// # 参数
    /// * `id` - 供应商连接主键。
    /// * `executor` - 调用方事务的执行器。
    ///
    /// # 返回
    /// 返回已存在的连接。
    ///
    /// # 错误
    /// 连接不存在返回 `NotFound`，仓储失败原样透传。
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
///
/// # 错误
/// 不返回错误。
pub fn map_command_shape_rejection(rejection: SupplierCommandShapeRejection) -> Error {
    Error::ValidationError(rejection.to_string())
}

/// 返回固定动作对应的权限名；权限求值由组合层执行。
///
/// # 参数
/// * `action` - 连接命令动作。
///
/// # 返回
/// 返回该动作的静态权限名。
///
/// # 错误
/// 不返回错误。
pub fn action_permission(action: SupplierConnectionAction) -> &'static str {
    match action {
        SupplierConnectionAction::UpdateBusinessProfile => "supplier_api_connection:update_business_profile",
        SupplierConnectionAction::BindEndpointReference => "supplier_api_connection:bind_endpoint_reference",
        SupplierConnectionAction::BindCredentialReference => {
            "supplier_api_connection:manage_credential_reference"
        },
        SupplierConnectionAction::RunHealthCheck => "supplier_api_connection:health_check",
        SupplierConnectionAction::Enable => "supplier_api_connection:enable",
        SupplierConnectionAction::Disable => "supplier_api_connection:disable",
        SupplierConnectionAction::StartCatalogSync => "supplier_api_connection:catalog_sync",
    }
}

/// 构造原动作阻塞投影，不添加权限或业务判断。
///
/// # 参数
/// * `action` - 被阻塞的动作名。
/// * `code` - 阻塞代码。
/// * `message` - 阻塞说明。
/// * `destination` - 可选目标工作区；`None` 表示不携带。
///
/// # 返回
/// 返回动作阻塞视图。
///
/// # 错误
/// 不返回错误。
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

/// 将实体层治理阻塞原因转换为服务响应视图，复用原动作阻塞投影。
///
/// # 参数
/// * `cause` - 实体层治理阻塞原因。
///
/// # 返回
/// 返回动作阻塞视图。
///
/// # 错误
/// 不返回错误。
pub fn governance_blocker_view(cause: SupplierGovernanceBlocker) -> SupplierActionBlockerView {
    blocker(cause.action.as_str(), cause.code, &cause.message, cause.destination_workspace_id)
}

/// 只投影绑定与可见状态，不暴露内部引用。
///
/// # 参数
/// * `bound` - 引用是否已绑定。
/// * `visible` - 引用是否对调用方可见。
///
/// # 返回
/// 返回安全引用视图；别名与版本保持为空。
///
/// # 错误
/// 不返回错误。
pub fn safe_reference(bound: bool, visible: bool) -> SafeReferenceView {
    SafeReferenceView { state: if bound { "BOUND" } else { "MISSING" }, alias: None, version: None, visible }
}

/// 把完整权威影响事实映射为原三个计数字段。
///
/// # 参数
/// * `impact` - 连接的权威影响事实。
///
/// # 返回
/// 返回活跃供给、未结供应商订单和活跃同步任务三个计数。
///
/// # 错误
/// 不返回错误。
pub fn impact_view(impact: SupplierConnectionImpact) -> RelatedImpactView {
    RelatedImpactView {
        active_offerings: impact.active_offerings,
        open_supplier_orders: impact.open_supplier_orders,
        active_sync_jobs: impact.active_sync_jobs,
    }
}

/// 校验实际版本与调用方期望版本一致。
///
/// # 参数
/// * `actual` - 当前持久化版本。
/// * `expected` - 调用方期望版本。
///
/// # 返回
/// 版本一致时无返回值。
///
/// # 错误
/// 版本不一致时返回 `ConflictError`。
pub(super) fn ensure_version(actual: u64, expected: u64) -> Result<()> {
    if actual == expected {
        return Ok(());
    }
    Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))
}

/// 对每段字节使用 u64 大端长度前缀后计算 SHA-256，供命令与任务共用身份。
///
/// # 参数
/// * `parts` - 按顺序参与摘要的文本段。
///
/// # 返回
/// 返回十六进制 SHA-256 摘要。
///
/// # 错误
/// 不返回错误。
pub fn digest(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    hex::encode(digest.finalize())
}
