//! `inbox_message`：已接收外部消息（商城关键事实或供应商回调）的普通入站记录（数据模型 §6.21）。
//!
//! 本表是外部消息的信封与契约审计真相：`payload_schema_version`、`source_sent_at` 以
//! 本表为准（`mall_order_fact` 必须引用本信封）。消息层去重键
//! `(source_system_id, source_event_id)` 与「非空 `business_fact_key` 在对应事实类型
//! 内唯一」由唯一索引在仓储层（P2）落实；先做消息去重再做业务事实去重（§8.4 第 3 条，
//! 去重校验依赖仓储，实体层只建模字段与不变式）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::{InboxMessageId, SourceSystemId};

/// 来源事件 ID 最大长度。
const SOURCE_EVENT_ID_MAX_LEN: usize = 256;
/// 业务事实键最大长度。
const BUSINESS_FACT_KEY_MAX_LEN: usize = 256;
/// 来源契约版本最大长度。
const SCHEMA_VERSION_MAX_LEN: usize = 64;
/// 规范化内容引用最大长度。
const PAYLOAD_REFERENCE_MAX_LEN: usize = 512;

/// 消息类型（数据模型 §6.21：商城关键事实或供应商回调等）。
///
/// 商城关键事实取值与 `erp-phase-2.md` §13.1 固定事实代码一致（§4.6 关键事实类型
/// 是固定业务代码）；「商城取消或退款动作请求」是业务动作请求而非结果事实，
/// 与结果事实分开建模（`erp-mall-data-mapping.md` §10.4.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageType {
    /// 支付成功事实（`PAYMENT_SUCCEEDED`）。
    PaymentSucceeded,
    /// 订单取消事实（`ORDER_CANCELED`）。
    OrderCanceled,
    /// 退款成功事实（`REFUND_SUCCEEDED`）。
    RefundSucceeded,
    /// 订单完成事实（`ORDER_COMPLETED`）。
    OrderCompleted,
    /// 卡券余额恢复事实（`CARD_BALANCE_RESTORED`）。
    CardBalanceRestored,
    /// 商城取消或退款动作请求（业务动作请求，非结果事实）。
    MallActionRequest,
    /// 供应商系统回调（状态回调、退款结果等）。
    SupplierCallback,
}

impl MessageType {
    /// 返回消息类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::PaymentSucceeded => "支付成功",
            Self::OrderCanceled => "订单取消",
            Self::RefundSucceeded => "退款成功",
            Self::OrderCompleted => "订单完成",
            Self::CardBalanceRestored => "卡券余额恢复",
            Self::MallActionRequest => "商城动作请求",
            Self::SupplierCallback => "供应商回调",
        }
    }

    /// 返回消息类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PaymentSucceeded => "PAYMENT_SUCCEEDED",
            Self::OrderCanceled => "ORDER_CANCELED",
            Self::RefundSucceeded => "REFUND_SUCCEEDED",
            Self::OrderCompleted => "ORDER_COMPLETED",
            Self::CardBalanceRestored => "CARD_BALANCE_RESTORED",
            Self::MallActionRequest => "MALL_ACTION_REQUEST",
            Self::SupplierCallback => "SUPPLIER_CALLBACK",
        }
    }
}

/// 消息处理状态（数据模型 §6.21：已接收、处理中、已处理、重复、失败、转人工）。
///
/// §7.7 明确投递状态由 `integration_error_task.status` 表达，不另设消息投递状态机，
/// 本枚举只按字典固化取值，不实现 `DocumentState` 迁移矩阵。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxMessageStatus {
    /// 已接收。
    #[default]
    Received,
    /// 处理中。
    Processing,
    /// 已处理。
    Processed,
    /// 重复。
    Duplicate,
    /// 失败。
    Failed,
    /// 转人工。
    ToManual,
}

