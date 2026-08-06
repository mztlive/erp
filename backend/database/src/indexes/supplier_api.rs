//! 域 D25 `supplier_api` 的索引声明：supplier_api_connection、supplier_api_capability。
//!
//! 集合名常量取 `SupplierApiExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::SupplierApiExt;
use crate::Result;

/// `supplier_api_connection` 集合名。
pub(crate) const SUPPLIER_API_CONNECTIONS: &str =
    <mongodb::Database as SupplierApiExt>::SUPPLIER_API_CONNECTIONS;
/// `supplier_api_capability` 集合名。
pub(crate) const SUPPLIER_API_CAPABILITIES: &str =
    <mongodb::Database as SupplierApiExt>::SUPPLIER_API_CAPABILITIES;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.14「必需约束与索引」：`connection_code` 身份类字段使用
/// **全局唯一索引**（与 accounts 的 code 处理一致）：连接属于稳定配置对象，
/// 软删除后仍保留身份，避免复用连接代码破坏历史轨迹与恢复语义；
/// `(connection_id, capability_code)` 能力声明唯一由复合唯一索引保证。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, SUPPLIER_API_CONNECTIONS, supplier_api_connection_indexes()).await?;
    create_indexes(db, SUPPLIER_API_CAPABILITIES, supplier_api_capability_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `supplier_api_connection` 的身份约束与查询索引。
fn supplier_api_connection_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_supplier_api_connections_connection_code",
            doc! { "connection_code": 1 },
        ),
        named_index(
            "idx_supplier_api_connections_supplier_status",
            doc! { "supplier_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `supplier_api_capability` 的能力声明唯一约束索引。
fn supplier_api_capability_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_supplier_api_capabilities_connection_capability",
        doc! { "connection_id": 1, "capability_code": 1 },
    )]
}

/// 构建命名普通索引。
fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).build())
        .build()
}

/// 构建命名唯一索引。
fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{supplier_api_capability_indexes, supplier_api_connection_indexes};

    #[test]
    fn connection_code_index_is_globally_unique() {
        let indexes = supplier_api_connection_indexes();

        let code_index = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_api_connections_connection_code")
            })
            .unwrap();
        assert_eq!(code_index.keys, doc! { "connection_code": 1 });
        assert_eq!(code_index.options.as_ref().unwrap().unique, Some(true));
    }

    #[test]
    fn connection_supplier_status_index_covers_the_required_query() {
        let indexes = supplier_api_connection_indexes();

        assert!(indexes.iter().any(|index| {
            index.keys == doc! { "supplier_id": 1, "status": 1 }
                && index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("idx_supplier_api_connections_supplier_status")
        }));
    }

    #[test]
    fn capability_connection_capability_index_is_unique() {
        let indexes = supplier_api_capability_indexes();

        let capability_index = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_supplier_api_capabilities_connection_capability")
            })
            .unwrap();
        assert_eq!(
            capability_index.keys,
            doc! { "connection_id": 1, "capability_code": 1 }
        );
        assert_eq!(capability_index.options.as_ref().unwrap().unique, Some(true));
    }
}
