//! 单据详情共用的审批运行事实；调用方负责完整业务对象读取授权。

mod dto;
mod query;

use application_core::AuditActor;
pub use dto::{DocumentApprovalHistoryItemView, DocumentApprovalInstanceView};
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_identity::{Permission, SharedRbacService};
use erp_workflow::service::approval::execution::RuntimeHistoryPage;
use mongodb::Database;
use persistence_core::Executor;
pub(crate) use query::load_document_runtime;

/// 与同一业务读取快照绑定的实际运行摘要和有界历史。
pub(crate) struct DocumentRuntime {
    pub instance: DocumentApprovalInstanceView,
    pub history: RuntimeHistoryPage,
    pub cancellable: bool,
}

/// 当前启用账号和角色授予的静态动作资格，不以对象读取范围代替操作权限。
pub(crate) async fn has_document_permission(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    resource: &str,
    action: &str,
    executor: &mut dyn Executor,
) -> crate::Result<bool> {
    let permission = Permission::parse(format!("{resource}:{action}"))?;
    let service = DataScopeService::new(db.clone(), rbac.clone());
    Ok(service.batch(actor, std::slice::from_ref(&permission), executor).has_permission(&permission).await?)
}
