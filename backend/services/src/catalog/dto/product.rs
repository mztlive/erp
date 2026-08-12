use entities::catalog::product_revision_media::MediaRole;
use entities::catalog::{
    EnableStatus, ListingStatus, ProductKind, ProductListingStatus, ProductRevision, Sku, SkuCoverageStatus,
    SkuRevision,
};
use entities::common::time::BusinessDate;
use entities::ids::{
    FileAssetId, ProductBrandId, ProductCategoryId, ProductId, SkuId, SkuRevisionId, UnitOfMeasureId,
};
use entities::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// 商品列表允许的排序字段白名单。
pub(crate) const PRODUCT_SORT_FIELDS: &[&str] = &["created_at", "product_no"];
/// 商品修订列表允许的排序字段白名单。
pub(crate) const PRODUCT_REVISION_SORT_FIELDS: &[&str] = &["created_at", "revision_no"];
/// SKU 列表允许的排序字段白名单。
pub(crate) const SKU_SORT_FIELDS: &[&str] = &["created_at", "sku_no"];
/// SKU 修订列表允许的排序字段白名单。
pub(crate) const SKU_REVISION_SORT_FIELDS: &[&str] = &["created_at", "revision_no"];

/// 商品（SPU）修订媒体输入（轮播图/详情图）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductMediaInput {
    /// 合规媒体文件（`file_asset`，D05）。
    pub file_asset_id: FileAssetId,
    /// 版本内展示顺序（非负）。
    #[validate(range(min = 0, message = "展示顺序不能为负数"))]
    pub sort_order: i32,
    /// 无障碍替代文本。
    pub alt_text: Option<String>,
}

/// SKU 规格名-值输入。
///
/// 规格在所属 SPU 内直接定义，不要求预先维护全局规格属性或枚举字典。
/// HTTP 字段名为兼容既有契约继续使用 `attribute_code` / `attribute_value_code`。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SpecEntryInput {
    /// SPU 局部规格名（例如“颜色”）。
    #[validate(custom(function = "non_blank", message = "规格名不能为空"))]
    pub attribute_code: String,
    /// SPU 局部规格值（例如“红色”）。
    #[validate(custom(function = "non_blank", message = "规格值不能为空"))]
    pub attribute_value_code: String,
}

/// SKU 输入行（W14 规格组合表的一行）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductSkuInput {
    /// 既有 SKU 的稳定 ID；创建或新增规格签名时必须为空。
    #[serde(default)]
    pub sku_id: Option<SkuId>,
    /// 既有 SKU 的期望当前修订 ID；用于阻断并发覆盖。
    #[serde(default)]
    pub expected_sku_revision_id: Option<SkuRevisionId>,
    /// 历史停用签名是否明确重新启用。
    #[serde(default)]
    pub reenable: bool,
    /// SKU 编号（全局唯一业务编码，允许手动覆盖）。
    #[validate(custom(function = "non_blank", message = "SKU编号不能为空"))]
    pub sku_no: String,
    /// 公司审核后的 SKU 名称（写入 SKU 修订快照，可与商品名称不同）。
    #[validate(custom(function = "non_blank", message = "SKU名称不能为空"))]
    pub name: String,
    /// 唯一基础单位（`unit_of_measure` 启用字典项）。
    pub base_unit_id: UnitOfMeasureId,
    /// 条码原值（可空）。
    pub barcode: Option<String>,
    /// 来源 SKU 主图（已归档受控文件，D05；可空）。
    pub main_image_asset_id: Option<FileAssetId>,
    /// 重量（千克，非负定点数）。
    pub weight_kg: Option<Quantity>,
    /// 体积（立方米，非负定点数）。
    pub volume_m3: Option<Quantity>,
    /// 公司对销售可见的含税价（非负定点金额）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 市场展示参考价（非负定点金额）。
    pub market_price: Option<Amount>,
    /// 规格属性-值对（空表示无规格 SKU）。
    #[serde(default)]
    #[validate(nested)]
    pub spec_entries: Vec<SpecEntryInput>,
}

