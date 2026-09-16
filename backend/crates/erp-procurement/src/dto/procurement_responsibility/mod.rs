//! 采购责任规则管理与逐行预览 DTO。

use erp_core::ids::{ProductCategoryId, SkuId};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::facts::ProductKind;
use crate::entity::procurement_responsibility::{
    EnableStatus, ProcurementResponsibilityRuleData, ProcurementResponsibilityRuleType,
};

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
    pub fn into_data(self) -> ProcurementResponsibilityRuleData {
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
    pub fn into_parts(self) -> (u64, ProcurementResponsibilityRuleData) {
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

impl ProcurementResponsibilityResolveLineView {
    /// 由必填行键构造解析行视图。
    ///
    /// # 参数
    /// * `line_key` - 调用方行键
    ///
    /// # 返回
    /// 返回未解析、成功与失败字段为空的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn new(line_key: impl Into<String>) -> Self {
        Self {
            line_key: line_key.into(),
            resolved: false,
            owner_user_id: None,
            owner_name: None,
            rule_id: None,
            rule_type: None,
            error: None,
        }
    }

    /// 设置是否解析成功。
    ///
    /// # 参数
    /// * `resolved` - 是否解析成功
    ///
    /// # 返回
    /// 返回更新后的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_resolved(mut self, resolved: bool) -> Self {
        self.resolved = resolved;
        self
    }

    /// 设置解析成功的负责人。
    ///
    /// # 参数
    /// * `owner_user_id` - 负责人账号 ID
    /// * `owner_name` - 负责人名称
    ///
    /// # 返回
    /// 返回更新后的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_owner(mut self, owner_user_id: impl Into<String>, owner_name: impl Into<String>) -> Self {
        self.owner_user_id = Some(owner_user_id.into());
        self.owner_name = Some(owner_name.into());
        self
    }

    /// 设置命中规则。
    ///
    /// # 参数
    /// * `rule_id` - 命中规则 ID
    /// * `rule_type` - 命中规则类型
    ///
    /// # 返回
    /// 返回更新后的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_rule(
        mut self,
        rule_id: impl Into<String>,
        rule_type: ProcurementResponsibilityRuleType,
    ) -> Self {
        self.rule_id = Some(rule_id.into());
        self.rule_type = Some(rule_type);
        self
    }

    /// 设置失败时的稳定诊断文案。
    ///
    /// # 参数
    /// * `error` - 稳定诊断文案
    ///
    /// # 返回
    /// 返回更新后的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }
}

/// 逐行责任预览响应。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProcurementResponsibilityResolveView {
    /// 与请求行顺序一致的结果。
    pub lines: Vec<ProcurementResponsibilityResolveLineView>,
}
