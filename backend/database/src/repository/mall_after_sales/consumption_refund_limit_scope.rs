//! 原消费退款额度范围快照（INT-R11）。
//!
//! 一次批量取回原消费事实与按 entry 聚合的历史退款净额（`APPLY − REVERSE`），
//! 固定两次有界读取，不随请求分配行数增长为 N+1。全部使用调用方 executor，
//! 不开事务；跨聚合归属与并发额度占用仍由 Service 负责。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use entities::ids::MallConsumptionEntryId;
use entities::mall_after_sales::MallRefundAllocation;
use entities::mall_order::MallConsumptionEntry;
use entities::money::Amount;

use super::super::extensions::MallOrderExt;
use super::{MallAfterSalesRepository, MALL_REFUND_ALLOCATIONS};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `mall_consumption_entries` 集合名（单一来源：`MallOrderExt`）。
const MALL_CONSUMPTION_ENTRIES: &str = <mongodb::Database as MallOrderExt>::MALL_CONSUMPTION_ENTRIES;

/// 原消费退款额度校验所需的持久化事实范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumptionRefundLimitScope {
    /// 已装载的原消费事实（按 ID 索引；请求中缺失的 ID 不出现）。
    pub entries: HashMap<MallConsumptionEntryId, MallConsumptionEntry>,
    /// 各 entry 历史退款净额（`APPLY − REVERSE`）；无历史时不出现，由调用方按精确零处理。
    pub historical_nets: HashMap<MallConsumptionEntryId, Amount>,
}

impl<'a> MallAfterSalesRepository<'a> {
    /// 批量读取原消费事实与历史退款净额（INT-R11）。
    ///
    /// 固定两次数据库访问：按 entry ID `$in` 取原消费事实；再按同一 ID 集合
    /// `$in` 取全部未删除退款分配并在仓储内按 `APPLY − REVERSE` 折叠净额。
    /// 空 ID 集合不访问数据库。全部使用调用方执行器，事务内调用看到同一
    /// 事务未提交写入；本方法不自行开启或提交事务。
    ///
    /// # 参数
    /// * `entry_ids` - 本请求引用的原消费事实 ID 集合（可含重复）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回原消费事实映射与历史净额映射；缺项由 Service／Entity 解释。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 历史净额折叠复用实体 `apply_to_net`（`Amount` 定点加减，不产生
    /// `Result` 溢出分支）；不返回 services DTO、HTTP View 或授权结论；
    /// 不裁决累计是否超限。
    pub async fn consumption_refund_limit_scope(
        &self,
        entry_ids: &[MallConsumptionEntryId],
        executor: &mut dyn Executor,
    ) -> Result<ConsumptionRefundLimitScope> {
        let unique_ids = unique_entry_ids(entry_ids);
        if unique_ids.is_empty() {
            return Ok(ConsumptionRefundLimitScope {
                entries: HashMap::new(),
                historical_nets: HashMap::new(),
            });
        }
        let entries = self.load_consumption_entries(&unique_ids, executor).await?;
        let historical_nets = self.load_historical_refund_nets(&unique_ids, executor).await?;
        Ok(ConsumptionRefundLimitScope {
            entries,
            historical_nets,
        })
    }

    /// 按 ID 集合批量装载原消费事实。
    ///
    /// # 参数
    /// * `entry_ids` - 已去重的原消费事实 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回按 ID 索引的原消费事实；缺失 ID 不生成条目。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn load_consumption_entries(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<MallConsumptionEntryId, MallConsumptionEntry>> {
        let rows = mongo_ops::find_many(
            &self
                .db
                .collection::<MallConsumptionEntry>(MALL_CONSUMPTION_ENTRIES),
            doc! {
                "id": { "$in": entry_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|entry| (MallConsumptionEntryId::new(entry.base.id.clone()), entry))
            .collect())
    }

