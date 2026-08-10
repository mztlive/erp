//! 域 D10 `catalog` 仓储：product_category、product_brand、unit_of_measure、sku_attribute、
//! sku_attribute_value、product_category_attribute、product(+_revision、_revision_media)、
//! sku(+_revision)、sku_revision_attribute_value、voucher_category_profile_revision。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充域特有
//! 查询与跨集合多步骤写入入口。集合名常量统一从 `CatalogExt` 关联常量导入。
//!
//! 树形字典 `product_category` 的 P1 实体未定义 `internal_code` 物化路径字段（P1
//! 冻结），子树查询以「层序 `$in` 批量展开」实现：每层一次 `$in` 查询，不产生
//! N+1；`internal_code` 落库后可替换为前缀范围查询（见域报告偏差说明）。
//!
//! 筛选/行类型定义在本文件，经 `CatalogExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::{CatalogExt, SupplierOfferingExt};
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

use entities::catalog::sku_attribute::AttributeValueType;
use entities::catalog::{
    EnableStatus, ListingStatus, Product, ProductBrand, ProductCategory, ProductCategoryAttribute,
    ProductKind, ProductListingStatus, ProductRevision, ProductRevisionMedia, Sku, SkuAttribute,
    SkuAttributeValue, SkuCoverageStatus, SkuRevision, SkuRevisionAttributeValue, UnitOfMeasure,
    VoucherCategoryProfileRevision,
};
use entities::common::time::BusinessDate;
use entities::ids::{ProductCategoryId, ProductId, ProductRevisionId, SkuAttributeId, SkuId};
use entities::money::{Amount, Quantity};
use entities::supplier_offering::{AvailabilityStatus, OfferingStatus};

/// `product_revision` 集合名（单一来源：`CatalogExt` 关联常量）。
const PRODUCT_REVISIONS: &str = <mongodb::Database as CatalogExt>::PRODUCT_REVISIONS;
/// `product_revision_media` 集合名（单一来源：`CatalogExt` 关联常量）。
const PRODUCT_REVISION_MEDIAS: &str = <mongodb::Database as CatalogExt>::PRODUCT_REVISION_MEDIAS;
/// `sku` 集合名（单一来源：`CatalogExt` 关联常量）。
const SKUS: &str = <mongodb::Database as CatalogExt>::SKUS;
/// `sku_revision` 集合名（单一来源：`CatalogExt` 关联常量）。
const SKU_REVISIONS: &str = <mongodb::Database as CatalogExt>::SKU_REVISIONS;
/// `sku_revision_attribute_value` 集合名（单一来源：`CatalogExt` 关联常量）。
const SKU_REVISION_ATTRIBUTE_VALUES: &str = <mongodb::Database as CatalogExt>::SKU_REVISION_ATTRIBUTE_VALUES;
/// `supplier_offering` 集合名（公司商品池资格依赖的供给稳定身份）。
const SUPPLIER_OFFERINGS: &str = <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERINGS;
/// `supplier_offering_revision` 集合名（公司商品池资格依赖的当前供给修订）。
const SUPPLIER_OFFERING_REVISIONS: &str =
    <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERING_REVISIONS;
/// `supplier_offering_availability` 集合名（公司商品池资格依赖的实时可供投影）。
const SUPPLIER_OFFERING_AVAILABILITIES: &str =
    <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERING_AVAILABILITIES;

/// 公司商品池只读查询行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SellableSkuRow {
    /// 稳定 SKU ID。
    pub sku_id: String,
    /// 稳定 SKU 乐观锁版本。
    pub sku_version: u64,
    /// 当前且符合资格的 SKU 修订 ID。
    pub sku_revision_id: String,
    /// 当前 SKU 修订号。
    pub sku_revision_no: u32,
    /// SKU 编码。
    pub sku_no: String,
    /// 所属稳定商品 ID。
    pub product_id: String,
    /// 商品编码。
    pub product_no: String,
    /// 商品业务类型。
    pub product_kind: ProductKind,
    /// 公司审核后的 SKU 名称。
    pub name: String,
    /// 稳定 SKU 的规范化规格属性签名。
    pub specification_signature: String,
    /// 公司审核后的规格文案。
    pub specification: Option<String>,
    /// 条码。
    pub barcode: Option<String>,
    /// 基础单位 ID。
    pub base_unit_id: String,
    /// 基础单位编码。
    pub base_unit_code: Option<String>,
    /// 基础单位名称。
    pub base_unit_name: Option<String>,
    /// 公司销售可见含税价。
    pub sales_visible_price_gross: Amount,
    /// 市场参考价。
    pub market_price: Option<Amount>,
    /// SKU 主图文件 ID。
    pub main_image_asset_id: Option<String>,
    /// 当前 SKU 修订生效开始日。
    pub effective_from: BusinessDate,
    /// 当前 SKU 修订生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 当前有效供给对应的去重供应商数量。
    pub supplier_count: u32,
    /// 当前有效供给的可供区域并集。
    #[serde(default)]
    pub supply_regions: Vec<String>,
}

/// 公司商品池聚合分页结果。
#[derive(Debug, Deserialize)]
struct SellableSkuFacet {
    /// 当前页数据。
    items: Vec<SellableSkuRow>,
    /// 总数聚合行。
    total: Vec<SellableSkuTotal>,
}

/// 公司商品池总数聚合行。
#[derive(Debug, Deserialize)]
struct SellableSkuTotal {
    /// 符合资格的 SKU 数量。
    count: i64,
}

/// 商品分类列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductCategoryRow {
    /// 实体主键。
    pub id: String,
    /// 稳定分类代码。
    pub category_code: String,
    /// 父分类；空表示根分类。
    pub parent_category_id: Option<String>,
    /// 分类名称。
    pub name: String,
    /// 分类允许的商品类型。
    pub product_kind: ProductKind,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 商品分类列表筛选条件（树形字典：支持按父节点与名称筛选）。
