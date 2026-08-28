//! W06 客户验收登记任务的原子责任编排。
//!
//! 发货或交付确认后按销售单聚合形成唯一开放任务；验收正式命令只能由当前
//! 具体责任人执行。当前可验收交付全部登记后完成任务，冲正或后续新交付再
//! 形成新的开放任务，历史终态保持不变。

use database::{AccessControlExt, Executor, SalesOrderExt, WorkItemExt};
use entities::ids::SalesOrderId;
use entities::sales_order::{BusinessType, SalesOrder};
use entities::work_item::{
    AssignmentSource, AvailableWorkItemAccount, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use entities::{Permission, PermissionSet};
use id_generator::next_id;

use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

const OBJECT_TYPE: &str = "sales_order";
const OWNER_ROLE: &str = "sales_order_owner";

/// 客户验收责任本次形成的业务原因。
#[derive(Debug, Clone, Copy)]
pub(crate) enum CustomerAcceptanceTaskReason {
    /// 发货或交付已经形成可验收事实。
    DeliveryAvailable,
    /// 已过账验收被冲正，重新释放可验收事实。
    ReopenedByReversal,
}

impl CustomerAcceptanceTaskReason {
    fn code(self) -> &'static str {
        match self {
            Self::DeliveryAvailable => "CUSTOMER_ACCEPTANCE_REQUIRED",
            Self::ReopenedByReversal => "CUSTOMER_ACCEPTANCE_REOPENED_BY_REVERSAL",
        }
    }
}

/// 为销售单当前可验收交付建立唯一开放验收任务。
pub(crate) async fn ensure_customer_acceptance_task(
    db: &mongodb::Database,
    sales_order_id: &SalesOrderId,
    reason: CustomerAcceptanceTaskReason,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let order = load_goods_service_order(db, sales_order_id, executor).await?;
    let existing = open_customer_acceptance_tasks(db, sales_order_id, executor).await?;
    match existing.as_slice() {
        [task] => {
            ensure_task_identity(task, &order)?;
            return Ok(task.clone());
        }
        [] => {}
        _ => {
            return Err(Error::BusinessLogicError(
                "当前销售单存在多个开放客户验收任务，请联系管理员处理后重试".to_string(),
            ));
        }
    }

    create_customer_acceptance_task(db, &order, reason, executor).await
}

/// 为验收正式命令加载当前任务并校验责任、任务身份和可选乐观锁。
pub(crate) async fn prepare_customer_acceptance_task_command(
    db: &mongodb::Database,
    sales_order_id: &SalesOrderId,
    actor_id: &str,
    work_item_id: Option<&str>,
    expected_task_version: Option<u64>,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let mut task = ensure_customer_acceptance_task(
        db,
        sales_order_id,
        CustomerAcceptanceTaskReason::DeliveryAvailable,
        executor,
    )
    .await?;
    ensure_command_identity(&task, work_item_id, expected_task_version)?;
    ensure_current_owner_execution_access(db, &task, actor_id, executor).await?;
    task.record_activity(actor_id, entities::common::time::Instant::now())?;
    Ok(task)
}

/// 按过账后的剩余可验收事实持久化任务活动或完成事实。
pub(crate) async fn persist_customer_acceptance_task_after_posting(
    db: &mongodb::Database,
    mut task: WorkItem,
    actor_id: &str,
    has_remaining_eligible: bool,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !has_remaining_eligible {
        task.complete_by_domain_command(actor_id, entities::common::time::Instant::now())?;
    }
    db.work_items().update(&mut task, executor).await?;
    Ok(())
}

async fn create_customer_acceptance_task(
    db: &mongodb::Database,
    order: &SalesOrder,
    reason: CustomerAcceptanceTaskReason,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let owner_user_id = order.stable.created_by.clone();
    let rbac = crate::iam::shared_rbac_service(db.clone());
    ensure_customer_acceptance_owner_eligible(db, &rbac, &owner_user_id, executor).await?;
    let task = WorkItem::new_with_responsibility_key(
        entities::ids::WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::CustomerAcceptanceRegistration,
            business_object_type: OBJECT_TYPE.to_string(),
            business_object_id: order.base.id.clone(),
            subject_version: order.base.version.to_string(),
            owner_role: OWNER_ROLE.to_string(),
            owner_organization_id: order.settlement_party_id.to_string(),
            owner_user_id,
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some(reason.code().to_string()),
            impact_summary: Some("不登记则销售明细不能完成履约".to_string()),
        },
        responsibility_key(&order.base.id),
    )?;
    db.work_items().create(&task, executor).await?;
    Ok(task)
}

