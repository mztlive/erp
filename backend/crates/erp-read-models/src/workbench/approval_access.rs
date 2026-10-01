//! 审批队列按精确任务责任授权，与普通业务对象范围隔离。
use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use erp_workflow::service::approval::execution::runtime_service::{
    approval_task_readable_with_executor, approval_tasks_readable_with_executor,
};
use erp_workflow::{WorkItem, WorkItemExt, WorkItemRow, WorkflowAuthorizationPort};
use persistence_core::Executor;

use super::access::{ActorAccess, authorized_fields};
use super::dto::WorkItemFields;
use super::facts::{apply_object_display, object_policy};
use super::{WorkbenchObjectFactMap, WorkbenchReadService};
use crate::Result;

impl<A: WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkbenchReadService<A> {
    /// 审批读取先证明任务链，其他任务仍执行原业务范围过滤。
    ///
    /// # 参数
    /// * `rows` - 原顺序候选任务行
    /// * `access` - 当前责任队列身份快照
    /// * `facts` - 本批对象事实，调用后只保留授权结果所需事实
    /// * `executor` - 调用方快照执行器
    ///
    /// # 返回
    /// 返回按原候选顺序授权的任务字段。
    ///
    /// # 错误
    /// 对象范围、运行责任链或权限证明失败时返回原错误。
    pub(super) async fn authorize_queue_rows(
        &self,
        rows: Vec<WorkItemRow>,
        access: &ActorAccess,
        facts: &mut WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItemFields>> {
        let original = approval_facts_for_rows(&rows, facts);
        self.filter_order_access_keeping_owned_fulfillment(
            &access.actor_id,
            rows.iter().map(super::query::owned_fulfillment_task),
            facts,
            executor,
        )
        .await?;
        let ids = rows
            .iter()
            .filter(|row| row.work_item_type.is_document_approval())
            .map(|row| row.id.clone())
            .collect::<Vec<_>>();
        let tasks = self
            .db
            .work_items()
            .list_active_by_ids(&ids, executor)
            .await?
            .into_iter()
            .map(|item| (item.base.id.clone(), item))
            .collect::<HashMap<_, _>>();
        let ordinary = ordinary_fields_for_rows(&rows, access, facts);
        let readable = self.readable_approval_ids(&rows, &tasks, access, executor).await?;
        let mut result = Vec::new();
        for (row, fields) in rows.into_iter().zip(ordinary) {
            if !row.work_item_type.is_document_approval() {
                result.extend(fields);
                continue;
            }
            let Some(item) = tasks.get(&row.id).filter(|item| item.base.version == row.version) else {
                continue;
            };
            if readable.contains(&item.base.id)
                && let Some(fields) = approval_item_fields(item, &original)
            {
                if let Some(policy) = object_policy(item.work_item_type, &item.business_object_type) {
                    let key = (policy.object_kind, item.business_object_id.clone());
                    if let Some(fact) = original.get(&key) {
                        facts.insert(key, fact.clone());
                    }
                }
                result.push(fields);
            }
        }
        Ok(result)
    }

    /// 同批账号只装载一次，运行资格仍逐任务证明并批量读取必要责任链。
    ///
    /// # 参数
    /// * `rows` / `tasks` - 原候选顺序与已批量加载任务
    /// * `access` - 当前账号快照
    /// * `executor` - 当前读取执行器
    ///
    /// # 返回
    /// 返回精确任务版本匹配且运行可读的任务 ID 集合。
    ///
    /// # 错误
    /// 账号及审批运行读取失败时返回原错误；账号不存在保持审批空集。
    async fn readable_approval_ids(
        &self,
        rows: &[WorkItemRow],
        tasks: &HashMap<String, WorkItem>,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        let items = rows
            .iter()
            .filter(|row| row.work_item_type.is_document_approval())
            .filter_map(|row| tasks.get(&row.id).filter(|item| item.base.version == row.version))
            .collect::<Vec<_>>();
        if items.is_empty() {
            return Ok(HashSet::new());
        }
        let Some(account) = self.auth.load_account(&access.actor_id, executor).await? else {
            return Ok(HashSet::new());
        };
        let actor = AuditActor::new(account.id, account.login_account, account.kind);
        let readable =
            approval_tasks_readable_with_executor(&self.db, &self.auth, &actor, &items, executor).await?;
        Ok(items
            .into_iter()
            .zip(readable)
            .filter(|(_, allowed)| *allowed)
            .map(|(item, _)| item.base.id.clone())
            .collect())
    }

    /// 已证明的审批任务只装配其对应提交版本，不要求普通销售或采购详情范围。
    ///
    /// # 参数
    /// * `item` - 当前审批任务
    /// * `access` - 当前身份快照
    /// * `facts` - 本次加载的对象事实
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回通过原运行责任链证明的展示字段，拒绝时返回 None。
    ///
    /// # 错误
    /// 账号、权限及冻结运行事实读取失败时保持原错误。
    pub(super) async fn authorize_approval_item(
        &self,
        item: &WorkItem,
        access: &ActorAccess,
        facts: &WorkbenchObjectFactMap,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItemFields>> {
        let Some(account) = self.auth.load_account(&access.actor_id, executor).await? else {
            return Ok(None);
        };
        let actor = AuditActor::new(account.id, account.login_account, account.kind);
        if !approval_task_readable_with_executor(&self.db, &self.auth, &actor, item, executor).await? {
            return Ok(None);
        }
        Ok(approval_item_fields(item, facts))
    }
}

/// 仅保存本批审批行实际需要的原始对象事实，避免克隆其他任务事实。
///
/// # 参数
/// * `rows` - 当前候选行
/// * `facts` - 未经普通业务范围过滤的对象事实
///
/// # 返回
/// 返回审批行对应的去重对象事实；无审批行时保持空映射。
///
/// # 错误
/// 无；未登记类型与缺失事实保持不可见。
fn approval_facts_for_rows(rows: &[WorkItemRow], facts: &WorkbenchObjectFactMap) -> WorkbenchObjectFactMap {
    let mut result = WorkbenchObjectFactMap::new();
    for row in rows.iter().filter(|row| row.work_item_type.is_document_approval()) {
        let Some(policy) = object_policy(row.work_item_type, &row.business_object_type) else {
            continue;
        };
        let key = (policy.object_kind, row.business_object_id.clone());
        if let Some(fact) = facts.get(&key) {
            result.entry(key).or_insert_with(|| fact.clone());
        }
    }
    result
}

/// 在恢复审批显示事实之前固定普通任务的原对象范围判定。
///
/// # 参数
/// * `rows` - 原候选顺序
/// * `access` - 责任队列身份快照
/// * `facts` - 已过滤的普通对象事实
///
/// # 返回
/// 返回与原行位置对应的普通任务字段，审批行或拒绝行占 None。
///
/// # 错误
/// 无。
fn ordinary_fields_for_rows(
    rows: &[WorkItemRow],
    access: &ActorAccess,
    facts: &WorkbenchObjectFactMap,
) -> Vec<Option<WorkItemFields>> {
    rows.iter()
        .map(|row| {
            if row.work_item_type.is_document_approval() {
                None
            } else {
                authorized_fields(vec![row.clone()], access, facts).into_iter().next()
            }
        })
        .collect()
}

/// 对已证明运行可读的任务装配冻结版本对应的对象展示。
///
/// # 参数
/// * `item` - 已授权审批任务
/// * `facts` - 本次读取的原始对象事实
///
/// # 返回
/// 返回展示字段；未登记类型或缺失事实保持 None。
///
/// # 错误
/// 无。
fn approval_item_fields(item: &WorkItem, facts: &WorkbenchObjectFactMap) -> Option<WorkItemFields> {
    let policy = object_policy(item.work_item_type, &item.business_object_type)?;
    let fact = facts.get(&(policy.object_kind, item.business_object_id.clone()))?;
    let mut fields = WorkItemFields::from(item.clone());
    apply_object_display(&mut fields, fact);
    Some(fields)
}

#[cfg(test)]
mod tests {
    use bpm::ApprovalNodeExecutionId;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;
    use erp_identity::Permission;
    use erp_workflow::entity::work_item::{
        AssignmentSource, DocumentApprovalWorkItemData, WorkItemData, WorkItemPriority, WorkItemType,
    };
    use erp_workflow::ports::{ObjectFact, ObjectKind};

    use super::super::facts::WorkbenchObjectFact;
    use super::*;

    /// 经正式责任范围构造器生成具有稳定销售行范围的供给分配投影。
    fn row(id: &str) -> WorkItemRow {
        let item = WorkItem::new_with_responsibility_scope(
            WorkItemId::new(id),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: "sales_order".into(),
                business_object_id: "sales-1".into(),
                subject_version: "1".into(),
                owner_role: "procurement".into(),
                owner_organization_id: "org-1".into(),
                owner_user_id: "actor".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("SALES_ORDER_EFFECTIVE".into()),
                impact_summary: None,
            },
            "procurement:actor",
            vec!["sales-line-1".into()],
        )
        .unwrap();
        serde_json::from_value(serde_json::to_value(item).unwrap()).unwrap()
    }

    /// 经专用审批构造器生成同一销售对象的合法审批投影。
    fn approval_row(id: &str) -> WorkItemRow {
        let item = WorkItem::new_document_approval(
            WorkItemId::new(id),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new("execution-1"),
                business_object_type: "sales_order".into(),
                business_object_id: "sales-1".into(),
                subject_version: "1".into(),
                owner_role: "sales_order_approver".into(),
                owner_organization_id: "org-1".into(),
                owner_user_id: "actor".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .unwrap();
        serde_json::from_value(serde_json::to_value(item).unwrap()).unwrap()
    }

    /// 原始审批事实只克隆实际审批键，普通任务与重复键不放大保存集合。
    #[test]
    fn approval_queue_fact_snapshot_keeps_only_distinct_approval_objects() {
        let approval = approval_row("approval");
        let ordinary = row("ordinary");
        let facts = WorkbenchObjectFactMap::from([
            (
                (ObjectKind::SalesOrder, "sales-1".into()),
                WorkbenchObjectFact::from_authority(ObjectFact::new("sales-1", "SO-1", "actor")),
            ),
            (
                (ObjectKind::PurchaseOrder, "purchase-1".into()),
                WorkbenchObjectFact::from_authority(ObjectFact::new("purchase-1", "PO-1", "actor")),
            ),
        ]);
        let saved = approval_facts_for_rows(&[approval.clone(), ordinary.clone(), approval], &facts);
        assert_eq!(saved.len(), 1);
        assert!(saved.contains_key(&(ObjectKind::SalesOrder, "sales-1".into())));
        assert!(approval_facts_for_rows(&[ordinary], &facts).is_empty());
    }

    /// 恢复审批展示事实之后不能重新授予先前已拒绝的普通任务访问权。
    #[test]
    fn approval_queue_ordinary_authorization_is_fixed_before_approval_restore() {
        let ordinary = row("ordinary");
        let approval = approval_row("approval");
        let rows = [approval, ordinary];
        let access = ActorAccess::new("actor".into())
            .with_permissions(vec![Permission::parse("purchase_order:create").unwrap()]);
        let mut filtered = WorkbenchObjectFactMap::new();
        let prepared = ordinary_fields_for_rows(&rows, &access, &filtered);
        assert!(prepared.iter().all(Option::is_none));
        filtered.insert(
            (ObjectKind::SalesOrder, "sales-1".into()),
            WorkbenchObjectFact::from_authority(ObjectFact::new("sales-1", "SO-1", "actor")),
        );
        assert!(ordinary_fields_for_rows(&rows, &access, &filtered)[1].is_some());
        assert!(prepared[1].is_none());
    }
}
