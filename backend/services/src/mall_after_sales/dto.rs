//! 域 D30 `mall_after_sales` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额一律十进制字符串。

use entities::ids::{
    MallAfterSalesRequestId, MallCardInstanceId, MallConsumptionEntryId, MallOrderId, MallOrderItemId,
    MallPaymentSourceId, MallRefundAllocationId,
};
use entities::mall_after_sales::{AfterSalesRequestStatus, AfterSalesRequestType};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 退款列表允许的排序字段白名单（api-contract §4：Service 层校验）。
pub(crate) const REFUND_SORT_FIELDS: &[&str] = &["refunded_at", "created_at"];
/// 余额恢复列表允许的排序字段白名单。
pub(crate) const RESTORATION_SORT_FIELDS: &[&str] = &["restored_at", "created_at"];
/// 售后请求列表允许的排序字段白名单。
pub(crate) const AFTER_SALES_REQUEST_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];

/// 排序方向。
pub use crate::query::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
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
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空。
use crate::query::non_blank;

/// 校验金额字符串为合法非负定点数值（小数位 ≤ 2）。
fn valid_amount(value: &str) -> std::result::Result<(), validator::ValidationError> {
    let amount = entities::money::Amount::from_str(value)
        .map_err(|_| validator::ValidationError::new("不是合法定点数值"))?;
    if amount.to_decimal().is_sign_negative() {
        return Err(validator::ValidationError::new("金额不能为负"));
    }
    Ok(())
}

/// 校验数量字符串为合法非负定点数值（小数位 ≤ 6）。
fn valid_quantity(value: &str) -> std::result::Result<(), validator::ValidationError> {
    let quantity = entities::money::Quantity::from_str(value)
        .map_err(|_| validator::ValidationError::new("不是合法数量"))?;
    if quantity.to_decimal().is_sign_negative() {
        return Err(validator::ValidationError::new("数量不能为负"));
    }
    Ok(())
}

/// 退款行载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RefundLineData {
    /// 稳定行号（从 1 起）。
    #[validate(range(min = 1, message = "退款行号必须从 1 开始"))]
    pub line_no: u32,
    /// 原商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 本商品实际退款数量（字符串，6 位小数）。
    #[validate(custom(function = "valid_quantity", message = "退款数量非法"))]
    pub refunded_quantity: String,
    /// 本商品实际退款金额（字符串）。
    #[validate(custom(function = "valid_amount", message = "退款金额非法"))]
    pub line_refund_amount: String,
}

/// 退款分配载荷（沿原支付来源拆分）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RefundAllocationData {
    /// 所属退款行号。
    #[validate(range(min = 1, message = "分配行号必须从 1 开始"))]
    pub line_no: u32,
    /// 稳定分配序号（从 1 起）。
    #[validate(range(min = 1, message = "分配序号必须从 1 开始"))]
    pub allocation_no: u32,
    /// 原商品 × 原支付来源消费事实。
    pub original_consumption_entry_id: MallConsumptionEntryId,
    /// 原卡券或微信来源。
    pub original_payment_source_id: MallPaymentSourceId,
    /// 实际冲减金额（字符串）。
    #[validate(custom(function = "valid_amount", message = "分配金额非法"))]
    pub allocated_refund_amount: String,
}

