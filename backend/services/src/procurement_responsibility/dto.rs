//! 采购责任规则管理与逐行预览 DTO。

use entities::catalog::{EnableStatus, ProductKind};
use entities::ids::{ProductCategoryId, SkuId};
use entities::procurement_responsibility::{
    ProcurementResponsibilityRule, ProcurementResponsibilityRuleData, ProcurementResponsibilityRuleType,
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

/// 创建采购责任规则请求。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateProcurementResponsibilityRuleRequest {
    /// 规则类型。
    pub rule_type: ProcurementResponsibilityRuleType,
    /// SKU 选择器。
    pub sku_id: Option<SkuId>,
    /// 分类选择器。
    pub category_id: Option<ProductCategoryId>,
    /// 服务区域选择器。
    #[validate(length(max = 128, message = "服务区域过长"))]
    pub service_region: Option<String>,
    /// 商品类型选择器。
    pub product_kind: Option<ProductKind>,
    /// 具体负责人账号 ID。
    #[validate(length(min = 1, max = 128, message = "采购负责人长度必须在1-128之间"))]
    pub owner_user_id: String,
    /// 启停状态。
    pub status: EnableStatus,
}

impl CreateProcurementResponsibilityRuleRequest {
    /// 转换为实体创建数据。
    ///
    /// # 返回
    /// 返回保持选择器语义的实体数据。
    ///
    /// # 错误
    /// 无；选择器形状由实体构造函数校验。
    pub(crate) fn into_data(self) -> ProcurementResponsibilityRuleData {
        ProcurementResponsibilityRuleData {
            rule_type: self.rule_type,
            sku_id: self.sku_id,
            category_id: self.category_id,
            service_region: self.service_region,
            product_kind: self.product_kind,
            owner_user_id: self.owner_user_id,
            status: self.status,
        }
    }
}

/// 整项更新采购责任规则请求。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateProcurementResponsibilityRuleRequest {
    /// 期望乐观锁版本。
    #[validate(range(min = 1, message = "乐观锁版本必须大于0"))]
    pub version: u64,
    /// 规则类型。
    pub rule_type: ProcurementResponsibilityRuleType,
    /// SKU 选择器。
    pub sku_id: Option<SkuId>,
    /// 分类选择器。
    pub category_id: Option<ProductCategoryId>,
    /// 服务区域选择器。
    #[validate(length(max = 128, message = "服务区域过长"))]
    pub service_region: Option<String>,
    /// 商品类型选择器。
    pub product_kind: Option<ProductKind>,
    /// 具体负责人账号 ID。
    #[validate(length(min = 1, max = 128, message = "采购负责人长度必须在1-128之间"))]
    pub owner_user_id: String,
    /// 启停状态。
    pub status: EnableStatus,
}

impl UpdateProcurementResponsibilityRuleRequest {
    /// 转换为实体整项更新数据。
    ///
    /// # 返回
    /// 返回期望版本与实体数据。
    ///
    /// # 错误
    /// 无；选择器形状由实体更新方法校验。
    pub(crate) fn into_parts(self) -> (u64, ProcurementResponsibilityRuleData) {
        let version = self.version;
        let data = ProcurementResponsibilityRuleData {
            rule_type: self.rule_type,
            sku_id: self.sku_id,
            category_id: self.category_id,
            service_region: self.service_region,
            product_kind: self.product_kind,
            owner_user_id: self.owner_user_id,
            status: self.status,
        };
        (version, data)
    }
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

/// 单条预览输入。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ProcurementResponsibilityResolveLineRequest {
    /// 调用方稳定行键，用于对应结果。
    #[validate(length(min = 1, max = 128, message = "行键长度必须在1-128之间"))]
    pub line_key: String,
    /// 精确 SKU。
    pub sku_id: SkuId,
    /// 服务区域。
    #[validate(length(max = 128, message = "服务区域过长"))]
    pub service_region: Option<String>,
}

/// 逐行责任预览请求。
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ProcurementResponsibilityResolveRequest {
    /// 待解析行，限制 1 至 200 行。
    #[validate(length(min = 1, max = 200, message = "解析行数必须在1-200之间"))]
    #[validate(nested)]
    pub lines: Vec<ProcurementResponsibilityResolveLineRequest>,
}

/// 单条解析成功视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementResponsibilityResolutionView {
    /// 调用方行键。
    pub line_key: String,
    /// 负责人账号 ID。
    pub owner_user_id: String,
    /// 负责人名称。
    pub owner_name: String,
    /// 命中规则 ID。
    pub rule_id: String,
    /// 命中规则类型。
    pub rule_type: ProcurementResponsibilityRuleType,
}

/// 单条预览结果；失败行不影响其他行的诊断结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementResponsibilityResolveLineView {
    /// 调用方行键。
    pub line_key: String,
    /// 是否解析成功。
    pub resolved: bool,
    /// 成功时的具体负责人。
    pub owner_user_id: Option<String>,
    /// 成功时的负责人名称。
    pub owner_name: Option<String>,
    /// 成功时命中规则 ID。
    pub rule_id: Option<String>,
    /// 成功时命中规则类型。
    pub rule_type: Option<ProcurementResponsibilityRuleType>,
    /// 失败时的稳定诊断文案。
    pub error: Option<String>,
}

/// 逐行责任预览响应。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementResponsibilityResolveView {
    /// 与请求行顺序一致的结果。
    pub lines: Vec<ProcurementResponsibilityResolveLineView>,
}

/// 返回默认第一页。
fn default_page() -> u64 {
    1
}

/// 返回默认每页条数。
fn default_page_size() -> u32 {
    50
}