impl InboxMessageStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Received => "已接收",
            Self::Processing => "处理中",
            Self::Processed => "已处理",
            Self::Duplicate => "重复",
            Self::Failed => "失败",
            Self::ToManual => "转人工",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Processing => "processing",
            Self::Processed => "processed",
            Self::Duplicate => "duplicate",
            Self::Failed => "failed",
            Self::ToManual => "to_manual",
        }
    }
}

/// 入站消息创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxMessageData {
    /// 来源系统 ID。
    pub source_system_id: SourceSystemId,
    /// 来源事件 ID（消息层幂等身份，与 `source_system_id` 组合唯一）。
    pub source_event_id: String,
    /// 消息类型。
    pub message_type: MessageType,
    /// 业务事实键（实时与历史回填使用同一业务事实键去重，商城订单号不能单独作幂等键）。
    pub business_fact_key: String,
    /// 来源契约版本。
    pub payload_schema_version: String,
    /// 规范化内容引用。
    pub payload_reference: Option<String>,
    /// 消息处理状态。
    pub status: InboxMessageStatus,
    /// 来源系统发送时间。
    pub source_sent_at: Option<Instant>,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 处理完成时间。
    pub processed_at: Option<Instant>,
}

/// 入站消息更新数据（只允许更新处理状态与处理完成时间，消息身份与幂等键不可修改）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct InboxMessageUpdate {
    /// 新的处理状态；`None` 表示不修改。
    pub status: Option<InboxMessageStatus>,
    /// 新的处理完成时间；`None` 表示不修改。
    pub processed_at: Option<Instant>,
}

/// 入站消息实体（数据模型 §6.21）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct InboxMessage {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源系统 ID。
    pub source_system_id: SourceSystemId,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 消息类型。
    pub message_type: MessageType,
    /// 业务事实键。
    pub business_fact_key: String,
    /// 来源契约版本。
    pub payload_schema_version: String,
    /// 规范化内容引用。
    pub payload_reference: Option<String>,
    /// 消息处理状态。
    pub status: InboxMessageStatus,
    /// 来源系统发送时间。
    pub source_sent_at: Option<Instant>,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 处理完成时间。
    pub processed_at: Option<Instant>,
}

