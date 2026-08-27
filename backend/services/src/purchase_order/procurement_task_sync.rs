//! 统一供给覆盖变化后的供给分配任务范围校验与生命周期同步。

use std::collections::{HashMap, HashSet};

use database::{Executor, SalesOrderExt, WorkItemExt};
use entities::common::time::Instant;
use entities::ids::{SalesOrderId, WorkItemId};
use entities::money::Quantity;
use entities::sales_order::SalesOrder;
use entities::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use id_generator::next_id;
use rust_decimal::Decimal;

use super::coverage::load_sales_procurement_coverage;
use crate::errors::{Error, Result};

/// 加载当前账号可执行的开放供给分配任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `work_item_id` - 客户端从采购创建依据取得的任务 ID
/// * `sales_order_id` - 本次采购依据所属销售单
/// * `actor_id` - 当前已认证账号
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回业务对象、任务类型、开放状态、责任人和冻结行范围均匹配的任务。
///
/// # 错误
/// 任务不存在、不是当前账号开放任务、销售单不匹配、责任范围缺失或仓储失败时返回错误。
///
/// # 关键业务约束
/// 采购创建权限不能只依赖页面筛选；事务内必须重新读取任务事实并校验具体责任行。
pub(super) async fn load_owned_open_procurement_task(
    db: &mongodb::Database,
    work_item_id: &str,
    sales_order_id: &SalesOrderId,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let item = db
        .work_items()
        .find_by_id(work_item_id, executor)
        .await?
        .ok_or_else(procurement_task_not_found)?;
    validate_procurement_task_access(&item, sales_order_id, actor_id)?;
    Ok(item)
}

/// 校验供给分配任务的业务对象、当前责任人、开放状态与冻结范围。
///
/// # 参数
/// * `item` - 已按客户端任务 ID 读取的持久化任务
/// * `sales_order_id` - 本次采购依据所属销售单
/// * `actor_id` - 当前已认证账号
///
/// # 返回
/// 任务属于当前账号且仍可处理时返回 `Ok(())`。
///
/// # 错误
/// 伪造、错单、他人或已关闭任务返回隐藏式未找到；当前责任人的任务已由并发请求完成时返回冲突；冻结范围损坏时返回冲突。
///
/// # 关键业务约束
/// 只有保留为终态任务当前责任人的账号可以获知并发完成事实，历史责任人和其他账号不得据此探测任务存在性。
fn validate_procurement_task_access(
    item: &WorkItem,
    sales_order_id: &SalesOrderId,
    actor_id: &str,
) -> Result<()> {
    if item.work_item_type != WorkItemType::ProcurementOrderCreation
        || item.business_object_type != "sales_order"
        || item.business_object_id != sales_order_id.to_string()
        || item.owner_user_id.as_deref() != Some(actor_id)
    {
        return Err(procurement_task_not_found());
    }
    if item.status == WorkItemStatus::Completed {
        return Err(procurement_quantity_changed());
    }
    if item.status != WorkItemStatus::Open {
        return Err(procurement_task_not_found());
    }
    if item.responsibility_key().is_none() || item.responsibility_scope_ids().is_empty() {
        return Err(Error::ConflictError("供给分配任务缺少冻结责任范围".to_string()));
    }
    Ok(())
}

