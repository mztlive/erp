//! 已授权对账差异关联对象的业务编号批量读取。

use std::collections::HashMap;

use erp_import::LegacyImportExt;
use erp_integration::dto::DifferenceView;
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use erp_supply::repository::SupplierFulfillmentExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

use crate::Result;

/// 批量读取已授权差异中类型明确的对象业务编号。
///
/// # 参数
/// `db` 为业务仓储入口，`items` 为已授权对象，`executor` 为调用方读取快照。
/// # 返回
/// 返回对象类型与身份索引的业务编号；未知类型和缺失关联没有名称。
/// # 错误
/// 关联仓储读取失败时返回错误。
pub(crate) async fn object_labels(
    db: &Database,
    items: &[DifferenceView],
    executor: &mut dyn Executor,
) -> Result<HashMap<(String, String), String>> {
    let mut labels = HashMap::new();
    for kind in ["sales_order", "purchase_order", "supplier_fulfillment_order", "legacy_import_batch"] {
        let ids = items
            .iter()
            .filter(|item| item.business_object_type == kind)
            .map(|item| item.business_object_id.clone())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            continue;
        }
        let filter = doc! { "id": { "$in": ids } };
        let found = object_numbers(db, kind, filter, executor).await?;
        labels.extend(
            found
                .into_iter()
                .filter(|(_, label)| !label.trim().is_empty())
                .map(|(id, label)| ((kind.to_string(), id), label)),
        );
    }
    Ok(labels)
}

async fn object_numbers(
    db: &Database,
    kind: &str,
    filter: Document,
    executor: &mut dyn Executor,
) -> Result<Vec<(String, String)>> {
    Ok(match kind {
        "sales_order" => db
            .sales_orders()
            .find_many(filter, executor)
            .await?
            .into_iter()
            .map(|item| (item.base.id, item.order_no))
            .collect(),
        "purchase_order" => db
            .purchase_orders()
            .find_many(filter, executor)
            .await?
            .into_iter()
            .map(|item| (item.base.id, item.purchase_no))
            .collect(),
        "supplier_fulfillment_order" => db
            .supplier_fulfillment_orders()
            .find_many(filter, executor)
            .await?
            .into_iter()
            .map(|item| (item.base.id, item.fulfillment_order_no))
            .collect(),
        "legacy_import_batch" => db
            .legacy_import_batches()
            .find_many(filter, executor)
            .await?
            .into_iter()
            .map(|item| (item.base.id, item.batch_no))
            .collect(),
        _ => Vec::new(),
    })
}
