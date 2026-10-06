//! Owned proposal collection and supplier-scoped pagination.

use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Pagination, QueryFilter, Repository};

use super::{DraftStatus, NewProductDraft, SupplierCategoryMapping};
use crate::ProductKind;

/// Bounded supplier proposal list query.
pub struct CatalogDraftFilter {
    pub supplier_id: String,
    pub status: Option<DraftStatus>,
    pub page: u64,
    pub page_size: u32,
}

impl QueryFilter for CatalogDraftFilter {
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "supplier_id": &self.supplier_id };
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for CatalogDraftFilter {
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// Catalog-owned collection access; collection names have one source.
pub trait CatalogPortalExt {
    const NEW_PRODUCT_DRAFTS: &'static str = "supplier_new_product_drafts";
    const SUPPLIER_CATEGORY_MAPPINGS: &'static str = "supplier_category_mappings";

    /// Access proposals through the standard optimistic repository.
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库的提报仓储。
    /// # 错误
    /// 无。
    fn new_product_drafts(&self) -> Repository<'_, NewProductDraft>;

    /// 读取商品域拥有的供应商分类映射仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 标准乐观锁映射仓储。
    /// # 错误
    /// 无。
    fn supplier_category_mappings(&self) -> Repository<'_, SupplierCategoryMapping>;
}

impl CatalogPortalExt for Database {
    fn new_product_drafts(&self) -> Repository<'_, NewProductDraft> {
        Repository::new(self, Self::NEW_PRODUCT_DRAFTS)
    }

    fn supplier_category_mappings(&self) -> Repository<'_, SupplierCategoryMapping> {
        Repository::new(self, Self::SUPPLIER_CATEGORY_MAPPINGS)
    }
}

/// 精确读取一个供应商、完整原始路径及商品类型下的独立映射。
/// # 参数
/// 服务端供应商归属、去首尾空白的原始完整路径和商品类型。
/// # 返回
/// 三个精确等值条件组成的仓储查询。
/// # 错误
/// 无；输入由服务校验。
pub(super) fn category_mapping_filter(
    supplier_id: &str,
    original_category_path: &str,
    product_kind: ProductKind,
) -> Document {
    doc! {
        "supplier_id": supplier_id,
        "original_category_path": original_category_path,
        "product_kind": product_kind.as_str(),
    }
}
