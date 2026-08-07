//! 域 D24 `supplier_catalog` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳，业务日期 `YYYY-MM-DD`；
//! 金额/数量以字符串传输（`entities::money` 的 serde 字符串形态）。
//!
//! 与 `erp-client/features/supplier-catalog/api.ts` 的差异（契约变更）：
//! - 前端为 session-mock 队列视图（`changeType`/`workItem` 等 W02 工作流投影），
//!   本阶段提供其底层实体接口（商品/SKU/映射/供给/入库批次），队列与工作项
//!   处理由 W02/D03 域承接；
//! - 媒体以来源 URL 快照登记，归档到受控文件资产由 D05 承接（本阶段
//!   `file_asset_id` 为空、`archive_status` 取 `PENDING_IMPORT`）。

use entities::supplier_catalog::{
    ArchiveStatus, AvailabilityStatus, CatalogItemStatus, CatalogSourceType, IntakeBatchStatus,
    MappingStatus, MediaUsage, OfferingStatus, SourceAttribute,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};

/// 供应商 SPU 列表允许的排序字段白名单。
pub(crate) const SUPPLIER_PRODUCT_SORT_FIELDS: &[&str] = &["created_at", "supplier_spu_code"];
/// 供应商 SKU 列表允许的排序字段白名单。
pub(crate) const SUPPLIER_SKU_SORT_FIELDS: &[&str] = &["created_at", "supplier_sku_code"];
/// 映射列表允许的排序字段白名单。
pub(crate) const MAPPING_SORT_FIELDS: &[&str] = &["created_at"];
/// 供给列表允许的排序字段白名单。
pub(crate) const OFFERING_SORT_FIELDS: &[&str] = &["created_at"];
/// 入库批次列表允许的排序字段白名单。
pub(crate) const INTAKE_BATCH_SORT_FIELDS: &[&str] = &["created_at"];

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
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

/// 契约目标形状的分页响应：`items` + `total` + `page` + `page_size`。
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

