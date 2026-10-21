//! 内部询价报价不可变版本、精确费用范围索引。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use super::InquirySupplyExt;

pub(crate) async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(<Database as InquirySupplyExt>::INQUIRY_QUOTES)
        .create_indexes(vec![
            index("uk_inquiry_internal_quote_id", doc! {"id":1}, true),
            index("uk_inquiry_internal_quote_revision", doc! {"quote_id":1,"revision_no":1}, true),
            index(
                "idx_inquiry_internal_quote_scope",
                doc! {"inquiry_id":1,"requirement_version":1,"feedback_round":1,"line_id":1,"quote_id":1,"revision_no":1},
                false,
            ),
            index("idx_inquiry_internal_quote_offering", doc! {"supplier_offering_id":1,"id":1}, false),
        ])
        .await?;
    db.collection::<Document>(<Database as InquirySupplyExt>::INQUIRY_FEE_BASES)
        .create_indexes(vec![
            index("uk_inquiry_internal_fee_basis_id", doc! {"id":1}, true),
            index(
                "uk_inquiry_internal_fee_scope",
                doc! {"inquiry_id":1,"requirement_version":1,"feedback_round":1,"fee_id":1},
                true,
            ),
        ])
        .await?;
    db.collection::<Document>(<Database as InquirySupplyExt>::INQUIRY_QUOTE_BALANCES)
        .create_indexes(vec![index("uk_inquiry_quote_balance_id", doc! {"id":1}, true)])
        .await?;
    db.collection::<Document>(<Database as InquirySupplyExt>::INQUIRY_QUOTE_RESERVATIONS)
        .create_indexes(vec![
            index("uk_inquiry_quote_reservation_id", doc! {"id":1}, true),
            index("idx_inquiry_quote_reservation_purchase", doc! {"purchase_order_id":1,"released":1}, false),
            index("idx_inquiry_quote_reservation_quote", doc! {"quote_id":1,"quote_revision_id":1}, false),
        ])
        .await?;
    Ok(())
}
fn index(name: &str, keys: Document, unique: bool) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.to_string()).unique(unique).build())
        .build()
}
