//! 工作流消费方端口的生产装配。

use std::sync::Arc;

use erp_identity::SharedRbacService;
use erp_workflow::ports::WorkflowAccountFact;
use erp_workflow::{Error as WorkflowError, WorkItemService};
use mongodb::Database;

use crate::errors::{Error, Result};

mod approval_objects;
mod approval_scope;
mod approval_source;
mod audit;
mod authorization;
mod object_facts;
mod order_access;
mod purchase_responsibility;
mod task_scope;
mod w29_close;
mod w29_reassign;
pub mod work_item_authorization;

pub use audit::WorkflowAudit;
pub use authorization::WorkflowAuth;
pub use object_facts::WorkflowObjectFacts;

fn map_service(error: Error) -> WorkflowError {
    match error {
        Error::Internal(message) => WorkflowError::Internal(message),
        Error::NotFound(message) => WorkflowError::NotFound(message),
        Error::ValidationError(message) => WorkflowError::ValidationError(message),
        Error::BusinessLogicError(message) => WorkflowError::BusinessLogicError(message),
        Error::ConflictError(message) => WorkflowError::ConflictError(message),
        Error::ReceiptDuplicate(error) => WorkflowError::ReceiptDuplicate(error),
        Error::TransientTransaction(error) => WorkflowError::TransientTransaction(error),
        Error::Forbidden(message) => WorkflowError::Forbidden(message),
        Error::Unauthenticated(message) => WorkflowError::Unauthenticated(message),
        Error::Logic(error) => WorkflowError::Logic(error),
        Error::Rbac(message) => WorkflowError::Rbac(message),
        Error::OutcomeUnknown(error) => WorkflowError::OutcomeUnknown(error),
        Error::RepositoryError(error) => WorkflowError::from(error),
        Error::Coded(code) => WorkflowError::Coded(code),
    }
}

/// 把身份账号转换成工作流账号事实。
///
/// # 参数
/// * `account` - 身份域账号核心。
///
/// # 返回
/// 返回带显示名和登录名的 `WorkflowAccountFact`。
///
/// # 错误
/// 不返回错误。
pub fn account_fact(account: &erp_identity::AccountCore) -> WorkflowAccountFact {
    WorkflowAccountFact::new(account.base.id.clone(), account.kind, account.can_login())
        .with_display_name(account.name.clone())
        .with_login_account(account.secret.account().to_string())
}

/// 装配授权、对象事实与审计端口齐全的工作项命令服务。
///
/// # 参数
/// * `db` - 工作项与身份集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
///
/// # 返回
/// 返回已注入三个组合端口的工作项服务。
///
/// # 错误
/// 不返回错误。
pub fn work_item_service(db: Database, rbac: SharedRbacService) -> WorkItemService<WorkflowAuth> {
    let auth = WorkflowAuth::new(db.clone(), rbac);
    let facts = Arc::new(WorkflowObjectFacts::new(db.clone()));
    let audit = Arc::new(WorkflowAudit::new(db.clone()));
    WorkItemService::with_ports(db, auth, facts, audit)
}

/// 为组合根构造工作流授权 adapter。
///
/// # 参数
/// * `db` - 身份与业务集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
///
/// # 返回
/// 返回未执行 I/O 的 `WorkflowAuth`。
///
/// # 错误
/// 不返回错误。
pub fn workflow_auth(db: Database, rbac: SharedRbacService) -> WorkflowAuth {
    WorkflowAuth::new(db, rbac)
}

/// 构造工作流审计 adapter。
///
/// # 参数
/// * `db` - 审计与业务集合所在数据库。
///
/// # 返回
/// 返回共享的 `WorkflowAudit`。
///
/// # 错误
/// 不返回错误。
pub fn workflow_audit(db: Database) -> Arc<WorkflowAudit> {
    Arc::new(WorkflowAudit::new(db))
}

/// 构造工作项对象事实 adapter。
///
/// # 参数
/// * `db` - 业务事实集合所在数据库。
///
/// # 返回
/// 返回共享的 `WorkflowObjectFacts`。
///
/// # 错误
/// 不返回错误。
pub fn workflow_object_facts(db: Database) -> Arc<WorkflowObjectFacts> {
    Arc::new(WorkflowObjectFacts::new(db))
}

/// 经组合根端口绑定已发布审批定义。
///
/// 领域流程必须从组合根取得 `object_read`。本函数不另开嵌套事务。
///
/// # 参数
/// * `db` - 单据与审批集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
/// * `object_read` - 组合根提供的审批对象读取端口。
/// * `command` - 绑定已发布定义的命令。
/// * `actor` - 当前操作人。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 成功时返回工作流绑定函数的结果；内层为 `None` 时原样返回 `None`。
///
/// # 错误
/// 工作流绑定失败时映射为流程 `Error`。
pub async fn bind_published_definition_on_document_create(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    object_read: &dyn erp_workflow::ApprovalObjectReadPort,
    command: &erp_workflow::service::approval::binding::BindPublishedDefinitionCommand,
    actor: &application_core::AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding>> {
    let auth = WorkflowAuth::new(db.clone(), rbac.clone());
    let audit = WorkflowAudit::new(db.clone());
    erp_workflow::service::approval::binding::bind_published_definition_on_document_create(
        db,
        &auth,
        object_read,
        &audit,
        command,
        actor,
        executor,
    )
    .await
    .map_err(map_service_err)
}

fn map_service_err(error: erp_workflow::Error) -> Error {
    Error::from(error)
}

/// 把已算出的绑定挂到已注册业务单据上。
///
/// # 参数
/// * `document` - 待写入绑定的业务单据。
/// * `binding` - 已发布定义绑定。
///
/// # 返回
/// 成功时返回工作流函数交回的绑定。
///
/// # 错误
/// 工作流拒绝挂载时经 `Error::from` 返回对应流程错误。
pub fn attach_published_binding(
    document: &mut erp_workflow::entity::document_registry::BusinessDocument,
    binding: erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding,
) -> Result<erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding> {
    erp_workflow::service::approval::binding::attach_published_binding(document, binding).map_err(Error::from)
}
