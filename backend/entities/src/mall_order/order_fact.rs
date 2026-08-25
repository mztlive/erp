//! `mall_order_fact` 与两张一对一扩展表 `mall_order_cancel_fact`、`mall_order_completion_fact`
//! （数据模型 §6.17）。
//!
//! `mall_order_fact` 是五类关键事实的共同事件信封：先保存通过完整性校验的原始事实，
//! 再做商品、卡实例、成本和供应商归集；归集条件缺失时保留事实并进入差异，不拒收、
//! 不复制第二份事实。事实不可变（§4.5），只有 `processing_status` 归集进度沿固定
//! 邻接推进。`business_fact_key` 业务事实层唯一、`inbox_message_id` 非空且唯一、
//! `(mall_id, source_event_id)` 消息层唯一等跨行约束由 P2 唯一索引落实。
//!
//! 扩展表的一对一关系（`mall_order_fact_id` 各自唯一）与「扩展表事实类型必须匹配」
//! 依赖跨表查询，由 P3 服务校验（P3 条目：§6.17 扩展表事实类型匹配）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::ensure_transition;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    InboxMessageId, MallAfterSalesRequestId, MallOrderCancelFactId, MallOrderCompletionFactId,
    MallOrderFactId,
};
use crate::mall_order::types::{CancelScope, DataSource, FactType, ProcessingStatus};
use crate::money::{Amount, Quantity};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 目标商城代码最大长度。
const MALL_ID_MAX_LEN: usize = 64;
/// 来源事件 ID 最大长度。
const EVENT_ID_MAX_LEN: usize = 256;
/// 业务事实键最大长度。
const BUSINESS_FACT_KEY_MAX_LEN: usize = 256;
/// 商城订单号最大长度。
const ORDER_NO_MAX_LEN: usize = 128;
/// 结果版本最大长度。
const VERSION_MAX_LEN: usize = 64;
/// 原始报文引用最大长度。
const PAYLOAD_REF_MAX_LEN: usize = 256;
/// 取消原因最大长度。
const REASON_MAX_LEN: usize = 512;

/// 关键事实创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallOrderFactData {
    /// 消息来源商城（`source_system.code`）。
    pub mall_id: String,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 承载契约版本、商城发送时间和原始载荷的共同信封。
    pub inbox_message_id: InboxMessageId,
    /// 事实类型。
    pub fact_type: FactType,
    /// 跨实时和回填的稳定事实键。
    pub business_fact_key: String,
    /// 商城订单号。
    pub external_order_no: String,
    /// 对应结果版本。
    pub external_order_version: String,
    /// 商城售后请求 ID；取消、退款、余额恢复必填。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 原支付成功事实；取消、退款、完成、余额恢复必填。
    pub original_payment_fact_id: Option<MallOrderFactId>,
    /// 事实发生时间。
    pub occurred_at: Instant,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 实时或历史回填。
    pub data_source: DataSource,
    /// 可选的加密原文引用。
    pub raw_payload_reference: Option<String>,
}

/// 关键事实实体（共同事件信封，数据模型 §6.17）。
///
/// 事实字段不可变；`processing_status` 初始为 `Saved`，经
/// [`MallOrderFact::update_processing_status`] 沿固定邻接推进。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallOrderFact {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 消息来源商城。
    pub mall_id: String,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 共同信封。
    pub inbox_message_id: InboxMessageId,
    /// 事实类型。
    pub fact_type: FactType,
    /// 跨实时和回填的稳定事实键。
    pub business_fact_key: String,
    /// 商城订单号。
    pub external_order_no: String,
    /// 对应结果版本。
    pub external_order_version: String,
    /// 商城售后请求 ID。
    pub after_sales_request_id: Option<MallAfterSalesRequestId>,
    /// 原支付成功事实。
    pub original_payment_fact_id: Option<MallOrderFactId>,
    /// 事实发生时间。
    pub occurred_at: Instant,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 实时或历史回填。
    pub data_source: DataSource,
    /// 可选的加密原文引用。
    pub raw_payload_reference: Option<String>,
    /// 处理状态。
    pub processing_status: ProcessingStatus,
}