/// 商城退款成功事实接收请求（`REFUND_SUCCEEDED`，§6.18）。
///
/// `business_fact_key`/`inbox_message_id` 幂等；退款头、行、初始 `APPLY` 分配
/// 与消费冲减在同一事务写入（§8.4 第 3 条）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReceiveRefundFactRequest {
    /// 消息来源商城。
    #[validate(custom(function = "non_blank", message = "来源商城不能为空"))]
    pub mall_id: String,
    /// 来源事件 ID。
    #[validate(custom(function = "non_blank", message = "来源事件ID不能为空"))]
    pub source_event_id: String,
    /// 共同信封（唯一）。
    #[validate(custom(function = "non_blank", message = "信封ID不能为空"))]
    pub inbox_message_id: String,
    /// 跨实时和回填的稳定事实键（业务幂等键）。
    #[validate(custom(function = "non_blank", message = "业务事实键不能为空"))]
    pub business_fact_key: String,
    /// 商城订单号。
    #[validate(custom(function = "non_blank", message = "商城订单号不能为空"))]
    pub external_order_no: String,
    /// 对应结果版本。
    #[validate(custom(function = "non_blank", message = "结果版本不能为空"))]
    pub external_order_version: String,
    /// 同一售后案件的商城售后请求 ID（必填）。
    pub after_sales_request_id: MallAfterSalesRequestId,
    /// 原支付成功事实（必填）。
    pub original_payment_fact_id: entities::ids::MallOrderFactId,
    /// 事实发生时间（秒级时间戳）。
    #[validate(range(min = 1, message = "事实发生时间必须大于 0"))]
    pub occurred_at: u64,
    /// ERP 接收时间（秒级时间戳）。
    #[validate(range(min = 1, message = "接收时间必须大于 0"))]
    pub received_at: u64,
    /// 实时或历史回填。
    pub data_source: entities::mall_order::DataSource,
    /// 可选的加密原文引用。
    pub raw_payload_reference: Option<String>,
    /// 商城退款单号。
    #[validate(custom(function = "non_blank", message = "退款单号不能为空"))]
    pub external_refund_no: String,
    /// 商城退款版本。
    #[validate(custom(function = "non_blank", message = "退款版本不能为空"))]
    pub external_refund_version: String,
    /// 实际成功退款金额（字符串）。
    #[validate(custom(function = "valid_amount", message = "退款金额非法"))]
    pub refund_amount: String,
    /// 实际退款时间（秒级时间戳）。
    #[validate(range(min = 1, message = "退款时间必须大于 0"))]
    pub refunded_at: u64,
    /// 商品退款行。
    #[validate(length(min = 1, max = 500, message = "退款行数量必须在1-500之间"))]
    #[validate(nested)]
    pub lines: Vec<RefundLineData>,
    /// 沿原支付来源的退款分配。
    #[validate(length(min = 1, max = 1000, message = "退款分配数量必须在1-1000之间"))]
    #[validate(nested)]
    pub allocations: Vec<RefundAllocationData>,
}

/// 余额恢复分配载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RestorationAllocationData {
    /// 稳定分配序号（从 1 起）。
    #[validate(range(min = 1, message = "分配序号必须从 1 开始"))]
    pub allocation_no: u32,
    /// 原 CARD 退款资金分配。
    pub mall_refund_allocation_id: MallRefundAllocationId,
    /// 实际恢复到的原支付卡实例。
    pub mall_card_instance_id: MallCardInstanceId,
    /// 本卡恢复金额（字符串）。
    #[validate(custom(function = "valid_amount", message = "恢复金额非法"))]
    pub restored_amount: String,
}

