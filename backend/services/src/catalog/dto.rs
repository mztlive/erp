//! 域 D10 `catalog` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额与数量为十进制字符串；
//! 生效日期为 `YYYY-MM-DD`（`BusinessDate` 的既有序列化形态）。
//!
//! 排序白名单校验辅助（`normalize_sort`/`PageParams`/`PageView`）与 D01
//! source_registry 同构；抽取到冻结的 `services/src/query.rs` 属地基修订
//! 候选（见域报告）。

use entities::catalog::product_revision_media::MediaRole;
use entities::catalog::{
    EnableStatus, ListingStatus, ProductBrand, ProductCategory, ProductKind, ProductListingStatus,
    ProductRevision, Sku, SkuAttribute, SkuAttributeValue, SkuCoverageStatus, SkuRevision, UnitOfMeasure,
    VoucherCategoryProfileRevision,
};
use entities::common::time::BusinessDate;
use entities::ids::{
    FileAssetId, ProductBrandId, ProductCategoryId, ProductId, SkuAttributeId, SkuId, SkuRevisionId,
    UnitOfMeasureId,
};
use entities::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 商品分类列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const PRODUCT_CATEGORY_SORT_FIELDS: &[&str] = &["created_at", "category_code", "name"];
/// 商品品牌列表允许的排序字段白名单。
pub(crate) const PRODUCT_BRAND_SORT_FIELDS: &[&str] = &["created_at", "brand_code", "name"];
/// 计量单位列表允许的排序字段白名单。
pub(crate) const UNIT_OF_MEASURE_SORT_FIELDS: &[&str] = &["created_at", "unit_code", "name"];
/// 规格属性列表允许的排序字段白名单。
pub(crate) const SKU_ATTRIBUTE_SORT_FIELDS: &[&str] = &["created_at", "attribute_code", "name"];
/// 规格属性值列表允许的排序字段白名单。
pub(crate) const SKU_ATTRIBUTE_VALUE_SORT_FIELDS: &[&str] =
    &["created_at", "value_code", "display_value", "sort_order"];
/// 商品列表允许的排序字段白名单。
pub(crate) const PRODUCT_SORT_FIELDS: &[&str] = &["created_at", "product_no"];
/// 商品修订列表允许的排序字段白名单。
pub(crate) const PRODUCT_REVISION_SORT_FIELDS: &[&str] = &["created_at", "revision_no"];
/// SKU 列表允许的排序字段白名单。
pub(crate) const SKU_SORT_FIELDS: &[&str] = &["created_at", "sku_no"];
/// SKU 修订列表允许的排序字段白名单。
pub(crate) const SKU_REVISION_SORT_FIELDS: &[&str] = &["created_at", "revision_no"];
/// 卡券类目扩展修订列表允许的排序字段白名单。
pub(crate) const VOUCHER_PROFILE_SORT_FIELDS: &[&str] = &["created_at", "revision_no"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) fn normalize_sort(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    allowed_fields: &'static [&'static str],
) -> Result<(&'static str, SortDir)> {
    let sort_by = match sort_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(field) => allowed_fields
            .iter()
            .find(|allowed| **allowed == field)
            .copied()
            .ok_or_else(|| Error::ValidationError(format!("不支持的排序字段: {field}")))?,
        None => "created_at",
    };
    let sort_dir = match sort_dir
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("asc") => SortDir::Asc,
        Some("desc") => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
        None => SortDir::Desc,
    };
    Ok((sort_by, sort_dir))
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 满足筛选条件的总数（非当前页条数）。
    pub total: i64,
    /// 当前页码（1 起）。
    pub page: u64,
    /// 请求的分页大小。
    pub page_size: u32,
}

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空 code/name 需要按「空白视为空」拒绝，落入 HTTP 400）。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 商品分类创建请求（HTTP 契约：`{ category_code, parent_category_id?, name,
/// product_kind, status? }`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateProductCategoryRequest {
    /// 稳定分类代码（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "分类代码不能为空"))]
    pub category_code: String,
    /// 父分类；空表示根分类。
    pub parent_category_id: Option<ProductCategoryId>,
    /// 分类名称。
    #[validate(custom(function = "non_blank", message = "分类名称不能为空"))]
    pub name: String,
    /// 分类允许的商品类型（只用于兼容性校验和筛选）。
    pub product_kind: ProductKind,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
}

