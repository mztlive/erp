//! 供给列表查询与跨域展示的唯一响应类型。
pub use application_core::PageView;
pub(crate) use application_core::{SortDir, normalize_sort};
use erp_core::ids::{SkuId, SupplierAccountId};
use erp_supply::entity::supplier_offering::{AvailabilityStatus, OfferingSourceType, OfferingStatus};
use serde::{Deserialize, Serialize};
use validator::Validate;
pub(crate) const OFFERING_SORT_FIELDS: &[&str] = &["created_at", "supplier_sku_code", "status"];
/// 供给列表查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SupplierOfferingListParams {
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
    /// 关键字：供应商订货编码、公司 SKU 编号或 SKU 名称。
    pub q: Option<String>,
    /// 公司 SKU。
    pub sku_id: Option<String>,
    /// 公司 SKU 编号筛选（模糊、忽略大小写）。
    pub sku_no: Option<String>,
    /// 公司商品（SPU）编号筛选（模糊、忽略大小写）。
    pub product_no: Option<String>,
    /// 供应商。
    pub supplier_id: Option<String>,
    /// 供给关系状态。
    pub status: Option<OfferingStatus>,
    /// 登记来源。
    pub source_type: Option<OfferingSourceType>,
    /// 当前可供状态。
    pub availability_status: Option<AvailabilityStatus>,
    /// 页码。
    #[validate(range(min = 1, message = "页码必须大于 0"))]
    pub page: Option<u64>,
    /// 每页数量。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在 1-100 之间"))]
    pub page_size: Option<u32>,
    /// 排序字段。
    pub sort_by: Option<String>,
    /// 排序方向。
    pub sort_dir: Option<String>,
}

/// 供给列表视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierOfferingView {
    /// 供给主键。
    pub id: String,
    /// 公司 SKU。
    pub sku_id: String,
    /// 公司 SKU 编号。
    pub sku_no: Option<String>,
    /// 公司商品编号。
    pub product_no: Option<String>,
    /// 公司 SKU 名称。
    pub sku_name: Option<String>,
    /// 公司 SKU 规格。
    pub specification: Option<String>,
    /// 供应商。
    pub supplier_id: String,
    /// 供应商编号。
    pub supplier_no: Option<String>,
    /// 供应商名称。
    pub supplier_name: Option<String>,
    /// 供应商侧商品编码。
    pub supplier_product_code: Option<String>,
    /// 供应商侧订货 SKU 编码。
    pub supplier_sku_code: String,
    /// 登记来源。
    pub source_type: OfferingSourceType,
    /// API 来源连接。
    pub source_connection_id: Option<String>,
    /// 供给关系状态。
    pub status: OfferingStatus,
    /// 当前商业条款修订。
    pub current_revision_id: Option<String>,
    /// 当前修订号。
    pub current_revision_no: Option<u32>,
    /// 一件代发含税价。
    pub dropship_supply_price_gross: Option<String>,
    /// 一件代发不含税价。
    pub dropship_supply_price_net: Option<String>,
    /// 集采含税价。
    pub bulk_supply_price_gross: Option<String>,
    /// 集采不含税价。
    pub bulk_supply_price_net: Option<String>,
    /// 进项税率。
    pub input_tax_rate: Option<String>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: Option<String>,
    /// 可供区域。
    pub supply_region: Vec<String>,
    /// 商品级能力。
    pub product_capabilities: Vec<String>,
    /// 一件代发快递说明。
    pub dropship_express: Option<String>,
    /// 运费。
    pub freight_amount: Option<String>,
    /// 服务费。
    pub service_fee_amount: Option<String>,
    /// 生效日期。
    pub valid_from: Option<String>,
    /// 失效日期。
    pub valid_to: Option<String>,
    /// 当前可供状态。
    pub availability_status: Option<AvailabilityStatus>,
    /// 当前可供数量。
    pub available_quantity: Option<String>,
    /// 可供来源更新时间。
    pub availability_source_updated_at: Option<i64>,
    /// 可供投影版本。
    pub availability_version: Option<u64>,
    /// 供给乐观锁版本。
    pub version: u64,
    /// 创建时间。
    pub created_at: u64,
    /// 当前维护人。
    #[serde(default)]
    pub maintainer_user_id: String,
    /// 授权行的维护人姓名；缺失时为空。
    pub maintainer_user_name: Option<String>,
    /// 当前业务组织。
    #[serde(default)]
    pub business_org_unit_id: String,
}

/// 列表响应保持现有字段并声明独立的授权时点及版本。
#[derive(Debug, Clone, Serialize)]
pub struct SupplierOfferingListView {
    /// 分页结果与归属口径。
    #[serde(flatten)]
    pub data: application_core::OwnershipPage<SupplierOfferingView>,
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
    /// 当前供给范围口径摘要，不含内部授权证明。
    pub scope_summary: &'static str,
}

impl SupplierOfferingView {
    /// 清除采购成本、税率和费用字段。
    pub fn redact_costs(&mut self) {
        self.dropship_supply_price_gross = None;
        self.dropship_supply_price_net = None;
        self.bulk_supply_price_gross = None;
        self.bulk_supply_price_net = None;
        self.input_tax_rate = None;
        self.freight_amount = None;
        self.service_fee_amount = None;
    }
}

