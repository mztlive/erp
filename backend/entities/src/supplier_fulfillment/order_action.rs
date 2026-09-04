//! `supplier_order_action` 与 `supplier_order_action_line`（数据模型 §6.19 供应商动作与动作行）。
//!
//! 动作类型与履约状态按字典建模（下单/查询/取消/退款）；动作到履约状态的推进是 P3
//! 编排，实体只固化动作枚举与动作行恒等。动作行冻结一次取消或退款实际提交给供应商的
//! 范围，创建后不可修改。

use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    SupplierFulfillmentItemId, SupplierFulfillmentOrderId, SupplierOrderActionId, SupplierOrderActionLineId,
};
use crate::money::{Amount, Quantity};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 幂等键最大长度。
const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;
/// 供应商请求号最大长度。
const EXTERNAL_REQUEST_ID_MAX_LEN: usize = 64;
/// 脱敏请求/响应摘要最大长度。
const SUMMARY_MAX_LEN: usize = 2048;

/// 供应商动作类型（数据模型 §6.19：下单、查询、取消、退款）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderActionType {
    /// 下单。
    Place,
    /// 查询。
    Query,
    /// 取消。
    Cancel,
    /// 退款。
    Refund,
}

impl SupplierOrderActionType {
    /// 返回动作类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Place => "下单",
            Self::Query => "查询",
            Self::Cancel => "取消",
            Self::Refund => "退款",
        }
    }

    /// 返回动作类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Place => "PLACE",
            Self::Query => "QUERY",
            Self::Cancel => "CANCEL",
            Self::Refund => "REFUND",
        }
    }
}

/// 供应商动作状态（数据模型 §6.19：待发送、发送中、结果未知、成功、明确失败、待人工）。
///
/// 固定枚举（§4.6），不属于数据模型第 7 章的固定状态机；投递重试编排（§7.7）由 P3 承担。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierOrderActionStatus {
    /// 待发送。
    Pending,
    /// 发送中。
    Sending,
    /// 结果未知：网络超时先查询原请求，不直接重复创建。
    ResultUnknown,
    /// 成功。
    Succeeded,
    /// 明确失败：业务明确拒绝不自动重试。
    Failed,
    /// 待人工。
    Manual,
}

impl SupplierOrderActionStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待发送",
            Self::Sending => "发送中",
            Self::ResultUnknown => "结果未知",
            Self::Succeeded => "成功",
            Self::Failed => "明确失败",
            Self::Manual => "待人工",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Sending => "SENDING",
            Self::ResultUnknown => "RESULT_UNKNOWN",
            Self::Succeeded => "SUCCEEDED",
            Self::Failed => "FAILED",
            Self::Manual => "MANUAL",
        }
    }
}

/// 供应商动作创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOrderActionData {
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 动作类型。
    pub action_type: SupplierOrderActionType,
    /// 对供应商动作幂等键（唯一；下单键为 ERP 供应商订单号，
    /// 取消/退款键为"订单号 + 动作类型"，由 P3 拼装）。
    pub idempotency_key: String,
    /// 动作状态。
    pub status: SupplierOrderActionStatus,
    /// 供应商请求号。
    pub external_request_id: Option<String>,
    /// 脱敏请求摘要。
    pub request_summary: Option<String>,
    /// 脱敏响应摘要。
    pub response_summary: Option<String>,
    /// 重试次数。
    pub attempt_count: u32,
    /// 下次重试时间。
    pub next_attempt_at: Option<Instant>,
}

impl SupplierOrderActionData {
    /// 构造首个供应商下单动作数据。
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单
    /// * `idempotency_key` - 与 ERP 供应商子订单号一致的幂等键
    /// * `line_count` - 本次下单明细数
    ///
    /// # 返回
    /// 返回待发送的 `PLACE` 动作数据。
    pub fn place(
        order_id: SupplierFulfillmentOrderId,
        idempotency_key: impl Into<String>,
        line_count: usize,
    ) -> Self {
        let idempotency_key = idempotency_key.into();
        Self {
            supplier_fulfillment_order_id: order_id,
            action_type: SupplierOrderActionType::Place,
            request_summary: Some(format!("下单 {idempotency_key} 明细 {line_count} 行")),
            idempotency_key,
            status: SupplierOrderActionStatus::Pending,
            external_request_id: None,
            response_summary: None,
            attempt_count: 0,
            next_attempt_at: None,
        }
    }

