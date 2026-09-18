use application_core::normalized_text;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    FileAssetId, ProductBrandId, ProductCategoryId, ProductId, SkuId, SkuRevisionId, UnitOfMeasureId,
};
use erp_core::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::common::{PageParams, non_blank, paging_params};
use crate::entity::catalog::product_revision_media::{MediaRole, ProductRevisionMedia};
use crate::entity::catalog::{
    EnableStatus, ListingStatus, ProductKind, ProductListingStatus, ProductRevision, Sku,
};
use crate::error::Result;
use crate::repository::{ProductRevisionRow, ProductRow, SkuRow};

/// 商品列表允许的排序字段白名单。
pub(crate) const PRODUCT_SORT_FIELDS: &[&str] = &["created_at", "product_no"];
/// 商品修订列表允许的排序字段白名单。
pub(crate) const PRODUCT_REVISION_SORT_FIELDS: &[&str] = &["created_at", "revision_no"];
/// SKU 列表允许的排序字段白名单。
pub(crate) const SKU_SORT_FIELDS: &[&str] = &["created_at", "sku_no"];

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
    /// 商品维护人；缺省为操作人。禁止用创建人字段兜底。
    #[serde(default)]
    pub maintainer_user_id: Option<String>,
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

/// 商品停用命令（当前商品修订、媒体和有效期终点均由后端读取并复制）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DisableProductRequest {
    /// 用户打开页面时看到的商品乐观锁版本；冲突时拒绝覆盖。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 本次停用原因，写入同一事务内的审计日志。
    #[validate(length(max = 256, message = "停用原因过长"))]
    pub change_reason: Option<String>,
    /// 停用修订的生效开始日。
    pub effective_from: BusinessDate,
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
    /// 当前维护人。
    #[serde(default)]
    pub maintainer_user_id: String,
    /// 当前业务组织。
    #[serde(default)]
    pub business_org_unit_id: String,
}

impl From<ProductRow> for ProductView {
    /// 从投影行构造响应视图，与 `From<Product>` 同语义（erp-catalog-002）。
    ///
    /// 确定性字段搬运只在此一处实现；`product_page_view` 等列表装配只做映射调用。
    ///
    /// # 参数
    /// * `row` - 商品列表投影行
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(row: ProductRow) -> Self {
        Self {
            id: row.id,
            product_no: row.product_no,
            product_kind: row.product_kind,
            name: row.name,
            category_id: row.category_id,
            brand_id: row.brand_id,
            status: row.status,
            listing_status: row.listing_status,
            listed_sku_count: row.listed_sku_count,
            sku_count: row.sku_count,
            supplied_sku_count: row.supplied_sku_count,
            priced_sku_count: row.priced_sku_count,
            current_revision_id: row.current_revision_id,
            created_at: row.created_at,
            version: row.version,
            maintainer_user_id: row.maintainer_user_id,
            business_org_unit_id: row.business_org_unit_id,
        }
    }
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

impl ProductListingView {
    /// 以商品稳定 ID 构造未上架的继承状态视图。
    ///
    /// # 参数
    /// * `product_id` - 商品稳定 ID
    ///
    /// # 返回
    /// 返回计数为零的未上架视图。
    ///
    /// # 错误
    /// 无。
    pub fn new(product_id: impl Into<String>) -> Self {
        Self {
            product_id: product_id.into(),
            listing_status: ProductListingStatus::Unlisted,
            listed_sku_count: 0,
            sku_count: 0,
        }
    }

    /// 设置继承的上架状态与 SKU 计数。
    ///
    /// # 参数
    /// * `listing_status` - 从当前启用 SKU 继承的状态
    /// * `listed_sku_count` - 当前已上架 SKU 数
    /// * `sku_count` - 当前启用 SKU 总数
    ///
    /// # 返回
    /// 返回更新后的状态视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_counts(
        mut self,
        listing_status: ProductListingStatus,
        listed_sku_count: u32,
        sku_count: u32,
    ) -> Self {
        self.listing_status = listing_status;
        self.listed_sku_count = listed_sku_count;
        self.sku_count = sku_count;
        self
    }
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

impl From<ProductRevisionMedia> for ProductRevisionMediaView {
    /// 从媒体实体构造响应视图（erp-catalog-002）。
    ///
    /// # 参数
    /// * `media` - 商品修订媒体实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(media: ProductRevisionMedia) -> Self {
        Self {
            id: media.base.id,
            file_asset_id: media.file_asset_id.to_string(),
            media_role: media.media_role,
            sort_order: media.sort_order,
            alt_text: media.alt_text,
        }
    }
}

impl From<ProductRevisionRow> for ProductRevisionView {
    /// 从投影行构造响应视图，与 `From<ProductRevision>` 同语义（erp-catalog-002）。
    ///
    /// 媒体行转换复用 `From<ProductRevisionMedia>`；列表装配只做映射调用。
    ///
    /// # 参数
    /// * `row` - 商品修订列表投影行（含已装配媒体行）
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(row: ProductRevisionRow) -> Self {
        Self {
            id: row.id,
            product_id: row.product_id,
            revision_no: row.revision_no,
            name: row.name,
            description: row.description,
            specification: row.specification,
            category_id: row.category_id,
            brand_id: row.brand_id,
            status: row.status,
            effective_from: row.effective_from,
            effective_to: row.effective_to,
            media: row.media.into_iter().map(Into::into).collect(),
            created_at: row.created_at,
            version: row.version,
        }
    }
}

/// 商品修订列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
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
        Ok(ProductRevisionListQuery {
            product_id: self.product_id.as_ref().map(|id| id.to_string()),
            status: self.status,
            paging: paging_params(
                self.page,
                self.page_size,
                &self.sort_by,
                &self.sort_dir,
                PRODUCT_REVISION_SORT_FIELDS,
            )?,
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

impl From<SkuRow> for SkuView {
    /// 从投影行构造响应视图，与 `From<Sku>` 同语义（erp-catalog-002）。
    ///
    /// # 参数
    /// * `row` - SKU 列表投影行（含已装配的当前修订名称）
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(row: SkuRow) -> Self {
        Self {
            id: row.id,
            sku_no: row.sku_no,
            product_id: row.product_id,
            base_unit_id: row.base_unit_id,
            specification_signature: row.specification_signature,
            status: row.status,
            listing_status: row.listing_status,
            current_revision_id: row.current_revision_id,
            name: row.name,
            created_at: row.created_at,
            version: row.version,
        }
    }
}

/// SKU 列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
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
        Ok(SkuListQuery {
            q: normalized_text(self.q.as_deref()),
            sku_no: normalized_text(self.sku_no.as_deref()),
            product_id: self.product_id.as_ref().map(|id| id.to_string()),
            status: self.status,
            listing_status: self.listing_status,
            paging: paging_params(self.page, self.page_size, &self.sort_by, &self.sort_dir, SKU_SORT_FIELDS)?,
        })
    }
}