async fn load_goods_service_order(
    db: &mongodb::Database,
    sales_order_id: &SalesOrderId,
    executor: &mut dyn Executor,
) -> Result<SalesOrder> {
    let order = db
        .sales_orders()
        .find_by_id(sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在，无法形成客户验收责任".to_string()))?;
    if order.business_type != BusinessType::GoodsService {
        return Err(Error::BusinessLogicError(
            "卡券销售单不形成客户验收登记任务".to_string(),
        ));
    }
    Ok(order)
}

async fn open_customer_acceptance_tasks(
    db: &mongodb::Database,
    sales_order_id: &SalesOrderId,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    Ok(db
        .work_items()
        .list_active_by_object(OBJECT_TYPE, sales_order_id.as_ref(), executor)
        .await?
        .into_iter()
        .filter(|item| item.work_item_type == WorkItemType::CustomerAcceptanceRegistration)
        .collect())
}

fn ensure_task_identity(task: &WorkItem, order: &SalesOrder) -> Result<()> {
    let matches = task.work_item_type == WorkItemType::CustomerAcceptanceRegistration
        && task.matches_business_object(OBJECT_TYPE, &order.base.id)
        && task.owner_role == OWNER_ROLE
        && task.owner_organization_id == order.settlement_party_id.to_string()
        && task.responsibility_key() == Some(responsibility_key(&order.base.id).as_str());
    if matches {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "客户验收任务责任身份与销售单不一致，请联系管理员修复后重试".to_string(),
    ))
}

fn ensure_command_identity(
    task: &WorkItem,
    work_item_id: Option<&str>,
    expected_task_version: Option<u64>,
) -> Result<()> {
    if work_item_id.is_some() != expected_task_version.is_some() {
        return Err(Error::ValidationError(
            "客户验收任务主键和期望版本必须同时提供".to_string(),
        ));
    }
    if work_item_id.is_some_and(|id| id != task.base.id) {
        return Err(Error::ConflictError(
            "客户验收任务已变化，请刷新后重试".to_string(),
        ));
    }
    if expected_task_version.is_some_and(|version| version != task.base.version) {
        return Err(Error::ConflictError(
            "客户验收任务版本已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

fn responsibility_key(sales_order_id: &str) -> String {
    format!("sales_order:{sales_order_id}:customer_acceptance")
}

async fn ensure_customer_acceptance_owner_eligible(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    owner_user_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let required_permissions = WorkItemType::CustomerAcceptanceRegistration
        .customer_acceptance_execution_permissions(OBJECT_TYPE)
        .expect("客户验收登记对象权限合同必须存在");
    let account = db
        .accounts()
        .find_work_item_account(owner_user_id, executor)
        .await?
        .ok_or_else(|| Error::BusinessLogicError("销售责任人账号不存在，无法形成客户验收任务".to_string()))?;
    AvailableWorkItemAccount::from_account(&account)
        .map_err(|_| Error::BusinessLogicError("销售责任人账号不可用，无法形成客户验收任务".to_string()))?;
    let granted = PermissionSet::new(rbac.permissions(account.kind, owner_user_id).await?);
    let required = PermissionSet::new(
        required_permissions
            .iter()
            .map(|code| Permission::parse(code).expect("固定客户验收操作权限必须合法")),
    );
    if granted.covers(&required) {
        return Ok(());
    }
    Err(Error::BusinessLogicError(
        "销售责任人缺少客户验收完整操作权限，请先调整角色或责任配置".to_string(),
    ))
}

async fn ensure_current_owner_execution_access(
    db: &mongodb::Database,
    task: &WorkItem,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let rbac = crate::iam::shared_rbac_service(db.clone());
    ensure_customer_acceptance_owner_eligible(db, &rbac, actor_id, executor).await?;
    if task.owner_user_id.as_deref() == Some(actor_id) {
        return Ok(());
    }
    Err(Error::Forbidden("只有当前销售责任人可以登记客户验收".to_string()))
}

#[cfg(test)]
mod tests {
    use entities::ids::WorkItemId;

    use super::*;

    fn task() -> WorkItem {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-1"),
            WorkItemData {
                work_item_type: WorkItemType::CustomerAcceptanceRegistration,
                business_object_type: OBJECT_TYPE.to_string(),
                business_object_id: "so-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: OWNER_ROLE.to_string(),
                owner_organization_id: "party-1".to_string(),
                owner_user_id: "sales-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("CUSTOMER_ACCEPTANCE_REQUIRED".to_string()),
                impact_summary: None,
            },
            responsibility_key("so-1"),
        )
        .expect("客户验收任务应合法")
    }

    #[test]
    fn command_context_requires_matching_task_and_version() {
        let task = task();
        assert!(ensure_command_identity(&task, None, None).is_ok());
        assert!(ensure_command_identity(&task, Some("wi-1"), Some(task.base.version)).is_ok());
        assert!(ensure_command_identity(&task, Some("wi-2"), Some(task.base.version)).is_err());
        assert!(ensure_command_identity(&task, Some("wi-1"), Some(task.base.version + 1)).is_err());
        assert!(ensure_command_identity(&task, Some("wi-1"), None).is_err());
    }
}
