//! 付款申请身份、任务关联、来源占用及希望付款日索引。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::{PurchasePaymentRequestExt, PurchasePaymentRequestSnapshotExt};

/// 幂等登记付款申请集合索引。
///
/// # 参数
/// * `db` - 目标数据库。
/// # 返回
/// 全部命名索引创建成功时无返回值。
/// # 错误
/// 唯一身份冲突或数据库错误时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(<Database as PurchasePaymentRequestExt>::PURCHASE_PAYMENT_REQUESTS)
        .create_indexes(vec![
            index(
                "idx_purchase_payment_requests_list",
                doc! {"deleted_at": 1, "created_at": -1, "id": -1},
                false,
            ),
            index("uk_purchase_payment_requests_id", doc! {"id": 1}, true),
            index("uk_purchase_payment_requests_no", doc! {"request_no": 1}, true),
            index("uk_purchase_payment_requests_work_item", doc! {"work_item_id": 1}, true),
            index(
                "idx_purchase_payment_requests_account_status",
                doc! {"lines.payable_account_id": 1, "status": 1},
                false,
            ),
            index(
                "idx_purchase_payment_requests_applicant",
                doc! {"created_by": 1, "created_at": -1, "id": -1},
                false,
            ),
            index(
                "idx_purchase_payment_requests_supplier",
                doc! {"supplier_id": 1, "status": 1, "data.desired_payment_date": 1},
                false,
            ),
        ])
        .await?;
    db.collection::<Document>(
        <Database as PurchasePaymentRequestSnapshotExt>::PURCHASE_PAYMENT_REQUEST_SNAPSHOTS,
    )
    .create_indexes(vec![
        index("uk_purchase_payment_request_snapshots_id", doc! {"id": 1}, true),
        index(
            "uk_purchase_payment_request_snapshots_submission",
            doc! {"request_id": 1, "submission_version": 1},
            true,
        ),
        index("uk_purchase_payment_request_snapshots_approval", doc! {"approval_instance_id": 1}, true),
    ])
    .await?;
    Ok(())
}

fn index(name: &str, keys: Document, unique: bool) -> IndexModel {
    let mut options = IndexOptions::builder().name(name.to_owned()).unique(unique).build();
    if name == "uk_purchase_payment_requests_work_item" {
        options.partial_filter_expression = Some(doc! {"work_item_id": {"$type": "string"}});
    }
    IndexModel::builder().keys(keys).options(options).build()
}
