//! 命令路径上的投影辅助，与变更结果共用。

use std::collections::HashSet;

use application_core::AuditActor;
use persistence_core::Executor;

use super::access::{TaskEligibility, authorized_item_fields};
use super::order_access::filter_order_facts;
use super::{WorkItemService, dto};
use crate::entity::work_item::WorkItem;
use crate::error::Result;
use crate::ports::ObjectFactMap;
use crate::service::approval::execution::runtime_service::approval_task_readable_with_executor;

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// 重读对象事实，只保留当前快照仍有权看到的投影。
    ///
    /// # 参数
    /// * `items` - 待投影的工作项
    /// * `access` - 当前账号访问快照
    /// * `executor` - 授权和业务读取共用的执行器
    ///
    /// # 返回
    /// 返回审批任务责任链匹配的投影，以及普通任务经对象事实过滤后的投影。
    ///
    /// # 错误
    /// 账号、审批责任链或对象事实读取失败时返回对应错误。
    pub async fn authorized_fields_for_items(
        &self,
        items: Vec<WorkItem>,
        access: &TaskEligibility,
        executor: &mut dyn Executor,
    ) -> Result<Vec<dto::WorkItemFields>> {
        let mut ordinary = Vec::new();
        let mut approval_fields = Vec::new();
        let actor = self
            .identity
            .directory()
            .account(&access.task.actor_id, executor)
            .await?
            .map(|account| AuditActor::new(account.id, account.login_account, account.kind));
        for item in items {
            if item.work_item_type.is_document_approval() {
                if let Some(actor) = &actor
                    && approval_task_readable_with_executor(&self.db, &self.auth, actor, &item, executor)
                        .await?
                {
                    approval_fields.push(dto::WorkItemFields::from(item));
                }
            } else {
                ordinary.push(item);
            }
        }
        let items = ordinary;
        let keys = items
            .iter()
            .filter_map(|item| {
                item.work_item_type
                    .brief_relation(&item.business_object_type)
                    .map(|policy| (policy.object_kind, item.business_object_id.clone()))
            })
            .collect::<HashSet<_>>();
        let mut facts: ObjectFactMap = self.facts.load_object_facts(&keys, executor).await?;
        filter_order_facts(&self.auth, &access.task.actor_id, &mut facts, executor).await?;
        approval_fields
            .extend(items.into_iter().filter_map(|item| authorized_item_fields(item, access, &facts)));
        Ok(approval_fields)
    }
}
