use entities::catalog::{EnableStatus, VoucherCategoryProfileRevision};
use entities::common::time::BusinessDate;
use entities::ids::{ProductBrandId, ProductCategoryId, SkuId, UnitOfMeasureId};
use entities::money::{Amount, Quantity};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};
use super::product::ProductSkuInput;

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

impl VoucherSkuInput {
    /// 构造缺省卡券唯一 SKU 输入。
    ///
    /// 默认单位 ID 必须由 Service 先从字典解析后再注入；本方法不访问数据库。
    ///
    /// # 参数
    /// * `base_unit_id` - 已解析出的默认基础单位
    ///
    /// # 返回
    /// 返回无条码、物流属性和价格的最小 SKU 输入。
    ///
    /// # 错误
    /// 无。
    pub fn default_for_unit(base_unit_id: UnitOfMeasureId) -> Self {
        Self {
            base_unit_id,
            barcode: None,
            weight_kg: None,
            volume_m3: None,
            sales_visible_price_gross: None,
            market_price: None,
        }
    }

    /// 把卡券类目 SKU 输入转换为通用商品唯一 SKU 输入。
    ///
    /// 转换结果固定：无既有身份、无规格、无主图且 `reenable=false`。
    ///
    /// # 参数
    /// * `voucher_no` - 同时作为商品编号与 SKU 编号的卡券编号
    /// * `name` - 商品与 SKU 当前名称
    ///
    /// # 返回
    /// 返回不携带既有身份、无规格和无主图的通用 SKU 输入。
    ///
    /// # 错误
    /// 无。
    pub fn into_product_sku(self, voucher_no: String, name: String) -> ProductSkuInput {
        ProductSkuInput {
            sku_id: None,
            expected_sku_revision_id: None,
            reenable: false,
            sku_no: voucher_no,
            name,
            base_unit_id: self.base_unit_id,
            barcode: self.barcode,
            main_image_asset_id: None,
            weight_kg: self.weight_kg,
            volume_m3: self.volume_m3,
            sales_visible_price_gross: self.sales_visible_price_gross,
            market_price: self.market_price,
            spec_entries: Vec::new(),
        }
    }
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
    use std::str::FromStr;

    use entities::ids::UnitOfMeasureId;
    use entities::money::{Amount, Quantity};

    use super::VoucherSkuInput;

    /// 缺省 SKU 只有已注入的单位，其余字段为空且转换后无身份/规格/主图。
    #[test]
    fn default_voucher_sku_converts_without_identity_or_spec() {
        let input = VoucherSkuInput::default_for_unit(UnitOfMeasureId::new("unit-sheet"));
        let sku = input.into_product_sku("V-001".to_string(), "体验卡".to_string());

        assert_eq!(sku.base_unit_id.as_ref(), "unit-sheet");
        assert!(sku.barcode.is_none());
        assert!(sku.weight_kg.is_none());
        assert!(sku.volume_m3.is_none());
        assert!(sku.sales_visible_price_gross.is_none());
        assert!(sku.market_price.is_none());
        assert!(sku.sku_id.is_none());
        assert!(sku.expected_sku_revision_id.is_none());
        assert!(!sku.reenable);
        assert!(sku.main_image_asset_id.is_none());
        assert!(sku.spec_entries.is_empty());
        assert_eq!(sku.sku_no, "V-001");
        assert_eq!(sku.name, "体验卡");
    }

    /// 完整 SKU 的条码、单位、重量、体积和价格在转换中不得丢失。
    #[test]
    fn complete_voucher_sku_preserves_measurable_fields() {
        let input = VoucherSkuInput {
            base_unit_id: UnitOfMeasureId::new("unit-sheet"),
            barcode: Some("6901234567890".to_string()),
            weight_kg: Some(Quantity::from_str("0.010000").unwrap()),
            volume_m3: Some(Quantity::from_str("0.000100").unwrap()),
            sales_visible_price_gross: Some(Amount::from_str("99.00").unwrap()),
            market_price: Some(Amount::from_str("129.00").unwrap()),
        };
        let sku = input.into_product_sku("V-002".to_string(), "礼品卡".to_string());

        assert_eq!(sku.barcode.as_deref(), Some("6901234567890"));
        assert_eq!(sku.base_unit_id.as_ref(), "unit-sheet");
        assert_eq!(sku.weight_kg, Some(Quantity::from_str("0.010000").unwrap()));
        assert_eq!(sku.volume_m3, Some(Quantity::from_str("0.000100").unwrap()));
        assert_eq!(
            sku.sales_visible_price_gross,
            Some(Amount::from_str("99.00").unwrap())
        );
        assert_eq!(sku.market_price, Some(Amount::from_str("129.00").unwrap()));
        assert!(sku.sku_id.is_none());
        assert!(sku.expected_sku_revision_id.is_none());
        assert!(!sku.reenable);
        assert!(sku.main_image_asset_id.is_none());
        assert!(sku.spec_entries.is_empty());
    }
}