    /// 构造取消或退款手工动作数据.
    ///
    /// # 参数
    /// * `order_id` - 供应商履约订单
    /// * `action_type` - 取消或退款
    /// * `idempotency_key` - 供应商动作幂等键
    /// * `reason_code` - 可选原因代码
    ///
    /// # 返回
    /// 返回待发送的手工调整动作数据。
    pub fn manual_adjustment(
        order_id: SupplierFulfillmentOrderId,
        action_type: SupplierOrderActionType,
        idempotency_key: impl Into<String>,
        reason_code: Option<&str>,
    ) -> Self {
        let idempotency_key = idempotency_key.into();
        let request_summary = reason_code.map_or_else(
            || format!("{} 手工调整", action_type.label()),
            |code| format!("{} 手工调整 原因 {code}", action_type.label()),
        );
        Self {
            supplier_fulfillment_order_id: order_id,
            action_type,
            idempotency_key,
            status: SupplierOrderActionStatus::Pending,
            external_request_id: None,
            request_summary: Some(request_summary),
            response_summary: None,
            attempt_count: 0,
            next_attempt_at: None,
        }
    }

    /// 构造已进入发送中的供应商结果查询意图。
    ///
    /// # 参数
    /// * `order_id` - 待调查供应商履约订单
    /// * `idempotency_key` - 查询意图的稳定幂等键
    /// * `request_summary` - 已脱敏调查意图摘要
    ///
    /// # 返回
    /// 返回 `QUERY/SENDING` 且首次尝试计数为一的动作数据。
    pub fn query_intent(
        order_id: SupplierFulfillmentOrderId,
        idempotency_key: impl Into<String>,
        request_summary: impl Into<String>,
    ) -> Self {
        Self {
            supplier_fulfillment_order_id: order_id,
            action_type: SupplierOrderActionType::Query,
            idempotency_key: idempotency_key.into(),
            status: SupplierOrderActionStatus::Sending,
            external_request_id: None,
            request_summary: Some(request_summary.into()),
            response_summary: None,
            attempt_count: 1,
            next_attempt_at: None,
        }
    }

    /// 构造已验证并持久化结果的供应商查询证据。
    ///
    /// # 参数
    /// * `order_id` - 已验证供应商履约订单
    /// * `idempotency_key` - 完成证据的稳定幂等键
    /// * `request_summary` - 已脱敏证据来源摘要
    /// * `response_summary` - 已序列化的正式验证结果
    ///
    /// # 返回
    /// 返回 `QUERY/SUCCEEDED` 且首次尝试计数为一的动作数据。
    pub fn query_result(
        order_id: SupplierFulfillmentOrderId,
        idempotency_key: impl Into<String>,
        request_summary: impl Into<String>,
        response_summary: impl Into<String>,
    ) -> Self {
        Self {
            supplier_fulfillment_order_id: order_id,
            action_type: SupplierOrderActionType::Query,
            idempotency_key: idempotency_key.into(),
            status: SupplierOrderActionStatus::Succeeded,
            external_request_id: None,
            request_summary: Some(request_summary.into()),
            response_summary: Some(response_summary.into()),
            attempt_count: 1,
            next_attempt_at: None,
        }
    }
}

/// 供应商动作更新数据（不含系统字段与关键字段）。
///
/// 子订单、动作类型、手工调整引用与幂等键创建后不可修改；重试计数走 [`SupplierOrderAction::record_attempt`]。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupplierOrderActionUpdate {
    /// 动作状态；`None` 表示不修改。
    pub status: Option<SupplierOrderActionStatus>,
    /// 供应商请求号；`None` 表示不修改。
    pub external_request_id: Option<String>,
    /// 请求摘要；`None` 表示不修改。
    pub request_summary: Option<String>,
    /// 响应摘要；`None` 表示不修改。
    pub response_summary: Option<String>,
    /// 重试次数；`None` 表示不修改。
    pub attempt_count: Option<u32>,
    /// 下次重试时间；`None` 表示不修改。
    pub next_attempt_at: Option<Instant>,
}