/// 卡券余额恢复事实接收请求（`CARD_BALANCE_RESTORED`，§6.18）。
///
/// `business_fact_key`/`inbox_message_id` 幂等；恢复头与分配同事务写入
/// （§8.4 第 4 条），只增加余额变动，不再次冲减消费、成本或应付。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReceiveBalanceRestorationRequest {
    /// 消息来源商城。
    #[validate(custom(function = "non_blank", message = "来源商城不能为空"))]
    pub mall_id: String,
    /// 来源事件 ID。
    #[validate(custom(function = "non_blank", message = "来源事件ID不能为空"))]
    pub source_event_id: String,
    /// 共同信封（唯一）。
    #[validate(custom(function = "non_blank", message = "信封ID不能为空"))]
    pub inbox_message_id: String,
    /// 跨实时和回填的稳定事实键（业务幂等键）。
    #[validate(custom(function = "non_blank", message = "业务事实键不能为空"))]
    pub business_fact_key: String,
    /// 商城订单号。
    #[validate(custom(function = "non_blank", message = "商城订单号不能为空"))]
    pub external_order_no: String,
    /// 对应结果版本。
    #[validate(custom(function = "non_blank", message = "结果版本不能为空"))]
    pub external_order_version: String,
    /// 同一售后案件的商城售后请求 ID（必填）。
    pub after_sales_request_id: MallAfterSalesRequestId,
    /// 原支付成功事实（必填）。
    pub original_payment_fact_id: entities::ids::MallOrderFactId,
    /// 事实发生时间（秒级时间戳）。
    #[validate(range(min = 1, message = "事实发生时间必须大于 0"))]
    pub occurred_at: u64,
    /// ERP 接收时间（秒级时间戳）。
    #[validate(range(min = 1, message = "接收时间必须大于 0"))]
    pub received_at: u64,
    /// 实时或历史回填。
    pub data_source: entities::mall_order::DataSource,
    /// 可选的加密原文引用。
    pub raw_payload_reference: Option<String>,
    /// 恢复单号。
    #[validate(custom(function = "non_blank", message = "恢复单号不能为空"))]
    pub external_restoration_no: String,
    /// 恢复版本。
    #[validate(custom(function = "non_blank", message = "恢复版本不能为空"))]
    pub version: String,
    /// 实际恢复金额（字符串）。
    #[validate(custom(function = "valid_amount", message = "恢复金额非法"))]
    pub restored_amount: String,
    /// 实际恢复时间（秒级时间戳）。
    #[validate(range(min = 1, message = "恢复时间必须大于 0"))]
    pub restored_at: u64,
    /// 按原 CARD 退款资金分配的恢复分配。
    #[validate(length(min = 1, max = 1000, message = "恢复分配数量必须在1-1000之间"))]
    #[validate(nested)]
    pub allocations: Vec<RestorationAllocationData>,
}

/// 退款头视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallRefundView {
    /// 退款头 ID。
    pub id: String,
    /// `REFUND_SUCCEEDED` 事实。
    pub mall_order_fact_id: String,
    /// 同一售后案件。
    pub after_sales_request_id: String,
    /// 来源商城。
    pub mall_id: String,
    /// 商城退款单号。
    pub external_refund_no: String,
    /// 商城退款版本。
    pub external_refund_version: String,
    /// 原订单。
    pub mall_order_id: String,
    /// 实际成功退款金额（字符串）。
    pub refund_amount: String,
    /// 实际退款时间（秒级时间戳）。
    pub refunded_at: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 余额恢复头视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallBalanceRestorationView {
    /// 恢复头 ID。
    pub id: String,
    /// `CARD_BALANCE_RESTORED` 事实。
    pub mall_order_fact_id: String,
    /// 同一售后案件。
    pub after_sales_request_id: String,
    /// 关联退款。
    pub mall_refund_id: String,
    /// 来源商城。
    pub mall_id: String,
    /// 恢复单号。
    pub external_restoration_no: String,
    /// 恢复版本。
    pub version: String,
    /// 实际恢复金额（字符串）。
    pub restored_amount: String,
    /// 实际恢复时间（秒级时间戳）。
    pub restored_at: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 售后请求列表行视图（投影；请求头表实体存在 P1 缺陷，见契约变更）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AfterSalesRequestView {
    /// 请求 ID。
    pub id: String,
    /// 来源商城。
    pub mall_id: String,
    /// 商城售后请求稳定身份。
    pub external_request_id: String,
    /// 原商城订单。
    pub mall_order_id: String,
    /// 取消或退款。
    pub request_type: AfterSalesRequestType,
    /// 请求状态。
    pub status: AfterSalesRequestStatus,
    /// 员工售后原因。
    pub reason: String,
    /// 商城申请时间（秒级时间戳）。
    pub created_at: u64,
    /// 乐观锁版本。
    pub version: u64,
}

/// 退款列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallRefundListParams {
    /// 原订单筛选。
    pub mall_order_id: Option<MallOrderId>,
    /// 同一售后案件筛选。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`refunded_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的退款列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallRefundListQuery {
    /// 原订单筛选。
    pub mall_order_id: Option<MallOrderId>,
    /// 同一售后案件筛选。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

/// Service 已判定的退款查询业务作用域（INT-E06）。
///
/// 作用域优先级属于纯查询范围规则，由查询 DTO 独占；
/// Repository 只接收已决定的单一领域 ID，不拥有优先级规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MallRefundScope {
    /// 按原商城订单读取退款事实。
    Order(MallOrderId),
    /// 按售后案件读取退款事实。
    AfterSalesRequest(MallAfterSalesRequestId),
}

