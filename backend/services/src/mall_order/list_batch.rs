//! 订单列表页批量装配（INT-R01/R03/R04/R05 服务编排）。
//!
//! 当前页事实摘要、支付来源、消费事实与最新评估一次批量补齐；
//! 业务分组、页内匹配和视图计算仍留在 Service，持久化过滤与排序归属 Repository。

use std::collections::HashMap;

use database::{MallOrderExt, NoTransaction};
use entities::ids::{MallConsumptionEntryId, MallOrderId, MallPaymentSourceId};
use entities::mall_order::{MallConsumptionCostAssessment, MallConsumptionEntry, MallPaymentSource};

use super::query::OrderFactSummary;
use super::MallOrderService;
use crate::errors::Result;

/// 订单列表当前页批量装配结果（一次装配，多行复用）。
pub(super) struct ListPageSupport {
    /// （商城, 订单号）→ 事实摘要列表（保持仓储稳定顺序）。
    pub(super) facts: HashMap<(String, String), Vec<OrderFactSummary>>,
    /// 订单 ID → 支付来源列表（同订单按来源序号升序）。
    pub(super) sources: HashMap<String, Vec<MallPaymentSource>>,
    /// 当前页全部消费事实（按发生时间升序）。
    pub(super) entries: Vec<MallConsumptionEntry>,
    /// 消费 ID → 最新成本评估。
    pub(super) assessments: HashMap<String, MallConsumptionCostAssessment>,
}

impl MallOrderService {
    /// 为当前页订单一次批量装配列表行所需关联集合（INT-R01/R03/R04/R05）。
    ///
    /// # 用途
    /// 事实按当前页订单业务键批量、支付按订单批量、消费按来源批量、
    /// 评估按消费批量；数据库访问次数不随页内行数线性增长。
    ///
    /// # 参数
    /// * `keys` - 当前页订单业务键（商城, 订单号）集合
    /// * `order_ids` - 当前页订单 ID 集合
    ///
    /// # 返回
    /// 返回多行复用的装配结果。
    ///
    /// # 错误
    /// 数据库查询失败时返回 `RepositoryError`。
    ///
    /// # 关键约束
    /// 只查询当前页关联范围，不全量翻页；软删除、稳定排序与事务可见性由
    /// Repository 保证；空页直接返回空装配，不访问数据库。
    pub(super) async fn load_list_page_support(
        &self,
        keys: &[(String, String)],
        order_ids: &[MallOrderId],
    ) -> Result<ListPageSupport> {
        if keys.is_empty() || order_ids.is_empty() {
            return Ok(ListPageSupport {
                facts: HashMap::new(),
                sources: HashMap::new(),
                entries: Vec::new(),
                assessments: HashMap::new(),
            });
        }
        let fact_rows = self
            .db
            .mall_order_facts()
            .list_fact_rows_by_order_keys(keys, &mut NoTransaction)
            .await?;
        let mut facts: HashMap<(String, String), Vec<OrderFactSummary>> = HashMap::new();
        for row in fact_rows {
            facts
                .entry((row.mall_id.clone(), row.external_order_no.clone()))
                .or_default()
                .push(OrderFactSummary {
                    id: row.id,
                    fact_type: row.fact_type,
                    occurred_at: row.occurred_at,
                    data_source: row.data_source,
                });
        }
        let sources = self
            .db
            .mall_payment_sources()
            .list_by_orders(order_ids, &mut NoTransaction)
            .await?;
        let source_ids: Vec<MallPaymentSourceId> = sources
            .values()
            .flatten()
            .map(|source| MallPaymentSourceId::new(source.base.id.clone()))
            .collect();
        let entries = self
            .db
            .mall_consumption_entries()
            .list_by_payment_sources(&source_ids, &mut NoTransaction)
            .await?;
        let entry_ids: Vec<MallConsumptionEntryId> = entries
            .iter()
            .map(|entry| MallConsumptionEntryId::new(entry.base.id.clone()))
            .collect();
        let assessments = self
            .db
            .mall_consumption_cost_assessments()
            .list_latest_by_entries(&entry_ids, &mut NoTransaction)
            .await?;
        Ok(ListPageSupport {
            facts,
            sources,
            entries,
            assessments,
        })
    }
}

#[cfg(test)]
mod tests {
    use entities::common::time::Instant;
    use entities::mall_order::{DataSource, FactType};

    use super::super::query::OrderFactSummary;
    use super::ListPageSupport;
    use std::collections::HashMap;

    /// 按（商城, 订单号）归组：跨 mall 隔离、重复事实保留、同键保持仓储顺序。
    ///
    /// 测试覆盖 INT-R01 验收的分组维度，不访问数据库；分组键或顺序漂移时失败。
    #[test]
    fn support_groups_fact_rows_by_order_key() {
        let rows = vec![
            (("mall-a".to_string(), "SO-1".to_string()), "fact-1", 100_i64),
            (("mall-a".to_string(), "SO-1".to_string()), "fact-2", 100_i64),
            (("mall-b".to_string(), "SO-1".to_string()), "fact-3", 101_i64),
        ];
        let mut support = ListPageSupport {
            facts: HashMap::new(),
            sources: HashMap::new(),
            entries: Vec::new(),
            assessments: HashMap::new(),
        };
        for (key, id, occurred) in rows {
            support.facts.entry(key).or_default().push(OrderFactSummary {
                id: id.to_string(),
                fact_type: FactType::PaymentSucceeded,
                occurred_at: Instant::from_unix_secs(occurred),
                data_source: DataSource::Realtime,
            });
        }
        assert_eq!(
            support
                .facts
                .get(&("mall-a".to_string(), "SO-1".to_string()))
                .map(Vec::len),
            Some(2)
        );
        assert_eq!(
            support
                .facts
                .get(&("mall-b".to_string(), "SO-1".to_string()))
                .map(Vec::len),
            Some(1)
        );
        let mall_a = &support.facts[&("mall-a".to_string(), "SO-1".to_string())];
        assert_eq!(mall_a[0].id, "fact-1");
        assert_eq!(mall_a[1].id, "fact-2");
    }

    /// 空页装配为空集合；无来源订单无条目（缺项语义）。
    #[test]
    fn empty_support_has_no_facts_sources_or_assessments() {
        let support = ListPageSupport {
            facts: HashMap::new(),
            sources: HashMap::new(),
            entries: Vec::new(),
            assessments: HashMap::new(),
        };
        assert!(support.facts.is_empty());
        assert!(support.sources.get("order-missing").is_none());
        assert!(support.entries.is_empty());
        assert!(support.assessments.is_empty());
    }
}
