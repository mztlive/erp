//! 门户归属查询、定向开放和稳定命令索引。
use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use super::PortalSupplyExt;
/// 登记供应门户拥有的命名索引。
/// # 参数
/// `db` 为目标数据库。
/// # 返回
/// 索引登记成功。
/// # 错误
/// 数据冲突或数据库失败。
pub async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(<Database as PortalSupplyExt>::PORTAL_APPLICATIONS)
        .create_indexes(vec![
            index("uk_portal_offering_application_id", doc! {"id":1}, true),
            index(
                "idx_portal_offering_application_supplier_status",
                doc! {"supplier_id":1,"status":1,"updated_at":-1,"id":1},
                false,
            ),
        ])
        .await?;
    db.collection::<Document>(<Database as PortalSupplyExt>::PORTAL_QUOTE_GRANTS)
        .create_indexes(vec![
            index("uk_portal_quote_grant_supplier_sku", doc! {"supplier_id":1,"sku_id":1}, true),
            index("idx_portal_quote_grant_supplier_active", doc! {"supplier_id":1,"active":1,"id":1}, false),
        ])
        .await?;
    db.collection::<Document>(<Database as PortalSupplyExt>::PORTAL_COMMAND_RECEIPTS)
        .create_indexes(vec![
            index("uk_portal_command_receipt_id", doc! {"id":1}, true),
            index("idx_portal_command_receipt_supplier", doc! {"supplier_id":1,"created_at":-1}, false),
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
