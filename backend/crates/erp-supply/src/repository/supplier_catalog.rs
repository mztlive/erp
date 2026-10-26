//! 供应商商品观察与扫描检查点；所有查询限定连接。
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::supplier_catalog::{SupplierCatalogItem, SupplierCatalogRun};

pub const ITEMS: &str = "supplier_catalog_items";
pub const RUNS: &str = "supplier_catalog_runs";

/// 供应商目录仓储访问入口。
pub trait SupplierCatalogExt {
    /// 取得目录记录仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库仓储。
    /// # 错误
    /// 不返回错误。
    fn supplier_catalog_items(&self) -> Repository<'_, SupplierCatalogItem>;
    /// 取得扫描检查点仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库仓储。
    /// # 错误
    /// 不返回错误。
    fn supplier_catalog_runs(&self) -> Repository<'_, SupplierCatalogRun>;
}
impl SupplierCatalogExt for Database {
    fn supplier_catalog_items(&self) -> Repository<'_, SupplierCatalogItem> {
        Repository::new(self, ITEMS)
    }
    fn supplier_catalog_runs(&self) -> Repository<'_, SupplierCatalogRun> {
        Repository::new(self, RUNS)
    }
}

/// 分页读取当前连接目录，稳定按名称和身份排序。
/// # 参数
/// 分页已由应用边界验证；`executor` 沿用调用方事务。
/// # 返回
/// 目录记录与本连接总数。
/// # 错误
/// 数据库查询失败时返回错误。
pub async fn page(
    db: &Database,
    connection: &str,
    page: u32,
    size: u32,
    executor: &mut dyn Executor,
) -> Result<(Vec<SupplierCatalogItem>, u64)> {
    let collection = db.collection::<SupplierCatalogItem>(ITEMS);
    let filter = doc! { "connection_id": connection };
    let total = mongo_ops::count_documents(&collection, filter.clone(), executor).await?;
    let options = FindOptions::builder()
        .sort(doc! { "name": 1, "id": 1 })
        .skip(u64::from(page - 1) * u64::from(size))
        .limit(i64::from(size))
        .build();
    let items = mongo_ops::find_many(&collection, filter, options, executor).await?;
    Ok((items, total))
}
