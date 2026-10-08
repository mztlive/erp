//! 已授权采购身份批量解析当前正式选源，历史缺失关系显式保留 None。

use std::collections::{HashMap, HashSet};

use erp_core::ids::PurchaseOrderId;
use erp_procurement::entity::purchase_order::{
    PurchaseLineType, PurchaseOfferingSource, PurchaseOrderStatus,
};
use erp_procurement::repository::{
    PurchaseOrderExt, PurchaseOrderRevisionLineRepositoryExt, PurchaseOrderSubmissionLineRepositoryExt,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 真实当前采购负责人和选源，不包含金额或付款。
#[derive(Default)]
pub(in crate::supplier_portal) struct PurchaseSources {
    pub owner_user_id: Option<String>,
    pub sources: Vec<Option<PurchaseOfferingSource>>,
}

/// 批量从当前指针读取选源，禁止按供应商与 SKU 构造关系。
///
/// # 参数
/// * `db` - 数据库。
/// * `ids` - 采购单身份。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回命中采购单的当前负责人和选源。未命中的身份不出现。已完成或已作废的单据不填选源。
///
/// # 错误
/// 采购单超过一千个或选源行超过一万行时返回 `ValidationError`。
/// 未完成单据缺少责任人或仓储读取失败时返回对应错误。
pub(in crate::supplier_portal) async fn current_purchase_sources(
    db: &Database,
    ids: &[PurchaseOrderId],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, PurchaseSources>> {
    if ids.len() > 1_000 {
        return Err(Error::ValidationError("采购提示读取超过一千个对象".into()));
    }
    let ids = ids.iter().map(ToString::to_string).collect::<HashSet<_>>().into_iter().collect::<Vec<_>>();
    let orders = db.purchase_order().find_orders_by_ids(&ids, executor).await?;
    let mut result = orders
        .iter()
        .map(|order| (order.base.id.clone(), PurchaseSources::default()))
        .collect::<HashMap<_, _>>();
    let mut revisions = HashMap::new();
    let mut submissions = HashMap::new();
    for order in orders {
        if matches!(order.stable.status, PurchaseOrderStatus::Completed | PurchaseOrderStatus::Voided) {
            continue;
        }
        if let Some(fact) = result.get_mut(&order.base.id) {
            fact.owner_user_id = Some(order.current_owner_user_id()?.to_string());
        }
        if let Some(id) = order.stable.current_revision_id {
            revisions.insert(id, order.base.id);
        } else if let Some(id) = order.current_submission_id {
            submissions.insert(id, order.base.id);
        }
    }
    let revision_ids = revisions.keys().cloned().map(Into::into).collect::<Vec<_>>();
    let submission_ids = submissions.keys().cloned().map(Into::into).collect::<Vec<_>>();
    let revision_lines =
        db.purchase_order_revision_lines().find_lines_by_revision_ids(&revision_ids, executor).await?;
    let submission_lines =
        db.purchase_order_submission_lines().find_lines_by_submission_ids(&submission_ids, executor).await?;
    if revision_lines.len() + submission_lines.len() > 10_000 {
        return Err(Error::ValidationError("采购提示选源行超过一万行".into()));
    }
    for line in revision_lines {
        if line.line_type == PurchaseLineType::ItemService
            && let Some(id) = revisions.get(&line.purchase_order_revision_id.to_string())
            && let Some(sources) = result.get_mut(id)
        {
            sources.sources.push(line.supplier_offering_source);
        }
    }
    for line in submission_lines {
        if line.line_type == PurchaseLineType::ItemService
            && let Some(id) = submissions.get(&line.purchase_order_submission_id.to_string())
            && let Some(sources) = result.get_mut(id)
        {
            sources.sources.push(line.supplier_offering_source);
        }
    }
    Ok(result)
}
