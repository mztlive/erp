//! 单任务业务读取资格；不构造展示投影。

use std::collections::HashSet;

use application_core::AuditActor;
use persistence_core::Executor;

use super::WorkItemService;
use super::access::{TaskEligibility, has_item_access};
use super::order_access::filter_order_facts;
use crate::entity::work_item::WorkItem;
use crate::error::{Error, Result};
use crate::service::approval::execution::runtime_service::approval_task_readable_with_executor;

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// 在当前执行器验证单任务业务读取资格，保留原来源入口权限。
    /// # 参数
    /// item 为当前任务，actor 和 access 为当前身份及资格，executor 沿用原事务。
    /// # 返回
    /// 审批责任链或普通任务的来源、参与关系和版本均满足时成功。
    /// # 错误
    /// 无业务读取资格时拒绝，取证故障保留原错误。
    pub(super) async fn require_business_read(
        &self,
        item: &WorkItem,
        actor: &AuditActor,
        access: &TaskEligibility,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let readable = if item.work_item_type.is_document_approval() {
            approval_task_readable_with_executor(&self.identity, &self.db, &self.auth, actor, item, executor)
                .await?
        } else {
            let policy = item
                .work_item_type
                .brief_relation(&item.business_object_type)
                .ok_or_else(|| Error::Forbidden("当前账号无权查看该业务对象".into()))?;
            let keys = HashSet::from([(policy.object_kind, item.business_object_id.clone())]);
            let mut facts = self.facts.load_object_facts(&keys, executor).await?;
            filter_order_facts(&self.auth, actor, &mut facts, executor).await?;
            let access = self.participation(access, &facts, executor).await?;
            has_item_access(item, &access, &facts)
        };
        if !readable {
            return Err(Error::Forbidden("当前账号无权查看该业务对象".into()));
        }
        Ok(())
    }
}
