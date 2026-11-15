//! 单据详情共用的审批运行事实；调用方负责完整业务对象读取授权。

use std::sync::Arc;

use erp_identity::AuthorizationService;
use erp_permissions::Permission as AccessPermission;
mod definition;
mod dto;
mod presentation;
mod summary;
mod summary_query;
pub use definition::{DocumentApprovalNodeView, binding_route};
mod query;

use application_core::AuditActor;
pub use dto::{DocumentApprovalHistoryItemView, DocumentApprovalInstanceView};
use erp_workflow::service::approval::execution::RuntimeHistoryPage;
use mongodb::Database;
use persistence_core::Executor;
pub use presentation::{
    DocumentApprovalDefinitionView, DocumentApprovalHistoryPageView, DocumentApprovalView,
};
pub(crate) use query::{load_document_runtime, load_document_runtime_for_instance};
pub use summary::{DocumentApprovalHistorySummaryItemView, DocumentApprovalInstanceSummaryView};
pub(crate) use summary_query::load_approval_summary;

/// 与同一业务读取快照绑定的实际运行摘要和有界历史。
pub(crate) struct DocumentRuntime {
    pub instance: DocumentApprovalInstanceView,
    pub history: RuntimeHistoryPage,
    pub cancellable: bool,
}

/// 当前启用账号和角色授予的静态动作资格，不以对象读取范围代替操作权限。
///
/// # 参数
/// * `db` - 应用数据库
/// * `rbac` - 当前 RBAC 快照服务
/// * `actor` - 已认证操作人
/// * `permission` - 固定功能权限常量
/// * `executor` - 调用方执行器
///
/// # 返回
/// 账号具备该静态动作权限时返回 `true`。
///
/// # 错误
/// 身份域授权读取失败时返回对应错误。
pub(crate) async fn has_document_permission(
    _db: &Database,
    rbac: &Arc<AuthorizationService>,
    actor: &AuditActor,
    permission: &AccessPermission,
    executor: &mut dyn Executor,
) -> crate::Result<bool> {
    Ok(rbac.has_permission(actor, permission, executor).await?)
}
