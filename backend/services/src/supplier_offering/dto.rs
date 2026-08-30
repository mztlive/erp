//! 供应商供给 HTTP DTO；Handler 直接复用本文件类型。

use entities::supplier_offering::{AvailabilityStatus, OfferingSourceType, OfferingStatus};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::{Error, Result};
use crate::publication::SystemSafetyPauseOperationView;

pub(crate) const OFFERING_SORT_FIELDS: &[&str] = &["created_at", "supplier_sku_code", "status"];

/// 分页响应。
#[derive(Debug, Clone, Serialize)]
pub struct PageView<T> {
    /// 当前页数据。
    pub items: Vec<T>,
    /// 总数。
    pub total: i64,
    /// 页码。
    pub page: u64,
    /// 每页数量。
    pub page_size: u32,
}

/// 排序方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortDir {
    /// 升序。
    Asc,
    /// 降序。
    Desc,
}

/// 规范化并校验排序字段与方向。
///
/// # 参数
/// * `sort_by` - 排序字段
/// * `sort_dir` - 排序方向
/// * `allowed_fields` - 字段白名单
///
/// # 返回
/// 返回规范化排序字段与方向。
///
/// # 错误
/// 字段或方向不在合同范围时返回错误。
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
        Some("desc") | None => SortDir::Desc,
        Some(other) => return Err(Error::ValidationError(format!("非法排序方向: {other}"))),
    };
    Ok((sort_by, sort_dir))
}

fn non_blank(value: &str) -> std::result::Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new("不能为空白"));
    }
    Ok(())
}

/// 供给列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierOfferingListParams {
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

/// 供给商业条款公共写入字段。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierOfferingTermsWrite {
    /// 一件代发含税价。
    #[validate(custom(function = "non_blank", message = "一件代发供给价不能为空"))]
    pub dropship_supply_price_gross: String,
    /// 集采含税价。
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
    /// 商品级能力。
    #[serde(default)]
    pub product_capabilities: Vec<String>,
    /// 生效日期。
    #[validate(custom(function = "non_blank", message = "有效期开始不能为空"))]
    pub valid_from: String,
    /// 失效日期。
    pub valid_to: Option<String>,
    /// 一件代发快递说明。
    pub dropship_express: Option<String>,
    /// 运费。
    pub freight_amount: Option<String>,
    /// 服务费。
    pub service_fee_amount: Option<String>,
}

/// 新增供给请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierOfferingRequest {
    /// 公司 SKU。
    #[validate(custom(function = "non_blank", message = "公司 SKU 不能为空"))]
    pub sku_id: String,
    /// 供应商。
    #[validate(custom(function = "non_blank", message = "供应商不能为空"))]
    pub supplier_id: String,
    /// 供应商侧商品编码。
    pub supplier_product_code: Option<String>,
    /// 供应商侧订货 SKU 编码。
    #[validate(custom(function = "non_blank", message = "供应商 SKU 编码不能为空"))]
    pub supplier_sku_code: String,
    /// 登记来源。
    pub source_type: OfferingSourceType,
    /// API 来源连接。
    pub source_connection_id: Option<String>,
    /// 首版商业条款。
    #[validate(nested)]
    pub terms: SupplierOfferingTermsWrite,
    /// 初始可供状态。
    pub availability_status: AvailabilityStatus,
    /// 初始可供数量。
    pub available_quantity: Option<String>,
    /// 来源更新时间；空表示服务端接收时间。
    pub source_updated_at: Option<i64>,
    /// 来源版本标识。
    pub source_revision_token: Option<String>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "登记原因不能为空"))]
    pub change_reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 新增供给结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateSupplierOfferingResult {
    /// 供给主键。
    pub offering_id: String,
    /// 首版商业条款主键。
    pub revision_id: String,
    /// 实时可供投影主键。
    pub availability_id: String,
    /// 修订号。
    pub revision_no: u32,
    /// 供给状态。
    pub status: OfferingStatus,
}

/// 保存供给商业条款修订请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReviseSupplierOfferingRequest {
    /// 期望当前修订号。
    #[validate(range(min = 1, message = "期望供给修订号必须大于 0"))]
    pub expected_revision_no: u32,
    /// 新商业条款。
    #[validate(nested)]
    pub terms: SupplierOfferingTermsWrite,
    /// 可选的新供给关系状态。
    pub status: Option<OfferingStatus>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 保存供给商业条款结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviseSupplierOfferingResult {
    /// 供给主键。
    pub offering_id: String,
    /// 新修订主键。
    pub revision_id: String,
    /// 新修订号。
    pub revision_no: u32,
    /// 供给关系状态。
    pub status: OfferingStatus,
    /// 供给乐观锁版本。
    pub version: u64,
    /// 同事务触发的安全暂停结果；没有当前在售影响或不构成安全原因时为空。
    pub safety_pause: Option<SystemSafetyPauseOperationView>,
}