/// 校验文本去除首尾空白后非空。
fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 供应商 SPU 列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierCatalogProductListParams {
    /// 供应商 SPU 编码/名称模糊匹配。
    pub q: Option<String>,
    /// 供应商筛选。
    pub supplier_id: Option<String>,
    /// 来源类型筛选。
    pub source_type: Option<CatalogSourceType>,
    /// 状态筛选。
    pub status: Option<CatalogItemStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`supplier_spu_code`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 供应商 SPU 视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierCatalogProductView {
    /// 实体主键。
    pub id: String,
    /// 来源供应商。
    pub supplier_id: String,
    /// 来源类型。
    pub source_type: CatalogSourceType,
    /// 供应商 SPU 编码。
    pub supplier_spu_code: String,
    /// 状态。
    pub status: CatalogItemStatus,
    /// 当前来源修订。
    pub current_revision_id: Option<String>,
    /// 当前修订号。
    pub current_revision_no: Option<u32>,
    /// 来源名称（当前修订）。
    pub name: Option<String>,
    /// 来源分类（当前修订）。
    pub source_category: Option<String>,
    /// 来源品牌（当前修订）。
    pub source_brand: Option<String>,
    /// 来源更新时间（当前修订，秒级时间戳）。
    pub source_updated_at: Option<u64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商 SKU 列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierCatalogSkuListParams {
    /// 所属供应商 SPU 筛选。
    pub supplier_catalog_product_id: Option<String>,
    /// 供应商 SKU 编码/名称模糊匹配。
    pub q: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`supplier_sku_code`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 供应商 SKU 视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierCatalogSkuView {
    /// 实体主键。
    pub id: String,
    /// 所属供应商 SPU。
    pub supplier_catalog_product_id: String,
    /// 供应商 SKU 编码。
    pub supplier_sku_code: String,
    /// 状态。
    pub status: CatalogItemStatus,
    /// 当前来源修订。
    pub current_revision_id: Option<String>,
    /// 当前修订号。
    pub current_revision_no: Option<u32>,
    /// 来源商品名称（当前修订）。
    pub name: Option<String>,
    /// 来源规格（当前修订）。
    pub specification: Option<String>,
    /// 来源单位快照（当前修订）。
    pub source_base_unit: Option<String>,
    /// 条码（当前修订）。
    pub barcode: Option<String>,
    /// 已规范化的来源规格属性（当前修订）。
    pub structured_attributes: Vec<SourceAttribute>,
    /// 来源 SKU 主图取回地址（当前修订）。
    pub source_main_image_url: Option<String>,
    /// 来源 SKU 主图已登记的文件资产（当前修订）。
    pub source_main_image_asset_id: Option<String>,
    /// 一件代发底价（当前修订）。
    pub dropship_floor_price_gross: Option<String>,
    /// 集采底价（当前修订）。
    pub bulk_floor_price_gross: Option<String>,
    /// 集采起订量（当前修订）。
    pub bulk_minimum_order_quantity: Option<String>,
    /// 可供数量（当前修订）。
    pub available_quantity: Option<String>,
    /// 可供状态（当前修订）。
    pub availability_status: Option<AvailabilityStatus>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 映射列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierProductMappingListParams {
    /// 供应商 SKU 筛选。
    pub supplier_catalog_sku_id: Option<String>,
    /// 公司 SKU 筛选。
    pub sku_id: Option<String>,
    /// 映射状态筛选。
    pub status: Option<MappingStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 供应商 SKU → 公司 SKU 映射视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierProductMappingView {
    /// 实体主键。
    pub id: String,
    /// 供应商 SKU。
    pub supplier_catalog_sku_id: String,
    /// 公司 SKU。
    pub sku_id: String,
    /// 映射状态。
    pub status: MappingStatus,
    /// 审核人（`Active` 必填）。
    pub approved_by: Option<String>,
    /// 审核时间（秒级时间戳）。
    pub approved_at: Option<u64>,
    /// 映射依据。
    pub reason: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供给列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierOfferingListParams {
    /// 公司 SKU 筛选。
    pub sku_id: Option<String>,
    /// 供应商筛选。
    pub supplier_id: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 供给视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOfferingView {
    /// 实体主键。
    pub id: String,
    /// 公司 SKU。
    pub sku_id: String,
    /// 供应商。
    pub supplier_id: String,
    /// 供应商 SKU。
    pub supplier_catalog_sku_id: String,
    /// 供给状态。
    pub status: OfferingStatus,
    /// 当前供给修订。
    pub current_revision_id: Option<String>,
    /// 当前修订号。
    pub current_revision_no: Option<u32>,
    /// 一件代发供给价（含税）。
    pub dropship_supply_price_gross: Option<String>,
    /// 一件代发供给价（不含税）。
    pub dropship_supply_price_net: Option<String>,
    /// 集采供给价（含税）。
    pub bulk_supply_price_gross: Option<String>,
    /// 集采供给价（不含税）。
    pub bulk_supply_price_net: Option<String>,
    /// 进项税率。
    pub input_tax_rate: Option<String>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: Option<String>,
    /// 可供区域。
    pub supply_region: Vec<String>,
    /// 可供状态。
    pub availability_status: Option<AvailabilityStatus>,
    /// 可供数量（当前修订）。
    pub available_quantity: Option<String>,
    /// 一件代发快递说明（当前修订）。
    pub dropship_express: Option<String>,
    /// 费用金额（当前修订）。
    pub freight_amount: Option<String>,
    /// 服务费金额（当前修订）。
    pub service_fee_amount: Option<String>,
    /// 商品级能力（当前修订）。
    pub product_capabilities: Vec<String>,
    /// 有效期开始。
    pub valid_from: Option<String>,
    /// 有效期结束。
    pub valid_to: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 入库批次列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierCatalogIntakeBatchListParams {
    /// 供应商筛选。
    pub supplier_id: Option<String>,
    /// 来源类型筛选。
    pub source_type: Option<CatalogSourceType>,
    /// 批次状态筛选。
    pub status: Option<IntakeBatchStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 入库批次视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierCatalogIntakeBatchView {
    /// 实体主键。
    pub id: String,
    /// 来源类型。
    pub source_type: CatalogSourceType,
    /// 来源供应商。
    pub supplier_id: String,
    /// 来源引用（参与唯一键）。
    pub source_reference: String,
    /// 批次状态。
    pub status: IntakeBatchStatus,
    /// 批次级错误说明。
    pub error_text: Option<String>,
    /// 明细条数。
    pub item_count: i64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 来源媒体写入项。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierCatalogMediaWrite {
    /// 媒体用途（`SPU_CAROUSEL`/`SPU_DETAIL`）。
    pub usage: MediaUsage,
    /// 来源取回地址（不得作为公司商品长期媒体值）。
    #[validate(custom(function = "non_blank", message = "媒体来源地址不能为空"))]
    pub url: String,
    /// 已登记的文件资产（上传后的受控文件；缺省为 `None`）。
    pub file_asset_id: Option<String>,
}

/// 供应商 SKU 写入项。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierCatalogSkuWrite {
    /// 供应商 SKU 编码。
    #[validate(custom(function = "non_blank", message = "供应商 SKU 编码不能为空"))]
    pub supplier_sku_code: String,
    /// 来源商品名称。
    #[validate(custom(function = "non_blank", message = "供应商商品名称不能为空"))]
    pub name: String,
    /// 来源规格。
    #[validate(custom(function = "non_blank", message = "供应商规格不能为空"))]
    pub specification: String,
    /// 来源单位快照。
    pub source_base_unit: Option<String>,
    /// 条码。
    pub barcode: Option<String>,
    /// 来源 SKU 主图取回地址（归档前快照；不得作为公司商品长期媒体值）。
    pub source_main_image_url: Option<String>,
    /// 来源 SKU 主图已登记的文件资产（上传后的受控文件；缺省为 `None`）。
    pub source_main_image_asset_id: Option<String>,
    /// 一件代发底价（含税运）。
    pub dropship_floor_price_gross: Option<String>,
    /// 集采底价（含税）。
    pub bulk_floor_price_gross: Option<String>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: Option<String>,
    /// 可供数量。
    pub available_quantity: Option<String>,
    /// 可供状态。
    pub availability_status: AvailabilityStatus,
    /// 已规范化的来源规格属性。
    pub structured_attributes: Vec<SourceAttribute>,
}

/// 创建供应商商品请求（Excel/API/手工共用同一命令形状）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierCatalogProductRequest {
    /// 来源类型。
    pub source_type: CatalogSourceType,
    /// 来源供应商。
    pub supplier_id: String,
    /// 来源引用（参与批次唯一键；缺省取幂等键）。
    pub source_reference: Option<String>,
    /// 供应商 SPU 编码。
    #[validate(custom(function = "non_blank", message = "供应商 SPU 编码不能为空"))]
    pub supplier_spu_code: String,
    /// SPU 名称。
    #[validate(custom(function = "non_blank", message = "供应商商品名称不能为空"))]
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 来源商品类型声明；手工来源必填。
    pub source_product_kind: Option<String>,
    /// 来源分类。
    pub source_category: Option<String>,
    /// 来源品牌。
    pub source_brand: Option<String>,
    /// 结构化描述属性。
    pub structured_attributes: Vec<SourceAttribute>,
    /// 来源媒体。
    pub media: Vec<SupplierCatalogMediaWrite>,
    /// 来源修订标识。
    pub source_revision_token: Option<String>,
    /// 有效期开始（`YYYY-MM-DD`）。
    pub valid_from: Option<String>,
    /// 有效期结束（`YYYY-MM-DD`）。
    pub valid_to: Option<String>,
    /// 供应商 SKU 集合（至少一行）。
    #[validate(length(min = 1, message = "至少需要一个供应商 SKU"))]
    pub skus: Vec<SupplierCatalogSkuWrite>,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 创建供应商商品结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreateSupplierCatalogProductResult {
    /// 供应商 SPU 主键。
    pub product_id: String,
    /// 供应商 SKU 主键集合。
    pub sku_ids: Vec<String>,
    /// 入库批次主键。
    pub intake_batch_id: String,
    /// 入库明细主键。
    pub intake_item_id: String,
    /// 是否幂等重放。
    pub replayed: bool,
    /// 业务引用。
    pub reference: String,
    /// 记录时间（秒级时间戳）。
    pub recorded_at: u64,
}

/// 供应商商品中心保存请求（形成新的来源修订，不写公司主档）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReviseSupplierCatalogProductRequest {
    /// 期望的当前来源修订号（乐观并发校验）。
    #[validate(range(min = 1, message = "期望来源修订号必须大于 0"))]
    pub expected_revision_no: u32,
    /// 供应商 SPU 编码。
    #[validate(custom(function = "non_blank", message = "供应商 SPU 编码不能为空"))]
    pub supplier_spu_code: String,
    /// SPU 名称。
    #[validate(custom(function = "non_blank", message = "供应商商品名称不能为空"))]
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 来源商品类型声明；手工来源必填。
    pub source_product_kind: Option<String>,
    /// 来源分类。
    pub source_category: Option<String>,
    /// 来源品牌。
    pub source_brand: Option<String>,
    /// 结构化描述属性。
    pub structured_attributes: Vec<SourceAttribute>,
    /// 来源媒体。
    pub media: Vec<SupplierCatalogMediaWrite>,
    /// 来源修订标识。
    pub source_revision_token: Option<String>,
    /// 有效期开始（`YYYY-MM-DD`）。
    pub valid_from: Option<String>,
    /// 有效期结束（`YYYY-MM-DD`）。
    pub valid_to: Option<String>,
    /// 完整 SKU 集合（整表替换）。
    pub skus: Vec<SupplierCatalogSkuWrite>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 供应商商品修订结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviseSupplierCatalogProductResult {
    /// 供应商 SPU 主键。
    pub product_id: String,
    /// 新来源修订号。
    pub revision_no: u32,
    /// 业务引用。
    pub reference: String,
    /// 记录时间（秒级时间戳）。
    pub recorded_at: u64,
}

/// 创建供应商 SKU → 公司 SKU 映射请求（初始 `PENDING`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierProductMappingRequest {
    /// 供应商 SKU。
    #[validate(custom(function = "non_blank", message = "供应商 SKU 不能为空"))]
    pub supplier_catalog_sku_id: String,
    /// 公司 SKU。
    #[validate(custom(function = "non_blank", message = "公司 SKU 不能为空"))]
    pub sku_id: String,
    /// 映射依据。
    pub reason: Option<String>,
}

/// 映射创建结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreateSupplierProductMappingResult {
    /// 映射主键。
    pub mapping_id: String,
    /// 映射状态。
    pub status: MappingStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 确认映射并登记双价供给请求（入池：映射 `Active` + 供给修订原子写入）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ApproveSupplierProductMappingRequest {
    /// 期望的映射乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 采购确认的一件代发供给价（含税；已含包装、发货费用）。
    #[validate(custom(function = "non_blank", message = "一件代发供给价不能为空"))]
    pub dropship_supply_price_gross: String,
    /// 采购确认的集采供给价（含税）。
    #[validate(custom(function = "non_blank", message = "集采供给价不能为空"))]
    pub bulk_supply_price_gross: String,
    /// 进项税率（如 `0.13`）。
    #[validate(custom(function = "non_blank", message = "进项税率不能为空"))]
    pub input_tax_rate: String,
    /// 集采起订量。
    #[validate(custom(function = "non_blank", message = "集采起订量不能为空"))]
    pub bulk_minimum_order_quantity: String,
    /// 可供区域（fail-closed 必填）。
    #[validate(length(min = 1, message = "可供区域不能为空"))]
    pub supply_region: Vec<String>,
    /// 有效期开始（`YYYY-MM-DD`）。
    #[validate(custom(function = "non_blank", message = "有效期开始不能为空"))]
    pub valid_from: String,
    /// 有效期结束（`YYYY-MM-DD`）。
    pub valid_to: Option<String>,
    /// 一件代发快递说明。
    pub dropship_express: Option<String>,
    /// 费用金额。
    pub freight_amount: Option<String>,
    /// 服务费金额。
    pub service_fee_amount: Option<String>,
    /// 可供数量。
    pub available_quantity: Option<String>,
}

/// 映射确认结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ApproveSupplierProductMappingResult {
    /// 映射主键。
    pub mapping_id: String,
    /// 映射状态。
    pub status: MappingStatus,
    /// 供给稳定身份。
    pub offering_id: String,
    /// 供给修订号。
    pub offering_revision_no: u32,
    /// 映射乐观锁版本。
    pub version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 供应商 SKU 相对公司商品池的状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PoolMatchStatus {
    /// 已有生效映射。
    Mapped,
    /// 未映射但有候选公司 SKU。
    HasCandidates,
    /// 未映射且无可靠候选。
    Unmatched,
}

/// 公司 SKU 匹配候选（供采购人工确认，不自动合并）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompanySkuMatchCandidateView {
    /// 公司 SKU ID。
    pub sku_id: String,
    /// 公司 SKU 编号。
    pub sku_no: String,
    /// 所属公司商品 ID。
    pub product_id: String,
    /// 公司商品编号。
    pub product_no: String,
    /// 展示名称（SKU 修订名或商品名）。
    pub name: String,
    /// 规格摘要。
    pub specification: Option<String>,
    /// 条码。
    pub barcode: Option<String>,
    /// 基础单位 ID。
    pub base_unit_id: String,
    /// 当前销售可见价。
    pub sales_visible_price_gross: Option<String>,
    /// 当前有效供应商供给数。
    pub active_supplier_count: u32,
    /// 匹配证据（如「条码一致」）。
    pub match_signals: Vec<String>,
    /// 匹配分（越高越优先展示）。
    pub score: u32,
}

/// 单个供应商 SKU 的池内状态与候选。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierSkuPoolMatchView {
    /// 供应商目录 SKU。
    pub supplier_catalog_sku_id: String,
    /// 供应商 SKU 编码。
    pub supplier_sku_code: String,
    /// 规格摘要。
    pub specification: Option<String>,
    /// 条码。
    pub barcode: Option<String>,
    /// 池内状态。
    pub pool_status: PoolMatchStatus,
    /// 已生效映射的公司 SKU（仅 `MAPPED`）。
    pub mapped_company_sku_id: Option<String>,
    /// 已生效映射的公司 SKU 编号。
    pub mapped_company_sku_no: Option<String>,
    /// 匹配候选（未映射时填充；已映射可为空）。
    pub candidates: Vec<CompanySkuMatchCandidateView>,
}

/// 供应商商品（SPU）下全部 SKU 的池内匹配结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierProductPoolMatchView {
    /// 供应商商品 ID。
    pub supplier_product_id: String,
    /// 当前 SPU 来源修订号。
    pub source_revision_no: u32,
    /// 各供应商 SKU 状态。
    pub items: Vec<SupplierSkuPoolMatchView>,
}

/// 关联入池：单行（挂已有公司 SKU + 双价供给）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LinkPromoteSkuItem {
    /// 供应商目录 SKU。
    #[validate(custom(function = "non_blank", message = "供应商 SKU 不能为空"))]
    pub supplier_catalog_sku_id: String,
    /// 目标公司 SKU。
    #[validate(custom(function = "non_blank", message = "公司 SKU 不能为空"))]
    pub company_sku_id: String,
    /// 一件代发供给价；空则回退目录代发底价。
    pub dropship_supply_price_gross: Option<String>,
    /// 集采供给价；空则回退目录集采底价。
    pub bulk_supply_price_gross: Option<String>,
}

/// 关联入池请求：已有公司 SKU 时只写映射 Active + 双价供给，不改公司价格。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LinkPromoteToCompanyPoolRequest {
    /// 供应商商品（SPU）。
    #[validate(custom(function = "non_blank", message = "供应商商品不能为空"))]
    pub supplier_product_id: String,
    /// 期望 SPU 来源修订号。
    #[validate(range(min = 1, message = "期望来源修订号必须大于 0"))]
    pub expected_source_revision_no: u32,
    /// 进项税率。
    #[validate(custom(function = "non_blank", message = "进项税率不能为空"))]
    pub input_tax_rate: String,
    /// 可供区域。
    #[validate(length(min = 1, message = "可供区域不能为空"))]
    pub supply_region: Vec<String>,
    /// 入选行（至少一行）。
    #[validate(length(min = 1, message = "至少选择一个供应商 SKU"))]
    #[validate(nested)]
    pub items: Vec<LinkPromoteSkuItem>,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 关联入池单行结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LinkPromoteSkuResult {
    /// 供应商目录 SKU。
    pub supplier_catalog_sku_id: String,
    /// 公司 SKU。
    pub company_sku_id: String,
    /// 映射 ID。
    pub mapping_id: String,
    /// 供给 ID。
    pub offering_id: String,
    /// 供给修订号。
    pub offering_revision_no: u32,
}

/// 关联入池结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LinkPromoteToCompanyPoolResult {
    /// 供应商商品。
    pub supplier_product_id: String,
    /// 各行结果。
    pub items: Vec<LinkPromoteSkuResult>,
    /// 业务引用。
    pub reference: String,
    /// 记录时间（秒）。
    pub recorded_at: u64,
}

/// 反向入池：单个供应商 SKU 行（新建公司 SKU + 映射 + 双价供给）。
///
/// 起订量不传：服务端读取供应商 SKU 当前来源修订上的 `bulk_minimum_order_quantity`。
/// 正式双价可省略：省略时回退目录底价；底价也缺失则校验失败。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReversePromoteSkuItem {
    /// 供应商目录 SKU 稳定身份。
    #[validate(custom(function = "non_blank", message = "供应商 SKU 不能为空"))]
    pub supplier_catalog_sku_id: String,
    /// 公司 SKU 编号；缺省时服务端按供应商 SKU 编码生成。
    pub sku_no: Option<String>,
    /// 一件代发供给价（含税运）；空则回退目录代发底价。
    pub dropship_supply_price_gross: Option<String>,
    /// 集采供给价（含税）；空则回退目录集采底价。
    pub bulk_supply_price_gross: Option<String>,
    /// 销售可见价（写入新建公司 `sku_revision`，必填）。
    #[validate(custom(function = "non_blank", message = "销售可见价不能为空"))]
    pub sales_visible_price_gross: String,
    /// 市场价（写入新建公司 `sku_revision`，必填）。
    #[validate(custom(function = "non_blank", message = "市场价不能为空"))]
    pub market_price: String,
}

/// 反向入池请求：以供应商 SPU 为上下文，同构新建公司 Product + 勾选 SKU 行，
/// 并原子写入映射与双价供给；确认即生效，不接受用户填写的生效日期。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReversePromoteToCompanyPoolRequest {
    /// 供应商商品（SPU）稳定身份。
    #[validate(custom(function = "non_blank", message = "供应商商品不能为空"))]
    pub supplier_product_id: String,
    /// 期望的供应商 SPU 来源修订号（并发保护）。
    #[validate(range(min = 1, message = "期望来源修订号必须大于 0"))]
    pub expected_source_revision_no: u32,
    /// 公司商品业务类型（独立稳定属性，创建后不可变）。
    pub product_kind: entities::catalog::ProductKind,
    /// 公司商品编号；缺省时服务端生成。
    pub product_no: Option<String>,
    /// ERP 分类 ID。
    #[validate(custom(function = "non_blank", message = "分类不能为空"))]
    pub category_id: String,
    /// ERP 品牌 ID。
    #[validate(custom(function = "non_blank", message = "品牌不能为空"))]
    pub brand_id: String,
    /// 公司 SKU 基础单位 ID（本批 SKU 共用；可后续在 W14 调整单行）。
    #[validate(custom(function = "non_blank", message = "基础单位不能为空"))]
    pub base_unit_id: String,
    /// 进项税率（正式供给，目录无此字段）。
    #[validate(custom(function = "non_blank", message = "进项税率不能为空"))]
    pub input_tax_rate: String,
    /// 可供区域（正式供给，目录无此字段）。
    #[validate(length(min = 1, message = "可供区域不能为空"))]
    pub supply_region: Vec<String>,
    /// 入选的供应商 SKU 行（至少一行；未选兄弟 SKU 保持未映射）。
    #[validate(length(min = 1, message = "至少选择一个供应商 SKU"))]
    #[validate(nested)]
    pub items: Vec<ReversePromoteSkuItem>,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 反向入池单行结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReversePromoteSkuResult {
    /// 供应商目录 SKU。
    pub supplier_catalog_sku_id: String,
    /// 新建公司 SKU。
    pub company_sku_id: String,
    /// 新建公司 SKU 修订。
    pub company_sku_revision_id: String,
    /// 映射主键。
    pub mapping_id: String,
    /// 供给稳定身份。
    pub offering_id: String,
    /// 供给修订号。
    pub offering_revision_no: u32,
}

/// 反向入池结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReversePromoteToCompanyPoolResult {
    /// 供应商商品（SPU）。
    pub supplier_product_id: String,
    /// 新建公司商品（SPU）。
    pub company_product_id: String,
    /// 公司商品编号。
    pub product_no: String,
    /// 公司商品类型。
    pub product_kind: entities::catalog::ProductKind,
    /// 各 SKU 行结果。
    pub items: Vec<ReversePromoteSkuResult>,
    /// 业务引用。
    pub reference: String,
    /// 记录时间（秒级时间戳）。
    pub recorded_at: u64,
}

/// 供给修订请求（改价/暂停/停止等，形成新的不可变供给修订）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReviseSupplierOfferingRequest {
    /// 期望的当前供给修订号（乐观并发校验）。
    #[validate(range(min = 1, message = "期望供给修订号必须大于 0"))]
    pub expected_revision_no: u32,
    /// 一件代发供给价（含税）。
    #[validate(custom(function = "non_blank", message = "一件代发供给价不能为空"))]
    pub dropship_supply_price_gross: String,
    /// 集采供给价（含税）。
    #[validate(custom(function = "non_blank", message = "集采供给价不能为空"))]
    pub bulk_supply_price_gross: String,
    /// 进项税率。
    #[validate(custom(function = "non_blank", message = "进项税率不能为空"))]
    pub input_tax_rate: String,
    /// 集采起订量。
    #[validate(custom(function = "non_blank", message = "集采起订量不能为空"))]
    pub bulk_minimum_order_quantity: String,
    /// 可供区域。
    #[validate(length(min = 1, message = "可供区域不能为空"))]
    pub supply_region: Vec<String>,
    /// 有效期开始。
    #[validate(custom(function = "non_blank", message = "有效期开始不能为空"))]
    pub valid_from: String,
    /// 有效期结束。
    pub valid_to: Option<String>,
    /// 一件代发快递说明。
    pub dropship_express: Option<String>,
    /// 费用金额。
    pub freight_amount: Option<String>,
    /// 服务费金额。
    pub service_fee_amount: Option<String>,
    /// 可供数量。
    pub available_quantity: Option<String>,
    /// 供给状态（启用/暂停/停止）。
    pub status: Option<OfferingStatus>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 供给修订结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviseSupplierOfferingResult {
    /// 供给稳定身份。
    pub offering_id: String,
    /// 新供给修订号。
    pub revision_no: u32,
    /// 供给状态。
    pub status: OfferingStatus,
    /// 供给乐观锁版本。
    pub version: u64,
    /// 业务引用。
    pub reference: String,
}

/// 供应商 SPU 详情视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierCatalogProductDetailView {
    /// SPU 视图。
    pub product: SupplierCatalogProductView,
    /// 来源修订历史（倒序）。
    pub revisions: Vec<SupplierCatalogProductRevisionView>,
    /// 媒体（当前修订）。
    pub media: Vec<SupplierCatalogMediaView>,
    /// SKU 列表（含各自修订）。
    pub skus: Vec<SupplierCatalogSkuDetailView>,
    /// 本 SPU 下的映射。
    pub mappings: Vec<SupplierProductMappingView>,
}

