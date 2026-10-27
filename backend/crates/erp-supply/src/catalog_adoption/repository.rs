use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::indexes::{named_index, unique_index};
use persistence_core::{Executor, Repository, Result};

use super::{CatalogAdoptionCommand, CatalogAdoptionPreview, CatalogAdoptionProduct, CatalogAdoptionReceipt};

const PREVIEWS: &str = "supplier_catalog_adoption_previews";
const RECEIPTS: &str = "supplier_catalog_adoption_receipts";
const PRODUCTS: &str = "supplier_catalog_adoption_products";
const COMMANDS: &str = "supplier_catalog_adoption_commands";

/// 选品冻结、原结果与来源商品映射的领域仓储。
pub trait CatalogAdoptionExt {
    /// 取得批次操作号原始载荷仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库仓储。
    /// # 错误
    /// 不返回错误。
    fn catalog_adoption_commands(&self) -> Repository<'_, CatalogAdoptionCommand>;
    /// 取得冻结预览仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库仓储。
    /// # 错误
    /// 不返回错误。
    fn catalog_adoption_previews(&self) -> Repository<'_, CatalogAdoptionPreview>;
    /// 取得永久选品回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库仓储。
    /// # 错误
    /// 不返回错误。
    fn catalog_adoption_receipts(&self) -> Repository<'_, CatalogAdoptionReceipt>;
    /// 取得来源商品与公司商品的唯一关系仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库仓储。
    /// # 错误
    /// 不返回错误。
    fn catalog_adoption_products(&self) -> Repository<'_, CatalogAdoptionProduct>;
}

impl CatalogAdoptionExt for Database {
    fn catalog_adoption_commands(&self) -> Repository<'_, CatalogAdoptionCommand> {
        Repository::new(self, COMMANDS)
    }
    fn catalog_adoption_previews(&self) -> Repository<'_, CatalogAdoptionPreview> {
        Repository::new(self, PREVIEWS)
    }
    fn catalog_adoption_receipts(&self) -> Repository<'_, CatalogAdoptionReceipt> {
        Repository::new(self, RECEIPTS)
    }
    fn catalog_adoption_products(&self) -> Repository<'_, CatalogAdoptionProduct> {
        Repository::new(self, PRODUCTS)
    }
}

/// 查询指定操作号是否已有该公司商品的成功选品回执。
/// # 参数
/// `db` 为当前数据库，`command` 为操作号，`product` 为公司商品身份，`executor` 为调用方执行器。
/// # 返回
/// 存在匹配回执时返回 true。
/// # 错误
/// 数据库查询失败时透传错误。
pub async fn has_product_receipt(
    db: &Database,
    command: &str,
    product: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    db.catalog_adoption_receipts()
        .exists(doc! {"command_id": command, "result.product_id": product}, executor)
        .await
}

/// 增量创建选品唯一身份及预览归属查询索引。
/// # 参数
/// `db` 为目标数据库。
/// # 返回
/// 所有命名索引建立成功。
/// # 错误
/// 唯一冲突或数据库失败时拒绝。
pub async fn ensure_indexes(db: &Database) -> Result<()> {
    db.collection::<CatalogAdoptionCommand>(COMMANDS)
        .create_index(unique_index("uk_catalog_adoption_command_id", doc! {"id":1}))
        .await?;
    db.collection::<CatalogAdoptionPreview>(PREVIEWS)
        .create_indexes([
            unique_index("uk_catalog_adoption_preview_id", doc! {"id":1}),
            named_index(
                "idx_catalog_adoption_preview_actor",
                doc! {"connection_id":1,"actor_id":1,"created_at":-1},
            ),
        ])
        .await?;
    db.collection::<CatalogAdoptionReceipt>(RECEIPTS)
        .create_indexes([
            named_index("idx_catalog_adoption_command_product", doc! {"command_id":1,"result.product_id":1}),
            unique_index("uk_catalog_adoption_receipt_id", doc! {"id":1}),
            unique_index("uk_catalog_adoption_source_item", doc! {"connection_id":1,"item_id":1}),
            unique_index("uk_catalog_adoption_command_item", doc! {"actor_id":1,"command_id":1,"item_id":1}),
        ])
        .await?;
    db.collection::<CatalogAdoptionProduct>(PRODUCTS)
        .create_indexes([
            unique_index("uk_catalog_adoption_product_id", doc! {"id":1}),
            unique_index(
                "uk_catalog_adoption_source_product",
                doc! {"connection_id":1,"source_product_id":1},
            ),
        ])
        .await?;
    Ok(())
}
