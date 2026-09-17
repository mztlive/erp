//! 供应商退款事实查询：退款头、分配与财务快照。

use super::*;

impl<'a> SupplierRefundFactRepository<'a> {
    /// 按「连接 + 外部退款号 + 外部退款版本」查找退款事实头。
    ///
    /// 唯一性由 `uk_supplier_refund_facts_connection_refund` 唯一索引保证
    /// （§6.19：外部退款身份与版本组成幂等键）。
    ///
    /// # 参数
    /// * `connection_id` - 供应商 API 连接
    /// * `external_refund_no` - 外部退款号
    /// * `external_refund_version` - 外部退款版本
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除退款事实头；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_connection_and_refund(
        &self,
        connection_id: &SupplierApiConnectionId,
        external_refund_no: &str,
        external_refund_version: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierRefundFact>> {
        self.find_one(
            doc! {
                "connection_id": connection_id.to_string(),
                "external_refund_no": external_refund_no,
                "external_refund_version": external_refund_version,
            },
            executor,
        )
        .await
    }

    /// 批量按供应商子订单查询退款事实头（`$in` 一次取回，避免 N+1）。
    ///
    /// 退款事实是冲减供应商成本和应付的唯一事实（§6.19），订单详情页按子订单
    /// 聚合退款时使用本方法。
    ///
    /// # 参数
    /// * `order_ids` - 供应商子订单 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中集合对应的全部未删除退款事实头。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_refund_facts_by_order_ids(
        &self,
        order_ids: &[SupplierFulfillmentOrderId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRefundFact>> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut facts = self
            .find_many(
                doc! { "supplier_fulfillment_order_id": { "$in": ids_to_strings(order_ids) } },
                executor,
            )
            .await?;
        facts.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(facts)
    }

    /// 判断指定入站消息是否已形成供应商退款正式事实。
    ///
    /// # 参数
    /// * `message_id` - 入站消息 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 已存在正式退款事实时返回 `true`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    ///
    /// # 约束
    /// 仅查询本仓储拥有的 `supplier_refund_fact` 集合，按入站消息引用判定存在性，不访问入站消息集合。
    pub async fn exists_by_inbox_message(
        &self,
        message_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        self.exists(doc! { "inbox_message_id": message_id }, executor).await
    }
}

impl<'a> SupplierRefundAllocationRepository<'a> {
    /// 批量按退款事实头查询退款分配行（`$in` 一次取回，避免 N+1）。
    ///
    /// 分配行是正式事实行，创建后不可修改（§6.19）；`validate_allocations` 与
    /// REVERSE 纠错编排在 P3 使用本方法加载全部分配行。
    ///
    /// # 参数
    /// * `fact_ids` - 供应商退款事实头 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中集合对应的全部未删除退款分配行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_allocations_by_fact_ids(
        &self,
        fact_ids: &[SupplierRefundFactId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRefundAllocation>> {
        if fact_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut allocations = self
            .find_many(doc! { "supplier_refund_fact_id": { "$in": ids_to_strings(fact_ids) } }, executor)
            .await?;
        allocations.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(allocations)
    }
}

/// 订单退款财务快照（FUL-R05）。
///
/// 只包含退款上限校验所需的两个持久化金额，不携带 services DTO 或授权
/// 结论；任一来源集合为空时对应金额为精确零。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefundFinancialSnapshot {
    /// 订单明细含税成本快照合计。
    pub order_cost_gross: Amount,
    /// 历史退款事实实际退款金额合计。
    pub refunded_total: Amount,
}

/// 单个退款事实头及其分配行的原始快照（FUL-R04）。
///
/// 只承载持久化实体与存储无关的归组关系：事实头保留软删除过滤后的
/// 原始实体，分配行按所属事实头归组并按主键稳定排序；不携带 services
/// response DTO、HTTP View 或授权结论，响应映射仍由 Service 承担。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierRefundFactBundle {
    /// 退款事实头实体。
    pub fact: SupplierRefundFact,
    /// 归属该事实头的未软删除分配行，按主键稳定排序；无分配行时为空。
    pub allocations: Vec<SupplierRefundAllocation>,
}