/// 供应商 SPU 来源修订视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierCatalogProductRevisionView {
    /// 修订主键。
    pub id: String,
    /// 修订号。
    pub revision_no: u32,
    /// SPU 名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 来源商品类型声明。
    pub source_product_kind: Option<String>,
    /// 来源分类。
    pub source_category: Option<String>,
    /// 来源品牌。
    pub source_brand: Option<String>,
    /// 结构化描述属性。
    pub structured_attributes: Vec<SourceAttribute>,
    /// 来源修订标识。
    pub source_revision_token: Option<String>,
    /// 来源更新时间（秒级时间戳）。
    pub source_updated_at: u64,
    /// 有效期开始。
    pub valid_from: Option<String>,
    /// 有效期结束。
    pub valid_to: Option<String>,
}

/// 来源媒体视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierCatalogMediaView {
    /// 媒体主键。
    pub id: String,
    /// 媒体用途。
    pub usage: MediaUsage,
    /// 来源取回地址。
    pub url: Option<String>,
    /// 已登记的文件资产（上传后的受控文件）。
    pub file_asset_id: Option<String>,
    /// 归档状态。
    pub archive_status: ArchiveStatus,
    /// 同用途展示顺序。
    pub sort_order: u32,
}

/// 供应商 SKU 详情视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierCatalogSkuDetailView {
    /// SKU 视图。
    pub sku: SupplierCatalogSkuView,
    /// 来源修订历史（倒序）。
    pub revisions: Vec<SupplierCatalogSkuRevisionView>,
}

