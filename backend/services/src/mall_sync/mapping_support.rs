//! 映射支撑事实读取与核对创建校验（INT-R18/INT-R23/INT-R24）。
//!
//! 正式责任任务使用精确有界读取（零/一/多分别对应缺失/唯一/损坏）；核对创建
//! 的 ERP 侧引用使用批量装载并保持请求顺序的首错语义；谱系过期使用调用方事务
//! 内的批量 CAS，冲突整体回滚。

use std::collections::{HashMap, HashSet};

use database::{CustomerExt, NoTransaction, SalesOrderExt, WorkItemExt};
use entities::ids::{CustomerAccountId, SalesOrderId};
use entities::mall_sync::MasterMappingTask;
use entities::sales_order::SalesOrder;
use entities::work_item::WorkItem;

use super::dto::{
    CreateMallSalesReconciliationJobRequest, SALES_ORDER_CUSTOMER_MISSING_MESSAGE,
    SALES_ORDER_NOT_FOUND_MESSAGE,
};
use super::prepared_command::preparation_error;
use super::{ensure_mapping_work_item_identity, MallSyncService};
use crate::errors::{Error, Result};

/// 在请求顺序中找首个缺失键（首错语义与旧逐行路径一致）。
///
/// # 参数
/// * `ordered_keys` - 按请求顺序的待校验键（含重复）
/// * `found` - 已装载事实的键集合
///
/// # 返回
/// 返回首个缺失键；全部命中时返回 `None`。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯内存函数，不访问数据库；调用方按返回键映射用户错误。
fn first_missing_key<'a>(ordered_keys: &'a [String], found: &HashSet<String>) -> Option<&'a str> {
    ordered_keys
        .iter()
        .find(|key| !found.contains(key.as_str()))
        .map(String::as_str)
}

/// 将谱系过期冲突映射为版本冲突错误（INT-R24 纯映射）。
///
/// 调用方事务内返回该错误即整体回滚，零部分提交。
///
/// # 参数
/// * `conflicts` - 版本冲突的目标 ID
///
/// # 返回
/// 返回 `ConflictError`，提示谱系已变化需刷新重试。
///
/// # 错误
/// 本身不失败；返回的错误由调用方在事务内抛出以触发回滚。
///
/// # 约束
/// 纯错误映射，不访问数据库；调用方必须保证 `conflicts` 非空。
fn expire_conflicts_error(conflicts: &[String]) -> Error {
    debug_assert!(!conflicts.is_empty());
    Error::ConflictError("当前来源身份谱系已变化，请刷新后重试".to_string())
}

impl MallSyncService {
    /// 按映射任务精确加载唯一正式责任任务（INT-R18）。
    ///
    /// 显式任务 ID 路径保持精确读取与身份校验；无显式 ID 时使用有界精确读取
    ///（至多两条）：零条表示尚无责任，一条为唯一责任，两条即数据损坏失败关闭。
    ///
    /// # 参数
    /// * `task` - 映射任务
    /// * `explicit_work_item_id` - 正式队列携带的任务 ID；`None` 表示按任务查找
    ///
    /// # 返回
    /// 返回唯一正式任务；无责任时返回 `None`。
    ///
    /// # 错误
    /// * `NotFound` - 显式任务不存在
    /// * `ValidationError` - 显式任务与映射任务或责任路由不一致
    /// * `Internal` - 同一映射任务存在多个正式任务，责任事实不唯一
    ///
    /// # 约束
    /// 大于一条即损坏，不回退、不猜测；relation subject 校验仍由调用方执行。
    pub(super) async fn mapping_work_item_for_task(
        &self,
        task: &MasterMappingTask,
        explicit_work_item_id: Option<&str>,
    ) -> Result<Option<WorkItem>> {
        if let Some(work_item_id) = explicit_work_item_id {
            let work_item_id =
                entities::mall_sync::command_preparation::prepare_text(work_item_id, "正式任务ID")
                    .map_err(preparation_error)?;
            let item = self
                .db
                .work_items()
                .find_by_id(&work_item_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("映射正式任务不存在".to_string()))?;
            ensure_mapping_work_item_identity(&item, task)?;
            return Ok(Some(item));
        }
        let mut candidates = self
            .db
            .work_items()
            .find_unique_for_master_mapping_task(&task.base.id, &mut NoTransaction)
            .await?;
        if candidates.len() > 1 {
            return Err(Error::Internal(
                "同一映射任务存在多个正式任务，责任事实不唯一".to_string(),
            ));
        }
        Ok(candidates.pop())
    }