/// 商品分类更新请求（携带乐观锁版本；`category_code` 与 `parent_category_id`
/// 不可在通用更新中修改，父分类移动走专门接口）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateProductCategoryRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 分类名称；缺省表示不修改。
    pub name: Option<String>,
    /// 分类允许的商品类型；缺省表示不修改。
    pub product_kind: Option<ProductKind>,
    /// 启停状态；缺省表示不修改。
    pub status: Option<EnableStatus>,
}

/// 移动商品分类到新父分类请求（树形维护：只允许移动叶节点或整棵子树）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MoveProductCategoryRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 新父分类；空表示提升为根分类。
    pub parent_category_id: Option<ProductCategoryId>,
}

/// 商品分类响应视图（契约形状：`id`/`category_code`/`parent_category_id`/
/// `name`/`product_kind`/`status`/`created_at`，另附 `version`）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductCategoryView {
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
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
}

impl From<ProductCategory> for ProductCategoryView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `category` - 商品分类实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(category: ProductCategory) -> Self {
        Self {
            id: category.base.id,
            category_code: category.category_code,
            parent_category_id: category.parent_category_id.map(|id| id.to_string()),
            name: category.name,
            product_kind: category.product_kind,
            status: category.stable.status,
            created_at: category.base.created_at,
            version: category.base.version,
        }
    }
}

/// 商品分类列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductCategoryListParams {
    /// 分类代码精确筛选。
    pub category_code: Option<String>,
    /// 名称字面量筛选（忽略大小写）。
    pub name: Option<String>,
    /// 父分类筛选：`root` 只匹配根分类；传入分类 ID 匹配其直接子节点。
    pub parent_category_id: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`category_code`/`name`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的商品分类列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProductCategoryListQuery {
    /// 分类代码精确筛选。
    pub category_code: Option<String>,
    /// 名称筛选。
    pub name: Option<String>,
    /// 父分类筛选（`None` 不筛选；`Some(None)` 根；`Some(Some(id))` 直接子节点）。
    pub parent_category_id: Option<Option<String>>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ProductCategoryListParams {
    /// 归一化商品分类列表查询参数。
    ///
    /// 文本筛选去首尾空白、`root` 特判根分类、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ProductCategoryListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, PRODUCT_CATEGORY_SORT_FIELDS)?;
        let parent_category_id = match self
            .parent_category_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some("root") => Some(None),
            Some(id) => Some(Some(id.to_string())),
            None => None,
        };
        Ok(ProductCategoryListQuery {
            category_code: normalized_text(self.category_code.as_deref()),
            name: normalized_text(self.name.as_deref()),
            parent_category_id,
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

/// 商品品牌创建请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateProductBrandRequest {
    /// 稳定品牌代码（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "品牌代码不能为空"))]
    pub brand_code: String,
    /// 品牌名称。
    #[validate(custom(function = "non_blank", message = "品牌名称不能为空"))]
    pub name: String,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
    /// 品牌 Logo（已登记受控文件，D05；可空）。
    pub logo_file_asset_id: Option<FileAssetId>,
}

/// 商品品牌更新请求（携带乐观锁版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateProductBrandRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 品牌名称；缺省表示不修改。
    pub name: Option<String>,
    /// 启停状态；缺省表示不修改。
    pub status: Option<EnableStatus>,
    /// 品牌 Logo（已登记受控文件，D05）；`null` 表示清除，缺省表示不修改。
    pub logo_file_asset_id: Option<Option<FileAssetId>>,
}

/// 商品品牌响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProductBrandView {
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
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<ProductBrand> for ProductBrandView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `brand` - 商品品牌实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(brand: ProductBrand) -> Self {
        Self {
            id: brand.base.id,
            brand_code: brand.brand_code,
            name: brand.name,
            logo_asset_id: brand.logo_file_asset_id.map(|id| id.to_string()),
            status: brand.stable.status,
            created_at: brand.base.created_at,
            version: brand.base.version,
        }
    }
}