/// 供应商动作实体（数据模型 §6.19，正式单据）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierOrderAction {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 动作类型。
    pub action_type: SupplierOrderActionType,
    /// 对供应商动作幂等键。
    pub idempotency_key: String,
    /// 动作状态。
    pub status: SupplierOrderActionStatus,
    /// 供应商请求号。
    pub external_request_id: Option<String>,
    /// 脱敏请求摘要。
    pub request_summary: Option<String>,
    /// 脱敏响应摘要。
    pub response_summary: Option<String>,
    /// 重试次数。
    pub attempt_count: u32,
    /// 下次重试时间。
    pub next_attempt_at: Option<Instant>,
}

impl SupplierOrderAction {
    /// 创建供应商动作。
    ///
    /// 完成幂等键与摘要的校验和规范化。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierOrderActionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的动作实体。
    ///
    /// # 错误
    /// 幂等键为空/超长或摘要超长时返回错误。
    pub fn new(id: SupplierOrderActionId, data: SupplierOrderActionData) -> Result<Self> {
        let idempotency_key = normalize_required_text(
            data.idempotency_key,
            "幂等键不能为空",
            IDEMPOTENCY_KEY_MAX_LEN,
            "幂等键过长",
        )?;
        let external_request_id = normalize_optional_text(
            data.external_request_id,
            "供应商请求号",
            EXTERNAL_REQUEST_ID_MAX_LEN,
        )?;
        let request_summary = normalize_optional_text(data.request_summary, "请求摘要", SUMMARY_MAX_LEN)?;
        let response_summary = normalize_optional_text(data.response_summary, "响应摘要", SUMMARY_MAX_LEN)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_fulfillment_order_id: data.supplier_fulfillment_order_id,
            action_type: data.action_type,
            idempotency_key,
            status: data.status,
            external_request_id,
            request_summary,
            response_summary,
            attempt_count: data.attempt_count,
            next_attempt_at: data.next_attempt_at,
        })
    }

    /// 更新供应商动作。
    ///
    /// 复用 `new` 的校验规则；子订单、动作类型、手工调整引用与幂等键不可修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 请求号或摘要为空/超长时返回错误。
    pub fn update(&mut self, update: SupplierOrderActionUpdate) -> Result<()> {
        if let Some(status) = update.status {
            self.status = status;
        }
        if let Some(external_request_id) = update.external_request_id {
            self.apply_external_request_id(external_request_id)?;
        }
        apply_summary(&mut self.request_summary, update.request_summary, "请求摘要")?;
        apply_summary(&mut self.response_summary, update.response_summary, "响应摘要")?;
        if let Some(attempt_count) = update.attempt_count {
            self.attempt_count = attempt_count;
        }
        if update.next_attempt_at.is_some() {
            self.next_attempt_at = update.next_attempt_at;
        }
        Ok(())
    }

    /// 记录一次发送重试。
    ///
    /// 重试次数加一（饱和），并设置下次重试时间；自动和人工重试继续使用原幂等键（§6.19）。
    ///
    /// # 参数
    /// * `next_attempt_at` - 下次重试时间；`None` 表示不再自动重试
    pub fn record_attempt(&mut self, next_attempt_at: Option<Instant>) {
        self.attempt_count = self.attempt_count.saturating_add(1);
        self.next_attempt_at = next_attempt_at;
    }

    /// 校验当前动作是指定订单的原业务动作。
    ///
    /// 调查目标只允许原下单、取消或退款动作；查询证据不能再次作为调查目标。
    ///
    /// # 参数
    /// * `order_id` - 当前供应商履约订单主键
    ///
    /// # 返回
    /// 归属一致且不是查询动作时返回 `Ok(())`。
    ///
    /// # 错误
    /// 动作属于其他订单或动作类型为查询时返回领域错误。
    pub fn ensure_original_for_order(&self, order_id: &str) -> Result<()> {
        if self.supplier_fulfillment_order_id.as_ref() != order_id {
            return Err(Error::from("供应商原动作不属于当前履约订单"));
        }
        if self.action_type == SupplierOrderActionType::Query {
            return Err(Error::from("调查目标必须是原下单、取消或退款动作"));
        }
        Ok(())
    }

    /// 应用供应商请求号更新。
    ///
    /// # 参数
    /// * `value` - 新的请求号
    ///
    /// # 错误
    /// 请求号为空或超长时返回错误。
    fn apply_external_request_id(&mut self, value: String) -> Result<()> {
        self.external_request_id = Some(normalize_required_text(
            value,
            "供应商请求号不能为空",
            EXTERNAL_REQUEST_ID_MAX_LEN,
            "供应商请求号过长",
        )?);
        Ok(())
    }
}