    /// 批量校验核对明细的 ERP 侧引用（INT-R23）。
    ///
    /// 一次批量装载去重后的销售单，再一次批量装载去重后的客户账号；缺失判定
    /// 按请求顺序返回首错。重复订单/客户只查一次，1000 项上限由请求校验保证。
    ///
    /// # 参数
    /// * `req` - 核对作业创建请求
    ///
    /// # 错误
    /// * `NotFound` - 销售单或客户账号不存在（请求顺序首错）
    ///
    /// # 约束
    /// 跨聚合拒绝与用户错误映射仍由 Service 解释；不改变软删除语义。
    pub(super) async fn ensure_erp_sides_exist(
        &self,
        req: &CreateMallSalesReconciliationJobRequest,
    ) -> Result<()> {
        let ordered_order_keys = req
            .items
            .iter()
            .filter_map(|item| item.sales_order_id.clone().map(|id| id.to_string()))
            .collect::<Vec<_>>();
        if ordered_order_keys.is_empty() {
            return Ok(());
        }
        let mut seen_orders = HashSet::new();
        let mut unique_order_ids = Vec::new();
        for key in &ordered_order_keys {
            if seen_orders.insert(key.clone()) {
                unique_order_ids.push(SalesOrderId::new(key));
            }
        }
        let orders = self
            .db
            .sales_orders()
            .find_orders_by_ids(&unique_order_ids, &mut NoTransaction)
            .await?;
        let order_map = orders
            .into_iter()
            .map(|order: SalesOrder| (order.base.id.clone(), order))
            .collect::<HashMap<_, _>>();
        let found_orders = order_map.keys().cloned().collect::<HashSet<_>>();
        if first_missing_key(&ordered_order_keys, &found_orders).is_some() {
            return Err(Error::NotFound(SALES_ORDER_NOT_FOUND_MESSAGE.to_string()));
        }
        let ordered_customer_keys = req
            .items
            .iter()
            .filter_map(|item| item.sales_order_id.clone())
            .map(|id| {
                order_map
                    .get(&id.to_string())
                    .map(|order| order.customer_id.to_string())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        let mut seen_customers = HashSet::new();
        let mut unique_customer_ids = Vec::new();
        for key in &ordered_customer_keys {
            if seen_customers.insert(key.clone()) {
                unique_customer_ids.push(CustomerAccountId::new(key));
            }
        }
        let accounts = self
            .db
            .customer_accounts()
            .find_accounts_by_ids(&unique_customer_ids, &mut NoTransaction)
            .await?;
        let found_customers = accounts
            .into_iter()
            .map(|account| account.base.id.clone())
            .collect::<HashSet<_>>();
        if first_missing_key(&ordered_customer_keys, &found_customers).is_some() {
            return Err(Error::NotFound(SALES_ORDER_CUSTOMER_MISSING_MESSAGE.to_string()));
        }
        Ok(())
    }
}

/// 检查谱系过期批量结果并在冲突时失败关闭（INT-R24 Service 侧门）。
///
/// # 参数
/// * `applied` - CAS 成功的目标 ID（仅用于完整性断言）
/// * `conflicts` - 版本冲突的目标 ID
/// * `expected` - 本次期望过期的目标总数
///
/// # 返回
/// 无冲突时返回 `Ok`；存在冲突时返回版本冲突错误，调用方事务整体回滚。
///
/// # 错误
/// 任一目标版本冲突时返回 `ConflictError`。
///
/// # 约束
/// 纯门控函数，不访问数据库；必须在调用方事务内调用。
pub(super) fn check_expire_outcome(applied: &[String], conflicts: &[String], expected: usize) -> Result<()> {
    if !conflicts.is_empty() {
        return Err(expire_conflicts_error(conflicts));
    }
    debug_assert_eq!(applied.len(), expected);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_expire_outcome, expire_conflicts_error, first_missing_key};
    use std::collections::HashSet;

    #[test]
    fn first_missing_key_reports_request_order_first_miss() {
        let found: HashSet<String> = ["so-1".to_string()].into_iter().collect();
        assert_eq!(
            first_missing_key(&["so-1".to_string(), "so-2".to_string()], &found),
            Some("so-2")
        );
        assert_eq!(
            first_missing_key(&["so-1".to_string(), "so-1".to_string()], &found),
            None
        );
        assert_eq!(first_missing_key(&Vec::<String>::new(), &found), None);
    }

    #[test]
    fn expire_conflicts_error_is_conflict_and_check_gates() {
        let error = expire_conflicts_error(&["target-1".to_string()]);
        assert!(matches!(error, crate::errors::Error::ConflictError(_)));
        assert!(check_expire_outcome(&["t-1".to_string()], &[], 1).is_ok());
        assert!(check_expire_outcome(&[], &["t-1".to_string()], 1).is_err());
        assert!(check_expire_outcome(&[], &[], 0).is_ok());
    }
}
