//! 商品范围列表与交接 DTO。

use application_core::normalized_text;
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::common::{PageParams, non_blank, paging_params, validate_sales_price_range};
use super::product::{PRODUCT_SORT_FIELDS, ProductView};
use crate::entity::catalog::{EnableStatus, ProductKind, ProductListingStatus, SkuCoverageStatus};
use crate::error::Result;

/// 列表响应保持现有字段并声明独立的授权时点及版本。
#[derive(Debug, Clone, Serialize)]
pub struct ProductListView {
    /// 分页结果与归属口径。
    #[serde(flatten)]
    pub data: application_core::OwnershipPage<ProductView>,
    /// 跨页与导出必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`；有规则但对象为空时不设置。
    pub empty_reason: Option<&'static str>,
    /// 当前商品范围口径摘要，不含内部授权证明。
    pub scope_summary: &'static str,
}

/// 商品维护人显式交接请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HandoverProductRequest {
    /// 目标维护人。
    #[validate(custom(function = "non_blank", message = "目标维护人不能为空"))]
    pub target_user_id: String,
    /// 显式目标业务组织；省略表示保留原组织。
    pub target_org_unit_id: Option<String>,
    /// 非空交接原因。
    #[validate(length(min = 1, max = 512, message = "交接原因不能为空"))]
    pub reason: String,
    /// 期望的商品乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_version: u64,
    /// 业务请求幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 商品维护人交接结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoverProductView {
    /// 商品稳定 ID。
    pub product_id: String,
    /// 交接后维护人。
    pub maintainer_user_id: String,
    /// 交接后业务组织。
    pub business_org_unit_id: String,
    /// 交接后商品版本。
    pub version: u64,
}

/// 商品交接待选目标。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandoverCandidateView {
    /// 目标账号 ID。
    pub user_id: String,
    /// 显示名。
    pub display_name: String,
    /// 登录账号。
    pub account: String,
}

/// 商品列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ProductListParams {
    /// 跨页与导出必须使用前一页的当前授权和业务版本。
    #[validate(length(min = 1, max = 256))]
    pub scope_version: Option<String>,
    /// 当前维护人 ID，逗号分隔，最多 100 项；只收窄授权结果。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 规则解析出的采购负责人，逗号分隔，最多 100 项；额外 AND。
    pub procurement_owner_user_ids: Option<application_core::QueryIds>,
    /// 当前业务组织，逗号分隔，最多 100 项；只收窄授权结果。
    pub org_unit_ids: Option<application_core::QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
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
    /// 当前维护人精确身份条件。
    pub owner_user_ids: Option<application_core::QueryIds>,
    /// 采购负责人筛选，不扩大维护人授权。
    pub procurement_owner_user_ids: Option<application_core::QueryIds>,
    /// 当前业务组织，只收窄授权结果。
    pub org_unit_ids: Option<application_core::QueryIds>,
    /// 组织筛选是否包含有效下级。
    pub include_descendants: Option<bool>,
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
        validate_sales_price_range(self.sales_price_min, self.sales_price_max)?;
        Ok(ProductListQuery {
            owner_user_ids: self.owner_user_ids.clone(),
            procurement_owner_user_ids: self.procurement_owner_user_ids.clone(),
            org_unit_ids: self.org_unit_ids.clone(),
            include_descendants: self.include_descendants,
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
            paging: paging_params(
                self.page,
                self.page_size,
                &self.sort_by,
                &self.sort_dir,
                PRODUCT_SORT_FIELDS,
            )?,
        })
    }
}