/// 应用摘要更新。
///
/// # 参数
/// * `target` - 目标摘要字段
/// * `value` - 新的摘要
/// * `label` - 字段中文名（用于错误信息）
///
/// # 错误
/// 摘要为空或超长时返回错误。
fn apply_summary(target: &mut Option<String>, value: Option<String>, label: &str) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    *target = Some(normalize_required_text(
        value,
        &format!("{label}不能为空"),
        SUMMARY_MAX_LEN,
        &format!("{label}过长"),
    )?);
    Ok(())
}

/// 供应商动作行创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOrderActionLineData {
    /// 所属动作头。
    pub supplier_order_action_id: SupplierOrderActionId,
    /// 动作内行号。
    pub line_no: u32,
    /// 本供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 本动作提交数量。
    pub quantity: Quantity,
    /// 本动作提交金额。
    pub amount: Amount,
}

impl SupplierOrderActionLineData {
    /// 构造按请求顺序编号的供应商动作行数据。
    ///
    /// # 参数
    /// * `action_id` - 所属供应商动作
    /// * `index` - 请求中的零基序号
    /// * `supplier_fulfillment_item_id` - 供应商履约明细
    /// * `quantity` - 提交数量
    /// * `amount` - 提交金额
    ///
    /// # 返回
    /// 返回行号从 1 起的动作行数据。
    pub fn from_request_index(
        action_id: SupplierOrderActionId,
        index: usize,
        supplier_fulfillment_item_id: SupplierFulfillmentItemId,
        quantity: Quantity,
        amount: Amount,
    ) -> Self {
        Self {
            supplier_order_action_id: action_id,
            line_no: u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1),
            supplier_fulfillment_item_id,
            quantity,
            amount,
        }
    }
}

/// 供应商动作行实体（数据模型 §6.19，冻结一次取消或退款实际提交给该供应商的范围）。
///
/// 随动作头同事务创建，创建后不可修改；"不得超过对应申请行尚未提交的净余额"是跨记录
/// 约束，由 P3 校验（§6.19）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierOrderActionLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属动作头。
    pub supplier_order_action_id: SupplierOrderActionId,
    /// 动作内行号。
    pub line_no: u32,
    /// 本供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 本动作提交数量。
    pub quantity: Quantity,
    /// 本动作提交金额。
    pub amount: Amount,
}