/// 商品品牌列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductBrandListParams {
    /// 品牌代码精确筛选。
    pub brand_code: Option<String>,
    /// 名称字面量筛选（忽略大小写）。
    pub name: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`brand_code`/`name`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的商品品牌列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProductBrandListQuery {
    /// 品牌代码精确筛选。
    pub brand_code: Option<String>,
    /// 名称筛选。
    pub name: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl ProductBrandListParams {
    /// 归一化商品品牌列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<ProductBrandListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, PRODUCT_BRAND_SORT_FIELDS)?;
        Ok(ProductBrandListQuery {
            brand_code: normalized_text(self.brand_code.as_deref()),
            name: normalized_text(self.name.as_deref()),
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

/// 计量单位创建请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateUnitOfMeasureRequest {
    /// 稳定单位代码（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "单位代码不能为空"))]
    pub unit_code: String,
    /// 单位名称。
    #[validate(custom(function = "non_blank", message = "单位名称不能为空"))]
    pub name: String,
    /// 单位符号。
    #[validate(custom(function = "non_blank", message = "单位符号不能为空"))]
    pub symbol: String,
    /// 允许数量小数位（0–6）。
    #[validate(range(min = 0, max = 6, message = "数量小数位必须在0-6之间"))]
    pub quantity_scale: u8,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
}

/// 计量单位更新请求（携带乐观锁版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateUnitOfMeasureRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 单位名称；缺省表示不修改。
    pub name: Option<String>,
    /// 单位符号；缺省表示不修改。
    pub symbol: Option<String>,
    /// 允许数量小数位；缺省表示不修改。
    pub quantity_scale: Option<u8>,
    /// 启停状态；缺省表示不修改。
    pub status: Option<EnableStatus>,
}

/// 计量单位响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UnitOfMeasureView {
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
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<UnitOfMeasure> for UnitOfMeasureView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `unit` - 计量单位实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(unit: UnitOfMeasure) -> Self {
        Self {
            id: unit.base.id,
            unit_code: unit.unit_code,
            name: unit.name,
            symbol: unit.symbol,
            quantity_scale: unit.quantity_scale,
            status: unit.stable.status,
            created_at: unit.base.created_at,
            version: unit.base.version,
        }
    }
}

/// 计量单位列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UnitOfMeasureListParams {
    /// 单位代码精确筛选。
    pub unit_code: Option<String>,
    /// 名称字面量筛选（忽略大小写）。
    pub name: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`unit_code`/`name`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的计量单位列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnitOfMeasureListQuery {
    /// 单位代码精确筛选。
    pub unit_code: Option<String>,
    /// 名称筛选。
    pub name: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl UnitOfMeasureListParams {
    /// 归一化计量单位列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<UnitOfMeasureListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, UNIT_OF_MEASURE_SORT_FIELDS)?;
        Ok(UnitOfMeasureListQuery {
            unit_code: normalized_text(self.unit_code.as_deref()),
            name: normalized_text(self.name.as_deref()),
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

/// 规格属性创建请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSkuAttributeRequest {
    /// 稳定属性代码（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "属性代码不能为空"))]
    pub attribute_code: String,
    /// 属性名称。
    #[validate(custom(function = "non_blank", message = "属性名称不能为空"))]
    pub name: String,
    /// 属性值类型（受控枚举或规范文本）。
    pub value_type: entities::catalog::sku_attribute::AttributeValueType,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
}

/// 规格属性更新请求（携带乐观锁版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSkuAttributeRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 属性名称；缺省表示不修改。
    pub name: Option<String>,
    /// 属性值类型；缺省表示不修改。
    pub value_type: Option<entities::catalog::sku_attribute::AttributeValueType>,
    /// 启停状态；缺省表示不修改。
    pub status: Option<EnableStatus>,
}

/// 规格属性响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkuAttributeView {
    /// 实体主键。
    pub id: String,
    /// 稳定属性代码。
    pub attribute_code: String,
    /// 属性名称。
    pub name: String,
    /// 属性值类型。
    pub value_type: entities::catalog::sku_attribute::AttributeValueType,
    /// 启停状态。
    pub status: EnableStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<SkuAttribute> for SkuAttributeView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `attribute` - 规格属性实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(attribute: SkuAttribute) -> Self {
        Self {
            id: attribute.base.id,
            attribute_code: attribute.attribute_code,
            name: attribute.name,
            value_type: attribute.value_type,
            status: attribute.stable.status,
            created_at: attribute.base.created_at,
            version: attribute.base.version,
        }
    }
}

/// 规格属性列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SkuAttributeListParams {
    /// 属性代码精确筛选。
    pub attribute_code: Option<String>,
    /// 名称字面量筛选（忽略大小写）。
    pub name: Option<String>,
    /// 属性值类型筛选。
    pub value_type: Option<entities::catalog::sku_attribute::AttributeValueType>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`attribute_code`/`name`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的规格属性列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkuAttributeListQuery {
    /// 属性代码精确筛选。
    pub attribute_code: Option<String>,
    /// 名称筛选。
    pub name: Option<String>,
    /// 属性值类型筛选。
    pub value_type: Option<entities::catalog::sku_attribute::AttributeValueType>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SkuAttributeListParams {
    /// 归一化规格属性列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SkuAttributeListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, SKU_ATTRIBUTE_SORT_FIELDS)?;
        Ok(SkuAttributeListQuery {
            attribute_code: normalized_text(self.attribute_code.as_deref()),
            name: normalized_text(self.name.as_deref()),
            value_type: self.value_type,
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

/// 规格属性值创建请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSkuAttributeValueRequest {
    /// 所属规格属性。
    pub attribute_id: SkuAttributeId,
    /// 稳定属性值代码（同一属性下唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "属性值代码不能为空"))]
    pub value_code: String,
    /// 展示值。
    #[validate(custom(function = "non_blank", message = "展示值不能为空"))]
    pub display_value: String,
    /// 展示排序（非负）。
    #[validate(range(min = 0, message = "展示排序不能为负数"))]
    pub sort_order: i32,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
}

/// 规格属性值更新请求（携带乐观锁版本）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSkuAttributeValueRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 展示值；缺省表示不修改。
    pub display_value: Option<String>,
    /// 展示排序；缺省表示不修改。
    pub sort_order: Option<i32>,
    /// 启停状态；缺省表示不修改。
    pub status: Option<EnableStatus>,
}

/// 规格属性值响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SkuAttributeValueView {
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
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

impl From<SkuAttributeValue> for SkuAttributeValueView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `value` - 规格属性值实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(value: SkuAttributeValue) -> Self {
        Self {
            id: value.base.id,
            attribute_id: value.attribute_id.to_string(),
            value_code: value.value_code,
            display_value: value.display_value,
            sort_order: value.sort_order,
            status: value.stable.status,
            created_at: value.base.created_at,
            version: value.base.version,
        }
    }
}

/// 规格属性值列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SkuAttributeValueListParams {
    /// 所属规格属性筛选。
    pub attribute_id: Option<SkuAttributeId>,
    /// 属性值代码精确筛选。
    pub value_code: Option<String>,
    /// 展示值字面量筛选（忽略大小写）。
    pub display_value: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`value_code`/`display_value`/`sort_order`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的规格属性值列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkuAttributeValueListQuery {
    /// 所属规格属性筛选。
    pub attribute_id: Option<String>,
    /// 属性值代码精确筛选。
    pub value_code: Option<String>,
    /// 展示值筛选。
    pub display_value: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SkuAttributeValueListParams {
    /// 归一化规格属性值列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SkuAttributeValueListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SKU_ATTRIBUTE_VALUE_SORT_FIELDS)?;
        Ok(SkuAttributeValueListQuery {
            attribute_id: self.attribute_id.as_ref().map(|id| id.to_string()),
            value_code: normalized_text(self.value_code.as_deref()),
            display_value: normalized_text(self.display_value.as_deref()),
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
            created_at: sku.base.created_at,
            version: sku.base.version,
        }
    }
}

/// SKU 列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SkuListParams {
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

/// 内联新建 VOUCHER 类型分类的输入（与 `category_id` 二选一）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct NewVoucherCategoryInput {
    /// 稳定分类代码（唯一，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "分类代码不能为空"))]
    pub category_code: String,
    /// 父分类；空表示根分类。
    pub parent_category_id: Option<ProductCategoryId>,
    /// 分类名称。
    #[validate(custom(function = "non_blank", message = "分类名称不能为空"))]
    pub name: String,
}

/// 卡券类目原子创建请求内嵌的 SKU 输入（无需单独填 SKU 编号，复用 `voucher_no`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct VoucherSkuInput {
    /// 唯一基础单位（`unit_of_measure` 启用字典项）。
    pub base_unit_id: UnitOfMeasureId,
    /// 条码原值（可空）。
    pub barcode: Option<String>,
    /// 重量（千克，非负定点数）。
    pub weight_kg: Option<Quantity>,
    /// 体积（立方米，非负定点数）。
    pub volume_m3: Option<Quantity>,
    /// 公司对销售可见的含税价（非负定点金额）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 市场展示参考价（非负定点金额）。
    pub market_price: Option<Amount>,
}

/// 卡券类目原子创建请求（商品 + 首个修订 + 唯一 SKU + 卡券类目扩展修订，
/// 必要时内联新建所属分类，全部在一个事务内原子写入）。
///
/// `voucher_no` 同时作为 `product_no` 与 `sku_no` 落库（业务上一个卡券类目即一个 SKU，
/// 无需分别填写两个编号）。
///
/// **默认字典**（`category_id`/`new_category`、`brand_id`、`sku` 均可省略）：
/// - 分类：共用卡券根分类（代码 `VOUCHER` / 名称「卡券」），不存在时自动创建；
/// - 品牌：固定「福尚云」（代码 `FSY`），不存在时自动创建；
/// - 基础单位：固定「张」，不存在时自动创建。
///   仍可显式传入覆盖默认；`category_id` 与 `new_category` 不可同时给出。
///   `description` 同时写入 `product_revision.description` 与
///   `voucher_category_profile_revision.description`。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateVoucherCategoryRequest {
    /// 卡券类目编号（全局唯一，同时作为 `product_no` 与 `sku_no`，创建后不可修改）。
    #[validate(custom(function = "non_blank", message = "卡券类目编号不能为空"))]
    pub voucher_no: String,
    /// 卡券类目名称。
    #[validate(custom(function = "non_blank", message = "卡券类目名称不能为空"))]
    pub name: String,
    /// 卡券类目描述。
    #[validate(custom(function = "non_blank", message = "卡券类目描述不能为空"))]
    pub description: String,
    /// 公司审核后的规格或服务内容。
    #[serde(default)]
    pub specification: Option<String>,
    /// 引用已有 VOUCHER 类型分类；与 `new_category` 互斥；都缺省则用共用卡券根分类。
    #[serde(default)]
    pub category_id: Option<ProductCategoryId>,
    /// 内联新建 VOUCHER 类型分类；与 `category_id` 互斥。
    #[serde(default)]
    #[validate(nested)]
    pub new_category: Option<NewVoucherCategoryInput>,
    /// ERP 品牌；缺省解析为「福尚云」。
    #[serde(default)]
    pub brand_id: Option<ProductBrandId>,
    /// 唯一 SKU 行；缺省基础单位为「张」，其它 SKU 属性为空。
    #[serde(default)]
    #[validate(nested)]
    pub sku: Option<VoucherSkuInput>,
    /// 启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<EnableStatus>,
    /// 生效开始日；缺省为服务端当天。
    #[serde(default)]
    pub effective_from: Option<BusinessDate>,
    /// 生效结束日；空表示无限期。
    #[serde(default)]
    pub effective_to: Option<BusinessDate>,
}

/// 卡券类目更新请求（追加商品修订 + SKU 修订 + 卡券类目扩展修订）。
///
/// 编号（`product_no`/`sku_no`）创建后不可改；分类 / 品牌 / 基础单位沿用当前修订，
/// 不在本接口维护。乐观锁取所属商品 `product.version`。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateVoucherCategoryRequest {
    /// 期望的商品乐观锁版本；与当前不一致时拒绝（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 卡券类目名称（写入商品修订与 SKU 修订名称快照）。
    #[validate(custom(function = "non_blank", message = "卡券类目名称不能为空"))]
    pub name: String,
    /// 卡券类目描述（写入商品修订与卡券类目扩展修订）。
    #[validate(custom(function = "non_blank", message = "卡券类目描述不能为空"))]
    pub description: String,
    /// 生效开始日；缺省为服务端当天。
    #[serde(default)]
    pub effective_from: Option<BusinessDate>,
    /// 生效结束日；空表示无限期。
    #[serde(default)]
    pub effective_to: Option<BusinessDate>,
}

