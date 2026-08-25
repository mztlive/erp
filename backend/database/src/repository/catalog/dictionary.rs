use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::catalog::voucher_defaults::{
    VOUCHER_DEFAULT_BRAND_CODE, VOUCHER_DEFAULT_BRAND_NAME, VOUCHER_DEFAULT_UNIT_CODE,
    VOUCHER_ROOT_CATEGORY_CODE,
};
use entities::catalog::{EnableStatus, ProductBrand, ProductCategory, UnitOfMeasure};
use entities::ids::{ProductBrandId, ProductCategoryId, UnitOfMeasureId};

use super::super::extensions::CatalogExt;
use super::super::regex_filter::insert_literal_regex_filter;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::{in_filter, sort_doc};
use super::CatalogRepository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

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

/// 商品创建或编辑时引用的分类、品牌与基础单位批量快照。
#[derive(Debug, Clone)]
pub struct CatalogReferenceData {
    /// 可选分类；调用方未要求分类查询或分类不存在时为空。
    pub category: Option<ProductCategory>,
    /// 品牌；不存在时为空。
    pub brand: Option<ProductBrand>,
    /// 命中的基础单位集合。
    pub units: Vec<UnitOfMeasure>,
}

impl<'a> CatalogRepository<'a> {
    /// 批量读取商品创建或编辑引用的分类、品牌与基础单位。
    ///
    /// # 参数
    /// * `category_id` - 可选分类稳定 ID；内联新建分类时传 `None`
    /// * `brand_id` - 品牌稳定 ID
    /// * `unit_ids` - SKU 基础单位稳定 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中的字典实体快照；缺失项由 Service 映射为业务错误。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn catalog_reference_data(
        &self,
        category_id: Option<&ProductCategoryId>,
        brand_id: &ProductBrandId,
        unit_ids: &[UnitOfMeasureId],
        executor: &mut dyn Executor,
    ) -> Result<CatalogReferenceData> {
        let category = match category_id {
            Some(category_id) => {
                self.db
                    .product_categories()
                    .find_by_id(category_id.as_ref(), executor)
                    .await?
            }
            None => None,
        };
        let brand = self
            .db
            .product_brands()
            .find_by_id(brand_id.as_ref(), executor)
            .await?;
        let units = if unit_ids.is_empty() {
            Vec::new()
        } else {
            self.db
                .unit_of_measures()
                .find_many(
                    in_filter("id", unit_ids.iter().map(ToString::to_string)),
                    executor,
                )
                .await?
        };
        Ok(CatalogReferenceData {
            category,
            brand,
            units,
        })
    }

    /// 按卡券默认稳定代码读取共用根分类。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回代码为 `VOUCHER` 的未删除分类；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn voucher_root_category(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductCategory>> {
        self.db
            .product_categories()
            .find_one_by_field("category_code", VOUCHER_ROOT_CATEGORY_CODE.to_string(), executor)
            .await
    }

    /// 按稳定代码或兼容名称读取卡券默认品牌。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 优先返回代码 `FSY` 的品牌；无代码命中时兼容返回名称“福尚云”的品牌。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn voucher_default_brand(&self, executor: &mut dyn Executor) -> Result<Option<ProductBrand>> {
        let by_code = self
            .db
            .product_brands()
            .find_one_by_field("brand_code", VOUCHER_DEFAULT_BRAND_CODE.to_string(), executor)
            .await?;
        if by_code.is_some() {
            return Ok(by_code);
        }
        self.db
            .product_brands()
            .find_one_by_field("name", VOUCHER_DEFAULT_BRAND_NAME.to_string(), executor)
            .await
    }

    /// 按稳定代码读取卡券默认基础单位。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回代码为“张”的未删除计量单位；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn voucher_default_unit(&self, executor: &mut dyn Executor) -> Result<Option<UnitOfMeasure>> {
        self.db
            .unit_of_measures()
            .find_one_by_field("unit_code", VOUCHER_DEFAULT_UNIT_CODE.to_string(), executor)
            .await
    }
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