impl MallOrderFact {
    /// 创建关键事实。
    ///
    /// 完成文本字段校验与规范化，并按 §6.17 强制事实类型相关的引用完整性与时序：
    /// - 取消、退款、余额恢复必填 `after_sales_request_id`，支付成功与订单完成不得携带；
    /// - 取消、退款、完成、余额恢复必填 `original_payment_fact_id`，支付成功不得携带；
    /// - `received_at` 不得早于 `occurred_at`。
    ///
    /// `processing_status` 固定为 `Saved`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallOrderFactId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的关键事实实体。
    ///
    /// # 错误
    /// 当文本为空/超长、事实类型关联不一致或时间倒挂时返回错误。
    pub fn new(id: MallOrderFactId, data: MallOrderFactData) -> Result<Self> {
        let mall_id = normalize_required_text(
            data.mall_id,
            "来源商城不能为空",
            MALL_ID_MAX_LEN,
            "来源商城代码过长",
        )?;
        let source_event_id = normalize_required_text(
            data.source_event_id,
            "来源事件ID不能为空",
            EVENT_ID_MAX_LEN,
            "来源事件ID过长",
        )?;
        let business_fact_key = normalize_required_text(
            data.business_fact_key,
            "业务事实键不能为空",
            BUSINESS_FACT_KEY_MAX_LEN,
            "业务事实键过长",
        )?;
        let external_order_no = normalize_required_text(
            data.external_order_no,
            "商城订单号不能为空",
            ORDER_NO_MAX_LEN,
            "商城订单号过长",
        )?;
        let external_order_version = normalize_required_text(
            data.external_order_version,
            "结果版本不能为空",
            VERSION_MAX_LEN,
            "结果版本过长",
        )?;
        let raw_payload_reference =
            normalize_optional_text(data.raw_payload_reference, "原始报文引用", PAYLOAD_REF_MAX_LEN)?;
        validate_fact_links(
            data.fact_type,
            data.after_sales_request_id.clone(),
            data.original_payment_fact_id.clone(),
            data.received_at,
            data.occurred_at,
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_id,
            source_event_id,
            inbox_message_id: data.inbox_message_id,
            fact_type: data.fact_type,
            business_fact_key,
            external_order_no,
            external_order_version,
            after_sales_request_id: data.after_sales_request_id,
            original_payment_fact_id: data.original_payment_fact_id,
            occurred_at: data.occurred_at,
            received_at: data.received_at,
            data_source: data.data_source,
            raw_payload_reference,
            processing_status: ProcessingStatus::Saved,
        })
    }

    /// 推进事实处理状态。
    ///
    /// 归集进度沿固定邻接推进（§6.17）：已保存 → 待归集 | 差异 | 拒绝；
    /// 待归集 → 已归集 | 差异；差异 → 待归集 | 拒绝；已归集与拒绝为终态。
    ///
    /// # 参数
    /// * `to` - 目标处理状态
    ///
    /// # 返回
    /// 迁移合法返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标不在 `from.allowed_next()` 中且与当前状态不同时返回
    /// `InvalidStateTransition`。
    pub fn update_processing_status(&mut self, to: ProcessingStatus) -> Result<()> {
        ensure_transition(self.processing_status, to)?;
        self.processing_status = to;
        Ok(())
    }

    /// 校验当前事实可作为指定商城订单的原支付事实。
    ///
    /// # 参数
    /// * `mall_id` - 后续事实声明的来源商城
    /// * `external_order_no` - 后续事实声明的商城订单号
    ///
    /// # 返回
    /// 当前事实为同商城同订单且已正式归集的支付成功事实时返回 `Ok(())`。
    ///
    /// # 错误
    /// 事实类型、商城订单关系或处理状态不满足时返回错误。
    pub fn ensure_attributed_payment_for(&self, mall_id: &str, external_order_no: &str) -> Result<()> {
        if !self.fact_type.is_payment_succeeded() {
            return Err(Error::from("原事实不是支付成功事实"));
        }
        if self.mall_id != mall_id || self.external_order_no != external_order_no {
            return Err(Error::from("原支付事实与本次事实的商城或订单不一致"));
        }
        if self.processing_status != ProcessingStatus::Attributed {
            return Err(Error::from("原支付事实尚未正式归集"));
        }
        Ok(())
    }
}