/// 卡券类目扩展修订响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct VoucherCategoryProfileView {
    /// 实体主键（本条扩展修订 ID）。
    pub id: String,
    /// 卡券类目使用的 VOUCHER SKU 稳定身份（列表/更新路径的稳定主键）。
    pub sku_id: String,
    /// SKU 编号（即卡券类目编号）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sku_no: Option<String>,
    /// 所属商品 SPU 身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    /// 所属商品当前乐观锁版本（更新时作为 `version` 提交）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_version: Option<u64>,
    /// 展示名称（当前商品/SKU 修订名称；无则回落描述）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 修订序号。
    pub revision_no: u32,
    /// 卡券类目描述。
    pub description: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 本条扩展修订乐观锁版本。
    pub version: u64,
}

impl From<VoucherCategoryProfileRevision> for VoucherCategoryProfileView {
    /// 从实体构造响应视图（关联字段由列表/更新服务后续补齐）。
    ///
    /// # 参数
    /// * `revision` - 卡券类目扩展修订实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(revision: VoucherCategoryProfileRevision) -> Self {
        Self {
            id: revision.base.id,
            sku_id: revision.sku_id.to_string(),
            sku_no: None,
            product_id: None,
            product_version: None,
            name: None,
            revision_no: revision.revision.revision_no,
            description: revision.description,
            status: revision.status,
            created_at: revision.base.created_at,
            version: revision.base.version,
        }
    }
}

