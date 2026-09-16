//! 通过拥有领域仓储装载一致快照，任一来源失败时整体失败。
use std::collections::BTreeSet;

use erp_finance::entity::cost::{CostAllocation, CostEntry};
use erp_finance::repository::CostExt;
use erp_finance::repository::cost::profit_loss::PROFIT_LOSS_ALLOCATION_LIMIT;
use erp_sales::entity::sales_order::{
    SalesOrderGoodsServiceLineRevision, SalesOrderRevision, SalesOrderRevisionLine,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::sales_order::profit_loss::{
    PROFIT_LOSS_ORDER_LIMIT, ProfitLossOrder, ProfitLossOrderFilter,
};
use mongodb::Database;
use persistence_core::Executor;

use super::dto::ProfitLossQuery;
use super::query::PeriodBounds;
use crate::{Error, Result};

/// 单次分析装载的正式事实，均属于同一事务快照。
#[derive(Debug, Default)]
pub(super) struct Sources {
    pub orders: Vec<ProfitLossOrder>,
    pub revisions: Vec<SalesOrderRevision>,
    pub lines: Vec<SalesOrderRevisionLine>,
    pub goods: Vec<SalesOrderGoodsServiceLineRevision>,
    pub allocations: Vec<CostAllocation>,
    pub entries: Vec<CostEntry>,
}
impl Sources {
    /// 限制查询规模；超限必须缩小日期/客户范围，不返回不完整汇总。
    pub async fn load(
        db: &Database,
        query: &ProfitLossQuery,
        bounds: PeriodBounds,
        authorized_scope: erp_sales::repository::sales_order::scope::SalesReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        let orders = authorized_orders(db, query, bounds, authorized_scope, executor).await?;
        let mut result = Self {
            orders,
            revisions: vec![],
            lines: vec![],
            goods: vec![],
            allocations: vec![],
            entries: vec![],
        };
        result.load_sales(db, executor).await?;
        result.load_costs(db, executor).await?;
        Ok(result)
    }
    /// 以批次读取当前正式销售版本；不回退到草稿或过期版本。
    async fn load_sales(&mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        let ids: Vec<_> = self.orders.iter().filter_map(|o| o.current_revision_id.clone()).collect();
        for chunk in ids.chunks(500) {
            self.revisions.extend(db.sales_order_revisions().find_revisions_by_ids(chunk, executor).await?);
            let revisions = chunk.iter().cloned().map(Into::into).collect::<Vec<_>>();
            self.lines
                .extend(db.sales_order_revision_lines().list_lines_by_revisions(&revisions, executor).await?);
        }
        let ids: Vec<_> = self.lines.iter().map(|l| l.base.id.clone().into()).collect();
        for chunk in ids.chunks(500) {
            self.goods.extend(
                db.sales_order_goods_service_line_revisions()
                    .list_by_revision_line_ids(chunk, executor)
                    .await?,
            );
        }
        Ok(())
    }
    /// 按分配引用找成本，预计采购原始事实没有分配时不猜测销售归属。
    async fn load_costs(&mut self, db: &Database, executor: &mut dyn Executor) -> Result<()> {
        let ids: Vec<_> = self.orders.iter().map(|o| o.id.clone()).collect();
        for chunk in ids.chunks(500) {
            self.allocations.extend(db.cost_allocations().profit_loss_allocations(chunk, executor).await?);
            if self.allocations.len() > PROFIT_LOSS_ALLOCATION_LIMIT {
                return Err(Error::ValidationError("成本分配超过 100000 条，请缩小范围".into()));
            }
        }
        let ids: Vec<_> = self
            .allocations
            .iter()
            .map(|a| a.cost_entry_id.to_string())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        for chunk in ids.chunks(500) {
            self.entries.extend(db.cost_entries().profit_loss_entries(chunk, executor).await?);
        }
        Ok(())
    }
}

/// 有界装载销售责任版本；用于首次查询及返回前撤权、责任交接复核。
pub(super) async fn authorized_orders(
    db: &Database,
    query: &ProfitLossQuery,
    bounds: PeriodBounds,
    authorized_scope: erp_sales::repository::sales_order::scope::SalesReadScope,
    executor: &mut dyn Executor,
) -> Result<Vec<ProfitLossOrder>> {
    let filter = ProfitLossOrderFilter {
        from: bounds.from,
        until: bounds.until,
        customer_id: query.customer_id.clone(),
        sales_order_id: query.sales_order_id.clone(),
        authorized_scope,
    };
    let orders = db.sales_orders().profit_loss_orders(&filter, executor).await?;
    if orders.len() > PROFIT_LOSS_ORDER_LIMIT {
        return Err(Error::ValidationError("匹配销售单超过 10000 单，请缩小期间或指定客户".into()));
    }
    Ok(orders)
}

/// 查询版本包含完整授权订单集合及责任版本，不返回订单身份集合。
pub(super) fn version(scope_version: &str, orders: &[ProfitLossOrder]) -> String {
    use std::hash::{Hash, Hasher};
    let mut fingerprint = std::collections::hash_map::DefaultHasher::new();
    let versions = orders.iter().map(|o| (&o.id, o.version)).collect::<std::collections::BTreeMap<_, _>>();
    versions.hash(&mut fingerprint);
    format!("{scope_version}:{:x}", fingerprint.finish())
}