/// 更新实时可供状态请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSupplierOfferingAvailabilityRequest {
    /// 可选期望投影版本；人工编辑时用于并发保护。
    pub expected_version: Option<u64>,
    /// 当前可供状态。
    pub availability_status: AvailabilityStatus,
    /// 当前可供数量。
    pub available_quantity: Option<String>,
    /// 来源更新时间；空表示服务端接收时间。
    pub source_updated_at: Option<i64>,
    /// 来源版本标识。
    pub source_revision_token: Option<String>,
    /// 变更原因。
    #[validate(custom(function = "non_blank", message = "变更原因不能为空"))]
    pub change_reason: String,
    /// 幂等键。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 更新实时可供状态结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateSupplierOfferingAvailabilityResult {
    /// 供给主键。
    pub offering_id: String,
    /// 当前可供状态。
    pub availability_status: AvailabilityStatus,
    /// 可供投影版本。
    pub availability_version: u64,
    /// 来源更新时间。
    pub source_updated_at: i64,
    /// 同事务触发的安全暂停结果；没有当前在售影响或不构成新安全原因时为空。
    pub safety_pause: Option<SystemSafetyPauseOperationView>,
}

/// 供应停止后续任务的固定决定类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierSupplyExceptionDecisionType {
    /// 确认停供来源与安全暂停影响已经核对，安全暂停继续生效。
    AcknowledgeSafetyPause,
}

/// 供应停止后续任务的强类型决定。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierSupplyExceptionDecision {
    /// 固定决定类型。
    #[serde(rename = "type")]
    pub decision_type: SupplierSupplyExceptionDecisionType,
    /// 任务绑定的供应商供给。
    #[validate(length(min = 1, max = 128, message = "供给ID不能为空或过长"))]
    pub offering_id: String,
    /// 已核对处置的外部或内部证据引用。
    #[validate(
        length(min = 1, max = 256, message = "证据引用不能为空或过长"),
        custom(function = "non_blank", message = "证据引用不能为空")
    )]
    pub evidence_reference: String,
    /// 核对结论；不得表达为恢复供给或恢复发布。
    #[validate(
        length(min = 1, max = 500, message = "核对结论不能为空或过长"),
        custom(function = "non_blank", message = "核对结论不能为空")
    )]
    pub comment: String,
}

/// 完成供应停止后续任务请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CompleteSupplierSupplyExceptionTaskRequest {
    /// 当前正式工作项。
    #[validate(length(min = 1, max = 128, message = "任务ID不能为空或过长"))]
    pub work_item_id: String,
    /// 当前工作项版本。
    #[validate(length(min = 1, max = 20, message = "任务版本格式非法"))]
    pub expected_task_version: String,
    /// 工作项冻结的安全暂停来源版本。
    #[validate(length(min = 1, max = 128, message = "来源版本不能为空或过长"))]
    pub expected_subject_version: String,
    /// 固定强类型决定。
    #[validate(nested)]
    pub decision: SupplierSupplyExceptionDecision,
    /// 客户端操作号。
    #[validate(
        length(min = 1, max = 128, message = "操作号不能为空或过长"),
        custom(function = "non_blank", message = "操作号不能为空")
    )]
    pub idempotency_key: String,
}

/// 完成供应停止后续任务结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompleteSupplierSupplyExceptionTaskResult {
    /// 已完成任务。
    pub work_item_id: String,
    /// 不可变安全暂停操作。
    pub safety_pause_operation_id: String,
    /// 本次核对证据引用。
    pub evidence_reference: String,
    /// 固定结果说明。
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, SortDir, SupplierOfferingListParams, SupplierOfferingView, OFFERING_SORT_FIELDS,
    };
    use entities::supplier_offering::{AvailabilityStatus, OfferingSourceType, OfferingStatus};

    #[test]
    fn sort_contract_rejects_unknown_fields() {
        assert_eq!(
            normalize_sort(
                &Some("status".to_string()),
                &Some("asc".to_string()),
                OFFERING_SORT_FIELDS
            )
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
        };
        view.redact_costs();
        assert!(view.dropship_supply_price_gross.is_none());
        assert_eq!(view.available_quantity.as_deref(), Some("5"));
        assert_eq!(view.supplier_sku_code, "S-1");
    }
}