/// 商品（SPU）创建请求（W14 正向建品：SPU + 首个商品修订 + 媒体 + 全部 SKU 行，
/// 在一个事务内原子写入）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateProductRequest {
    /// 创建原因，写入同一事务内的审计日志。
    pub change_reason: Option<String>,
    /// 商品编号（全局唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "商品编号不能为空"))]
    pub product_no: String,
    /// 商品业务类型（独立必填稳定属性，创建后不可变）。
    pub product_kind: ProductKind,
    /// 公司审核后的商品名称。
    #[validate(custom(function = "non_blank", message = "商品名称不能为空"))]
    pub name: String,
    /// 公司审核后的描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// ERP 分类。
    pub category_id: ProductCategoryId,
    /// ERP 品牌。
    pub brand_id: ProductBrandId,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
    /// SPU 轮播图媒体行（可空）。
    #[serde(default)]
    #[validate(nested)]
    pub carousel_media: Vec<ProductMediaInput>,
    /// SPU 详情图媒体行（可空）。
    #[serde(default)]
    #[validate(nested)]
    pub detail_media: Vec<ProductMediaInput>,
    /// SKU 行（至少一行）。
    #[validate(length(min = 1, message = "至少需要一个SKU"))]
    #[validate(nested)]
    pub skus: Vec<ProductSkuInput>,
}

/// 商品（SPU）规格编辑请求（W14 编辑商品：一次性提交修订后的全部 SKU 行，
/// 服务端按规范化签名分类为保留/新增/重新启用/移除）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateProductRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 重新启用历史停用 SKU 时的变更原因。
    pub change_reason: Option<String>,
    /// 公司审核后的商品名称。
    #[validate(custom(function = "non_blank", message = "商品名称不能为空"))]
    pub name: String,
    /// 公司审核后的描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// ERP 分类。
    pub category_id: ProductCategoryId,
    /// ERP 品牌。
    pub brand_id: ProductBrandId,
    /// 启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
    /// SPU 轮播图媒体行（可空）。
    #[serde(default)]
    #[validate(nested)]
    pub carousel_media: Vec<ProductMediaInput>,
    /// SPU 详情图媒体行（可空）。
    #[serde(default)]
    #[validate(nested)]
    pub detail_media: Vec<ProductMediaInput>,
    /// SKU 行（至少一行）。
    #[validate(length(min = 1, message = "至少需要一个SKU"))]
    #[validate(nested)]
    pub skus: Vec<ProductSkuInput>,
}

/// 商品（SPU）响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductView {
    /// 实体主键。
    pub id: String,
    /// 商品编号。
    pub product_no: String,
    /// 商品业务类型。
    pub product_kind: ProductKind,
    /// 当前商品名称；没有当前修订时为空。
    pub name: Option<String>,
    /// 当前商品分类；没有当前修订时为空。
    pub category_id: Option<String>,
    /// 当前商品品牌；没有当前修订时为空。
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
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

/// SPU 下全部当前启用 SKU 的上/下架请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProductListingRequest {
    /// 目标上架状态；一次性应用于 SPU 下全部当前启用 SKU。
    pub listing_status: ListingStatus,
}

/// 单个 SKU 上/下架请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSkuListingRequest {
    /// SKU 期望乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 目标上架状态。
    pub listing_status: ListingStatus,
}

/// SPU 继承上架状态响应。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductListingView {
    /// 商品稳定 ID。
    pub product_id: String,
    /// 从当前启用 SKU 继承的状态。
    pub listing_status: ProductListingStatus,
    /// 当前已上架 SKU 数。
    pub listed_sku_count: u32,
    /// 当前启用 SKU 总数。
    pub sku_count: u32,
}