impl MallRefundListQuery {
    /// 选择退款列表唯一业务作用域（INT-E06）。
    ///
    /// # 参数
    /// * `self` - 已完成参数校验和排序白名单归一化的退款查询
    ///
    /// # 返回
    /// 有原订单或售后案件时返回单一类型化作用域；均未提供时返回 `None`，
    /// 调用方不得以此退化为全表读取。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 原订单始终优先于售后案件；本方法是作用域优先级的唯一来源。
    pub(crate) fn scope(&self) -> Option<MallRefundScope> {
        if let Some(mall_order_id) = &self.mall_order_id {
            return Some(MallRefundScope::Order(mall_order_id.clone()));
        }
        self.after_sales_request_id
            .as_ref()
            .cloned()
            .map(MallRefundScope::AfterSalesRequest)
    }
}

impl MallRefundListParams {
    /// 归一化退款列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallRefundListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, REFUND_SORT_FIELDS)?;
        Ok(MallRefundListQuery {
            mall_order_id: self.mall_order_id.clone(),
            after_sales_request_id: self.after_sales_request_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 余额恢复列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallBalanceRestorationListParams {
    /// 同一售后案件筛选。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`restored_at`/`created_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的余额恢复列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallBalanceRestorationListQuery {
    /// 同一售后案件筛选。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MallBalanceRestorationListParams {
    /// 归一化余额恢复列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallBalanceRestorationListQuery> {
        let (sort_by, sort_dir) = normalize_sort(&self.sort_by, &self.sort_dir, RESTORATION_SORT_FIELDS)?;
        Ok(MallBalanceRestorationListQuery {
            after_sales_request_id: self.after_sales_request_id.clone(),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 售后请求列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AfterSalesRequestListParams {
    /// 来源商城模糊筛选。
    pub mall_id: Option<String>,
    /// 原订单筛选。
    pub mall_order_id: Option<MallOrderId>,
    /// 请求类型筛选。
    pub request_type: Option<AfterSalesRequestType>,
    /// 请求状态筛选。
    pub status: Option<AfterSalesRequestStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的售后请求列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AfterSalesRequestListQuery {
    /// 来源商城模糊筛选。
    pub mall_id: Option<String>,
    /// 原订单筛选。
    pub mall_order_id: Option<MallOrderId>,
    /// 请求类型筛选。
    pub request_type: Option<AfterSalesRequestType>,
    /// 请求状态筛选。
    pub status: Option<AfterSalesRequestStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl AfterSalesRequestListParams {
    /// 归一化售后请求列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<AfterSalesRequestListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, AFTER_SALES_REQUEST_SORT_FIELDS)?;
        Ok(AfterSalesRequestListQuery {
            mall_id: normalized_text(self.mall_id.as_deref()),
            mall_order_id: self.mall_order_id.clone(),
            request_type: self.request_type,
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

/// 事实接收结果视图（D30 写入口共用）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReceivedFactView {
    /// 事实 ID。
    pub fact_id: String,
    /// 事实类型。
    pub fact_type: entities::mall_order::FactType,
    /// 处理状态。
    pub processing_status: entities::mall_order::ProcessingStatus,
    /// 是否幂等命中既有事实。
    pub idempotent_hit: bool,
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_sort, MallRefundListQuery, MallRefundScope, PageParams, ReceiveBalanceRestorationRequest,
        ReceiveRefundFactRequest, RefundAllocationData, RefundLineData, RestorationAllocationData,
    };
    use crate::mall_after_sales::dto::{AfterSalesRequestListParams, MallRefundListParams, SortDir};
    use entities::ids::{
        MallAfterSalesRequestId, MallConsumptionEntryId, MallOrderFactId, MallOrderId, MallOrderItemId,
        MallPaymentSourceId,
    };
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["refunded_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["refunded_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" refunded_at ".to_string()),
            &Some(" desc ".to_string()),
            &["refunded_at", "created_at"],
        )
        .unwrap();
        assert_eq!(field, "refunded_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn list_params_normalize_paging_and_sort_defaults() {
        let params = MallRefundListParams {
            mall_order_id: None,
            after_sales_request_id: None,
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("refunded_at".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "refunded_at");

        let params = AfterSalesRequestListParams {
            mall_id: Some(" mall-a ".to_string()),
            mall_order_id: None,
            request_type: None,
            status: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.mall_id.as_deref(), Some("mall-a"));
        assert_eq!(query.paging.page, 1);
    }

    /// 构造退款查询作用域测试使用的已归一化查询。
    ///
    /// 参数分别提供可选订单和售后案件，返回固定第一页、默认排序的查询；
    /// 不执行参数解析或任何外部 I/O。
    fn refund_query(order_id: Option<&str>, request_id: Option<&str>) -> MallRefundListQuery {
        MallRefundListQuery {
            mall_order_id: order_id.map(MallOrderId::new),
            after_sales_request_id: request_id.map(MallAfterSalesRequestId::new),
            paging: PageParams {
                page: 1,
                page_size: 20,
                sort_by: "created_at",
                sort_dir: SortDir::Desc,
            },
        }
    }

    /// 验证同时提供两个作用域时查询只选择原订单（INT-E06）。
    ///
    /// 测试直接调用查询自带的作用域选择器，不访问 Repository；优先级漂移时失败。
    #[test]
    fn refund_query_scope_prefers_order() {
        assert_eq!(
            refund_query(Some("order-1"), Some("request-1")).scope(),
            Some(MallRefundScope::Order(MallOrderId::new("order-1")))
        );
    }

    /// 验证售后案件可独立选择，缺少两个作用域时不产生仓储查询条件（INT-E06）。
    ///
    /// 测试覆盖案件命中与无作用域边界，确保空查询不会退化为全表读取。
    #[test]
    fn refund_query_scope_handles_request_and_missing_scope() {
        assert_eq!(
            refund_query(None, Some("request-1")).scope(),
            Some(MallRefundScope::AfterSalesRequest(MallAfterSalesRequestId::new(
                "request-1"
            )))
        );
        assert_eq!(refund_query(None, None).scope(), None);
    }

    /// 验证仅提供原订单时查询选择原订单作用域（INT-E06）。
    ///
    /// 补齐四格矩阵中缺失的一格：单作用域分支不得退化为空或误选案件。
    #[test]
    fn refund_query_scope_handles_order_only() {
        assert_eq!(
            refund_query(Some("order-1"), None).scope(),
            Some(MallRefundScope::Order(MallOrderId::new("order-1")))
        );
    }

    /// 构造嵌套校验测试使用的合法退款接收请求。
    ///
    /// 返回最小合法载荷；调用方可按需篡改单个内部字段以触发嵌套拒绝；
    /// 不访问数据库或外部 I/O。
    fn valid_refund_request() -> ReceiveRefundFactRequest {
        ReceiveRefundFactRequest {
            mall_id: "mall-a".to_string(),
            source_event_id: "evt-1".to_string(),
            inbox_message_id: "inbox-1".to_string(),
            business_fact_key: "key-1".to_string(),
            external_order_no: "SO-1".to_string(),
            external_order_version: "v1".to_string(),
            after_sales_request_id: MallAfterSalesRequestId::new("req-1"),
            original_payment_fact_id: MallOrderFactId::new("fact-pay-1"),
            occurred_at: 1,
            received_at: 1,
            data_source: entities::mall_order::DataSource::Realtime,
            raw_payload_reference: None,
            external_refund_no: "R-1".to_string(),
            external_refund_version: "v1".to_string(),
            refund_amount: "10.00".to_string(),
            refunded_at: 1,
            lines: vec![RefundLineData {
                line_no: 1,
                mall_order_item_id: MallOrderItemId::new("item-1"),
                refunded_quantity: "1.000000".to_string(),
                line_refund_amount: "10.00".to_string(),
            }],
            allocations: vec![RefundAllocationData {
                line_no: 1,
                allocation_no: 1,
                original_consumption_entry_id: MallConsumptionEntryId::new("ce-1"),
                original_payment_source_id: MallPaymentSourceId::new("ps-1"),
                allocated_refund_amount: "10.00".to_string(),
            }],
        }
    }

    /// 验证合法退款载荷通过嵌套校验（INT-E07）。
    ///
    /// 测试覆盖空集合拒绝前的合法基线，确保白名单内的金额、数量与编号可通过。
    #[test]
    fn refund_request_accepts_valid_nested_payload() {
        assert!(valid_refund_request().validate().is_ok());
    }

    /// 验证内部非法金额、数量与编号被嵌套校验拒绝（INT-E07）。
    ///
    /// 测试覆盖行金额、分配金额、退款数量、行号与分配序号的非法形态；
    /// 任一内部失败必须使整体请求校验失败。
    #[test]
    fn refund_request_rejects_invalid_nested_amounts_and_numbers() {
        let mut req = valid_refund_request();
        req.lines[0].line_refund_amount = "-1.00".to_string();
        assert!(req.validate().is_err());

        let mut req = valid_refund_request();
        req.allocations[0].allocated_refund_amount = "not-a-number".to_string();
        assert!(req.validate().is_err());

        let mut req = valid_refund_request();
        req.lines[0].refunded_quantity = "-1.000000".to_string();
        assert!(req.validate().is_err());

        let mut req = valid_refund_request();
        req.lines[0].line_no = 0;
        assert!(req.validate().is_err());

        let mut req = valid_refund_request();
        req.allocations[0].allocation_no = 0;
        assert!(req.validate().is_err());
    }

    /// 验证空集合被长度校验拒绝（INT-E07）。
    ///
    /// 测试覆盖行与分配分别为空的边界，确保空载荷不会进入额度校验。
    #[test]
    fn refund_request_rejects_empty_collections() {
        let mut req = valid_refund_request();
        req.lines.clear();
        assert!(req.validate().is_err());

        let mut req = valid_refund_request();
        req.allocations.clear();
        assert!(req.validate().is_err());
    }

    /// 验证恢复请求内部非法金额与编号被嵌套校验拒绝（INT-E07）。
    ///
    /// 测试覆盖恢复金额非法与分配序号非法的内层形态，确保恢复载荷与退款载荷
    /// 共用相同的嵌套拒绝时点。
    #[test]
    fn restoration_request_rejects_invalid_nested_payload() {
        use entities::ids::{MallCardInstanceId, MallRefundAllocationId};
        let mut req = ReceiveBalanceRestorationRequest {
            mall_id: "mall-a".to_string(),
            source_event_id: "evt-1".to_string(),
            inbox_message_id: "inbox-1".to_string(),
            business_fact_key: "key-1".to_string(),
            external_order_no: "SO-1".to_string(),
            external_order_version: "v1".to_string(),
            after_sales_request_id: MallAfterSalesRequestId::new("req-1"),
            original_payment_fact_id: MallOrderFactId::new("fact-pay-1"),
            occurred_at: 1,
            received_at: 1,
            data_source: entities::mall_order::DataSource::Realtime,
            raw_payload_reference: None,
            external_restoration_no: "RS-1".to_string(),
            version: "v1".to_string(),
            restored_amount: "5.00".to_string(),
            restored_at: 1,
            allocations: vec![RestorationAllocationData {
                allocation_no: 1,
                mall_refund_allocation_id: MallRefundAllocationId::new("ra-1"),
                mall_card_instance_id: MallCardInstanceId::new("card-1"),
                restored_amount: "5.00".to_string(),
            }],
        };
        assert!(req.validate().is_ok());
        req.allocations[0].restored_amount = "-5.00".to_string();
        assert!(req.validate().is_err());
        req.allocations[0].restored_amount = "5.00".to_string();
        req.allocations[0].allocation_no = 0;
        assert!(req.validate().is_err());
    }

    /// 验证超长集合被长度上限拒绝，边界数量仍通过（INT-E07）。
    ///
    /// 上限与同步快照 500 条、对账差异 1000 条 precedent 同量级；
    /// 测试覆盖行 500/501 与分配 1000/1001 边界，确保超长载荷在查库前失败关闭。
    #[test]
    fn refund_request_rejects_overlong_collections() {
        let line = || RefundLineData {
            line_no: 1,
            mall_order_item_id: MallOrderItemId::new("item-1"),
            refunded_quantity: "1.000000".to_string(),
            line_refund_amount: "10.00".to_string(),
        };
        let allocation = || RefundAllocationData {
            line_no: 1,
            allocation_no: 1,
            original_consumption_entry_id: MallConsumptionEntryId::new("ce-1"),
            original_payment_source_id: MallPaymentSourceId::new("ps-1"),
            allocated_refund_amount: "10.00".to_string(),
        };
        let mut req = valid_refund_request();
        req.lines = (0..500).map(|_| line()).collect();
        req.allocations = (0..1000).map(|_| allocation()).collect();
        assert!(req.validate().is_ok());
        req.lines.push(line());
        assert!(req.validate().is_err());
        req.lines.pop();
        req.allocations.push(allocation());
        assert!(req.validate().is_err());
    }

    /// 验证恢复超长分配被长度上限拒绝（INT-E07）。
    ///
    /// 测试覆盖恢复分配 1000/1001 边界，与退款载荷共用相同的查库前拒绝时点。
    #[test]
    fn restoration_request_rejects_overlong_allocations() {
        use entities::ids::{MallCardInstanceId, MallRefundAllocationId};
        let allocation = || RestorationAllocationData {
            allocation_no: 1,
            mall_refund_allocation_id: MallRefundAllocationId::new("ra-1"),
            mall_card_instance_id: MallCardInstanceId::new("card-1"),
            restored_amount: "5.00".to_string(),
        };
        let mut restore = ReceiveBalanceRestorationRequest {
            mall_id: "mall-a".to_string(),
            source_event_id: "evt-1".to_string(),
            inbox_message_id: "inbox-1".to_string(),
            business_fact_key: "key-1".to_string(),
            external_order_no: "SO-1".to_string(),
            external_order_version: "v1".to_string(),
            after_sales_request_id: MallAfterSalesRequestId::new("req-1"),
            original_payment_fact_id: MallOrderFactId::new("fact-pay-1"),
            occurred_at: 1,
            received_at: 1,
            data_source: entities::mall_order::DataSource::Realtime,
            raw_payload_reference: None,
            external_restoration_no: "RS-1".to_string(),
            version: "v1".to_string(),
            restored_amount: "5.00".to_string(),
            restored_at: 1,
            allocations: (0..1000).map(|_| allocation()).collect(),
        };
        assert!(restore.validate().is_ok());
        restore.allocations.push(allocation());
        assert!(restore.validate().is_err());
    }
}