impl SupplierOfferingListParams {
    /// 返回规整后的公司 SKU 主键。
    ///
    /// # 参数
    /// 无，读取 `self.sku_id`。
    ///
    /// # 返回
    /// 去空白后非空时返回类型化主键，否则返回 `None`。
    ///
    /// # 错误
    /// 永不失败；非法形态由后续仓储精确过滤处理。
    ///
    /// # 约束
    /// 纯内存转换，不触碰 I/O、时钟或密钥。
    pub fn typed_sku_id(&self) -> Option<SkuId> {
        typed_id(self.sku_id.as_deref(), SkuId::new)
    }

    /// 返回规整后的供应商主键。
    ///
    /// # 参数
    /// 无，读取 `self.supplier_id`。
    ///
    /// # 返回
    /// 去空白后非空时返回类型化主键，否则返回 `None`。
    ///
    /// # 错误
    /// 永不失败；非法形态由后续仓储精确过滤处理。
    ///
    /// # 约束
    /// 纯内存转换，不触碰 I/O、时钟或密钥。
    pub fn typed_supplier_id(&self) -> Option<SupplierAccountId> {
        typed_id(self.supplier_id.as_deref(), SupplierAccountId::new)
    }
}

/// 规整可选 ID 字符串。
///
/// # 参数
/// * `value` - 原始字符串
/// * `constructor` - 类型化主键构造器
///
/// # 返回
/// 去空白后非空时返回类型化主键，否则返回 `None`。
///
/// # 错误
/// 永不失败。
///
/// # 约束
/// 纯内存转换，不触碰 I/O、时钟或密钥。
fn typed_id<T>(value: Option<&str>, constructor: impl Fn(String) -> T) -> Option<T> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(|value| constructor(value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sort_contract_rejects_unknown_fields() {
        assert_eq!(
            normalize_sort(&Some("status".to_string()), &Some("asc".to_string()), OFFERING_SORT_FIELDS)
                .unwrap(),
            ("status", SortDir::Asc)
        );
        assert!(normalize_sort(&Some("unsafe".to_string()), &None, OFFERING_SORT_FIELDS).is_err());
    }
    #[test]
    fn list_filter_contract_accepts_source_and_availability() {
        let params: SupplierOfferingListParams = serde_json::from_value(serde_json::json!({
            "source_type": "EXCEL",
            "availability_status": "AVAILABLE"
        }))
        .unwrap();
        assert_eq!(params.source_type, Some(OfferingSourceType::Excel));
        assert_eq!(params.availability_status, Some(AvailabilityStatus::Available));
    }

    #[test]
    fn list_params_split_maintainer_and_procurement_owner_ids() {
        let params: SupplierOfferingListParams = serde_json::from_value(serde_json::json!({
            "owner_user_ids": "user-2,user-1,user-2",
            "procurement_owner_user_ids": "buyer-1",
            "org_unit_ids": "org-1",
            "include_descendants": true,
            "scope_version": "v1"
        }))
        .unwrap();
        assert_eq!(
            params.owner_user_ids.as_ref().unwrap().as_slice(),
            &["user-1".to_string(), "user-2".to_string()]
        );
        assert_eq!(params.procurement_owner_user_ids.as_ref().unwrap().as_slice(), &["buyer-1".to_string()]);
        assert_eq!(params.org_unit_ids.as_ref().unwrap().as_slice(), &["org-1".to_string()]);
        assert_eq!(params.include_descendants, Some(true));
        assert_eq!(params.scope_version.as_deref(), Some("v1"));
        assert!(
            serde_json::from_value::<SupplierOfferingListParams>(serde_json::json!({
                "created_by_user_ids": "user-1"
            }))
            .is_err()
        );
    }
    #[test]
    fn cost_redaction_keeps_identity_and_availability() {
        let mut view = SupplierOfferingView {
            id: "o1".to_string(),
            sku_id: "s1".to_string(),
            sku_no: Some("SKU-1".to_string()),
            product_no: None,
            sku_name: Some("商品".to_string()),
            specification: None,
            supplier_id: "supplier-1".to_string(),
            supplier_no: None,
            supplier_name: None,
            supplier_product_code: None,
            supplier_sku_code: "S-1".to_string(),
            source_type: OfferingSourceType::Manual,
            source_connection_id: None,
            status: OfferingStatus::Active,
            current_revision_id: None,
            current_revision_no: None,
            dropship_supply_price_gross: Some("10".to_string()),
            dropship_supply_price_net: Some("9".to_string()),
            bulk_supply_price_gross: Some("8".to_string()),
            bulk_supply_price_net: Some("7".to_string()),
            input_tax_rate: Some("0.13".to_string()),
            bulk_minimum_order_quantity: Some("10".to_string()),
            supply_region: vec![],
            product_capabilities: vec![],
            dropship_express: None,
            freight_amount: Some("1".to_string()),
            service_fee_amount: None,
            valid_from: None,
            valid_to: None,
            availability_status: None,
            available_quantity: Some("5".to_string()),
            availability_source_updated_at: None,
            availability_version: None,
            version: 1,
            created_at: 1,
            maintainer_user_id: "user-1".to_string(),
            maintainer_user_name: None,
            business_org_unit_id: "org-1".to_string(),
        };
        view.redact_costs();
        assert!(view.dropship_supply_price_gross.is_none());
        assert_eq!(view.available_quantity.as_deref(), Some("5"));
        assert_eq!(view.supplier_sku_code, "S-1");
    }
    #[test]
    fn typed_list_ids_trim_and_omit_blank() {
        let params = SupplierOfferingListParams {
            sku_id: Some("  ".to_string()),
            supplier_id: Some(" supplier-1 ".to_string()),
            ..Default::default()
        };
        assert!(params.typed_sku_id().is_none());
        assert_eq!(params.typed_supplier_id().unwrap().to_string(), "supplier-1");
    }
}
