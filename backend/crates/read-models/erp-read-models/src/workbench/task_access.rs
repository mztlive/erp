//! 供给任务只对当前负责人开放必要销售资料；普通销售范围保持独立。

use std::collections::HashSet;

use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::{WorkItem, WorkItemRow, WorkflowAuthorizationPort};
use persistence_core::Executor;

use super::access::{ActorAccess, authorized_item_fields};
use super::dto::WorkItemFields;
use super::facts::{OwnedFulfillmentTask, object_policy};
use super::{WorkbenchObjectFactMap, WorkbenchReadService};
use crate::Result;

/// 从持久化任务证明当前责任，再使用原权限与对象存在性校验。
///
/// # 参数
/// * `item` - 当前任务实体，必须与候选行版本一致。
/// * `access` - 已验证账号的权限快照。
/// * `facts` - 未经普通销售范围过滤的任务对象事实。
/// # 返回
/// 当前开放供给任务的授权字段；非本人、终态或缺采购权限返回 `None`。
/// # 错误
/// 不返回错误；事实缺失或责任范围不完整时失败关闭。
pub(super) fn procurement_item_fields(
    item: &WorkItem,
    access: &ActorAccess,
    facts: &WorkbenchObjectFactMap,
) -> Option<WorkItemFields> {
    if !item.owns_procurement_task(&access.actor_id)
        && !(item.work_item_type == WorkItemType::SalesInquiryProcessing
            && item.is_owned_by(&access.actor_id))
    {
        return None;
    }
    authorized_item_fields(item.clone(), access, facts)
}

/// 从工作项行借用履约授权所需的对象与负责人身份。
///
/// # 参数
/// * `row` - 仓储工作项行
///
/// # 返回
/// 返回借用该行标识的 `OwnedFulfillmentTask`。
///
/// # 错误
/// 不返回错误。
pub(super) fn owned_fulfillment_task(row: &WorkItemRow) -> OwnedFulfillmentTask<'_> {
    OwnedFulfillmentTask {
        work_item_type: row.work_item_type,
        business_object_type: &row.business_object_type,
        business_object_id: &row.business_object_id,
        owner_user_id: row.owner_user_id.as_deref(),
    }
}

fn owned_fulfillment_task_item(item: &WorkItem) -> OwnedFulfillmentTask<'_> {
    OwnedFulfillmentTask {
        work_item_type: item.work_item_type,
        business_object_type: &item.business_object_type,
        business_object_id: &item.business_object_id,
        owner_user_id: item.owner_user_id.as_deref(),
    }
}

impl<A: WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkbenchReadService<A> {
    /// 为完整任务列表重新读取对象并执行参与权过滤。
    ///
    /// # 参数
    /// * `items` - 待授权的工作项
    /// * `access` - 当前操作人的队列访问事实
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回通过对象授权的任务字段；未授权的任务被略去。
    ///
    /// # 错误
    /// 对象事实读取或审批授权失败时返回对应错误。
    pub(super) async fn authorized_fields_for_items(
        &self,
        items: Vec<WorkItem>,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemFields>> {
        let keys = items
            .iter()
            .filter_map(|item| {
                object_policy(item.work_item_type, &item.business_object_type)
                    .map(|policy| (policy.object_kind, item.business_object_id.clone()))
            })
            .collect::<HashSet<_>>();
        let mut facts = self.load_object_facts(&keys, executor).await?;
        let original = facts.clone();
        let ordinary_keys = items
            .iter()
            .filter(|item| {
                !item.owns_procurement_task(&access.actor_id)
                    && item.work_item_type != WorkItemType::SalesInquiryProcessing
            })
            .filter_map(|item| {
                object_policy(item.work_item_type, &item.business_object_type)
                    .map(|policy| (policy.object_kind, item.business_object_id.clone()))
            })
            .collect::<HashSet<_>>();
        facts.retain(|key, _| ordinary_keys.contains(key));
        self.filter_order_access_keeping_owned_fulfillment(
            &access.actor_id,
            items.iter().map(owned_fulfillment_task_item),
            &mut facts,
            executor,
        )
        .await?;
        let mut fields = Vec::new();
        for item in items {
            if item.work_item_type.is_document_approval() {
                if let Some(authorized) =
                    self.authorize_approval_item(&item, access, &original, executor).await?
                {
                    fields.push(authorized);
                }
            } else if let Some(authorized) = procurement_item_fields(&item, access, &original)
                .or_else(|| authorized_item_fields(item, access, &facts))
            {
                fields.push(authorized);
            }
        }
        Ok(fields)
    }
}
