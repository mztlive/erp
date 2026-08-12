use entities::catalog::{EnableStatus, ProductBrand, UnitOfMeasure};
use entities::ids::FileAssetId;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// 商品品牌列表允许的排序字段白名单。
pub(crate) const PRODUCT_BRAND_SORT_FIELDS: &[&str] = &["created_at", "brand_code", "name"];
/// 计量单位列表允许的排序字段白名单。
pub(crate) const UNIT_OF_MEASURE_SORT_FIELDS: &[&str] = &["created_at", "unit_code", "name"];
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
