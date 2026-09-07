//! 采购责任管理列表参数与展示契约。
use erp_core::ids::{ProductCategoryId, SkuId};
use erp_procurement::entity::facts::ProductKind;
use erp_procurement::entity::procurement_responsibility::{
    EnableStatus, ProcurementResponsibilityRule, ProcurementResponsibilityRuleType,
};
use serde::{Deserialize, Serialize};
use validator::Validate;
/// 规则列表查询参数。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ProcurementResponsibilityRuleListParams {
    /// 规则类型筛选。
    pub rule_type: Option<ProcurementResponsibilityRuleType>,
    /// 负责人筛选。
    pub owner_user_id: Option<String>,
    /// 状态筛选。
    pub status: Option<EnableStatus>,
    /// 页码，从 1 开始。
    #[serde(default = "default_page")]
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: u64,
    /// 每页条数。
    #[serde(default = "default_page_size")]
    #[validate(range(min = 1, max = 200, message = "每页条数必须在1-200之间"))]
    pub page_size: u32,
}

/// 采购责任规则管理视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementResponsibilityRuleView {
    /// 规则主键。
    pub id: String,
    /// 规则类型。
    pub rule_type: ProcurementResponsibilityRuleType,
    /// 展示优先级。
    pub priority: u8,
    /// SKU 选择器。
    pub sku_id: Option<SkuId>,
    /// SKU 业务编号展示。
    pub sku_no: Option<String>,
    /// SKU 当前修订名称展示。
    pub sku_name: Option<String>,
    /// 分类选择器。
    pub category_id: Option<ProductCategoryId>,
    /// 分类名称展示。
    pub category_name: Option<String>,
    /// 规范化服务区域。
    pub service_region: Option<String>,
    /// 商品类型选择器。
    pub product_kind: Option<ProductKind>,
    /// 具体负责人账号 ID。
    pub owner_user_id: String,
    /// 具体负责人展示姓名。
    pub owner_name: Option<String>,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间。
    pub created_at: u64,
    /// 更新时间。
    pub updated_at: u64,
}

/// 规则分页视图。
#[derive(Debug, Clone, Serialize)]
pub struct ProcurementResponsibilityRulePageView {
    /// 当前页规则。
    pub items: Vec<ProcurementResponsibilityRuleView>,
    /// 总数。
    pub total: i64,
    /// 页码。
    pub page: u64,
    /// 每页条数。
    pub page_size: u32,
}

impl From<ProcurementResponsibilityRule> for ProcurementResponsibilityRuleView {
    /// 将规则实体转换为管理视图。
    ///
    /// # 参数
    /// * `rule` - 规则实体
    ///
    /// # 返回
    /// 返回不暴露内部唯一键和审计冗余字段的视图。
    fn from(rule: ProcurementResponsibilityRule) -> Self {
        Self {
            id: rule.base.id,
            rule_type: rule.rule_type,
            priority: rule.rule_type.priority(),
            sku_id: rule.sku_id,
            sku_no: None,
            sku_name: None,
            category_id: rule.category_id,
            category_name: None,
            service_region: rule.service_region,
            product_kind: rule.product_kind,
            owner_user_id: rule.owner_user_id,
            owner_name: None,
            status: rule.status,
            version: rule.base.version,
            created_at: rule.base.created_at,
            updated_at: rule.base.updated_at,
        }
    }
}

/// 返回默认第一页。
fn default_page() -> u64 {
    1
}

/// 返回默认每页条数。
fn default_page_size() -> u32 {
    50
}