/// 校验事实类型相关的引用完整性与时间顺序。
///
/// # 参数
/// * `fact_type` - 事实类型
/// * `after_sales_request_id` - 售后请求 ID
/// * `original_payment_fact_id` - 原支付事实 ID
/// * `received_at` - ERP 接收时间
/// * `occurred_at` - 事实发生时间
///
/// # 返回
/// 关联与时序一致返回 `Ok(())`。
///
/// # 错误
/// 关联缺失/多余或接收时间早于发生时间时返回错误。
fn validate_fact_links(
    fact_type: FactType,
    after_sales_request_id: Option<MallAfterSalesRequestId>,
    original_payment_fact_id: Option<MallOrderFactId>,
    received_at: Instant,
    occurred_at: Instant,
) -> Result<()> {
    if after_sales_request_id.is_some() != fact_type.requires_after_sales_request() {
        return Err(Error::from(match fact_type.requires_after_sales_request() {
            true => "取消、退款和余额恢复必须携带商城售后请求ID",
            false => "支付成功与订单完成不得携带商城售后请求ID",
        }));
    }
    if original_payment_fact_id.is_some() != fact_type.requires_original_payment() {
        return Err(Error::from(match fact_type.requires_original_payment() {
            true => "取消、退款、完成和余额恢复必须关联原支付事实",
            false => "支付成功事实不得关联原支付事实",
        }));
    }
    if received_at < occurred_at {
        return Err(Error::from("ERP 接收时间不得早于事实发生时间"));
    }
    Ok(())
}

/// 订单取消事实创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallOrderCancelFactData {
    /// `ORDER_CANCELED` 事实。
    pub mall_order_fact_id: MallOrderFactId,
    /// 来源取消版本。
    pub cancel_version: String,
    /// 整单或明细。
    pub cancel_scope: CancelScope,
    /// 实际取消数量。
    pub actual_canceled_quantity: Quantity,
    /// 实际取消金额。
    pub actual_canceled_amount: Amount,
    /// 取消原因。
    pub reason: String,
}

/// 订单取消事实扩展实体（数据模型 §6.17）。
///
/// `ORDER_CANCELED` 只记录取消结果；发生资金退回时仍必须另有 `REFUND_SUCCEEDED`，
/// 取消本身不冲减消费或支付来源。不可变，只提供 `new()`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallOrderCancelFact {
    #[serde(flatten)]
    pub base: BaseModel,
    /// `ORDER_CANCELED` 事实。
    pub mall_order_fact_id: MallOrderFactId,
    /// 来源取消版本。
    pub cancel_version: String,
    /// 整单或明细。
    pub cancel_scope: CancelScope,
    /// 实际取消数量。
    pub actual_canceled_quantity: Quantity,
    /// 实际取消金额。
    pub actual_canceled_amount: Amount,
    /// 取消原因。
    pub reason: String,
}

impl MallOrderCancelFact {
    /// 创建订单取消事实扩展。
    ///
    /// 完成文本校验与规范化；取消数量与金额必须非负（实际取消范围不可为负）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallOrderCancelFactId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的取消事实扩展实体。
    ///
    /// # 错误
    /// 当文本为空/超长或取消数量/金额为负时返回错误。
    pub fn new(id: MallOrderCancelFactId, data: MallOrderCancelFactData) -> Result<Self> {
        let cancel_version = normalize_required_text(
            data.cancel_version,
            "取消版本不能为空",
            VERSION_MAX_LEN,
            "取消版本过长",
        )?;
        let reason =
            normalize_required_text(data.reason, "取消原因不能为空", REASON_MAX_LEN, "取消原因过长")?;
        if data.actual_canceled_quantity.to_decimal().is_sign_negative() {
            return Err(Error::from("实际取消数量不能为负"));
        }
        if data.actual_canceled_amount.to_decimal().is_sign_negative() {
            return Err(Error::from("实际取消金额不能为负"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_order_fact_id: data.mall_order_fact_id,
            cancel_version,
            cancel_scope: data.cancel_scope,
            actual_canceled_quantity: data.actual_canceled_quantity,
            actual_canceled_amount: data.actual_canceled_amount,
            reason,
        })
    }
}

/// 订单完成事实创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallOrderCompletionFactData {
    /// `ORDER_COMPLETED` 事实。
    pub mall_order_fact_id: MallOrderFactId,
    /// 来源完成版本。
    pub completion_version: String,
    /// 商城实际完成时间。
    pub completed_at: Instant,
}