#[derive(Debug, Clone)]
pub struct ProductCategoryFilter {
    /// 分类代码精确匹配；`None` 表示不筛选。
    pub category_code: Option<String>,
    /// 名称字面量正则（忽略大小写）；`None` 表示不筛选。
    pub name: Option<String>,
    /// 父分类筛选：`Some(Some(id))` 匹配该父节点的直接子节点，
    /// `Some(None)` 只匹配根节点，`None` 不筛选。
    pub parent_category_id: Option<Option<String>>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`category_code`/`name`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductCategoryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(code) = &self.category_code {
            filter.insert("category_code", code);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        if let Some(parent) = &self.parent_category_id {
            filter.insert(
                "parent_category_id",
                match parent {
                    Some(id) => Bson::String(id.to_string()),
                    None => Bson::Null,
                },
            );
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProductCategoryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductCategory> {
    /// 分页检索商品分类列表（投影查询）。
    ///
    /// 只返回 [`ProductCategoryRow`] 所需的列表字段，不加载整文档；
    /// 排序字段白名单化（`created_at`/`category_code`/`name`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_product_categories(
        &self,
        filter: &ProductCategoryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductCategoryRow>> {
        let options = FindOptions::builder()
            .sort(product_category_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_category_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductCategoryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 查询指定父分类的直接子节点（投影行，按分类代码升序）。
    ///
    /// # 参数
    /// * `parent_category_id` - 父分类；`None` 表示查询根分类
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回子分类投影行集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_children(
        &self,
        parent_category_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductCategoryRow>> {
        let parents = parent_category_id
            .map(|id| vec![id.to_string()])
            .unwrap_or_default();
        self.find_children_of(&parents, executor).await
    }

    /// 查询整个子树（含全部后代层级，投影行）。
    ///
    /// P1 实体未定义 `internal_code` 物化路径（P1 冻结），因此采用层序展开：
    /// 每层一次 `$in` 批量查询，避免逐行 N+1；`internal_code` 落库后
    /// 可替换为前缀范围查询。
    ///
    /// # 参数
    /// * `root_id` - 子树根节点
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回根节点之下全部层级的分类投影行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_subtree(
        &self,
        root_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductCategoryRow>> {
        let mut rows = Vec::new();
        let mut frontier = vec![root_id.to_string()];
        loop {
            let children = self.find_children_of(&frontier, executor).await?;
            if children.is_empty() {
                return Ok(rows);
            }
            frontier = children.iter().map(|row| row.id.clone()).collect();
            rows.extend(children);
        }
    }

    /// 层序批量查询一批父分类的直接子节点（`$in`，一次取回）。
    async fn find_children_of(
        &self,
        parent_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductCategoryRow>> {
        let filter = if parent_ids.is_empty() {
            doc! { "parent_category_id": null, "deleted_at": NOT_DELETED_TIMESTAMP_BSON }
        } else {
            doc! {
                "parent_category_id": { "$in": parent_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            }
        };
        let options = FindOptions::builder()
            .sort(doc! { "category_code": 1 })
            .projection(product_category_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductCategoryRow>();
        mongo_ops::find_many(&collection, filter, options, executor).await
    }
}

/// 商品品牌列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductBrandRow {
    /// 实体主键。
    pub id: String,
    /// 稳定品牌代码。
    pub brand_code: String,
    /// 品牌名称。
    pub name: String,
    /// 品牌 Logo（已登记受控文件，D05）。
    pub logo_asset_id: Option<String>,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 商品品牌列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProductBrandFilter {
    /// 品牌代码精确匹配；`None` 表示不筛选。
    pub brand_code: Option<String>,
    /// 名称字面量正则（忽略大小写）；`None` 表示不筛选。
    pub name: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`brand_code`/`name`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductBrandFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(code) = &self.brand_code {
            filter.insert("brand_code", code);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProductBrandFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductBrand> {
    /// 分页检索商品品牌列表（投影查询）。
    ///
    /// 只返回 [`ProductBrandRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`brand_code`/`name`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_product_brands(
        &self,
        filter: &ProductBrandFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductBrandRow>> {
        let options = FindOptions::builder()
            .sort(product_brand_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_brand_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductBrandRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 计量单位列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnitOfMeasureRow {
    /// 实体主键。
    pub id: String,
    /// 稳定单位代码。
    pub unit_code: String,
    /// 单位名称。
    pub name: String,
    /// 单位符号。
    pub symbol: String,
    /// 允许数量小数位。
    pub quantity_scale: u8,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 计量单位列表筛选条件。
#[derive(Debug, Clone)]
pub struct UnitOfMeasureFilter {
    /// 单位代码精确匹配；`None` 表示不筛选。
    pub unit_code: Option<String>,
    /// 名称字面量正则（忽略大小写）；`None` 表示不筛选。
    pub name: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`unit_code`/`name`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for UnitOfMeasureFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(code) = &self.unit_code {
            filter.insert("unit_code", code);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for UnitOfMeasureFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, UnitOfMeasure> {
    /// 分页检索计量单位列表（投影查询）。
    ///
    /// 只返回 [`UnitOfMeasureRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`unit_code`/`name`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_unit_of_measures(
        &self,
        filter: &UnitOfMeasureFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<UnitOfMeasureRow>> {
        let options = FindOptions::builder()
            .sort(unit_of_measure_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(unit_of_measure_projection())
            .build();
        let collection = self.collection().clone_with_type::<UnitOfMeasureRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 规格属性列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuAttributeRow {
    /// 实体主键。
    pub id: String,
    /// 稳定属性代码。
    pub attribute_code: String,
    /// 属性名称。
    pub name: String,
    /// 属性值类型。
    pub value_type: AttributeValueType,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 规格属性列表筛选条件。
#[derive(Debug, Clone)]
pub struct SkuAttributeFilter {
    /// 属性代码精确匹配；`None` 表示不筛选。
    pub attribute_code: Option<String>,
    /// 名称字面量正则（忽略大小写）；`None` 表示不筛选。
    pub name: Option<String>,
    /// 属性值类型；`None` 表示不筛选。
    pub value_type: Option<AttributeValueType>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`attribute_code`/`name`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SkuAttributeFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(code) = &self.attribute_code {
            filter.insert("attribute_code", code);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        if let Some(value_type) = self.value_type {
            filter.insert("value_type", value_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SkuAttributeFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SkuAttribute> {
    /// 分页检索规格属性列表（投影查询）。
    ///
    /// 只返回 [`SkuAttributeRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`attribute_code`/`name`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_sku_attributes(
        &self,
        filter: &SkuAttributeFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuAttributeRow>> {
        let options = FindOptions::builder()
            .sort(sku_attribute_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_attribute_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuAttributeRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 规格属性值列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuAttributeValueRow {
    /// 实体主键。
    pub id: String,
    /// 所属规格属性。
    pub attribute_id: String,
    /// 稳定属性值代码。
    pub value_code: String,
    /// 展示值。
    pub display_value: String,
    /// 展示排序。
    pub sort_order: i32,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 规格属性值列表筛选条件。
#[derive(Debug, Clone)]
pub struct SkuAttributeValueFilter {
    /// 所属规格属性；`None` 表示不筛选。
    pub attribute_id: Option<String>,
    /// 属性值代码精确匹配；`None` 表示不筛选。
    pub value_code: Option<String>,
    /// 展示值字面量正则（忽略大小写）；`None` 表示不筛选。
    pub display_value: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`value_code`/`display_value`/`sort_order`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SkuAttributeValueFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(attribute_id) = &self.attribute_id {
            filter.insert("attribute_id", attribute_id);
        }
        if let Some(code) = &self.value_code {
            filter.insert("value_code", code);
        }
        insert_literal_regex_filter(&mut filter, "display_value", self.display_value.as_deref());
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SkuAttributeValueFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SkuAttributeValue> {
    /// 分页检索规格属性值列表（投影查询）。
    ///
    /// 只返回 [`SkuAttributeValueRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`value_code`/`display_value`/`sort_order`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_sku_attribute_values(
        &self,
        filter: &SkuAttributeValueFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuAttributeValueRow>> {
        let options = FindOptions::builder()
            .sort(sku_attribute_value_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_attribute_value_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuAttributeValueRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 批量查询一组规格属性下的全部属性值（`$in`，一次取回）。
    ///
    /// 用于字典下拉与跨属性取值组装，避免逐属性 N+1。
    ///
    /// # 参数
    /// * `attribute_ids` - 规格属性 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除属性值实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_attribute_ids(
        &self,
        attribute_ids: &[SkuAttributeId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuAttributeValue>> {
        if attribute_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = attribute_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "attribute_id": { "$in": ids } }, executor)
            .await
    }
}

/// 分类-属性适用关系列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductCategoryAttributeRow {
    /// 实体主键。
    pub id: String,
    /// 商品分类。
    pub category_id: String,
    /// 规格属性。
    pub attribute_id: String,
    /// 是否必填。
    pub required_flag: bool,
    /// 展示排序。
    pub sort_order: i32,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 分类-属性适用关系列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProductCategoryAttributeFilter {
    /// 商品分类；`None` 表示不筛选。
    pub category_id: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`sort_order`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductCategoryAttributeFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(category_id) = &self.category_id {
            filter.insert("category_id", category_id);
        }
        filter
    }
}

impl Pagination for ProductCategoryAttributeFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductCategoryAttribute> {
    /// 分页检索分类-属性适用关系列表（投影查询）。
    ///
    /// 只返回 [`ProductCategoryAttributeRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`sort_order`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_product_category_attributes(
        &self,
        filter: &ProductCategoryAttributeFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductCategoryAttributeRow>> {
        let options = FindOptions::builder()
            .sort(product_category_attribute_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_category_attribute_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductCategoryAttributeRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 批量查询一组分类的适用属性（`$in`，一次取回）。
    ///
    /// 用于多分类并存的属性组装，避免逐分类 N+1。
    ///
    /// # 参数
    /// * `category_ids` - 商品分类 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除适用关系实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_category_ids(
        &self,
        category_ids: &[ProductCategoryId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductCategoryAttribute>> {
        if category_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = category_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "category_id": { "$in": ids } }, executor)
            .await
    }
}

/// 商品列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductRow {
    /// 实体主键。
    pub id: String,
    /// 商品编号。
    pub product_no: String,
    /// 商品业务类型。
    pub product_kind: ProductKind,
    /// 当前商品名称。
    pub name: Option<String>,
    /// 当前商品分类 ID。
    pub category_id: Option<String>,
    /// 当前商品品牌 ID。
    pub brand_id: Option<String>,
    /// 启停状态。
    pub status: EnableStatus,
    /// 从当前启用 SKU 继承的上架状态。
    pub listing_status: ProductListingStatus,
    /// 当前已上架 SKU 数。
    pub listed_sku_count: u32,
    /// 当前启用 SKU 总数。
    pub sku_count: u32,
    /// 当前存在有效供给关系的启用 SKU 数。
    pub supplied_sku_count: u32,
    /// 当前已填写销售价的启用 SKU 数。
    pub priced_sku_count: u32,
    /// 当前商品修订 ID。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 商品列表聚合分页结果。
#[derive(Debug, Deserialize)]
struct ProductFacet {
    /// 当前页数据。
    items: Vec<ProductRow>,
    /// 总数聚合行。
    total: Vec<ProductTotal>,
}

/// 商品列表总数聚合行。
#[derive(Debug, Deserialize)]
struct ProductTotal {
    /// 符合筛选的商品数量。
    count: i64,
}

/// 商品列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProductFilter {
    /// 商品编号字面量正则（忽略大小写）；`None` 表示不筛选。
    pub product_no: Option<String>,
    /// 商品与 SKU 统一关键字；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 商品业务类型；`None` 表示不筛选。
    pub product_kind: Option<ProductKind>,
    /// 当前商品分类；`None` 表示不筛选。
    pub category_id: Option<String>,
    /// 当前商品品牌；`None` 表示不筛选。
    pub brand_id: Option<String>,
    /// 当前启用 SKU 的有效供给供应商；`None` 表示不筛选。
    pub supplier_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 当前启用 SKU 继承上架状态；`None` 表示不筛选。
    pub listing_status: Option<ProductListingStatus>,
    /// 当前启用 SKU 的有效供给覆盖状态；`None` 表示不筛选。
    pub supply_coverage: Option<SkuCoverageStatus>,
    /// 当前启用 SKU 销售价下限（含）；`None` 表示无下限。
    pub sales_price_min: Option<Amount>,
    /// 当前启用 SKU 销售价上限（含）；`None` 表示无上限。
    pub sales_price_max: Option<Amount>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`product_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "product_no", self.product_no.as_deref());
        if let Some(product_kind) = self.product_kind {
            filter.insert("product_kind", product_kind.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProductFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 商品修订列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 所属商品 SPU。
    pub product_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 公司审核后的商品名称。
    pub name: String,
    /// 公司审核后的商品描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// ERP 分类 ID。
    pub category_id: String,
    /// ERP 品牌 ID。
    pub brand_id: String,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 商品修订列表筛选条件（修订表追加写入，无软删除过滤）。
#[derive(Debug, Clone)]
pub struct ProductRevisionFilter {
    /// 所属商品 SPU；`None` 表示不筛选。
    pub product_id: Option<String>,
    /// 修订启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订表不参与软删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(product_id) = &self.product_id {
            filter.insert("product_id", product_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProductRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductRevision> {
    /// 分页检索商品修订列表（投影查询）。
    ///
    /// 只返回 [`ProductRevisionRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`revision_no`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_product_revisions(
        &self,
        filter: &ProductRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRevisionRow>> {
        let options = FindOptions::builder()
            .sort(product_revision_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 批量查询一组商品的修订（`$in`，一次取回）。
    ///
    /// 用于按 SKU 所属商品聚合修订明细，避免逐商品 N+1。
    ///
    /// # 参数
    /// * `product_ids` - 商品 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的修订实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_product_ids(
        &self,
        product_ids: &[ProductId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductRevision>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = product_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "product_id": { "$in": ids } }, executor)
            .await
    }
}

/// SKU 列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuRow {
    /// 实体主键。
    pub id: String,
    /// SKU 编号。
    pub sku_no: String,
    /// 所属 SPU。
    pub product_id: String,
    /// 唯一基础单位。
    pub base_unit_id: String,
    /// 规范化规格签名。
    pub specification_signature: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 上架状态；旧文档缺失值按原有可售行为兼容为已上架。
    #[serde(default = "legacy_sku_listing_status")]
    pub listing_status: ListingStatus,
    /// 当前 SKU 修订 ID。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 上架概念引入前的 SKU 投影行保持原有可售行为。
fn legacy_sku_listing_status() -> ListingStatus {
    ListingStatus::Listed
}

/// SKU 列表筛选条件。
#[derive(Debug, Clone)]
pub struct SkuFilter {
    /// SKU 编号字面量正则（忽略大小写）；`None` 表示不筛选。
    pub sku_no: Option<String>,
    /// 所属 SPU；`None` 表示不筛选。
    pub product_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 上架状态；`None` 表示不筛选。
    pub listing_status: Option<ListingStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`sku_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SkuFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "sku_no", self.sku_no.as_deref());
        if let Some(product_id) = &self.product_id {
            filter.insert("product_id", product_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(listing_status) = self.listing_status {
            let value = match listing_status {
                ListingStatus::Listed => doc! { "$in": [ListingStatus::Listed.as_str(), null] },
                ListingStatus::Unlisted => doc! { "$eq": ListingStatus::Unlisted.as_str() },
            };
            filter.insert("listing_status", value);
        }
        filter
    }
}

impl Pagination for SkuFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductRevisionMedia> {
    /// 按商品修订 ID 批量读取媒体行。
    ///
    /// # 参数
    /// * `revision_ids` - 商品修订 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的媒体行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_media_by_revision_ids(
        &self,
        revision_ids: &[ProductRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductRevisionMedia>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "product_revision_id",
                revision_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, Sku> {
    /// 批量查询一组商品下的全部 SKU。
    ///
    /// # 参数
    /// * `product_ids` - 商品稳定 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回匹配的未删除 SKU 实体。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_product_ids(
        &self,
        product_ids: &[ProductId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter("product_id", product_ids.iter().map(ToString::to_string)),
            executor,
        )
        .await
    }

    /// 分页检索 SKU 列表（投影查询）。
    ///
    /// 只返回 [`SkuRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`sku_no`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_skus(
        &self,
        filter: &SkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRow>> {
        let options = FindOptions::builder()
            .sort(sku_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按（商品，规范化规格签名）查找稳定 SKU 身份。
    ///
    /// 唯一性由 `uk_skus_product_spec` 唯一索引保证（数据模型 §6.3：签名在全
    /// 生命周期永久唯一，不随停用释放）。
    ///
    /// # 参数
    /// * `product_id` - 所属 SPU
    /// * `specification_signature` - 规范化规格签名
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除 SKU；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_spec_signature(
        &self,
        product_id: &ProductId,
        specification_signature: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Sku>> {
        self.find_one(
            doc! {
                "product_id": product_id.to_string(),
                "specification_signature": specification_signature,
            },
            executor,
        )
        .await
    }
}

/// SKU 修订列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 所属稳定 SKU。
    pub sku_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 公司审核后的 SKU 名称。
    pub name: String,
    /// 公司审核后的 SKU 描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// 条码原值（规范化精确查询字段）。
    pub barcode: Option<String>,
    /// 来源 SKU 主图（已归档受控文件，D05）。
    pub source_main_image_asset_id: Option<String>,
    /// 重量（千克）。
    pub weight_kg: Option<Quantity>,
    /// 体积（立方米）。
    pub volume_m3: Option<Quantity>,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 公司对销售可见的含税价格（Decimal128 定点金额）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 市场参考价。
    pub market_price: Option<Amount>,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// SKU 修订列表筛选条件（修订表追加写入，无软删除过滤）。
#[derive(Debug, Clone)]
pub struct SkuRevisionFilter {
    /// 所属稳定 SKU；`None` 表示不筛选。
    pub sku_id: Option<String>,
    /// 名称字面量正则（忽略大小写）；`None` 表示不筛选。
    pub name: Option<String>,
    /// 条码精确匹配（按实体构造时 trim 规范化后的持久化值）；`None` 表示不筛选。
    pub barcode: Option<String>,
    /// 修订启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SkuRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订表不参与软删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        if let Some(barcode) = &self.barcode {
            filter.insert("barcode", normalized_barcode(barcode));
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SkuRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SkuRevision> {
    /// 分页检索 SKU 修订列表（投影查询）。
    ///
    /// 只返回 [`SkuRevisionRow`] 所需的列表字段（含 Decimal128 销售可见价，
    /// 不做舍入或换算）；排序字段白名单化（`created_at`/`revision_no`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_sku_revisions(
        &self,
        filter: &SkuRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRevisionRow>> {
        let options = FindOptions::builder()
            .sort(sku_revision_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按规范化条码精确查询全部「在用」SKU 修订。
    ///
    /// 条码走 `idx_sku_revisions_barcode` 精确查询索引；同一条码允许命中多个
    /// 在用 SKU 修订（数据模型 §6.3：同一条码出现多个在用 SKU 时由 Service
    /// 阻断正式启用并转人工，条码不是稳定身份）。
    ///
    /// # 参数
    /// * `barcode` - 条码原值（内部按 trim 规范化后精确匹配）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回启用状态且条码精确匹配的 SKU 修订集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_active_by_barcode(
        &self,
        barcode: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuRevision>> {
        self.find_many(
            doc! {
                "barcode": normalized_barcode(barcode),
                "status": EnableStatus::Active.as_str(),
            },
            executor,
        )
        .await
    }

    /// 批量查询一组 SKU 的修订（`$in`，一次取回）。
    ///
    /// 用于按 SKU 聚合修订明细，避免逐 SKU N+1。
    ///
    /// # 参数
    /// * `sku_ids` - SKU ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的修订实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_sku_ids(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuRevision>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = sku_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "sku_id": { "$in": ids } }, executor).await
    }
}

/// 卡券类目扩展修订列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoucherCategoryProfileRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 卡券类目使用的 VOUCHER SKU 稳定身份。
    pub sku_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 卡券类目描述。
    pub description: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 卡券类目扩展修订列表筛选条件（修订表追加写入，无软删除过滤）。
#[derive(Debug, Clone)]
pub struct VoucherCategoryProfileRevisionFilter {
    /// 卡券类目 SKU；`None` 表示不筛选。
    pub sku_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for VoucherCategoryProfileRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订表不参与软删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for VoucherCategoryProfileRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, VoucherCategoryProfileRevision> {
    /// 分页检索卡券类目扩展修订列表（投影查询）。
    ///
    /// 只返回 [`VoucherCategoryProfileRevisionRow`] 所需的列表字段；排序字段
    /// 白名单化（`created_at`/`revision_no`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_voucher_category_profile_revisions(
        &self,
        filter: &VoucherCategoryProfileRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<VoucherCategoryProfileRevisionRow>> {
        let options = FindOptions::builder()
            .sort(voucher_revision_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(voucher_revision_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<VoucherCategoryProfileRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// D10 域专用仓储：跨集合聚合查询与必须位于事务内的多步骤写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型承载公司商品池等跨集合
/// 查询，以及依赖事务的跨集合原子写入入口，由 `CatalogExt::catalog()` 访问。
pub struct CatalogRepository<'a> {
    db: &'a Database,
}

impl<'a> CatalogRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 分页查询商品及当前启用 SKU 的聚合筛选结果。
    ///
    /// 统一关键字覆盖商品编号/名称与 SKU 编号/名称/规格/条码；上架状态、
    /// 供给覆盖和销售价区间均按当前启用 SKU 实时派生，不在商品主表冗余落库。
    ///
    /// # 参数
    /// * `filter` - 商品、当前修订与 SKU 聚合筛选条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页商品投影与满足筛选条件的总数。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或结果反序列化失败时返回错误。
    pub async fn search_products(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRow>> {
        let facet = self
            .aggregate_products(product_list_pipeline(filter), executor)
            .await?;
        Ok(PageResult {
            items: facet.items,
            total: facet.total.first().map_or(0, |row| row.count),
        })
    }

    /// 执行商品列表类型化聚合并收集唯一的 facet 结果。
    async fn aggregate_products(
        &self,
        pipeline: Vec<Document>,
        executor: &mut dyn Executor,
    ) -> Result<ProductFacet> {
        let collection = self
            .db
            .collection::<Product>(<mongodb::Database as CatalogExt>::PRODUCTS);
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<ProductFacet>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            }
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<ProductFacet>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        Ok(rows.into_iter().next().unwrap_or(ProductFacet {
            items: Vec::new(),
            total: Vec::new(),
        }))
    }

    /// 分页查询销售可用的公司 SKU。
    ///
    /// 资格固定为：稳定 SKU 启用、当前 SKU 修订启用且处于生效区间并已配置
    /// 销售可见价、至少一条启用且当前修订可供并处于有效区间的供给。结果不含
    /// 供应商身份、采购成本、税率或起订量。
    ///
    /// # 参数
    /// * `keyword` - 可选名称、编码或条码关键字
    /// * `product_kind` - 可选公司商品类型
    /// * `eligibility_as_of` - 服务端解释的销售资格业务日期
    /// * `page` - 从一开始的页码
    /// * `page_size` - 单页条数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页资格投影及资格结果总数。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或结果反序列化失败时返回错误。
    pub async fn search_sellable_skus(
        &self,
        keyword: Option<&str>,
        product_kind: Option<ProductKind>,
        eligibility_as_of: BusinessDate,
        page: u64,
        page_size: u32,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SellableSkuRow>> {
        let match_doc = sellable_sku_match(keyword, product_kind, &[]);
        let pipeline = sellable_sku_pipeline(match_doc, eligibility_as_of, Some((page, page_size)));
        let facet = self.aggregate_sellable_skus(pipeline, executor).await?;
        Ok(PageResult {
            items: facet.items,
            total: facet.total.first().map_or(0, |row| row.count),
        })
    }

    /// 批量校验销售单引用的精确公司 SKU 修订是否仍具备销售资格。
    ///
    /// # 参数
    /// * `refs` - 销售单锁定的稳定 SKU 与修订 ID 集合
    /// * `eligibility_as_of` - 服务端解释的销售资格业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回仍符合资格的精确引用；调用方必须与请求集合逐项比对并 fail-closed。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或结果反序列化失败时返回错误。
    pub async fn find_sellable_sku_refs(
        &self,
        refs: &[(String, String)],
        eligibility_as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>> {
        if refs.is_empty() {
            return Ok(Vec::new());
        }
        let match_doc = sellable_sku_match(None, None, refs);
        let pipeline = sellable_sku_pipeline(match_doc, eligibility_as_of, None);
        Ok(self.aggregate_sellable_skus(pipeline, executor).await?.items)
    }

    /// 执行公司商品池类型化聚合并收集唯一的 facet 结果。
    async fn aggregate_sellable_skus(
        &self,
        pipeline: Vec<Document>,
        executor: &mut dyn Executor,
    ) -> Result<SellableSkuFacet> {
        let collection = self.db.collection::<Sku>(SKUS);
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<SellableSkuFacet>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            }
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<SellableSkuFacet>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        Ok(rows.into_iter().next().unwrap_or(SellableSkuFacet {
            items: Vec::new(),
            total: Vec::new(),
        }))
    }

    /// 建立「稳定 SKU + 首个 SKU 修订 + 修订规格属性值」（跨集合多步骤写入）。
    ///
    /// 依次写入 `skus`、`sku_revisions`、`sku_revision_attribute_values`，
    /// 保证「SKU 身份 + 修订快照 + 规格值」原子可见（数据模型 §6.3）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时各笔写入各自自动提交，中途失败会留下有 SKU 没有修订的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `sku` - 待写入的稳定 SKU
    /// * `revision` - 待写入的 SKU 首个修订
    /// * `attribute_values` - 待写入的修订规格属性值
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_sku_with_revision(
        &self,
        sku: &Sku,
        revision: &SkuRevision,
        attribute_values: &[SkuRevisionAttributeValue],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<Sku>(SKUS), sku, executor).await?;
        mongo_ops::insert_one(
            &self.db.collection::<SkuRevision>(SKU_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SkuRevisionAttributeValue>(SKU_REVISION_ATTRIBUTE_VALUES),
            attribute_values.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }

    /// 建立「商品修订 + SPU 级媒体行」（跨集合多步骤写入）。
    ///
    /// 依次写入 `product_revisions` 与 `product_revision_medias`，保证修订与
    /// 媒体行原子可见（数据模型 §6.3：媒体行随所属修订一并不可变）。
    /// **必须收到事务执行器**：传入 `NoTransaction` 时两笔写入各自自动提交，
    /// 中途失败会留下没有媒体行的修订；Service 必须通过事务会话调用。
    ///
    /// # 参数
    /// * `revision` - 待写入的商品修订
    /// * `medias` - 待写入的修订媒体行
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_product_revision_with_media(
        &self,
        revision: &ProductRevision,
        medias: &[ProductRevisionMedia],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<ProductRevision>(PRODUCT_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<ProductRevisionMedia>(PRODUCT_REVISION_MEDIAS),
            medias.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 按实体构造时的 trim 规则规范化条码（与 `SkuRevision::new` 一致）。
///
/// # 参数
/// * `barcode` - 条码原值
///
/// # 返回
/// 返回去除首尾空白后的规范化值。
fn normalized_barcode(barcode: &str) -> &str {
    barcode.trim()
}

/// 构造 ID 集合批量匹配条件。
///
/// # 参数
/// * `field` - 匹配字段名
/// * `values` - 待匹配的 ID 字符串集合
///
/// # 返回
/// 返回批量查询条件文档。
fn in_filter(field: &str, values: impl IntoIterator<Item = String>) -> Document {
    let values: Vec<Bson> = values.into_iter().map(Bson::String).collect();
    doc! { field: { "$in": values } }
}

/// 构造商品列表的当前修订与 SKU 聚合管道。
fn product_list_pipeline(filter: &ProductFilter) -> Vec<Document> {
    let not_deleted = NOT_DELETED_TIMESTAMP_BSON;
    let mut pipeline = vec![
        doc! { "$match": filter.to_doc() },
        product_revision_lookup(not_deleted),
        doc! { "$unwind": { "path": "$product_revision", "preserveNullAndEmptyArrays": true } },
    ];
    append_product_revision_filters(&mut pipeline, filter);
    pipeline.extend([
        product_sku_lookup(not_deleted),
        doc! {
            "$set": {
                "sku_count": { "$size": "$skus" },
                "listed_sku_count": {
                    "$size": {
                        "$filter": {
                            "input": "$skus",
                            "as": "sku",
                            "cond": "$$sku.is_listed",
                        }
                    }
                },
                "supplied_sku_count": {
                    "$size": {
                        "$filter": {
                            "input": "$skus",
                            "as": "sku",
                            "cond": "$$sku.is_supplied",
                        }
                    }
                },
                "priced_sku_count": {
                    "$size": {
                        "$filter": {
                            "input": "$skus",
                            "as": "sku",
                            "cond": "$$sku.is_priced",
                        }
                    }
                },
            }
        },
        doc! {
            "$set": {
                "listing_status": {
                    "$switch": {
                        "branches": [
                            {
                                "case": {
                                    "$and": [
                                        { "$gt": ["$sku_count", 0] },
                                        { "$eq": ["$listed_sku_count", "$sku_count"] },
                                    ]
                                },
                                "then": ProductListingStatus::Listed.as_str(),
                            },
                            {
                                "case": { "$gt": ["$listed_sku_count", 0] },
                                "then": ProductListingStatus::PartiallyListed.as_str(),
                            },
                        ],
                        "default": ProductListingStatus::Unlisted.as_str(),
                    }
                }
            }
        },
    ]);
    append_product_aggregate_filters(&mut pipeline, filter);
    pipeline.push(product_facet(filter));
    pipeline
}

/// 构造商品当前修订关联，列表只读取稳定商品指向的当前版本。
fn product_revision_lookup(not_deleted: i64) -> Document {
    doc! {
        "$lookup": {
            "from": PRODUCT_REVISIONS,
            "let": { "revision_id": "$current_revision_id" },
            "pipeline": [{
                "$match": {
                    "$expr": { "$eq": ["$id", "$$revision_id"] },
                    "deleted_at": not_deleted,
                }
            }],
            "as": "product_revision",
        }
    }
}

/// 构造当前启用 SKU、当前 SKU 修订与有效供给关系的批量关联。
fn product_sku_lookup(not_deleted: i64) -> Document {
    doc! {
        "$lookup": {
            "from": SKUS,
            "let": { "product_id": "$id" },
            "pipeline": [
                {
                    "$match": {
                        "$expr": { "$eq": ["$product_id", "$$product_id"] },
                        "deleted_at": not_deleted,
                        "status": EnableStatus::Active.as_str(),
                    }
                },
                {
                    "$lookup": {
                        "from": SKU_REVISIONS,
                        "let": { "revision_id": "$current_revision_id" },
                        "pipeline": [{
                            "$match": {
                                "$expr": { "$eq": ["$id", "$$revision_id"] },
                                "deleted_at": not_deleted,
                            }
                        }],
                        "as": "revision",
                    }
                },
                { "$unwind": { "path": "$revision", "preserveNullAndEmptyArrays": true } },
                {
                    "$lookup": {
                        "from": SUPPLIER_OFFERINGS,
                        "let": { "sku_id": "$id" },
                        "pipeline": [{
                            "$match": {
                                "$expr": { "$eq": ["$sku_id", "$$sku_id"] },
                                "deleted_at": not_deleted,
                                "status": OfferingStatus::Active.as_str(),
                                "current_revision_id": { "$ne": null },
                            }
                        }, {
                            "$project": { "_id": 0, "supplier_id": 1 }
                        }],
                        "as": "offerings",
                    }
                },
                {
                    "$project": {
                        "_id": 0,
                        "sku_no": 1,
                        "specification_signature": 1,
                        "revision": 1,
                        "sales_price": "$revision.sales_visible_price_gross",
                        "supplier_ids": { "$setUnion": ["$offerings.supplier_id", []] },
                        "is_listed": {
                            "$eq": [
                                { "$ifNull": ["$listing_status", ListingStatus::Listed.as_str()] },
                                ListingStatus::Listed.as_str(),
                            ]
                        },
                        "is_supplied": { "$gt": [{ "$size": "$offerings" }, 0] },
                        "is_priced": {
                            "$ne": [
                                { "$ifNull": ["$revision.sales_visible_price_gross", null] },
                                null,
                            ]
                        },
                    }
                },
            ],
            "as": "skus",
        }
    }
}

/// 追加只依赖当前商品修订的分类与品牌筛选。
fn append_product_revision_filters(pipeline: &mut Vec<Document>, filter: &ProductFilter) {
    if let Some(category_id) = &filter.category_id {
        pipeline.push(doc! { "$match": { "product_revision.category_id": category_id } });
    }
    if let Some(brand_id) = &filter.brand_id {
        pipeline.push(doc! { "$match": { "product_revision.brand_id": brand_id } });
    }
}

/// 追加统一关键字与 SKU 聚合状态筛选。
fn append_product_aggregate_filters(pipeline: &mut Vec<Document>, filter: &ProductFilter) {
    if let Some(keyword) = filter.keyword.as_deref() {
        let pattern = regex::escape(keyword);
        pipeline.push(doc! {
            "$match": {
                "$or": [
                    { "product_no": { "$regex": &pattern, "$options": "i" } },
                    { "product_revision.name": { "$regex": &pattern, "$options": "i" } },
                    { "skus.sku_no": { "$regex": &pattern, "$options": "i" } },
                    { "skus.specification_signature": { "$regex": &pattern, "$options": "i" } },
                    { "skus.revision.name": { "$regex": &pattern, "$options": "i" } },
                    { "skus.revision.specification": { "$regex": &pattern, "$options": "i" } },
                    { "skus.revision.barcode": { "$regex": &pattern, "$options": "i" } },
                ]
            }
        });
    }
    if let Some(status) = filter.listing_status {
        pipeline.push(doc! { "$match": { "listing_status": status.as_str() } });
    }
    if let Some(supplier_id) = &filter.supplier_id {
        pipeline.push(doc! { "$match": { "skus.supplier_ids": supplier_id } });
    }
    append_coverage_filter(pipeline, "supplied_sku_count", filter.supply_coverage);
    append_sales_price_filter(pipeline, filter.sales_price_min, filter.sales_price_max);
}

/// 追加销售价闭区间筛选；商品下至少一个当前启用 SKU 的销售价必须落入区间。
fn append_sales_price_filter(pipeline: &mut Vec<Document>, minimum: Option<Amount>, maximum: Option<Amount>) {
    if minimum.is_none() && maximum.is_none() {
        return;
    }
    let mut range = Document::new();
    if let Some(minimum) = minimum {
        range.insert("$gte", amount_filter_bson(minimum));
    }
    if let Some(maximum) = maximum {
        range.insert("$lte", amount_filter_bson(maximum));
    }
    pipeline.push(doc! {
        "$match": {
            "skus": {
                "$elemMatch": { "sales_price": range }
            }
        }
    });
}

/// 把查询金额转换为与库内价格一致的 BSON Decimal128。
fn amount_filter_bson(amount: Amount) -> Bson {
    Bson::Decimal128(
        amount
            .to_string()
            .parse()
            .expect("合法 Amount 必须可转换为 MongoDB Decimal128"),
    )
}

/// 追加完整、部分或无覆盖筛选；空 SKU 集合只属于无覆盖。
fn append_coverage_filter(
    pipeline: &mut Vec<Document>,
    count_field: &str,
    coverage: Option<SkuCoverageStatus>,
) {
    let count_ref = format!("${count_field}");
    let expression = match coverage {
        Some(SkuCoverageStatus::Complete) => doc! {
            "$and": [
                { "$gt": ["$sku_count", 0] },
                { "$eq": [count_ref, "$sku_count"] },
            ]
        },
        Some(SkuCoverageStatus::Partial) => doc! {
            "$and": [
                { "$gt": [&count_ref, 0] },
                { "$lt": [count_ref, "$sku_count"] },
            ]
        },
        Some(SkuCoverageStatus::None) => doc! { "$eq": [count_ref, 0] },
        None => return,
    };
    pipeline.push(doc! { "$match": { "$expr": expression } });
}

/// 构造商品列表分页、排序与投影 facet。
fn product_facet(filter: &ProductFilter) -> Document {
    let item_stages = vec![
        doc! { "$sort": product_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending) },
        doc! { "$skip": filter.skip() as i64 },
        doc! { "$limit": filter.limit() },
        doc! {
            "$project": {
                "_id": 0,
                "id": 1,
                "product_no": 1,
                "product_kind": 1,
                "name": "$product_revision.name",
                "category_id": "$product_revision.category_id",
                "brand_id": "$product_revision.brand_id",
                "status": 1,
                "listing_status": 1,
                "listed_sku_count": 1,
                "sku_count": 1,
                "supplied_sku_count": 1,
                "priced_sku_count": 1,
                "current_revision_id": 1,
                "version": 1,
                "created_at": 1,
            }
        },
    ];
    doc! {
        "$facet": {
            "items": item_stages,
            "total": [{ "$count": "count" }],
        }
    }
}

/// 构造公司商品池聚合的稳定 SKU 前置过滤。
fn sellable_sku_match(
    keyword: Option<&str>,
    product_kind: Option<ProductKind>,
    refs: &[(String, String)],
) -> Document {
    let mut filter = doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "status": EnableStatus::Active.as_str(),
        "listing_status": { "$in": [ListingStatus::Listed.as_str(), null] },
        "current_revision_id": { "$ne": null },
    };
    if !refs.is_empty() {
        filter.insert(
            "$or",
            refs.iter()
                .map(|(sku_id, sku_revision_id)| {
                    Bson::Document(doc! {
                        "id": sku_id,
                        "current_revision_id": sku_revision_id,
                    })
                })
                .collect::<Vec<_>>(),
        );
    }
    if let Some(kind) = product_kind {
        filter.insert("__product_kind", bson_for(kind));
    }
    if let Some(value) = keyword.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("__keyword", regex::escape(value));
    }
    filter
}

/// 把可序列化枚举转换为 BSON 标量。
fn bson_for<T: Serialize>(value: T) -> Bson {
    mongodb::bson::to_bson(&value).expect("域枚举必须可序列化为 BSON")
}

/// 构造公司商品池资格与只读投影聚合管道。
fn sellable_sku_pipeline(
    mut initial_match: Document,
    eligibility_as_of: BusinessDate,
    paging: Option<(u64, u32)>,
) -> Vec<Document> {
    let product_kind = initial_match.remove("__product_kind");
    let keyword = initial_match.remove("__keyword");
    let as_of = eligibility_as_of.to_string();
    let not_deleted = NOT_DELETED_TIMESTAMP_BSON;
    let zero_quantity = Bson::Decimal128("0".parse().expect("零必须可转换为 MongoDB Decimal128"));
    let mut after_lookups = Vec::new();
    if let Some(kind) = product_kind {
        after_lookups.push(doc! { "$match": { "product.product_kind": kind } });
    }
    if let Some(Bson::String(pattern)) = keyword {
        after_lookups.push(doc! {
            "$match": {
                "$or": [
                    { "sku_no": { "$regex": &pattern, "$options": "i" } },
                    { "sku_revision.name": { "$regex": &pattern, "$options": "i" } },
                    { "product.product_no": { "$regex": &pattern, "$options": "i" } },
                ]
            }
        });
    }

    let (skip, limit) = paging.map_or((0_i64, i64::MAX), |(page, page_size)| {
        (
            ((page.saturating_sub(1)) * u64::from(page_size)) as i64,
            i64::from(page_size),
        )
    });
    let mut item_stages = vec![doc! { "$sort": { "sku_no": 1, "id": 1 } }, doc! { "$skip": skip }];
    if paging.is_some() {
        item_stages.push(doc! { "$limit": limit });
    }
    item_stages.push(doc! {
        "$project": {
            "_id": 0,
            "sku_id": "$id",
            "sku_version": "$version",
            "sku_revision_id": "$sku_revision.id",
            "sku_revision_no": "$sku_revision.revision_no",
            "sku_no": 1,
            "product_id": 1,
            "product_no": "$product.product_no",
            "product_kind": "$product.product_kind",
            "name": "$sku_revision.name",
            "specification_signature": 1,
            "specification": "$sku_revision.specification",
            "barcode": "$sku_revision.barcode",
            "base_unit_id": 1,
            "base_unit_code": "$unit.unit_code",
            "base_unit_name": "$unit.name",
            "sales_visible_price_gross": "$sku_revision.sales_visible_price_gross",
            "market_price": "$sku_revision.market_price",
            "main_image_asset_id": "$sku_revision.source_main_image_asset_id",
            "effective_from": "$sku_revision.effective_from",
            "effective_to": "$sku_revision.effective_to",
            "supplier_count": { "$size": "$supplier_ids" },
            "supply_regions": 1,
        }
    });

    let mut pipeline = vec![
        doc! { "$match": initial_match },
        doc! {
            "$lookup": {
                "from": SKU_REVISIONS,
                "let": { "revision_id": "$current_revision_id" },
                "pipeline": [{
                    "$match": {
                        "$expr": { "$eq": ["$id", "$$revision_id"] },
                        "deleted_at": not_deleted,
                        "status": EnableStatus::Active.as_str(),
                        "sales_visible_price_gross": { "$ne": null },
                        "effective_from": { "$lte": &as_of },
                        "$or": [
                            { "effective_to": null },
                            { "effective_to": { "$gt": &as_of } },
                        ]
                    }
                }],
                "as": "sku_revision"
            }
        },
        doc! { "$unwind": "$sku_revision" },
        doc! {
            "$lookup": {
                "from": <mongodb::Database as CatalogExt>::PRODUCTS,
                "let": { "product_id": "$product_id" },
                "pipeline": [{
                    "$match": {
                        "$expr": { "$eq": ["$id", "$$product_id"] },
                        "deleted_at": not_deleted,
                        "status": EnableStatus::Active.as_str(),
                        "current_revision_id": { "$ne": null },
                    }
                }],
                "as": "product"
            }
        },
        doc! { "$unwind": "$product" },
        doc! {
            "$lookup": {
                "from": PRODUCT_REVISIONS,
                "let": { "revision_id": "$product.current_revision_id" },
                "pipeline": [{
                    "$match": {
                        "$expr": { "$eq": ["$id", "$$revision_id"] },
                        "deleted_at": not_deleted,
                        "status": EnableStatus::Active.as_str(),
                        "effective_from": { "$lte": &as_of },
                        "$or": [
                            { "effective_to": null },
                            { "effective_to": { "$gt": &as_of } },
                        ]
                    }
                }],
                "as": "product_revision"
            }
        },
        doc! { "$unwind": "$product_revision" },
        doc! {
            "$lookup": {
                "from": <mongodb::Database as CatalogExt>::UNIT_OF_MEASURES,
                "let": { "unit_id": "$base_unit_id" },
                "pipeline": [{
                    "$match": {
                        "$expr": { "$eq": ["$id", "$$unit_id"] },
                        "deleted_at": not_deleted,
                    }
                }],
                "as": "unit"
            }
        },
        doc! { "$unwind": { "path": "$unit", "preserveNullAndEmptyArrays": true } },
        doc! {
            "$lookup": {
                "from": SUPPLIER_OFFERINGS,
                "let": { "sku_id": "$id" },
                "pipeline": [
                    {
                        "$match": {
                            "$expr": { "$eq": ["$sku_id", "$$sku_id"] },
                            "deleted_at": not_deleted,
                            "status": OfferingStatus::Active.as_str(),
                            "current_revision_id": { "$ne": null },
                        }
                    },
                    {
                        "$lookup": {
                            "from": SUPPLIER_OFFERING_REVISIONS,
                            "let": { "revision_id": "$current_revision_id" },
                            "pipeline": [{
                                "$match": {
                                    "$expr": { "$eq": ["$id", "$$revision_id"] },
                                    "deleted_at": not_deleted,
                                    "valid_from": { "$lte": &as_of },
                                    "$or": [
                                        { "valid_to": null },
                                        { "valid_to": { "$gt": &as_of } },
                                    ]
                                }
                            }],
                            "as": "revision"
                        }
                    },
                    { "$unwind": "$revision" },
                    {
                        "$lookup": {
                            "from": SUPPLIER_OFFERING_AVAILABILITIES,
                            "let": { "offering_id": "$id" },
                            "pipeline": [{
                                "$match": {
                                    "$expr": { "$eq": ["$supplier_offering_id", "$$offering_id"] },
                                    "deleted_at": not_deleted,
                                    "availability_status": AvailabilityStatus::Available.as_str(),
                                    "$or": [
                                        { "available_quantity": null },
                                        { "available_quantity": { "$gt": &zero_quantity } },
                                    ],
                                }
                            }],
                            "as": "availability"
                        }
                    },
                    { "$unwind": "$availability" },
                    { "$project": { "_id": 0, "supplier_id": 1, "supply_region": "$revision.supply_region" } },
                ],
                "as": "offerings"
            }
        },
        doc! { "$match": { "offerings.0": { "$exists": true } } },
        doc! {
            "$set": {
                "supplier_ids": { "$setUnion": ["$offerings.supplier_id", []] },
                "supply_regions": {
                    "$reduce": {
                        "input": "$offerings.supply_region",
                        "initialValue": [],
                        "in": { "$setUnion": ["$$value", "$$this"] }
                    }
                }
            }
        },
    ];
    pipeline.extend(after_lookups);
    pipeline.push(doc! {
        "$facet": {
            "items": item_stages,
            "total": [{ "$count": "count" }],
        }
    });
    pipeline
}

/// 构建商品分类排序文档（白名单：`created_at`/`category_code`/`name`）。
///
/// # 参数
/// * `sort_by` - 排序字段；非法或缺失时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn product_category_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("category_code") => "category_code",
        Some("name") => "name",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建商品品牌排序文档（白名单：`created_at`/`brand_code`/`name`）。
fn product_brand_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("brand_code") => "brand_code",
        Some("name") => "name",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建计量单位排序文档（白名单：`created_at`/`unit_code`/`name`）。
fn unit_of_measure_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("unit_code") => "unit_code",
        Some("name") => "name",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建规格属性排序文档（白名单：`created_at`/`attribute_code`/`name`）。
fn sku_attribute_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("attribute_code") => "attribute_code",
        Some("name") => "name",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建规格属性值排序文档（白名单：`created_at`/`value_code`/`display_value`/`sort_order`）。
fn sku_attribute_value_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("value_code") => "value_code",
        Some("display_value") => "display_value",
        Some("sort_order") => "sort_order",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建分类-属性适用关系排序文档（白名单：`created_at`/`sort_order`）。
fn product_category_attribute_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("sort_order") => "sort_order",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建商品排序文档（白名单：`created_at`/`product_no`）。
fn product_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("product_no") => "product_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建商品修订排序文档（白名单：`created_at`/`revision_no`）。
fn product_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("revision_no") => "revision_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建 SKU 排序文档（白名单：`created_at`/`sku_no`）。
fn sku_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("sku_no") => "sku_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建 SKU 修订排序文档（白名单：`created_at`/`revision_no`）。
fn sku_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("revision_no") => "revision_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建卡券类目修订排序文档（白名单：`created_at`/`revision_no`）。
fn voucher_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("revision_no") => "revision_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建排序文档。
///
/// # 参数
/// * `field` - 已通过白名单校验的排序字段
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(field: &str, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction }
}

/// 商品分类列表投影字段。
fn product_category_projection() -> Document {
    doc! {
        "id": 1,
        "category_code": 1,
        "parent_category_id": 1,
        "name": 1,
        "product_kind": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 商品品牌列表投影字段。
fn product_brand_projection() -> Document {
    doc! {
        "id": 1,
        "brand_code": 1,
        "name": 1,
        "logo_file_asset_id": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 计量单位列表投影字段。
fn unit_of_measure_projection() -> Document {
    doc! {
        "id": 1,
        "unit_code": 1,
        "name": 1,
        "symbol": 1,
        "quantity_scale": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 规格属性列表投影字段。
fn sku_attribute_projection() -> Document {
    doc! {
        "id": 1,
        "attribute_code": 1,
        "name": 1,
        "value_type": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 规格属性值列表投影字段。
fn sku_attribute_value_projection() -> Document {
    doc! {
        "id": 1,
        "attribute_id": 1,
        "value_code": 1,
        "display_value": 1,
        "sort_order": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 分类-属性适用关系列表投影字段。
fn product_category_attribute_projection() -> Document {
    doc! {
        "id": 1,
        "category_id": 1,
        "attribute_id": 1,
        "required_flag": 1,
        "sort_order": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 商品修订列表投影字段。
fn product_revision_projection() -> Document {
    doc! {
        "id": 1,
        "product_id": 1,
        "revision_no": 1,
        "name": 1,
        "description": 1,
        "specification": 1,
        "category_id": 1,
        "brand_id": 1,
        "status": 1,
        "effective_from": 1,
        "effective_to": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// SKU 列表投影字段。
fn sku_projection() -> Document {
    doc! {
        "id": 1,
        "sku_no": 1,
        "product_id": 1,
        "base_unit_id": 1,
        "specification_signature": 1,
        "status": 1,
        "listing_status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// SKU 修订列表投影字段。
fn sku_revision_projection() -> Document {
    doc! {
        "id": 1,
        "sku_id": 1,
        "revision_no": 1,
        "name": 1,
        "description": 1,
        "specification": 1,
        "barcode": 1,
        "source_main_image_asset_id": 1,
        "weight_kg": 1,
        "volume_m3": 1,
        "status": 1,
        "sales_visible_price_gross": 1,
        "market_price": 1,
        "effective_from": 1,
        "effective_to": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 卡券类目扩展修订列表投影字段。
fn voucher_revision_projection() -> Document {
    doc! {
        "id": 1,
        "sku_id": 1,
        "revision_no": 1,
        "description": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        amount_filter_bson, product_list_pipeline, sellable_sku_match, sellable_sku_pipeline, sort_doc,
        ProductFilter, QueryFilter, SkuAttributeFilter, SkuAttributeValueFilter, SkuFilter,
        SkuRevisionFilter, SkuRow,
    };
    use entities::catalog::sku_attribute::AttributeValueType;
    use entities::catalog::{
        EnableStatus, ListingStatus, ProductKind, ProductListingStatus, SkuCoverageStatus,
    };
    use entities::common::time::BusinessDate;
    use entities::money::Amount;
    use mongodb::bson::{doc, Bson};
    use std::str::FromStr;

    #[test]
    fn sku_attribute_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SkuAttributeFilter {
            attribute_code: Some("SIZE".to_string()),
            name: Some("尺".to_string()),
            value_type: Some(AttributeValueType::Enum),
            status: Some(EnableStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("attribute_code").unwrap(), "SIZE");
        assert_eq!(document.get_str("value_type").unwrap(), "enum");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn sku_attribute_value_filter_applies_regex_and_attribute_scoping() {
        let filter = SkuAttributeValueFilter {
            attribute_id: Some("attr-1".to_string()),
            value_code: Some("L".to_string()),
            display_value: Some("大号".to_string()),
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("attribute_id").unwrap(), "attr-1");
        assert_eq!(document.get_str("value_code").unwrap(), "L");
        assert!(document.get("display_value").is_some());
        assert!(document.get("status").is_none());
    }

    #[test]
    fn sku_revision_filter_normalizes_barcode_for_exact_query() {
        let filter = SkuRevisionFilter {
            sku_id: Some("sku-1".to_string()),
            name: None,
            barcode: Some(" 6901234567890 ".to_string()),
            status: Some(EnableStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("barcode").unwrap(), "6901234567890");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn sku_filter_applies_listing_status() {
        let filter = SkuFilter {
            sku_no: None,
            product_id: Some("product-1".to_string()),
            status: Some(EnableStatus::Active),
            listing_status: Some(ListingStatus::Listed),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("product_id").unwrap(), "product-1");
        assert_eq!(document.get_str("status").unwrap(), "active");
        let listing = document.get_document("listing_status").unwrap();
        assert_eq!(listing.get_array("$in").unwrap().len(), 2);
    }

    #[test]
    fn legacy_sku_row_without_listing_status_is_treated_as_listed() {
        let row: SkuRow = mongodb::bson::from_document(doc! {
            "id": "sku-1",
            "sku_no": "SKU-001",
            "product_id": "product-1",
            "base_unit_id": "unit-1",
            "specification_signature": "",
            "status": "active",
            "current_revision_id": "sku-revision-1",
            "version": 1_i64,
            "created_at": 1_i64,
        })
        .unwrap();

        assert_eq!(row.listing_status, ListingStatus::Listed);
    }

    #[test]
    fn sort_doc_applies_direction() {
        assert_eq!(sort_doc("created_at", false), doc! { "created_at": -1 });
        assert_eq!(sort_doc("sku_no", true), doc! { "sku_no": 1 });
    }

    /// 商品列表统一搜索必须覆盖商品与 SKU 字段，并在分页前应用聚合筛选。
    #[test]
    fn product_list_pipeline_applies_keyword_and_sku_coverage_filters() {
        let filter = ProductFilter {
            product_no: None,
            keyword: Some("礼盒.*".to_string()),
            product_kind: Some(ProductKind::Physical),
            category_id: Some("category-1".to_string()),
            brand_id: Some("brand-1".to_string()),
            supplier_id: Some("supplier-1".to_string()),
            status: Some(EnableStatus::Active),
            listing_status: Some(ProductListingStatus::PartiallyListed),
            supply_coverage: Some(SkuCoverageStatus::Complete),
            sales_price_min: Some(Amount::from_str("100.00").unwrap()),
            sales_price_max: Some(Amount::from_str("200.00").unwrap()),
            page: 2,
            page_size: 20,
            sort_by: Some("product_no".to_string()),
            sort_ascending: true,
        };
        let pipeline = product_list_pipeline(&filter);
        let json = format!("{pipeline:?}");

        assert!(json.contains("product_revision.name"));
        assert!(json.contains("skus.sku_no"));
        assert!(json.contains("skus.revision.barcode"));
        assert!(json.contains("supplied_sku_count"));
        assert!(json.contains("priced_sku_count"));
        assert!(json.contains("sales_price"));
        assert!(json.contains("$gte"));
        assert!(json.contains("$lte"));
        let Bson::Decimal128(minimum) = amount_filter_bson(Amount::from_str("100.00").unwrap()) else {
            panic!("销售价筛选值必须使用 Decimal128");
        };
        assert_eq!(minimum.to_string(), "100.00");
        assert!(json.contains("partially_listed"));
        assert!(json.contains("category-1"));
        assert!(json.contains("brand-1"));
        assert!(json.contains("supplier_ids"));
        assert!(json.contains("supplier-1"));
        assert!(json.contains("礼盒"));
        assert!(!json.contains("礼盒.*"));
        assert!(json.contains("$facet"));
    }

    /// 公司商品池管道同时约束 SKU 已上架、当前修订与有效供给，并禁止投影采购成本。
    #[test]
    fn sellable_sku_pipeline_is_fail_closed_and_cost_safe() {
        let date = BusinessDate::from_ymd(2026, 8, 8).unwrap();
        let filter = sellable_sku_match(Some("礼盒"), Some(ProductKind::Physical), &[]);
        let pipeline = sellable_sku_pipeline(filter, date, Some((2, 20)));
        let json = format!("{pipeline:?}");

        assert!(json.contains("sales_visible_price_gross"));
        assert!(json.contains("listing_status"));
        assert!(json.contains("listed"));
        assert!(json.contains("availability_status"));
        assert!(json.contains("available_quantity"));
        assert!(json.contains("$gt"));
        assert!(json.contains("AVAILABLE"));
        assert!(json.contains("product_revision"));
        assert!(json.contains("specification_signature"));
        assert!(json.contains("supplier_count"));
        assert!(json.contains("eligibility") || json.contains("valid_from"));
        assert!(!json.contains("dropship_supply_price"));
        assert!(!json.contains("bulk_supply_price"));
        assert!(!json.contains("input_tax_rate"));
    }

    /// 销售单资格复核按稳定 SKU 与当前修订成对匹配，禁止只按 SKU 猜修订。
    #[test]
    fn sellable_sku_reference_filter_matches_exact_pair() {
        let refs = vec![("sku-1".to_string(), "sku-rev-2".to_string())];
        let filter = sellable_sku_match(None, None, &refs);
        let serialized = format!("{filter:?}");

        assert!(serialized.contains("sku-1"));
        assert!(serialized.contains("sku-rev-2"));
        assert!(serialized.contains("current_revision_id"));
    }
}