    /// 按原消费 ID 集合批量装载退款分配并折叠历史净额。
    ///
    /// # 参数
    /// * `entry_ids` - 已去重的原消费事实 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回按 entry 聚合的历史净额；无分配的 entry 不出现。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn load_historical_refund_nets(
        &self,
        entry_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<MallConsumptionEntryId, Amount>> {
        let rows = mongo_ops::find_many(
            &self
                .db
                .collection::<MallRefundAllocation>(MALL_REFUND_ALLOCATIONS),
            doc! {
                "original_consumption_entry_id": { "$in": entry_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await?;
        Ok(build_historical_refund_nets(&rows))
    }
}

/// 按首次出现顺序去重 entry ID。
///
/// # 参数
/// * `entry_ids` - 原始 ID 切片（可含重复）
///
/// # 返回
/// 返回去重后的字符串 ID 列表。
fn unique_entry_ids(entry_ids: &[MallConsumptionEntryId]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for id in entry_ids {
        let key = id.to_string();
        if seen.insert(key.clone()) {
            unique.push(key);
        }
    }
    unique
}

/// 将退款分配折叠为按原消费 entry 的历史净额。
///
/// # 参数
/// * `allocations` - 未删除退款分配
///
/// # 返回
/// 返回 `APPLY − REVERSE` 净额映射；空输入返回空映射。
fn build_historical_refund_nets(
    allocations: &[MallRefundAllocation],
) -> HashMap<MallConsumptionEntryId, Amount> {
    let mut nets: HashMap<MallConsumptionEntryId, Amount> = HashMap::new();
    for allocation in allocations {
        let entry = nets
            .entry(allocation.original_consumption_entry_id.clone())
            .or_insert_with(zero_amount);
        *entry = allocation.apply_to_net(*entry);
    }
    nets
}

/// 返回精确零金额。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

#[cfg(test)]
mod tests {
    use super::{build_historical_refund_nets, unique_entry_ids, zero_amount};
    use entities::ids::{
        MallConsumptionEntryId, MallPaymentSourceId, MallRefundAllocationId, MallRefundLineId,
    };
    use entities::mall_after_sales::{AllocationAction, MallRefundAllocation, MallRefundAllocationData};
    use entities::money::Amount;
    use std::str::FromStr;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn allocation(
        entry_id: &str,
        amount_value: &str,
        action: AllocationAction,
        reverses: Option<&str>,
        reversal_entry: &str,
    ) -> MallRefundAllocation {
        MallRefundAllocation::new(
            MallRefundAllocationId::new(format!("ra-{entry_id}-{amount_value}-{reversal_entry}")),
            MallRefundAllocationData {
                mall_refund_line_id: MallRefundLineId::new("rl-1"),
                allocation_no: 1,
                original_consumption_entry_id: MallConsumptionEntryId::new(entry_id),
                original_payment_source_id: MallPaymentSourceId::new("ps-1"),
                allocated_refund_amount: amount(amount_value),
                allocation_action: action,
                reverses_allocation_id: reverses.map(MallRefundAllocationId::new),
                reversal_consumption_entry_id: Some(MallConsumptionEntryId::new(reversal_entry)),
            },
        )
        .unwrap()
    }

    /// 空输入与重复 ID 去重。
    #[test]
    fn unique_entry_ids_dedups_and_handles_empty() {
        assert!(unique_entry_ids(&[]).is_empty());
        let ids = unique_entry_ids(&[
            MallConsumptionEntryId::new("ce-1"),
            MallConsumptionEntryId::new("ce-2"),
            MallConsumptionEntryId::new("ce-1"),
        ]);
        assert_eq!(ids, vec!["ce-1".to_string(), "ce-2".to_string()]);
    }

    /// APPLY/REVERSE 混合与无历史精确零。
    #[test]
    fn build_historical_refund_nets_folds_apply_reverse_and_empty() {
        assert!(build_historical_refund_nets(&[]).is_empty());
        let nets = build_historical_refund_nets(&[
            allocation("ce-1", "50.00", AllocationAction::Apply, None, "rev-1"),
            allocation(
                "ce-1",
                "10.00",
                AllocationAction::Reverse,
                Some("ra-apply"),
                "rev-2",
            ),
            allocation("ce-2", "20.00", AllocationAction::Apply, None, "rev-3"),
        ]);
        assert_eq!(
            nets.get(&MallConsumptionEntryId::new("ce-1")).copied(),
            Some(amount("40.00"))
        );
        assert_eq!(
            nets.get(&MallConsumptionEntryId::new("ce-2")).copied(),
            Some(amount("20.00"))
        );
        assert_eq!(zero_amount(), amount("0.00"));
    }
}
