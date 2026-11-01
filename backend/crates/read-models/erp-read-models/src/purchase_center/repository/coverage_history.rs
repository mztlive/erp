//! 采购引用的历史销售版本行批量加载。
//!
//! 采购按稳定销售行计入当前覆盖，并以引用的销售版本行作为受影响判定基准。
//! 本模块按销售版本批量读取公共行与商品子行，查询次数与采购数量无关。

use std::collections::{HashMap, HashSet};

use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_procurement::entity::purchase_order::{HistoricalSalesLineFact, PurchaseOrder, maintained_baselines};
use erp_procurement::repository::purchase_source_update::list_by_sales_order;
use erp_sales::entity::sales_order::LineType;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, Result};

use super::mapping::{sales_goods_fact, sales_line_fact};

/// 采购引用版本与维持登记版本的判定基准。
type SourceBaselines = (HashMap<String, HistoricalSalesLineFact>, HashMap<String, String>);

/// 加载采购的受影响判定基准：维持登记版本，以及采购引用或维持的历史版本行。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `current_revision_id` - 销售当前版本
/// * `sales_order_id` - 来源销售单
/// * `orders` - 参与覆盖的采购单
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回历史版本行映射与采购单 ID 到维持版本的映射。
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误。
pub async fn load_source_baselines(
    db: &Database,
    current_revision_id: &str,
    sales_order_id: &SalesOrderId,
    orders: &[PurchaseOrder],
    executor: &mut dyn Executor,
) -> Result<SourceBaselines> {
    let updates = list_by_sales_order(db, sales_order_id.as_ref(), executor).await?;
    let maintained = maintained_baselines(orders, &updates);
    let revisions = orders
        .iter()
        .map(|order| order.sales_order_revision_id.to_string())
        .chain(maintained.values().cloned());
    let historical = load_historical_sales_lines(db, current_revision_id, revisions, executor).await?;
    Ok((historical, maintained))
}

/// 批量加载指定历史销售版本的商品/服务行。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `current_revision_id` - 销售当前版本；不在结果中重复加载
/// * `revision_ids` - 采购引用或维持登记的销售版本，可重复
/// * `executor` - 调用方执行器；事务内重验必须复用
///
/// # 返回
/// 返回历史版本行 ID 到公共行与商品子行的映射；未删除且有商品子行的行才进入结果。
///
/// # 错误
/// MongoDB 查询或反序列化失败时返回错误。
///
/// # 约束
/// 公共行与商品子行各一次批量读取，不得逐采购查询。
pub async fn load_historical_sales_lines(
    db: &Database,
    current_revision_id: &str,
    revision_ids: impl IntoIterator<Item = String>,
    executor: &mut dyn Executor,
) -> Result<HashMap<String, HistoricalSalesLineFact>> {
    let revisions = revision_ids
        .into_iter()
        .filter(|id| id != current_revision_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .map(SalesOrderRevisionId::new)
        .collect::<Vec<_>>();
    if revisions.is_empty() {
        return Ok(HashMap::new());
    }
    let lines = db
        .sales_order_revision_lines()
        .list_lines_by_revisions(&revisions, executor)
        .await?
        .into_iter()
        .filter(|line| line.base.deleted_at == 0 && line.line_type == LineType::GoodsService)
        .collect::<Vec<_>>();
    let line_ids = lines.iter().map(|line| SalesOrderRevisionLineId::new(&line.base.id)).collect::<Vec<_>>();
    let mut goods = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(&line_ids, executor)
        .await?
        .into_iter()
        .filter(|goods| goods.base.deleted_at == 0)
        .map(|goods| (goods.revision_line_id.to_string(), goods))
        .collect::<HashMap<_, _>>();
    Ok(lines
        .into_iter()
        .filter_map(|line| {
            let goods_line = sales_goods_fact(goods.remove(&line.base.id)?);
            let revision_id = line.sales_order_revision_id.to_string();
            Some((
                line.base.id.clone(),
                HistoricalSalesLineFact {
                    sales_order_revision_id: revision_id,
                    revision_line: sales_line_fact(line),
                    goods_line,
                },
            ))
        })
        .collect())
}