/// 卡券类目扩展修订列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct VoucherCategoryProfileListParams {
    /// 卡券类目 SKU 筛选。
    pub sku_id: Option<SkuId>,
    /// 启停状态筛选。
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

/// 归一化后的卡券类目扩展修订列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VoucherCategoryProfileListQuery {
    /// 卡券类目 SKU 筛选。
    pub sku_id: Option<String>,
    /// 启停状态筛选。
    pub status: Option<EnableStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl VoucherCategoryProfileListParams {
    /// 归一化卡券类目扩展修订列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<VoucherCategoryProfileListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, VOUCHER_PROFILE_SORT_FIELDS)?;
        Ok(VoucherCategoryProfileListQuery {
            sku_id: self.sku_id.as_ref().map(|id| id.to_string()),
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

#[cfg(test)]
mod tests {
    use super::{normalize_sort, SortDir};
    use entities::catalog::{ListingStatus, ProductKind, ProductListingStatus, SkuCoverageStatus};
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" created_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at"],
        )
        .unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn product_category_params_normalize_root_parent_and_paging() {
        let params: super::ProductCategoryListParams = serde_json::from_value(serde_json::json!({
            "category_code": " CAT-001 ",
            "parent_category_id": "root",
            "page": 2,
            "page_size": 50,
            "sort_by": "name",
            "sort_dir": "asc",
        }))
        .unwrap();
        let query = params.normalized().unwrap();
        assert_eq!(query.category_code.as_deref(), Some("CAT-001"));
        assert_eq!(query.parent_category_id, Some(None));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "name");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn product_params_normalize_filters_and_defaults() {
        let params: super::ProductListParams = serde_json::from_value(serde_json::json!({
            "product_no": " P-1 ",
            "keyword": " 礼盒 ",
            "product_kind": "PHYSICAL",
            "category_id": " category-1 ",
            "brand_id": " brand-1 ",
            "supplier_id": " supplier-1 ",
            "status": "active",
            "listing_status": "partially_listed",
            "supply_coverage": "complete",
            "sales_price_min": "100.00",
            "sales_price_max": "200.00",
        }))
        .unwrap();
        let query = params.normalized().unwrap();
        assert_eq!(query.product_no.as_deref(), Some("P-1"));
        assert_eq!(query.keyword.as_deref(), Some("礼盒"));
        assert_eq!(query.product_kind, Some(ProductKind::Physical));
        assert_eq!(query.category_id.as_deref(), Some("category-1"));
        assert_eq!(query.brand_id.as_deref(), Some("brand-1"));
        assert_eq!(query.supplier_id.as_deref(), Some("supplier-1"));
        assert_eq!(query.listing_status, Some(ProductListingStatus::PartiallyListed));
        assert_eq!(query.supply_coverage, Some(SkuCoverageStatus::Complete));
        assert_eq!(query.sales_price_min.unwrap().to_string(), "100.00");
        assert_eq!(query.sales_price_max.unwrap().to_string(), "200.00");
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn product_params_reject_inverted_sales_price_range() {
        let params: super::ProductListParams = serde_json::from_value(serde_json::json!({
            "sales_price_min": "200.00",
            "sales_price_max": "100.00",
        }))
        .unwrap();

        assert!(params.normalized().is_err());
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params: super::SkuListParams = serde_json::from_value(serde_json::json!({
            "page": 0,
            "page_size": 1000,
        }))
        .unwrap();
        assert!(params.validate().is_err());
    }

    #[test]
    fn sku_params_accept_listing_status_filter() {
        let params: super::SkuListParams = serde_json::from_value(serde_json::json!({
            "listing_status": "listed",
        }))
        .unwrap();
        let query = params.normalized().unwrap();

        assert_eq!(query.listing_status, Some(ListingStatus::Listed));
    }

    #[test]
    fn create_product_request_rejects_empty_skus() {
        let request: super::CreateProductRequest = serde_json::from_value(serde_json::json!({
            "product_no": "P-1",
            "product_kind": "PHYSICAL",
            "name": "商品",
            "category_id": "cat-1",
            "brand_id": "brand-1",
            "effective_from": "2026-01-01",
            "skus": [],
        }))
        .unwrap();
        assert!(request.validate().is_err(), "空 SKU 列表必须被拒绝");
    }

    fn voucher_category_request_json() -> serde_json::Value {
        serde_json::json!({
            "voucher_no": "VC-1",
            "name": "满100减20券",
            "description": "满100元可用",
            "category_id": "cat-1",
            "brand_id": "brand-1",
            "sku": { "base_unit_id": "unit-1" },
            "effective_from": "2026-01-01",
        })
    }

    #[test]
    fn create_voucher_category_request_rejects_blank_voucher_no() {
        let mut value = voucher_category_request_json();
        value["voucher_no"] = serde_json::json!("   ");
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(request.validate().is_err(), "空白卡券类目编号必须被拒绝");
    }

    #[test]
    fn create_voucher_category_request_rejects_blank_name_and_description() {
        let mut value = voucher_category_request_json();
        value["name"] = serde_json::json!("");
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(request.validate().is_err(), "空名称必须被拒绝");

        let mut value = voucher_category_request_json();
        value["description"] = serde_json::json!("  ");
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(request.validate().is_err(), "空白描述必须被拒绝");
    }

    #[test]
    fn create_voucher_category_request_rejects_blank_new_category_fields() {
        let mut value = voucher_category_request_json();
        value.as_object_mut().unwrap().remove("category_id");
        value["new_category"] = serde_json::json!({ "category_code": "", "name": "卡券分类" });
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(request.validate().is_err(), "空白分类代码必须被拒绝");

        let mut value = voucher_category_request_json();
        value.as_object_mut().unwrap().remove("category_id");
        value["new_category"] = serde_json::json!({ "category_code": "VC-CAT", "name": "  " });
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(request.validate().is_err(), "空白分类名称必须被拒绝");
    }

    #[test]
    fn create_voucher_category_request_allows_minimal_identity_only() {
        let value = serde_json::json!({
            "voucher_no": "VC-MIN",
            "name": "心意卡",
            "description": "员工福利卡",
        });
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(
            request.validate().is_ok(),
            "仅身份字段应通过校验，字典由服务端默认"
        );
        assert!(request.category_id.is_none());
        assert!(request.brand_id.is_none());
        assert!(request.sku.is_none());
        assert!(request.effective_from.is_none());
    }
}