impl SupplierOrderActionLine {
    /// 创建供应商动作行。
    ///
    /// 校验数量与金额必须大于零（取消/退款实际提交范围必须是正量）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierOrderActionLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的动作行实体。
    ///
    /// # 错误
    /// 数量或金额小于等于零时返回错误。
    pub fn new(id: SupplierOrderActionLineId, data: SupplierOrderActionLineData) -> Result<Self> {
        if data.quantity.to_decimal() <= Decimal::ZERO {
            return Err(Error::from("动作行数量必须大于零"));
        }
        if data.amount.to_decimal() <= Decimal::ZERO {
            return Err(Error::from("动作行金额必须大于零"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_order_action_id: data.supplier_order_action_id,
            line_no: data.line_no,
            supplier_fulfillment_item_id: data.supplier_fulfillment_item_id,
            quantity: data.quantity,
            amount: data.amount,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{
        SupplierFulfillmentItemId, SupplierOrderActionId, SupplierOrderActionLineId,
    };
    use std::str::FromStr;

    fn sample_data() -> SupplierOrderActionData {
        SupplierOrderActionData {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            action_type: SupplierOrderActionType::Place,
            idempotency_key: " FO-2026-001 ".to_string(),
            status: SupplierOrderActionStatus::Pending,
            external_request_id: None,
            request_summary: None,
            response_summary: None,
            attempt_count: 0,
            next_attempt_at: None,
        }
    }

    fn sample_line_data() -> SupplierOrderActionLineData {
        SupplierOrderActionLineData {
            supplier_order_action_id: SupplierOrderActionId::new("action-1"),
            line_no: 1,
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
            quantity: Quantity::from_str("2.000000").unwrap(),
            amount: Amount::from_str("19.98").unwrap(),
        }
    }

    #[test]
    fn new_accepts_place_and_query_without_manual_adjustment() {
        let action = SupplierOrderAction::new(SupplierOrderActionId::new("action-1"), sample_data()).unwrap();
        assert_eq!(action.idempotency_key, "FO-2026-001");
        assert_eq!(action.status, SupplierOrderActionStatus::Pending);
        assert_eq!(action.attempt_count, 0);
        assert!(action.ensure_original_for_order("order-1").is_ok());
        assert!(action.ensure_original_for_order("order-2").is_err());

        let query = SupplierOrderActionData {
            action_type: SupplierOrderActionType::Query,
            idempotency_key: "query-key-1".to_string(),
            ..sample_data()
        };
        assert!(SupplierOrderAction::new(SupplierOrderActionId::new("action-2"), query).is_ok());
    }

    #[test]
    fn data_factories_freeze_action_types_and_line_sequence() {
        let place = SupplierOrderActionData::place(SupplierFulfillmentOrderId::new("order-1"), "FO-1", 2);
        assert_eq!(place.action_type, SupplierOrderActionType::Place);
        assert_eq!(place.status, SupplierOrderActionStatus::Pending);
        assert!(place.request_summary.unwrap().contains("2 行"));

        let manual_adjustment = SupplierOrderActionData::manual_adjustment(
            SupplierFulfillmentOrderId::new("order-1"),
            SupplierOrderActionType::Cancel,
            "cancel-key",
            Some("CUSTOMER_REQUEST"),
        );
        assert_eq!(manual_adjustment.action_type, SupplierOrderActionType::Cancel);

        let query_intent = SupplierOrderActionData::query_intent(
            SupplierFulfillmentOrderId::new("order-1"),
            "query-key",
            "query intent",
        );
        assert_eq!(query_intent.action_type, SupplierOrderActionType::Query);
        assert_eq!(query_intent.status, SupplierOrderActionStatus::Sending);
        assert_eq!(query_intent.attempt_count, 1);

        let query_result = SupplierOrderActionData::query_result(
            SupplierFulfillmentOrderId::new("order-1"),
            "result-key",
            "result evidence",
            "{\"result\":\"accepted\"}",
        );
        assert_eq!(query_result.status, SupplierOrderActionStatus::Succeeded);
        assert_eq!(
            query_result.response_summary.as_deref(),
            Some("{\"result\":\"accepted\"}")
        );

        let line = SupplierOrderActionLineData::from_request_index(
            SupplierOrderActionId::new("action-1"),
            0,
            SupplierFulfillmentItemId::new("item-1"),
            Quantity::from_str("1").unwrap(),
            Amount::from_str("1").unwrap(),
        );
        assert_eq!(line.line_no, 1);
    }

    #[test]
    fn new_accepts_cancel_and_refund_with_manual_adjustment() {
        for action_type in [SupplierOrderActionType::Cancel, SupplierOrderActionType::Refund] {
            let data = SupplierOrderActionData {
                action_type,
                idempotency_key: format!("order-1-{}", action_type.as_str()),
                ..sample_data()
            };
            let action = SupplierOrderAction::new(SupplierOrderActionId::new("action-3"), data).unwrap();
            assert_eq!(action.action_type, action_type);
        }
    }

    #[test]
    fn new_accepts_actions_without_manual_adjustment_reference() {
        let cancel_without_request = SupplierOrderActionData {
            action_type: SupplierOrderActionType::Cancel,
            ..sample_data()
        };
        assert!(
            SupplierOrderAction::new(SupplierOrderActionId::new("action-4"), cancel_without_request).is_ok()
        );

        let place_with_request = SupplierOrderActionData {
            action_type: SupplierOrderActionType::Place,
            ..sample_data()
        };
        assert!(
            SupplierOrderAction::new(SupplierOrderActionId::new("action-5"), place_with_request).is_ok()
        );
    }

    #[test]
    fn new_rejects_empty_or_overlong_idempotency_key_and_summaries() {
        let empty_key = SupplierOrderActionData {
            idempotency_key: "   ".to_string(),
            ..sample_data()
        };
        assert!(SupplierOrderAction::new(SupplierOrderActionId::new("action-6"), empty_key).is_err());

        let overlong_key = SupplierOrderActionData {
            idempotency_key: "k".repeat(129),
            ..sample_data()
        };
        assert!(SupplierOrderAction::new(SupplierOrderActionId::new("action-7"), overlong_key).is_err());

        let overlong_summary = SupplierOrderActionData {
            response_summary: Some("s".repeat(2049)),
            ..sample_data()
        };
        assert!(SupplierOrderAction::new(SupplierOrderActionId::new("action-8"), overlong_summary).is_err());
    }

    #[test]
    fn update_applies_mutable_fields_and_record_attempt_counts() {
        let mut action =
            SupplierOrderAction::new(SupplierOrderActionId::new("action-1"), sample_data()).unwrap();
        action
            .update(SupplierOrderActionUpdate {
                status: Some(SupplierOrderActionStatus::Succeeded),
                external_request_id: Some(" REQ-1 ".to_string()),
                request_summary: Some("summary".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(action.status, SupplierOrderActionStatus::Succeeded);
        assert_eq!(action.external_request_id.as_deref(), Some("REQ-1"));
        assert_eq!(action.request_summary.as_deref(), Some("summary"));
        assert_eq!(action.idempotency_key, "FO-2026-001", "关键字段不可修改");

        action.record_attempt(Some(Instant::from_unix_secs(1_700_000_100)));
        action.record_attempt(None);
        assert_eq!(action.attempt_count, 2);
        assert!(action.next_attempt_at.is_none());
    }

    #[test]
    fn update_rejects_blank_external_request_id() {
        let mut action =
            SupplierOrderAction::new(SupplierOrderActionId::new("action-1"), sample_data()).unwrap();
        assert!(action
            .update(SupplierOrderActionUpdate {
                external_request_id: Some("   ".to_string()),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn line_new_accepts_valid_line() {
        let line = SupplierOrderActionLine::new(
            SupplierOrderActionLineId::new("action-line-1"),
            sample_line_data(),
        )
        .unwrap();
        assert_eq!(line.line_no, 1);
        assert_eq!(line.quantity, Quantity::from_str("2.000000").unwrap());
        assert_eq!(line.amount, Amount::from_str("19.98").unwrap());
    }

    #[test]
    fn line_new_rejects_non_positive_quantity_or_amount() {
        let zero_quantity = SupplierOrderActionLineData {
            quantity: Quantity::from_str("0.000000").unwrap(),
            ..sample_line_data()
        };
        assert!(
            SupplierOrderActionLine::new(SupplierOrderActionLineId::new("action-line-2"), zero_quantity)
                .is_err()
        );

        let negative_amount = SupplierOrderActionLineData {
            amount: Amount::from_str("-1.00").unwrap(),
            ..sample_line_data()
        };
        assert!(SupplierOrderActionLine::new(
            SupplierOrderActionLineId::new("action-line-3"),
            negative_amount
        )
        .is_err());
    }
}
