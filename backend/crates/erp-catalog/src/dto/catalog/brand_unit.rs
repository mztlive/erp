use application_core::normalized_text;
use erp_core::ids::FileAssetId;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::common::{PageParams, non_blank, paging_params};
use crate::entity::catalog::{EnableStatus, ProductBrand, UnitOfMeasure};
use crate::error::Result;
use crate::repository::{ProductBrandRow, UnitOfMeasureRow};

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

impl CreateProductBrandRequest {
    /// 以稳定品牌代码与品牌名称构造创建请求。
    ///
    /// # 参数
    /// * `brand_code` - 稳定品牌代码
    /// * `name` - 品牌名称
    ///
    /// # 返回
    /// 返回待补齐可选字段的创建请求。
    ///
    /// # 错误
    /// 无。
    pub fn new(brand_code: impl Into<String>, name: impl Into<String>) -> Self {
        Self { brand_code: brand_code.into(), name: name.into(), status: None, logo_file_asset_id: None }
    }

    /// 设置启停状态。
    ///
    /// # 参数
    /// * `status` - 启停状态
    ///
    /// # 返回
    /// 返回更新后的创建请求。
    ///
    /// # 错误
    /// 无。
    pub fn with_status(mut self, status: EnableStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// 设置品牌 Logo。
    ///
    /// # 参数
    /// * `logo_file_asset_id` - 已登记受控文件
    ///
    /// # 返回
    /// 返回更新后的创建请求。
    ///
    /// # 错误
    /// 无。
    pub fn with_logo_file_asset_id(mut self, logo_file_asset_id: FileAssetId) -> Self {
        self.logo_file_asset_id = Some(logo_file_asset_id);
        self
    }
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

impl From<ProductBrandRow> for ProductBrandView {
    /// 从投影行构造响应视图，与 `From<ProductBrand>` 同语义。
    ///
    /// # 参数
    /// * `row` - 商品品牌列表投影行
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(row: ProductBrandRow) -> Self {
        Self {
            id: row.id,
            brand_code: row.brand_code,
            name: row.name,
            logo_asset_id: row.logo_asset_id,
            status: row.status,
            created_at: row.created_at,
            version: row.version,
        }
    }
}

/// 商品品牌列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProductBrandListParams {
    /// 多业务字段字面量关键词，空白不筛选。
    #[validate(length(max = 200))]
    pub q: Option<String>,
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
    /// 多业务字段字面量关键词，空白不筛选。
    pub q: Option<String>,
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
        Ok(ProductBrandListQuery {
            q: normalized_text(self.q.as_deref()),
            brand_code: normalized_text(self.brand_code.as_deref()),
            name: normalized_text(self.name.as_deref()),
            status: self.status,
            paging: paging_params(
                self.page,
                self.page_size,
                &self.sort_by,
                &self.sort_dir,
                PRODUCT_BRAND_SORT_FIELDS,
            )?,
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

impl From<UnitOfMeasureRow> for UnitOfMeasureView {
    /// 从投影行构造响应视图，与 `From<UnitOfMeasure>` 同语义。
    ///
    /// # 参数
    /// * `row` - 计量单位列表投影行
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(row: UnitOfMeasureRow) -> Self {
        Self {
            id: row.id,
            unit_code: row.unit_code,
            name: row.name,
            symbol: row.symbol,
            quantity_scale: row.quantity_scale,
            status: row.status,
            created_at: row.created_at,
            version: row.version,
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
        Ok(UnitOfMeasureListQuery {
            unit_code: normalized_text(self.unit_code.as_deref()),
            name: normalized_text(self.name.as_deref()),
            status: self.status,
            paging: paging_params(
                self.page,
                self.page_size,
                &self.sort_by,
                &self.sort_dir,
                UNIT_OF_MEASURE_SORT_FIELDS,
            )?,
        })
    }
}

#[cfg(test)]
mod tests {
    use validator::Validate;

    use super::CreateProductBrandRequest;
    use crate::entity::catalog::EnableStatus;

    #[test]
    fn brand_request_new_carries_required_fields() {
        let request = CreateProductBrandRequest::new("BR-001", "品牌");
        assert_eq!(request.brand_code, "BR-001");
        assert_eq!(request.name, "品牌");
        assert_eq!(request.status, None);
        assert!(request.validate().is_ok());
    }

    #[test]
    fn brand_request_chainable_setters_fill_optional_fields() {
        let request = CreateProductBrandRequest::new("BR-002", "品牌二").with_status(EnableStatus::Disabled);
        assert_eq!(request.status, Some(EnableStatus::Disabled));
        assert!(request.validate().is_ok());

        let empty = CreateProductBrandRequest::new("   ", "品牌");
        assert!(empty.validate().is_err());
    }
}
