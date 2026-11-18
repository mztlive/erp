//! 技术凭据身份及人工调查索引；不得对授权或刷新未知记录配置TTL。

use mongodb::bson::{Document, doc};
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use crate::repository::SupplierCredentialExt;

pub(super) fn definitions() -> Vec<IndexModel> {
    vec![
        unique_index("uk_supplier_credentials_id", doc! {"id":1}),
        unique_index("uk_supplier_credentials_binding", doc! {"provider":1,"account_scope":1,"app_key":1}),
        named_index(
            "ix_supplier_credentials_refresh",
            doc! {"provider":1,"refresh.state":1,"refresh.lease_until":1},
        ),
    ]
}

/// 仅创建技术凭据必要索引，首次明确导入之前必须调用本入口。
///
/// 凭据引用及账号应用唯一约束和刷新查询索引已经建立。现有重复身份、权限不足或索引创建失败时拒绝后续导入。
///
/// `db` 为目标应用数据库；只读预览和状态查询不得调用本入口。
pub async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(Database::SUPPLIER_CREDENTIALS).create_indexes(definitions()).await?;
    Ok(())
}
