//! 余额恢复额度关联事实图与历史净额（INT-R12）。
//!
//! 一次批量取回退款分配、退款行、退款头、原支付来源，以及按原退款分配聚合的
//! 历史恢复合计。固定有界读取，不随请求分配行数退化为逐条 N+1。全部使用调用方
//! executor，不开事务；same-case、card 归属与并发额度占用仍由 Service 负责。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use mongodb::options::FindOptions;

use entities::ids::{MallPaymentSourceId, MallRefundAllocationId, MallRefundId, MallRefundLineId};
use entities::mall_after_sales::{
    MallBalanceRestorationAllocation, MallRefund, MallRefundAllocation, MallRefundLine,
};
use entities::mall_order::MallPaymentSource;
use entities::money::Amount;

use super::super::extensions::MallOrderExt;
use super::{
    MallAfterSalesRepository, MALL_BALANCE_RESTORATION_ALLOCATIONS, MALL_REFUNDS, MALL_REFUND_ALLOCATIONS,
    MALL_REFUND_LINES,
};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `mall_payment_sources` 集合名（单一来源：`MallOrderExt`）。
const MALL_PAYMENT_SOURCES: &str = <mongodb::Database as MallOrderExt>::MALL_PAYMENT_SOURCES;

/// 余额恢复额度校验所需的关联事实图与历史净额。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorationLimitScope {
    /// 原退款分配（按 ID 索引；请求中缺失的 ID 不出现）。
    pub refund_allocations: HashMap<MallRefundAllocationId, MallRefundAllocation>,
    /// 退款行（按 ID 索引）。
    pub refund_lines: HashMap<MallRefundLineId, MallRefundLine>,
    /// 退款头（按 ID 索引）。
    pub refunds: HashMap<MallRefundId, MallRefund>,
    /// 原支付来源（按 ID 索引）。
    pub payment_sources: HashMap<MallPaymentSourceId, MallPaymentSource>,
    /// 各原退款分配的历史已恢复合计；无历史时不出现，由调用方按精确零处理。
    pub historical_restored: HashMap<MallRefundAllocationId, Amount>,
}

impl<'a> MallAfterSalesRepository<'a> {
    /// 批量加载余额恢复关联事实图与历史恢复净额（INT-R12）。
    ///
    /// 固定五次数据库访问：退款分配 `$in`、退款行 `$in`、退款头 `$in`、支付来源
    /// `$in`、恢复分配 `$in` 后折叠历史合计。空 ID 集合不访问数据库。全部使用
    /// 调用方执行器，事务内调用看到同一事务未提交写入；本方法不自行开启或提交
    /// 事务。
    ///
    /// # 参数
    /// * `refund_allocation_ids` - 本请求引用的原退款分配 ID 集合（可含重复）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回关联事实图与历史恢复合计；缺项由 Service／Entity 解释。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 不返回 services DTO、HTTP View 或授权结论；不裁决 same-case／card 归属。
    pub async fn restoration_limit_scope(
        &self,
        refund_allocation_ids: &[MallRefundAllocationId],
        executor: &mut dyn Executor,
    ) -> Result<RestorationLimitScope> {
        let unique_ids = unique_refund_allocation_ids(refund_allocation_ids);
        if unique_ids.is_empty() {
            return Ok(RestorationLimitScope {
                refund_allocations: HashMap::new(),
                refund_lines: HashMap::new(),
                refunds: HashMap::new(),
                payment_sources: HashMap::new(),
                historical_restored: HashMap::new(),
            });
        }

        let refund_allocations = self.load_refund_allocations(&unique_ids, executor).await?;
        let line_ids = unique_strings(
            refund_allocations
                .values()
                .map(|allocation| allocation.mall_refund_line_id.to_string()),
        );
        let refund_lines = self.load_refund_lines(&line_ids, executor).await?;
        let refund_ids = unique_strings(refund_lines.values().map(|line| line.mall_refund_id.to_string()));
        let refunds = self.load_refunds(&refund_ids, executor).await?;
        let source_ids = unique_strings(
            refund_allocations
                .values()
                .map(|allocation| allocation.original_payment_source_id.to_string()),
        );
        let payment_sources = self.load_payment_sources(&source_ids, executor).await?;
        let historical_restored = self.load_historical_restored(&unique_ids, executor).await?;
        Ok(RestorationLimitScope {
            refund_allocations,
            refund_lines,
            refunds,
            payment_sources,
            historical_restored,
        })
    }

