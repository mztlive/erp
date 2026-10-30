use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index, unique_partial_index};

use super::repository::{MEMBERS, SCANS};
use crate::repository::supplier_catalog::ITEMS;

/// 登记完整范围、账号扫描围栏及逐池成员查询索引，不清除历史证据。
/// # 参数
/// `db` 为当前应用数据库。
/// # 返回
/// 新增集合和目录规格反查索引全部建立成功。
/// # 错误
/// 既有重复身份、重复活动扫描或数据库错误时拒绝启动。
pub async fn ensure_indexes(db: &Database) -> Result<()> {
    db.collection::<Document>(SCANS).create_indexes(vec![
        unique_index("uk_catalog_membership_scan_id", doc! {"id":1}),
        unique_partial_index("uk_catalog_membership_active_account", doc! {"connection_id":1,"scope.account_scope":1}, doc! {"active":true,"deleted_at":0_i64}),
        named_index("idx_catalog_membership_baseline", doc! {"connection_id":1,"supplier_id":1,"scope.account_scope":1,"state":1,"completed_at":-1,"id":-1}),
        named_index("idx_catalog_membership_namespace", doc! {"connection_id":1,"scope.account_scope":1,"id":1}),
    ]).await?;
    db.collection::<Document>(MEMBERS).create_indexes(vec![
        unique_index("uk_catalog_pool_member_id", doc! {"id":1}),
        unique_index("uk_catalog_pool_member_source", doc! {"connection_id":1,"account_scope":1,"pool_id":1,"sku.spec_id":1}),
        named_index("idx_catalog_pool_member_reconcile", doc! {"connection_id":1,"supplier_id":1,"account_scope":1,"active":1,"id":1}),
        named_index("idx_catalog_pool_member_current", doc! {"connection_id":1,"supplier_id":1,"account_scope":1,"sku.spec_id":1,"last_seen_run":1,"pool_id":1,"active":1}),
    ]).await?;
    db.collection::<Document>(ITEMS)
        .create_indexes(vec![named_index(
            "idx_supplier_catalog_spec_membership",
            doc! {"connection_id":1,"spec_id":1,"deleted_at":1,"id":1},
        )])
        .await?;
    Ok(())
}