/// 按销售单当前统一供给覆盖同步全部供给分配任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `sales_order_id` - 采购覆盖发生变化的来源销售单
/// * `executor` - 与采购写入共用的数据访问执行器；写链路必须传入同一事务会话
///
/// # 返回
/// 开放任务摘要、自动完成和释放后新任务均同步成功时返回 `Ok(())`。
///
/// # 错误
/// 销售单、采购覆盖、任务范围或仓储写入不一致时返回错误。
///
/// # 关键业务约束
/// 开放任务归零后进入不可逆终态；历史终态范围重新出现剩余量时创建新任务，不重开旧任务。
pub(crate) async fn sync_procurement_tasks_for_sales_order(
    db: &mongodb::Database,
    sales_order_id: &SalesOrderId,
    executor: &mut dyn Executor,
) -> Result<()> {
    let order = db
        .sales_orders()
        .find_by_id(sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    let coverage = load_sales_procurement_coverage(db, &order, executor).await?;
    let remaining_by_line = coverage
        .lines
        .into_iter()
        .map(|line| {
            (
                line.revision_line.sales_order_line_id.to_string(),
                line.summary.remaining_quantity,
            )
        })
        .collect::<HashMap<_, _>>();
    let tasks = db
        .work_items()
        .list_procurement_by_sales_order_newest_first(sales_order_id.as_ref(), executor)
        .await?;
    synchronize_tasks(
        db,
        &order,
        &coverage.revision.base.id,
        &remaining_by_line,
        tasks,
        executor,
    )
    .await
}

/// 根据同一销售单的当前剩余量更新任务并补建释放任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 来源销售单
/// * `subject_version` - 当前销售修订 ID
/// * `remaining_by_line` - 稳定销售行到当前剩余数量的映射
/// * `tasks` - 该销售单全部供给分配任务，按更新时间倒序
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 同步完成返回 `Ok(())`。
///
/// # 错误
/// 任务责任键、范围、负责人或仓储写入不一致时返回错误。
async fn synchronize_tasks(
    db: &mongodb::Database,
    order: &SalesOrder,
    subject_version: &str,
    remaining_by_line: &HashMap<String, Quantity>,
    tasks: Vec<WorkItem>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut open_keys = HashSet::new();
    let mut terminal_candidates = Vec::new();
    for mut task in tasks {
        let key = task
            .responsibility_key()
            .ok_or_else(|| Error::ConflictError("供给分配任务缺少责任键".to_string()))?
            .to_string();
        let remaining = remaining_for_scope(task.responsibility_scope_ids(), remaining_by_line)?;
        if task.status == WorkItemStatus::Open {
            if !open_keys.insert(key) {
                return Err(Error::ConflictError(
                    "同一责任范围存在多条开放供给分配任务".to_string(),
                ));
            }
            if remaining.to_decimal().is_zero() {
                task.complete_when_requirement_satisfied(Instant::now())
                    .map_err(Error::Logic)?;
                db.work_items().update(&mut task, executor).await?;
            } else {
                let impact =
                    procurement_impact_summary(order, task.responsibility_scope_ids().len(), remaining);
                if task.impact_summary.as_deref() != Some(impact.as_str()) {
                    task.update_impact_summary(Some(impact)).map_err(Error::Logic)?;
                    db.work_items().update(&mut task, executor).await?;
                }
            }
        } else if !remaining.to_decimal().is_zero() && remaining.to_decimal().is_sign_positive() {
            terminal_candidates.push((key, task, remaining));
        }
    }

    for (key, task, remaining) in terminal_candidates {
        if open_keys.contains(&key) {
            continue;
        }
        let impact = procurement_impact_summary(order, task.responsibility_scope_ids().len(), remaining);
        let new_task = task
            .successor_for_released_requirement(
                WorkItemId::new(next_id()),
                subject_version.to_string(),
                Some(impact),
            )
            .map_err(Error::Logic)?;
        db.work_items().create(&new_task, executor).await?;
        open_keys.insert(key);
    }
    Ok(())
}

/// 汇总冻结责任范围内的当前剩余数量。
///
/// # 参数
/// * `scope_ids` - 任务冻结的稳定销售行 ID
/// * `remaining_by_line` - 当前销售版本逐行剩余数量
///
/// # 返回
/// 返回范围内仍存在销售行的剩余数量之和；当前版本已移除的行按零处理。
///
/// # 错误
/// 责任范围为空或数量汇总无法构造为领域数量时返回错误。
fn remaining_for_scope(
    scope_ids: &[String],
    remaining_by_line: &HashMap<String, Quantity>,
) -> Result<Quantity> {
    if scope_ids.is_empty() {
        return Err(Error::ConflictError("供给分配任务责任范围为空".to_string()));
    }
    let total = scope_ids.iter().fold(Decimal::ZERO, |sum, line_id| {
        sum + remaining_by_line
            .get(line_id)
            .copied()
            .map(Quantity::to_decimal)
            .unwrap_or(Decimal::ZERO)
    });
    Quantity::try_from(total).map_err(Error::Logic)
}

/// 生成供给分配任务当前影响摘要。
///
/// # 参数
/// * `order` - 来源销售单
/// * `line_count` - 冻结责任行数
/// * `remaining` - 当前责任范围剩余数量
///
/// # 返回
/// 返回包含销售单号、责任行数和剩余数量的稳定中文摘要。
///
/// # 错误
/// 无。
fn procurement_impact_summary(order: &SalesOrder, line_count: usize, remaining: Quantity) -> String {
    format!(
        "销售单 {} 的 {line_count} 行待分配供给，剩余数量 {remaining}",
        order.order_no
    )
}

/// 返回采购剩余数量已由并发命令推进后的稳定冲突错误。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回 HTTP 409 对应的可刷新重试错误。
///
/// # 错误
/// 无。
fn procurement_quantity_changed() -> Error {
    Error::ConflictError("可分配供给数量已更新，请刷新后重试".to_string())
}

/// 返回不泄露他人任务存在性的统一未找到错误。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回采购任务不可用的 `NotFound` 错误。
///
/// # 错误
/// 无。
fn procurement_task_not_found() -> Error {
    Error::NotFound("供给分配任务不存在或不可处理".to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use entities::common::time::Instant;
    use entities::ids::{SalesOrderId, WorkItemId};
    use entities::money::Quantity;
    use entities::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType};

    use super::{remaining_for_scope, validate_procurement_task_access};
    use crate::errors::Error;

    /// 构造带稳定销售行范围的供给分配任务。
    fn procurement_task(owner_user_id: &str) -> WorkItem {
        WorkItem::new_with_responsibility_scope(
            WorkItemId::new("task-1"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: "sales_order".to_string(),
                business_object_id: "sales-1".to_string(),
                subject_version: "revision-1".to_string(),
                owner_role: "procurement".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: owner_user_id.to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("SALES_ORDER_EFFECTIVE".to_string()),
                impact_summary: None,
            },
            "procurement:user-1",
            vec!["line-1".to_string()],
        )
        .unwrap()
    }

    #[test]
    fn completed_current_owner_task_reports_quantity_conflict() {
        let mut item = procurement_task("user-1");
        item.complete_when_requirement_satisfied(Instant::from_unix_secs(10))
            .unwrap();

        let error =
            validate_procurement_task_access(&item, &SalesOrderId::new("sales-1"), "user-1").unwrap_err();

        assert!(matches!(
            error,
            Error::ConflictError(message) if message == "可分配供给数量已更新，请刷新后重试"
        ));
    }

    #[test]
    fn another_actor_cannot_probe_open_or_completed_task() {
        let mut item = procurement_task("user-1");
        assert!(matches!(
            validate_procurement_task_access(&item, &SalesOrderId::new("sales-1"), "user-2"),
            Err(Error::NotFound(_))
        ));

        item.complete_when_requirement_satisfied(Instant::from_unix_secs(10))
            .unwrap();
        assert!(matches!(
            validate_procurement_task_access(&item, &SalesOrderId::new("sales-1"), "user-2"),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn previous_owner_cannot_probe_task_after_reassignment_and_completion() {
        let mut item = procurement_task("user-1");
        item.reassign("user-2", Instant::from_unix_secs(5)).unwrap();
        item.complete_when_requirement_satisfied(Instant::from_unix_secs(10))
            .unwrap();

        assert!(matches!(
            validate_procurement_task_access(&item, &SalesOrderId::new("sales-1"), "user-1"),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn task_for_another_sales_order_remains_hidden() {
        let item = procurement_task("user-1");

        assert!(matches!(
            validate_procurement_task_access(&item, &SalesOrderId::new("sales-2"), "user-1"),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn scope_remaining_sums_current_lines_and_ignores_removed_lines() {
        let remaining = HashMap::from([
            ("line-a".to_string(), Quantity::from_str("4").unwrap()),
            ("line-b".to_string(), Quantity::from_str("2.5").unwrap()),
        ]);
        let total = remaining_for_scope(
            &["line-a".to_string(), "line-b".to_string(), "removed".to_string()],
            &remaining,
        )
        .unwrap();
        assert_eq!(total, Quantity::from_str("6.5").unwrap());
    }

    #[test]
    fn scope_remaining_rejects_empty_responsibility_scope() {
        assert!(remaining_for_scope(&[], &HashMap::new()).is_err());
    }
}