/// 商品列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductListParams {
    /// 商品编号字面量筛选（忽略大小写）。
    pub product_no: Option<String>,
    /// 商品与 SKU 统一关键字（商品编号/名称、SKU 编号/名称/规格/条码）。
    pub keyword: Option<String>,
    /// 商品业务类型筛选。
    pub product_kind: Option<ProductKind>,
    /// 当前商品分类筛选。
    pub category_id: Option<String>,
    /// 当前商品品牌筛选。
    pub brand_id: Option<String>,
    /// 当前启用 SKU 的有效供给供应商筛选。
    pub supplier_id: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 从当前启用 SKU 继承的上架状态筛选。
    pub listing_status: Option<ProductListingStatus>,
    /// 当前启用 SKU 的有效供给覆盖状态。
    pub supply_coverage: Option<SkuCoverageStatus>,
    /// 当前启用 SKU 销售价下限（含）。
    pub sales_price_min: Option<Amount>,
    /// 当前启用 SKU 销售价上限（含）。
    pub sales_price_max: Option<Amount>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`product_no`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的商品列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProductListQuery {
    /// 商品编号筛选。
    pub product_no: Option<String>,
    /// 商品与 SKU 统一关键字。
    pub keyword: Option<String>,
    /// 商品业务类型筛选。
    pub product_kind: Option<ProductKind>,
    /// 当前商品分类筛选。
    pub category_id: Option<String>,
    /// 当前商品品牌筛选。
    pub brand_id: Option<String>,
    /// 有效供给供应商筛选。
    pub supplier_id: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// SKU 继承上架状态筛选。
    pub listing_status: Option<ProductListingStatus>,
    /// 有效供给覆盖筛选。
    pub supply_coverage: Option<SkuCoverageStatus>,
    /// 销售价下限（含）。
    pub sales_price_min: Option<Amount>,
    /// 销售价上限（含）。
    pub sales_price_max: Option<Amount>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ProductListParams {
    /// 归一化商品列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ProductListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, PRODUCT_SORT_FIELDS)?;
        validate_sales_price_range(self.sales_price_min, self.sales_price_max)?;
        Ok(ProductListQuery {
            product_no: normalized_text(self.product_no.as_deref()),
            keyword: normalized_text(self.keyword.as_deref()),
            product_kind: self.product_kind,
            category_id: normalized_text(self.category_id.as_deref()),
            brand_id: normalized_text(self.brand_id.as_deref()),
            supplier_id: normalized_text(self.supplier_id.as_deref()),
            status: self.status,
            listing_status: self.listing_status,
            supply_coverage: self.supply_coverage,
            sales_price_min: self.sales_price_min,
            sales_price_max: self.sales_price_max,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 校验商品列表销售价区间。
fn validate_sales_price_range(minimum: Option<Amount>, maximum: Option<Amount>) -> Result<()> {
    if minimum.is_some_and(|value| value.to_decimal().is_sign_negative())
        || maximum.is_some_and(|value| value.to_decimal().is_sign_negative())
    {
        return Err(Error::ValidationError("销售价不能小于 0".to_string()));
    }
    if matches!((minimum, maximum), (Some(minimum), Some(maximum)) if minimum > maximum) {
        return Err(Error::ValidationError("最低销售价不能高于最高销售价".to_string()));
    }
    Ok(())
}

/// 商品修订媒体响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductRevisionMediaView {
    /// 媒体主键。
    pub id: String,
    /// 合规媒体文件（`file_asset`，D05）。
    pub file_asset_id: String,
    /// 媒体用途（`carousel`/`detail`/`attachment`）。
    pub media_role: MediaRole,
    /// 版本内展示顺序。
    pub sort_order: i32,
    /// 无障碍替代文本。
    pub alt_text: Option<String>,
}

/// 商品修订响应视图（修订表追加写入，只读）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductRevisionView {
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
    /// SPU 级媒体行（轮播/详情；由列表 handler 批量装配）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<ProductRevisionMediaView>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<ProductRevision> for ProductRevisionView {
    /// 从实体构造响应视图（不含媒体行；列表装配见 `product_revision_list`）。
    ///
    /// # 参数
    /// * `revision` - 商品修订实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(revision: ProductRevision) -> Self {
        Self {
            id: revision.base.id,
            product_id: revision.product_id.to_string(),
            revision_no: revision.revision.revision_no,
            name: revision.name,
            description: revision.description,
            specification: revision.specification,
            category_id: revision.category_id.to_string(),
            brand_id: revision.brand_id.to_string(),
            status: revision.status,
            effective_from: revision.effective_from,
            effective_to: revision.effective_to,
            media: Vec::new(),
            created_at: revision.base.created_at,
            version: revision.base.version,
        }
    }
}

/// 商品修订列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductRevisionListParams {
    /// 所属商品 SPU 筛选。
    pub product_id: Option<ProductId>,
    /// 修订启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的商品修订列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProductRevisionListQuery {
    /// 所属商品 SPU 筛选。
    pub product_id: Option<String>,
    /// 修订启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ProductRevisionListParams {
    /// 归一化商品修订列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ProductRevisionListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, PRODUCT_REVISION_SORT_FIELDS)?;
        Ok(ProductRevisionListQuery {
            product_id: self.product_id.as_ref().map(|id| id.to_string()),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// SKU 响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkuView {
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
    /// SKU 上架状态。
    pub listing_status: ListingStatus,
    /// 当前 SKU 修订 ID。
    pub current_revision_id: Option<String>,
    /// 当前 SKU 修订名称（公司审核后的 SKU 名称；无当前修订时为空）。
    pub name: Option<String>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<Sku> for SkuView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `sku` - SKU 实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(sku: Sku) -> Self {
        Self {
            id: sku.base.id,
            sku_no: sku.sku_no,
            product_id: sku.product_id.to_string(),
            base_unit_id: sku.base_unit_id.to_string(),
            specification_signature: sku.specification_signature,
            status: sku.stable.status,
            listing_status: sku.listing_status,
            current_revision_id: sku.stable.current_revision_id,
            name: None,
            created_at: sku.base.created_at,
            version: sku.base.version,
        }
    }
}

/// SKU 列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SkuListParams {
    /// 关键字：SKU 编号或当前修订名称（模糊、忽略大小写）。
    pub q: Option<String>,
    /// SKU 编号字面量筛选（忽略大小写）。
    pub sku_no: Option<String>,
    /// 所属 SPU 筛选。
    pub product_id: Option<ProductId>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 上架状态筛选。
    pub listing_status: Option<ListingStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`sku_no`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的 SKU 列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkuListQuery {
    /// 关键字筛选。
    pub q: Option<String>,
    /// SKU 编号筛选。
    pub sku_no: Option<String>,
    /// 所属 SPU 筛选。
    pub product_id: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 上架状态筛选。
    pub listing_status: Option<ListingStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SkuListParams {
    /// 归一化 SKU 列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SkuListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, SKU_SORT_FIELDS)?;
        Ok(SkuListQuery {
            q: normalized_text(self.q.as_deref()),
            sku_no: normalized_text(self.sku_no.as_deref()),
            product_id: self.product_id.as_ref().map(|id| id.to_string()),
            status: self.status,
            listing_status: self.listing_status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// SKU 修订响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkuRevisionView {
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
    /// 条码原值。
    pub barcode: Option<String>,
    /// 来源 SKU 主图（已归档受控文件，D05）。
    pub source_main_image_asset_id: Option<String>,
    /// 重量（千克）。
    pub weight_kg: Option<entities::money::Quantity>,
    /// 体积（立方米）。
    pub volume_m3: Option<entities::money::Quantity>,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 公司对销售可见的含税价格（字符串形态）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 市场参考价。
    pub market_price: Option<Amount>,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<SkuRevision> for SkuRevisionView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `revision` - SKU 修订实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(revision: SkuRevision) -> Self {
        Self {
            id: revision.base.id,
            sku_id: revision.sku_id.to_string(),
            revision_no: revision.revision.revision_no,
            name: revision.name,
            description: revision.description,
            specification: revision.specification,
            barcode: revision.barcode,
            source_main_image_asset_id: revision
                .source_main_image_asset_id
                .as_ref()
                .map(|id| id.to_string()),
            weight_kg: revision.weight_kg,
            volume_m3: revision.volume_m3,
            status: revision.status,
            sales_visible_price_gross: revision.sales_visible_price_gross,
            market_price: revision.market_price,
            effective_from: revision.effective_from,
            effective_to: revision.effective_to,
            created_at: revision.base.created_at,
            version: revision.base.version,
        }
    }
}

/// SKU 修订列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SkuRevisionListParams {
    /// 所属稳定 SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 名称字面量筛选（忽略大小写）。
    pub name: Option<String>,
    /// 条码精确筛选（内部按 trim 规范化）。
    pub barcode: Option<String>,
    /// 修订启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的 SKU 修订列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkuRevisionListQuery {
    /// 所属稳定 SKU 筛选。
    pub sku_id: Option<String>,
    /// 名称筛选。
    pub name: Option<String>,
    /// 条码筛选。
    pub barcode: Option<String>,
    /// 修订启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SkuRevisionListParams {
    /// 归一化 SKU 修订列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SkuRevisionListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, SKU_REVISION_SORT_FIELDS)?;
        Ok(SkuRevisionListQuery {
            sku_id: self.sku_id.as_ref().map(|id| id.to_string()),
            name: normalized_text(self.name.as_deref()),
            barcode: normalized_text(self.barcode.as_deref()),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}
