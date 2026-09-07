//! 供给列表的唯一跨域批量读取，沿原当前指针装配展示事实。
use erp_catalog::{CatalogExt, Product, Sku, SkuRevision};
use erp_party::{Party, PartyExt, PartyRepository, PartyRevision, PartyRevisionRepository};
use erp_supplier::{SupplierAccount, SupplierAccountRepository, SupplierExt};
use erp_supply::entity::supplier_offering::SupplierOfferingRevision;
use erp_supply::repository::{supplier_offering::SupplierOfferingRow, SupplierOfferingExt};
use mongodb::{
    bson::{doc, Bson, Document},
    Database,
};
use persistence_core::{Executor, Result};
use std::collections::HashMap;

mod list_filter;
pub use list_filter::{SupplierOfferingListBundle, SupplierOfferingListQuery};
const SUPPLIER_ACCOUNTS: &str = <Database as SupplierExt>::SUPPLIER_ACCOUNTS;
const PARTIES: &str = <Database as PartyExt>::PARTIES;
const PARTY_REVISIONS: &str = <Database as PartyExt>::PARTY_REVISIONS;
/// 供给跨域只读仓储，不持有事务边界。
pub struct SupplierOfferingReadRepository<'a> {
    db: &'a Database,
}
impl<'a> SupplierOfferingReadRepository<'a> {
    /// 绑定数据库；构造不查询事实。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
    /// 批量读取冻结的供给当前修订；使用调用方执行器。
    async fn load_current_revisions(
        &self,
        rows: &[SupplierOfferingRow],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SupplierOfferingRevision>> {
        self.db
            .supplier_offering_repository()
            .load_current_revisions(rows, executor)
            .await
    }
    /// 批量加载供给列表展示所需的跨域只读实体。
    ///
    /// 本方法封装公司 SKU/商品、供应商账户及主体当前修订的批量 `$in` 查询，
    /// Service 只负责把返回实体组装为响应视图，不接触 BSON 查询细节。
    ///
    /// # 参数
    /// * `rows` - 当前页供给投影行
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回 SKU、SKU 当前修订、商品、供应商账户、主体、主体当前修订。
    ///
    /// # 错误
    /// 任一 MongoDB 批量查询失败时返回错误。
    pub async fn load_display_entities(
        &self,
        rows: &[SupplierOfferingRow],
        executor: &mut dyn Executor,
    ) -> Result<SupplierOfferingDisplayEntities> {
        let sku_ids = unique_strings(rows.iter().map(|row| row.sku_id.to_string()));
        let skus = self
            .db
            .skus()
            .find_many(in_filter("id", sku_ids), executor)
            .await?;
        let sku_revision_ids = unique_strings(
            skus.iter()
                .filter_map(|sku| sku.stable.current_revision_id.clone()),
        );
        let product_ids = unique_strings(skus.iter().map(|sku| sku.product_id.to_string()));
        let sku_revisions = self
            .db
            .sku_revisions()
            .find_many(in_filter("id", sku_revision_ids), executor)
            .await?;
        let products = self
            .db
            .products()
            .find_many(in_filter("id", product_ids), executor)
            .await?;

        let supplier_ids = unique_strings(rows.iter().map(|row| row.supplier_id.to_string()));
        let suppliers = SupplierAccountRepository::new(self.db, SUPPLIER_ACCOUNTS)
            .find_many(in_filter("id", supplier_ids), executor)
            .await?;
        let party_ids = unique_strings(suppliers.iter().map(|supplier| supplier.party_id.to_string()));
        let parties = PartyRepository::new(self.db, PARTIES)
            .find_many(in_filter("id", party_ids), executor)
            .await?;
        let party_revision_ids = unique_strings(
            parties
                .iter()
                .filter_map(|party| party.stable.current_revision_id.clone()),
        );
        let party_revisions = PartyRevisionRepository::new(self.db, PARTY_REVISIONS)
            .find_many(in_filter("id", party_revision_ids), executor)
            .await?;
        Ok(SupplierOfferingDisplayEntities {
            skus,
            sku_revisions,
            products,
            suppliers,
            parties,
            party_revisions,
        })
    }
}
/// 供给列表展示所需的跨域只读实体。
#[derive(Debug, Clone)]
pub struct SupplierOfferingDisplayEntities {
    /// 公司 SKU。
    pub skus: Vec<Sku>,
    /// SKU 当前修订。
    pub sku_revisions: Vec<SkuRevision>,
    /// 公司商品。
    pub products: Vec<Product>,
    /// 供应商账户。
    pub suppliers: Vec<SupplierAccount>,
    /// 主体。
    pub parties: Vec<Party>,
    /// 主体当前修订。
    pub party_revisions: Vec<PartyRevision>,
}
/// 对字符串集合排序去重，供跨域批量查询稳定生成 `$in` 候选。
fn unique_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut values = values.into_iter().collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}
fn in_filter(field: &str, values: impl IntoIterator<Item = String>) -> Document {
    let values = values.into_iter().map(Bson::String).collect::<Vec<_>>();
    doc! { field: { "$in": values } }
}