impl InboxMessage {
    /// 创建入站消息记录。
    ///
    /// 完成 `source_event_id`、`business_fact_key`、`payload_schema_version` 的校验与
    /// 规范化（去首尾空白、非空、长度上限），并强制不变式：状态为已处理时必须同时
    /// 提供处理完成时间（关联一致性）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::InboxMessageId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的入站消息实体。
    ///
    /// # 错误
    /// 当来源事件 ID/业务事实键/契约版本为空或超长、内容引用超长，或已处理状态
    /// 缺少处理完成时间时返回错误。
    pub fn new(id: InboxMessageId, data: InboxMessageData) -> Result<Self> {
        let source_event_id = normalize_required_text(
            data.source_event_id,
            "来源事件ID不能为空",
            SOURCE_EVENT_ID_MAX_LEN,
            "来源事件ID过长",
        )?;
        let business_fact_key = normalize_required_text(
            data.business_fact_key,
            "业务事实键不能为空",
            BUSINESS_FACT_KEY_MAX_LEN,
            "业务事实键过长",
        )?;
        let payload_schema_version = normalize_required_text(
            data.payload_schema_version,
            "来源契约版本不能为空",
            SCHEMA_VERSION_MAX_LEN,
            "来源契约版本过长",
        )?;
        let payload_reference = normalize_optional_text(
            data.payload_reference,
            "规范化内容引用",
            PAYLOAD_REFERENCE_MAX_LEN,
        )?;
        if data.status == InboxMessageStatus::Processed && data.processed_at.is_none() {
            return Err(Error::from("消息状态为已处理时必须提供处理完成时间"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            source_system_id: data.source_system_id,
            source_event_id,
            message_type: data.message_type,
            business_fact_key,
            payload_schema_version,
            payload_reference,
            status: data.status,
            source_sent_at: data.source_sent_at,
            received_at: data.received_at,
            processed_at: data.processed_at,
        })
    }

    /// 更新入站消息的处理状态与处理完成时间。
    ///
    /// 消息身份（来源系统、来源事件 ID）与幂等键（业务事实键）不在通用更新中修改；
    /// 强制不变式：状态置为已处理必须同时提供处理完成时间，设置处理完成时间必须
    /// 同时把状态置为已处理。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当已处理状态与处理完成时间不配套时返回错误。
    pub fn update(&mut self, update: InboxMessageUpdate) -> Result<()> {
        if update.status == Some(InboxMessageStatus::Processed) && update.processed_at.is_none() {
            return Err(Error::from("标记已处理时必须同时提供处理完成时间"));
        }
        if update.processed_at.is_some() && update.status != Some(InboxMessageStatus::Processed) {
            return Err(Error::from("设置处理完成时间必须同时把状态置为已处理"));
        }
        if let Some(status) = update.status {
            self.status = status;
        }
        if let Some(processed_at) = update.processed_at {
            self.processed_at = Some(processed_at);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, MessageType};
    use crate::common::time::Instant;
    use crate::ids::{InboxMessageId, SourceSystemId};

    const RECEIVED_AT: i64 = 1_700_000_000;
    const SENT_AT: i64 = 1_699_999_900;
    const PROCESSED_AT: i64 = 1_700_000_100;

    fn message_data() -> InboxMessageData {
        InboxMessageData {
            source_system_id: SourceSystemId::new("sys-mall-1"),
            source_event_id: " evt-1001 ".to_string(),
            message_type: MessageType::PaymentSucceeded,
            business_fact_key: " mall-1|PAYMENT_SUCCEEDED|SO-2026-001|v3 ".to_string(),
            payload_schema_version: " v1.2 ".to_string(),
            payload_reference: Some(" archive://2026/msg-1001 ".to_string()),
            status: InboxMessageStatus::Received,
            source_sent_at: Some(Instant::from_unix_secs(SENT_AT)),
            received_at: Instant::from_unix_secs(RECEIVED_AT),
            processed_at: None,
        }
    }

    #[test]
    fn new_trims_and_normalizes_all_text_fields() {
        let message = InboxMessage::new(InboxMessageId::new("msg-1"), message_data()).unwrap();

        assert_eq!(message.source_event_id, "evt-1001");
        assert_eq!(
            message.business_fact_key,
            "mall-1|PAYMENT_SUCCEEDED|SO-2026-001|v3"
        );
        assert_eq!(message.payload_schema_version, "v1.2");
        assert_eq!(
            message.payload_reference.as_deref(),
            Some("archive://2026/msg-1001")
        );
        assert_eq!(message.status, InboxMessageStatus::Received);
        assert_eq!(message.source_system_id, SourceSystemId::new("sys-mall-1"));
        assert_eq!(message.source_sent_at, Some(Instant::from_unix_secs(SENT_AT)));
        assert_eq!(message.received_at.unix_secs(), RECEIVED_AT);
        assert!(message.processed_at.is_none());
    }

    #[test]
    fn new_rejects_empty_required_fields() {
        let empty_event = InboxMessageData {
            source_event_id: "  ".to_string(),
            ..message_data()
        };
        assert!(InboxMessage::new(InboxMessageId::new("msg-2"), empty_event).is_err());

        let empty_key = InboxMessageData {
            business_fact_key: "  ".to_string(),
            ..message_data()
        };
        assert!(InboxMessage::new(InboxMessageId::new("msg-3"), empty_key).is_err());

        let empty_version = InboxMessageData {
            payload_schema_version: "  ".to_string(),
            ..message_data()
        };
        assert!(InboxMessage::new(InboxMessageId::new("msg-4"), empty_version).is_err());
    }

    #[test]
    fn new_rejects_overlong_fields() {
        let overlong_event = InboxMessageData {
            source_event_id: "e".repeat(257),
            ..message_data()
        };
        assert!(InboxMessage::new(InboxMessageId::new("msg-5"), overlong_event).is_err());

        let overlong_key = InboxMessageData {
            business_fact_key: "k".repeat(257),
            ..message_data()
        };
        assert!(InboxMessage::new(InboxMessageId::new("msg-6"), overlong_key).is_err());

        let overlong_reference = InboxMessageData {
            payload_reference: Some("r".repeat(513)),
            ..message_data()
        };
        assert!(InboxMessage::new(InboxMessageId::new("msg-7"), overlong_reference).is_err());
    }

    #[test]
    fn new_rejects_processed_without_completed_time() {
        let processed_without_time = InboxMessageData {
            status: InboxMessageStatus::Processed,
            processed_at: None,
            ..message_data()
        };
        assert!(InboxMessage::new(InboxMessageId::new("msg-8"), processed_without_time).is_err());
    }

    #[test]
    fn update_applies_status_and_processed_at_only() {
        let mut message = InboxMessage::new(InboxMessageId::new("msg-9"), message_data()).unwrap();
        let original_event_id = message.source_event_id.clone();

        message
            .update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Processed),
                processed_at: Some(Instant::from_unix_secs(PROCESSED_AT)),
            })
            .unwrap();

