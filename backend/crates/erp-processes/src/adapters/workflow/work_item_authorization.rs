//! 将原工作流任务授权入口装配为详情消费的唯一窄读取端口。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::SharedRbacService;
use erp_read_models::ports::work_item_authorization::{AuthorizedTaskFact, WorkItemAuthorizationReadPort};
use erp_workflow::service::work_item::WorkItemService;
use mongodb::Database;
use services::workflow_compose::{work_item_service, WorkflowAuth};

/// 私有持有完整工作流服务，仅向详情暴露授权后的五项事实。
pub struct WorkItemAuthorizationAdapter {
    service: WorkItemService<WorkflowAuth>,
}

impl WorkItemAuthorizationAdapter {
    /// 装配原工作流服务；构造时不查询、不授权或写入。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self {
            service: work_item_service(db, rbac),
        }
    }
}

#[async_trait]
impl WorkItemAuthorizationReadPort for WorkItemAuthorizationAdapter {
    /// 在调用方原时点执行完整工作流授权，再投影消费字段。
    ///
    /// # Errors
    /// 原工作流错误直接透传，不按字符串重新分类。
    async fn authorize(&self, id: &str, actor: &AuditActor) -> erp_workflow::Result<AuthorizedTaskFact> {
        let authorized = self.service.authorize_work_item(id, actor).await?;
        Ok(AuthorizedTaskFact {
            work_item_type: authorized.item.work_item_type,
            business_object_type: authorized.item.business_object_type,
            business_object_id: authorized.item.business_object_id,
            subject_version: authorized.item.subject_version,
            allowed_actions: authorized.allowed_actions,
        })
    }
}
