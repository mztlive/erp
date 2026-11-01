//! 门户过程请求合同；归属与内部责任不接收外部调用方指定。

use erp_catalog::portal::{ExistingProductRef, NewProductInput, NormalizedProduct};
use erp_supplier::portal::CooperationRequest;
use serde::{Deserialize, Serialize};

/// 已保存申请的版本命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalTransition {
    #[serde(alias = "version")]
    pub expected_version: u64,
    pub idempotency_key: String,
}

/// 一次具体采购人的审核决定。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortalDecision {
    Approve,
    Return,
}

/// 申请、任务及匹配版本共同冻结的内部审核命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalReview {
    #[serde(alias = "version")]
    pub expected_version: u64,
    pub work_item_id: String,
    pub work_item_version: u64,
    pub idempotency_key: String,
    pub decision: PortalDecision,
    pub comment: Option<String>,
    pub maintainer_user_id: Option<String>,
    pub normalized_product: Option<NormalizedProduct>,
    pub existing_product: Option<ExistingProductRef>,
    #[serde(default)]
    pub existing_offerings: Vec<ReviewedOfferingRef>,
    #[serde(default)]
    pub availability_reported_at_confirmed: bool,
}

/// 内部核对时冻结的已有供给身份，阻断确认期间版本漂移。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedOfferingRef {
    pub row_id: String,
    pub offering_id: String,
    pub expected_offering_version: u64,
    pub expected_revision_no: u32,
}

/// 新品草稿保存命令。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewProductSave {
    pub input: NewProductInput,
    #[serde(alias = "version")]
    pub expected_version: Option<u64>,
    pub idempotency_key: String,
}

/// 当前供应商合作条款的可编辑申请。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CooperationSave {
    pub input: CooperationRequest,
    #[serde(alias = "version")]
    pub expected_version: Option<u64>,
    pub idempotency_key: String,
}

/// 内部将精确公司SKU定向开放给精确供应商。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalGrantInput {
    pub supplier_id: String,
    pub sku_id: String,
    pub active: bool,
    pub expected_version: Option<u64>,
    pub idempotency_key: String,
}