        assert_eq!(message.status, InboxMessageStatus::Processed);
        assert_eq!(message.processed_at, Some(Instant::from_unix_secs(PROCESSED_AT)));
        assert_eq!(
            message.source_event_id, original_event_id,
            "消息身份不可被通用更新修改"
        );
        assert_eq!(
            message.source_system_id,
            SourceSystemId::new("sys-mall-1"),
            "来源系统不可被通用更新修改"
        );
        assert_eq!(
            message.business_fact_key,
            "mall-1|PAYMENT_SUCCEEDED|SO-2026-001|v3"
        );
    }

    #[test]
    fn update_rejects_inconsistent_status_and_time() {
        let mut message = InboxMessage::new(InboxMessageId::new("msg-10"), message_data()).unwrap();

        let processed_without_time = InboxMessageUpdate {
            status: Some(InboxMessageStatus::Processed),
            processed_at: None,
        };
        assert!(message.update(processed_without_time).is_err());

        let time_without_processed = InboxMessageUpdate {
            status: Some(InboxMessageStatus::Processing),
            processed_at: Some(Instant::from_unix_secs(PROCESSED_AT)),
        };
        assert!(message.update(time_without_processed).is_err());
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_expose_labels() {
        assert_eq!(
            serde_json::to_string(&MessageType::PaymentSucceeded).unwrap(),
            "\"PAYMENT_SUCCEEDED\""
        );
        assert_eq!(
            serde_json::to_string(&MessageType::CardBalanceRestored).unwrap(),
            "\"CARD_BALANCE_RESTORED\""
        );
        assert_eq!(
            serde_json::to_string(&MessageType::SupplierCallback).unwrap(),
            "\"SUPPLIER_CALLBACK\""
        );
        assert_eq!(
            serde_json::to_string(&InboxMessageStatus::ToManual).unwrap(),
            "\"to_manual\""
        );

        assert_eq!(MessageType::OrderCanceled.label(), "订单取消");
        assert_eq!(MessageType::MallActionRequest.label(), "商城动作请求");
        assert_eq!(InboxMessageStatus::Duplicate.label(), "重复");
        assert_eq!(InboxMessageStatus::Received.as_str(), "received");
    }

    #[test]
    fn entity_roundtrip_through_bson() {
        let message = InboxMessage::new(InboxMessageId::new("msg-11"), message_data()).unwrap();
        let roundtrip: InboxMessage =
            bson::deserialize_from_document(bson::serialize_to_document(&message).unwrap()).unwrap();
        assert_eq!(roundtrip, message);
    }
}
