//! 采购责任筛选的最小目录事实；不装载名称、金额、媒体或规格全文。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::CatalogRepository;
use super::shared::{PRODUCT_CATEGORIES, PRODUCT_REVISIONS, SKUS, batch_ids_filter};
use crate::entity::catalog::ProductKind;
use crate::repository::CatalogExt;

/// 商品稳定身份及现行采购上下文。
#[derive(Debug, Deserialize)]
pub struct ProcurementProductFact {
    /// 商品身份。
    pub id: String,
    /// 原商品类型。
    pub product_kind: ProductKind,
    /// 商品当前修订指针。
    pub current_revision_id: Option<String>,
}

/// SKU 稳定身份及所属商品；保留原解析包含停用 SKU 的口径。
#[derive(Debug, Deserialize)]
pub struct ProcurementSkuFact {
    /// SKU 身份。
    pub id: String,
    /// 商品身份。
    pub product_id: String,
}

/// 商品当前修订中的分类引用。
#[derive(Debug, Deserialize)]
pub struct ProcurementRevisionFact {
    /// 修订身份。
    pub id: String,
    /// 修订引用的分类。
    pub category_id: String,
}

/// 分类父指针；不含字典展示内容。
#[derive(Debug, Deserialize)]
pub struct ProcurementCategoryFact {
    /// 分类身份。
    pub id: String,
    /// 父分类；根分类为空。
    pub parent_category_id: Option<String>,
}

impl CatalogRepository<'_> {
    /// 批量读取候选商品的类型和当前修订指针。
    ///
    /// # 参数
    /// * `ids` - 已授权且满足基础筛选的商品主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回未软删除商品的最小事实；空输入不查询。
    ///
    /// # 错误
    /// MongoDB 读取或投影反序列化失败时返回错误。
    pub async fn procurement_products(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementProductFact>> {
        self.procurement_projection(
            <mongodb::Database as CatalogExt>::PRODUCTS,
            "id",
            ids,
            doc! { "id": 1, "product_kind": 1, "current_revision_id": 1 },
            executor,
        )
        .await
    }

    /// 批量读取候选商品下所有未删除 SKU 身份。
    ///
    /// # 参数
    /// * `product_ids` - 候选商品主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回 SKU 身份和商品引用；保持采购解析原有的所有 SKU 口径。
    ///
    /// # 错误
    /// MongoDB 读取或反序列化失败时返回错误。
    pub async fn procurement_product_skus(
        &self,
        product_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementSkuFact>> {
        self.procurement_projection(SKUS, "product_id", product_ids, sku_projection(), executor).await
    }

    /// 批量读取供给引用的未删除 SKU 身份。
    ///
    /// # 参数
    /// * `ids` - 供给引用的 SKU 主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回 SKU 身份和商品引用；空输入不查询。
    ///
    /// # 错误
    /// MongoDB 读取或反序列化失败时返回错误。
    pub async fn procurement_skus(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementSkuFact>> {
        self.procurement_projection(SKUS, "id", ids, sku_projection(), executor).await
    }

    /// 批量读取当前商品修订的分类引用。
    ///
    /// # 参数
    /// * `ids` - 商品稳定身份当前指向的修订主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回未删除修订中的分类事实，保持当前指针语义。
    ///
    /// # 错误
    /// MongoDB 读取或反序列化失败时返回错误。
    pub async fn procurement_revisions(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementRevisionFact>> {
        self.procurement_projection(
            PRODUCT_REVISIONS,
            "id",
            ids,
            doc! { "id": 1, "category_id": 1 },
            executor,
        )
        .await
    }

    /// 一次读取一层分类父指针，供请求内祖先批量展开。
    ///
    /// # 参数
    /// * `ids` - 当前层去重后的分类主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回未删除分类的父引用；未返回的分类由调用方作为缺失末节点处理。
    ///
    /// # 错误
    /// MongoDB 读取或反序列化失败时返回错误。
    pub async fn procurement_categories(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcurementCategoryFact>> {
        self.procurement_projection(
            PRODUCT_CATEGORIES,
            "id",
            ids,
            doc! { "id": 1, "parent_category_id": 1 },
            executor,
        )
        .await
    }

    /// 读取单一领域集合的最小采购事实，空集合及软删除口径统一处理。
    async fn procurement_projection<T: DeserializeOwned + Send + Sync>(
        &self,
        collection: &str,
        field: &str,
        ids: &[String],
        projection: Document,
        executor: &mut dyn Executor,
    ) -> Result<Vec<T>> {
        let Some(mut filter) = batch_ids_filter(field, ids) else {
            return Ok(Vec::new());
        };
        filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
        mongo_ops::find_many(
            &self.db.collection::<T>(collection),
            filter,
            FindOptions::builder().projection(projection).build(),
            executor,
        )
        .await
    }
}

/// 两类 SKU 采购事实查询共用稳定身份和商品引用投影。
fn sku_projection() -> Document {
    doc! { "id": 1, "product_id": 1 }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::deserialize_from_document;

    use super::*;

    #[test]
    /// 最小事实不要求实体中的名称、金额、媒体、状态或系统审计字段。
    fn procurement_projection_accepts_only_required_facts() {
        let product: ProcurementProductFact = deserialize_from_document(doc! {
            "id": "product", "product_kind": "PHYSICAL", "current_revision_id": "revision"
        })
        .unwrap();
        let sku: ProcurementSkuFact =
            deserialize_from_document(doc! { "id": "sku", "product_id": "product" }).unwrap();
        let revision: ProcurementRevisionFact =
            deserialize_from_document(doc! { "id": "revision", "category_id": "leaf" }).unwrap();
        let category: ProcurementCategoryFact =
            deserialize_from_document(doc! { "id": "leaf", "parent_category_id": "parent" }).unwrap();
        assert_eq!(product.current_revision_id.as_deref(), Some(revision.id.as_str()));
        assert_eq!(product.id, sku.product_id);
        assert_eq!(revision.category_id, category.id);
        assert_eq!(category.parent_category_id.as_deref(), Some("parent"));
    }

    #[test]
    /// 缺少业务身份时拒绝；允许原有缺失当前修订和根分类。
    fn procurement_projection_rejects_missing_identity_and_keeps_optional_links() {
        assert!(deserialize_from_document::<ProcurementSkuFact>(doc! { "product_id": "product" }).is_err());
        let product: ProcurementProductFact =
            deserialize_from_document(doc! { "id": "product", "product_kind": "PHYSICAL" }).unwrap();
        let category: ProcurementCategoryFact = deserialize_from_document(doc! { "id": "root" }).unwrap();
        assert!(product.current_revision_id.is_none());
        assert!(category.parent_category_id.is_none());
    }
}
