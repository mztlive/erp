use entities::catalog::{EnableStatus, VoucherCategoryProfileRevision};
use entities::common::time::BusinessDate;
use entities::ids::{ProductBrandId, ProductCategoryId, SkuId, UnitOfMeasureId};
use entities::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// 卡券类目扩展修订列表允许的排序字段白名单。
pub(crate) const VOUCHER_PROFILE_SORT_FIELDS: &[&str] = &["created_at", "revision_no"];
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
