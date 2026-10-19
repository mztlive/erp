//! 应付与申请占用变化后，在同一事务同步采购经办人的付款安排责任。
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_core::money::Amount;
use erp_procurement::entity::payment_arrangement::PaymentArrangementState;
use erp_read_models::purchase_center::payment_arrangement::{PaymentArrangementFacts, arrangement_facts};
use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};
use erp_workflow::{WorkItem, WorkItemExt, WorkItemType};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 按余额同步付款安排；原申请释放时取消旧跟进日期，重新提醒。
/// # 参数
/// 数据库、采购单、是否由申请释放触发、调用方唯一事务执行器。
/// # 返回
/// 创建、刷新或完成开放任务；重复同步不重复开立。
/// # 错误
/// 采购负责人缺失、任务合同或仓储写入失败时返回错误，交由原事务回滚。
pub(crate) async fn sync_payment_arrangement(
    db: &Database,
    id: &str,
    released: bool,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut facts = arrangement_facts(db, id, executor).await?;
    let now = Instant::now();
    if !facts.state.needs_task() {
        if let Some(mut task) = facts.task {
            task.complete_payment_arrangement(now)?;
            db.work_items().update(&mut task, executor).await?;
        }
        return Ok(());
    }
    let mut summary = arrangement_summary(&facts);
    if released {
        summary.push_str("；原付款申请占用已释放，请重新核对付款安排");
    }
    if let Some(mut task) = facts.task.take() {
        let previous = task.clone();
        task.update_impact_summary(Some(summary))?;
        if let Some(revision) = &facts.order.stable.current_revision_id {
            task.subject_version = revision.clone();
        }
        if released {
            task.due_at = None;
        }
        if previous != task {
            db.work_items().update(&mut task, executor).await?;
        }
    } else {
        let task = arrangement_task(&facts, summary)?;
        db.work_items().create(&task, executor).await?;
    }
    Ok(())
}
/// 余额与阻断摘要不包含其他采购或内部机制信息。
fn arrangement_summary(facts: &PaymentArrangementFacts) -> String {
    let suffix = if facts.state == PaymentArrangementState::Blocked {
        "销售变更影响尚未处理，暂停申请和付款；请先处理采购依据"
    } else {
        "请核对付款条件后申请采购付款，或安排后续跟进"
    };
    let available = facts
        .account
        .as_ref()
        .map(|account| account.open_total.checked_sub(account.payment_request_reserved_total))
        .unwrap_or_else(Amount::zero);
    format!("{}：可申请 {} 元；{}", facts.order.purchase_no, available, suffix)
}
/// 任务归属采购经办人，不归属最终审批人。
fn arrangement_task(facts: &PaymentArrangementFacts, summary: String) -> Result<WorkItem> {
    let order = &facts.order;
    let owner = order
        .owner_user_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Error::ConflictError("采购负责人缺失，无法安排付款跟进".into()))?;
    WorkItem::new_with_responsibility_key(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::PurchasePaymentArrangement,
            business_object_type: "purchase_order".into(),
            business_object_id: order.base.id.clone(),
            subject_version: order
                .stable
                .current_revision_id
                .clone()
                .ok_or_else(|| Error::ConflictError("采购生效版本缺失".into()))?,
            owner_role: "role-procurement".into(),
            owner_organization_id: order.business_org_unit_id.clone(),
            owner_user_id: owner.into(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some("PURCHASE_PAYMENT_ARRANGEMENT_REQUIRED".into()),
            impact_summary: Some(summary),
        },
        format!("purchase_payment_arrangement:{}", order.base.id),
    )
    .map_err(Error::from)
}