    /// 按 ID 集合批量装载退款分配。
    ///
    /// # 参数
    /// * `ids` - 已去重的退款分配 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回按 ID 索引的退款分配；缺失 ID 不生成条目。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn load_refund_allocations(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<MallRefundAllocationId, MallRefundAllocation>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = mongo_ops::find_many(
            &self
                .db
                .collection::<MallRefundAllocation>(MALL_REFUND_ALLOCATIONS),
            doc! { "id": { "$in": ids }, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            FindOptions::default(),
            executor,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|allocation| {
                (
                    MallRefundAllocationId::new(allocation.base.id.clone()),
                    allocation,
                )
            })
            .collect())
    }

    /// 按 ID 集合批量装载退款行。
    ///
    /// # 参数
    /// * `ids` - 已去重的退款行 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回按 ID 索引的退款行。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn load_refund_lines(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<MallRefundLineId, MallRefundLine>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = mongo_ops::find_many(
            &self.db.collection::<MallRefundLine>(MALL_REFUND_LINES),
            doc! { "id": { "$in": ids }, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            FindOptions::default(),
            executor,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|line| (MallRefundLineId::new(line.base.id.clone()), line))
            .collect())
    }

    /// 按 ID 集合批量装载退款头。
    ///
    /// # 参数
    /// * `ids` - 已去重的退款头 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回按 ID 索引的退款头。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn load_refunds(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<MallRefundId, MallRefund>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = mongo_ops::find_many(
            &self.db.collection::<MallRefund>(MALL_REFUNDS),
            doc! { "id": { "$in": ids }, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            FindOptions::default(),
            executor,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|refund| (MallRefundId::new(refund.base.id.clone()), refund))
            .collect())
    }

    /// 按 ID 集合批量装载支付来源。
    ///
    /// # 参数
    /// * `ids` - 已去重的支付来源 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回按 ID 索引的支付来源。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn load_payment_sources(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<MallPaymentSourceId, MallPaymentSource>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = mongo_ops::find_many(
            &self.db.collection::<MallPaymentSource>(MALL_PAYMENT_SOURCES),
            doc! { "id": { "$in": ids }, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            FindOptions::default(),
            executor,
        )
        .await?;
        Ok(rows
            .into_iter()
            .map(|source| (MallPaymentSourceId::new(source.base.id.clone()), source))
            .collect())
    }

    /// 按原退款分配 ID 集合批量装载恢复分配并折叠历史合计。
    ///
    /// # 参数
    /// * `ids` - 已去重的原退款分配 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回按原退款分配聚合的历史恢复合计。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn load_historical_restored(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<MallRefundAllocationId, Amount>> {
        let rows = mongo_ops::find_many(
            &self
                .db
                .collection::<MallBalanceRestorationAllocation>(MALL_BALANCE_RESTORATION_ALLOCATIONS),
            doc! {
                "mall_refund_allocation_id": { "$in": ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::default(),
            executor,
        )
        .await?;
        Ok(build_historical_restored(&rows))
    }
}

/// 按首次出现顺序去重退款分配 ID。
///
/// # 参数
/// * `ids` - 原始 ID 切片（可含重复）
///
/// # 返回
/// 返回去重后的字符串 ID 列表。
fn unique_refund_allocation_ids(ids: &[MallRefundAllocationId]) -> Vec<String> {
    unique_strings(ids.iter().map(ToString::to_string))
}

/// 按首次出现顺序去重字符串。
///
/// # 参数
/// * `ids` - 字符串迭代
///
/// # 返回
/// 返回去重后的列表。
fn unique_strings(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            unique.push(id);
        }
    }
    unique
}

/// 将恢复分配折叠为按原退款分配的历史合计。
///
/// # 参数
/// * `allocations` - 未删除恢复分配
///
/// # 返回
/// 返回历史恢复合计映射；空输入返回空映射。
fn build_historical_restored(
    allocations: &[MallBalanceRestorationAllocation],
) -> HashMap<MallRefundAllocationId, Amount> {
    let mut totals: HashMap<MallRefundAllocationId, Amount> = HashMap::new();
    for allocation in allocations {
        let total = totals
            .entry(allocation.mall_refund_allocation_id.clone())
            .or_insert_with(zero_amount);
        *total = allocation.add_to_total(*total);
    }
    totals
}

/// 返回精确零金额。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

#[cfg(test)]
mod tests {
    use super::{build_historical_restored, unique_refund_allocation_ids, unique_strings, zero_amount};
    use entities::ids::{
        MallBalanceRestorationAllocationId, MallBalanceRestorationId, MallCardInstanceId,
        MallRefundAllocationId,
    };
    use entities::mall_after_sales::{
        MallBalanceRestorationAllocation, MallBalanceRestorationAllocationData,
    };
    use entities::money::Amount;
    use std::str::FromStr;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn restoration_allocation(
        refund_allocation_id: &str,
        amount_value: &str,
        no: u32,
    ) -> MallBalanceRestorationAllocation {
        MallBalanceRestorationAllocation::new(
            MallBalanceRestorationAllocationId::new(format!("bra-{refund_allocation_id}-{no}")),
            MallBalanceRestorationAllocationData {
                mall_balance_restoration_id: MallBalanceRestorationId::new("br-1"),
                allocation_no: no,
                mall_refund_allocation_id: MallRefundAllocationId::new(refund_allocation_id),
                mall_card_instance_id: MallCardInstanceId::new("card-1"),
                restored_amount: amount(amount_value),
            },
        )
        .unwrap()
    }

    /// 空输入与重复 ID 去重。
    #[test]
    fn unique_helpers_dedup_and_handle_empty() {
        assert!(unique_refund_allocation_ids(&[]).is_empty());
        assert!(unique_strings(Vec::<String>::new()).is_empty());
        let ids = unique_refund_allocation_ids(&[
            MallRefundAllocationId::new("ra-1"),
            MallRefundAllocationId::new("ra-2"),
            MallRefundAllocationId::new("ra-1"),
        ]);
        assert_eq!(ids, vec!["ra-1".to_string(), "ra-2".to_string()]);
    }

    /// 多次历史恢复合计与精确零。
    #[test]
    fn build_historical_restored_sums_and_empty() {
        assert!(build_historical_restored(&[]).is_empty());
        let totals = build_historical_restored(&[
            restoration_allocation("ra-1", "10.00", 1),
            restoration_allocation("ra-1", "15.00", 2),
            restoration_allocation("ra-2", "5.00", 1),
        ]);
        assert_eq!(
            totals.get(&MallRefundAllocationId::new("ra-1")).copied(),
            Some(amount("25.00"))
        );
        assert_eq!(
            totals.get(&MallRefundAllocationId::new("ra-2")).copied(),
            Some(amount("5.00"))
        );
        assert_eq!(zero_amount(), amount("0.00"));
    }
}