/// 订单完成事实扩展实体（数据模型 §6.17）。
///
/// `ORDER_COMPLETED` 不覆盖供应商履约事实，只记录商城订单完成结果。不可变，
/// 只提供 `new()`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallOrderCompletionFact {
    #[serde(flatten)]
    pub base: BaseModel,
    /// `ORDER_COMPLETED` 事实。
    pub mall_order_fact_id: MallOrderFactId,
    /// 来源完成版本。
    pub completion_version: String,
    /// 商城实际完成时间。
    pub completed_at: Instant,
}

impl MallOrderCompletionFact {
    /// 创建订单完成事实扩展。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallOrderCompletionFactId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的完成事实扩展实体。
    ///
    /// # 错误
    /// 当完成版本为空或超长时返回错误。
    pub fn new(id: MallOrderCompletionFactId, data: MallOrderCompletionFactData) -> Result<Self> {
        let completion_version = normalize_required_text(
            data.completion_version,
            "完成版本不能为空",
            VERSION_MAX_LEN,
            "完成版本过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_order_fact_id: data.mall_order_fact_id,
            completion_version,
            completed_at: data.completed_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MallOrderCancelFact, MallOrderCancelFactData, MallOrderCompletionFact, MallOrderCompletionFactData,
        MallOrderFact, MallOrderFactData,
    };
    use crate::common::state::{ensure_transition, DocumentState};
    use crate::common::time::Instant;
    use crate::ids::{
        InboxMessageId, MallAfterSalesRequestId, MallOrderCancelFactId, MallOrderCompletionFactId,
        MallOrderFactId,
    };
    use crate::mall_order::types::{CancelScope, DataSource, FactType, ProcessingStatus};
    use crate::money::{Amount, Quantity};
    use std::str::FromStr;

    fn fact_data() -> MallOrderFactData {
        MallOrderFactData {
            mall_id: " mall-a ".to_string(),
            source_event_id: " evt-100 ".to_string(),
            inbox_message_id: InboxMessageId::new("inbox-1"),
            fact_type: FactType::PaymentSucceeded,
            business_fact_key: " mall-a:PAYMENT:SO-1:v1 ".to_string(),
            external_order_no: " SO-1 ".to_string(),
            external_order_version: " v1 ".to_string(),
            after_sales_request_id: None,
            original_payment_fact_id: None,
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            received_at: Instant::from_unix_secs(1_700_000_100),
            data_source: DataSource::Realtime,
            raw_payload_reference: Some(" storage/payload-1 ".to_string()),
        }
    }

    /// happy path：文本规范化、信封字段落库、处理状态初始为已保存。
    #[test]
    fn fact_new_trims_fields_and_starts_saved() {
        let fact = MallOrderFact::new(MallOrderFactId::new("fact-1"), fact_data()).unwrap();

        assert_eq!(fact.mall_id, "mall-a");
        assert_eq!(fact.source_event_id, "evt-100");
        assert_eq!(fact.external_order_no, "SO-1");
        assert_eq!(fact.external_order_version, "v1");
        assert_eq!(fact.business_fact_key, "mall-a:PAYMENT:SO-1:v1");
        assert_eq!(fact.raw_payload_reference.as_deref(), Some("storage/payload-1"));
        assert_eq!(fact.processing_status, ProcessingStatus::Saved);
        assert_eq!(fact.fact_type, FactType::PaymentSucceeded);
    }

    /// 失败路径：必填空、超长、时间倒挂。
    #[test]
    fn fact_new_rejects_empty_overlong_and_inverted_time() {
        let empty = MallOrderFactData {
            business_fact_key: "  ".to_string(),
            ..fact_data()
        };
        assert!(MallOrderFact::new(MallOrderFactId::new("f2"), empty).is_err());

        let overlong = MallOrderFactData {
            source_event_id: "e".repeat(257),
            ..fact_data()
        };
        assert!(MallOrderFact::new(MallOrderFactId::new("f3"), overlong).is_err());

        let inverted = MallOrderFactData {
            occurred_at: Instant::from_unix_secs(1_700_000_200),
            ..fact_data()
        };
        assert!(MallOrderFact::new(MallOrderFactId::new("f4"), inverted).is_err());
    }

    /// 关联不一致：取消/退款/余额恢复必填售后请求与原支付；支付/完成不得携带。
    #[test]
    fn fact_new_rejects_link_mismatch_by_fact_type() {
        let refund_missing_request = MallOrderFactData {
            fact_type: FactType::RefundSucceeded,
            after_sales_request_id: None,
            original_payment_fact_id: Some(MallOrderFactId::new("fact-0")),
            ..fact_data()
        };
        assert!(MallOrderFact::new(MallOrderFactId::new("f5"), refund_missing_request).is_err());

        let cancel_missing_payment = MallOrderFactData {
            fact_type: FactType::OrderCanceled,
            after_sales_request_id: Some(MallAfterSalesRequestId::new("as-1")),
            original_payment_fact_id: None,
            ..fact_data()
        };
        assert!(MallOrderFact::new(MallOrderFactId::new("f6"), cancel_missing_payment).is_err());

        let payment_with_request = MallOrderFactData {
            after_sales_request_id: Some(MallAfterSalesRequestId::new("as-1")),
            ..fact_data()
        };
        assert!(MallOrderFact::new(MallOrderFactId::new("f7"), payment_with_request).is_err());

        let completed_with_request = MallOrderFactData {
            fact_type: FactType::OrderCompleted,
            after_sales_request_id: Some(MallAfterSalesRequestId::new("as-1")),
            original_payment_fact_id: Some(MallOrderFactId::new("fact-0")),
            ..fact_data()
        };
        assert!(MallOrderFact::new(MallOrderFactId::new("f8"), completed_with_request).is_err());

        let restoration_ok = MallOrderFactData {
            fact_type: FactType::CardBalanceRestored,
            after_sales_request_id: Some(MallAfterSalesRequestId::new("as-1")),
            original_payment_fact_id: Some(MallOrderFactId::new("fact-0")),
            ..fact_data()
        };
        assert!(MallOrderFact::new(MallOrderFactId::new("f9"), restoration_ok).is_ok());
    }

    /// 处理状态状态机：合法/非法迁移与终态定向断言。
    #[test]
    fn attributed_payment_relationship_is_entity_owned() {
        let mut fact = MallOrderFact::new(MallOrderFactId::new("fact-1"), fact_data()).unwrap();
        assert!(fact.ensure_attributed_payment_for("mall-a", "SO-1").is_err());
        fact.update_processing_status(ProcessingStatus::PendingAttribution)
            .unwrap();
        fact.update_processing_status(ProcessingStatus::Attributed)
            .unwrap();
        assert!(fact.ensure_attributed_payment_for("mall-a", "SO-1").is_ok());
        assert!(fact.ensure_attributed_payment_for("mall-b", "SO-1").is_err());
        fact.fact_type = FactType::OrderCanceled;
        assert!(fact.ensure_attributed_payment_for("mall-a", "SO-1").is_err());
    }

    #[test]
    fn processing_status_machine_directed_edges() {
        assert!(ensure_transition(ProcessingStatus::Saved, ProcessingStatus::PendingAttribution).is_ok());
        assert!(ensure_transition(ProcessingStatus::Saved, ProcessingStatus::Difference).is_ok());
        assert!(ensure_transition(ProcessingStatus::Saved, ProcessingStatus::Rejected).is_ok());
        assert!(
            ensure_transition(ProcessingStatus::PendingAttribution, ProcessingStatus::Attributed).is_ok()
        );
        assert!(
            ensure_transition(ProcessingStatus::Difference, ProcessingStatus::PendingAttribution).is_ok()
        );

        assert!(
            ensure_transition(ProcessingStatus::Attributed, ProcessingStatus::PendingAttribution).is_err()
        );
        assert!(ensure_transition(ProcessingStatus::Rejected, ProcessingStatus::Saved).is_err());
        assert!(ensure_transition(ProcessingStatus::Attributed, ProcessingStatus::Difference).is_err());
        assert_eq!(
            ProcessingStatus::Attributed.allowed_next(),
            &[] as &[ProcessingStatus]
        );
        assert_eq!(
            ProcessingStatus::Rejected.allowed_next(),
            &[] as &[ProcessingStatus]
        );
    }

    /// happy path + 状态机：事实归集进度按固定邻接推进。
    #[test]
    fn fact_processing_status_advances_and_blocks_invalid() {
        let mut fact = MallOrderFact::new(MallOrderFactId::new("fact-1"), fact_data()).unwrap();

        fact.update_processing_status(ProcessingStatus::PendingAttribution)
            .unwrap();
        fact.update_processing_status(ProcessingStatus::Attributed)
            .unwrap();
        assert!(fact
            .update_processing_status(ProcessingStatus::PendingAttribution)
            .is_err());
        assert!(
            fact.update_processing_status(ProcessingStatus::Attributed)
                .is_ok(),
            "幂等迁移恒合法"
        );
    }

    /// 取消事实：happy path 与负数量/负金额拒绝。
    #[test]
    fn cancel_fact_trims_and_rejects_negative_scope() {
        let data = MallOrderCancelFactData {
            mall_order_fact_id: MallOrderFactId::new("fact-1"),
            cancel_version: " v2 ".to_string(),
            cancel_scope: CancelScope::WholeOrder,
            actual_canceled_quantity: Quantity::from_str("2.000000").unwrap(),
            actual_canceled_amount: Amount::from_str("199.00").unwrap(),
            reason: " 员工取消 ".to_string(),
        };
        let fact = MallOrderCancelFact::new(MallOrderCancelFactId::new("cf-1"), data).unwrap();
        assert_eq!(fact.cancel_version, "v2");
        assert_eq!(fact.reason, "员工取消");

        let negative_amount = MallOrderCancelFactData {
            actual_canceled_amount: Amount::from_str("-1.00").unwrap(),
            ..MallOrderCancelFactData {
                mall_order_fact_id: MallOrderFactId::new("fact-1"),
                cancel_version: "v2".to_string(),
                cancel_scope: CancelScope::WholeOrder,
                actual_canceled_quantity: Quantity::from_str("2.000000").unwrap(),
                actual_canceled_amount: Amount::from_str("1.00").unwrap(),
                reason: "员工取消".to_string(),
            }
        };
        assert!(MallOrderCancelFact::new(MallOrderCancelFactId::new("cf-2"), negative_amount).is_err());

        let blank_reason = MallOrderCancelFactData {
            reason: "  ".to_string(),
            ..MallOrderCancelFactData {
                mall_order_fact_id: MallOrderFactId::new("fact-1"),
                cancel_version: "v2".to_string(),
                cancel_scope: CancelScope::WholeOrder,
                actual_canceled_quantity: Quantity::from_str("2.000000").unwrap(),
                actual_canceled_amount: Amount::from_str("1.00").unwrap(),
                reason: "员工取消".to_string(),
            }
        };
        assert!(MallOrderCancelFact::new(MallOrderCancelFactId::new("cf-3"), blank_reason).is_err());
    }

    /// 完成事实：happy path 与版本为空拒绝。
    #[test]
    fn completion_fact_trims_and_rejects_blank_version() {
        let data = MallOrderCompletionFactData {
            mall_order_fact_id: MallOrderFactId::new("fact-1"),
            completion_version: " v5 ".to_string(),
            completed_at: Instant::from_unix_secs(1_700_000_300),
        };
        let fact = MallOrderCompletionFact::new(MallOrderCompletionFactId::new("cp-1"), data).unwrap();
        assert_eq!(fact.completion_version, "v5");
        assert_eq!(fact.completed_at, Instant::from_unix_secs(1_700_000_300));

        let blank = MallOrderCompletionFactData {
            completion_version: "  ".to_string(),
            ..MallOrderCompletionFactData {
                mall_order_fact_id: MallOrderFactId::new("fact-1"),
                completion_version: "v5".to_string(),
                completed_at: Instant::from_unix_secs(1_700_000_300),
            }
        };
        assert!(MallOrderCompletionFact::new(MallOrderCompletionFactId::new("cp-2"), blank).is_err());
    }
}