/// 供应商 SKU 来源修订视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierCatalogSkuRevisionView {
    /// 修订主键。
    pub id: String,
    /// 修订号。
    pub revision_no: u32,
    /// 来源商品名称。
    pub name: String,
    /// 来源规格。
    pub specification: String,
    /// 来源单位快照。
    pub source_base_unit: Option<String>,
    /// 条码。
    pub barcode: Option<String>,
    /// 已规范化的来源规格属性。
    pub structured_attributes: Vec<SourceAttribute>,
    /// 来源 SKU 主图取回地址（归档前快照）。
    pub source_main_image_url: Option<String>,
    /// 来源 SKU 主图已登记的文件资产（上传后的受控文件）。
    pub source_main_image_asset_id: Option<String>,
    /// 一件代发底价。
    pub dropship_floor_price_gross: Option<String>,
    /// 集采底价。
    pub bulk_floor_price_gross: Option<String>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: Option<String>,
    /// 可供数量。
    pub available_quantity: Option<String>,
    /// 可供状态。
    pub availability_status: AvailabilityStatus,
    /// 来源更新时间（秒级时间戳）。
    pub source_updated_at: u64,
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, SortDir, SupplierCatalogProductListParams};
    use entities::supplier_catalog::CatalogSourceType;
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields() {
        assert!(normalize_sort(&Some("price".to_string()), &None, &["created_at"]).is_err());
        let (field, direction) = normalize_sort(
            &Some(" supplier_spu_code ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "supplier_spu_code"],
        )
        .unwrap();
        assert_eq!(field, "supplier_spu_code");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_validate_paging_bounds() {
        let params = SupplierCatalogProductListParams {
            q: Some(" SKU ".to_string()),
            supplier_id: Some(" sup-1 ".to_string()),
            source_type: Some(CatalogSourceType::Manual),
            status: None,
            page: Some(2),
            page_size: Some(50),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_ok());

        let invalid = SupplierCatalogProductListParams {
            page: Some(0),
            page_size: Some(u32::MAX),
            ..params
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn create_request_rejects_blank_and_empty_skus() {
        let request: super::CreateSupplierCatalogProductRequest = serde_json::from_value(json!({
            "source_type": "MANUAL",
            "supplier_id": "sup-1",
            "supplier_spu_code": "SPU-1",
            "name": "测试商品",
            "structured_attributes": [],
            "media": [],
            "skus": [],
            "idempotency_key": "k-1",
        }))
        .unwrap();
        assert!(request.validate().is_err());

        let request: super::CreateSupplierCatalogProductRequest = serde_json::from_value(json!({
            "source_type": "MANUAL",
            "supplier_id": "sup-1",
            "supplier_spu_code": "  ",
            "name": "空白编码",
            "structured_attributes": [],
            "media": [],
            "skus": [{
                "supplier_sku_code": "S1",
                "name": "n",
                "specification": "s",
                "availability_status": "AVAILABLE",
                "structured_attributes": []
            }],
            "idempotency_key": "k-2",
        }))
        .unwrap();
        assert!(request.validate().is_err(), "空白 SPU 编码必须被拒绝");
    }
}
