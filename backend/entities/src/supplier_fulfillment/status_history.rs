//! `supplier_order_status_history`（数据模型 §6.19 供应商订单状态历史）。
//!
//! 不可变追加记录，只 `new` 不 `update`；回调幂等唯一
//! `(connection_id, external_event_id)` 与"状态版本和发生时间共同校验乱序"由唯一索引
//! 和 P3 编排保证（§6.19），实体层强制单条记录必须是履约状态机的合法迁移，
//! 且接收时间不早于发生时间。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::source::SourceType;
use crate::common::state::ensure_transition;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SupplierApiConnectionId, SupplierOrderStatusHistoryId};
use crate::validation::normalize_required_text;

use super::status::FulfillmentStatus;

/// 供应商状态版本最大长度。
const STATUS_VERSION_MAX_LEN: usize = 64;
/// 外部事件 ID 最大长度。
const EXTERNAL_EVENT_ID_MAX_LEN: usize = 128;

/// 供应商订单状态历史创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOrderStatusHistoryData {
    /// 供应商连接。
    pub connection_id: SupplierApiConnectionId,
    /// 原状态。
    pub previous_status: FulfillmentStatus,
    /// 新状态。
    pub new_status: FulfillmentStatus,
    /// 供应商状态版本。
    pub supplier_status_version: String,
    /// 业务发生时间。
    pub occurred_at: Instant,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 外部事件 ID（与连接组成回调幂等键）。
    pub external_event_id: String,
    /// 来源。
    pub source_type: SourceType,
}

/// 供应商订单状态历史实体（数据模型 §6.19，不可变追加记录）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierOrderStatusHistory {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 供应商连接。
    pub connection_id: SupplierApiConnectionId,
    /// 原状态。
    pub previous_status: FulfillmentStatus,
    /// 新状态。
    pub new_status: FulfillmentStatus,
    /// 供应商状态版本。
    pub supplier_status_version: String,
    /// 业务发生时间。
    pub occurred_at: Instant,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 外部事件 ID。
    pub external_event_id: String,
    /// 来源。
    pub source_type: SourceType,
}

impl SupplierOrderStatusHistory {
    /// 创建状态历史记录。
    ///
    /// 完成版本与事件 ID 的校验和规范化，并强制三条不变式：原状态不等于新状态；
    /// 迁移必须是履约状态机的合法迁移（§7.6）；接收时间不早于发生时间。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierOrderStatusHistoryId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的状态历史实体。
    ///
    /// # 错误
    /// 字段为空/超长、原新状态相同、迁移非法或接收时间早于发生时间时返回错误。
    pub fn new(id: SupplierOrderStatusHistoryId, data: SupplierOrderStatusHistoryData) -> Result<Self> {
        let supplier_status_version = normalize_required_text(
            data.supplier_status_version,
            "供应商状态版本不能为空",
            STATUS_VERSION_MAX_LEN,
            "供应商状态版本过长",
        )?;
        let external_event_id = normalize_required_text(
            data.external_event_id,
            "外部事件ID不能为空",
            EXTERNAL_EVENT_ID_MAX_LEN,
            "外部事件ID过长",
        )?;
        if data.previous_status == data.new_status {
            return Err(Error::from("原状态与新状态不得相同"));
        }
        ensure_transition(data.previous_status, data.new_status)?;
        if data.received_at < data.occurred_at {
            return Err(Error::from("接收时间不得早于发生时间"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            connection_id: data.connection_id,
            previous_status: data.previous_status,
            new_status: data.new_status,
            supplier_status_version,
            occurred_at: data.occurred_at,
            received_at: data.received_at,
            external_event_id,
            source_type: data.source_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SupplierOrderStatusHistoryId;

    fn sample_data() -> SupplierOrderStatusHistoryData {
        SupplierOrderStatusHistoryData {
            connection_id: SupplierApiConnectionId::new("connection-1"),
            previous_status: FulfillmentStatus::Received,
            new_status: FulfillmentStatus::Submitting,
            supplier_status_version: " v5 ".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            received_at: Instant::from_unix_secs(1_700_000_100),
            external_event_id: " EVT-1001 ".to_string(),
            source_type: SourceType::SupplierCallback,
        }
    }

    #[test]
    fn new_accepts_valid_record_and_normalizes_fields() {
        let history =
            SupplierOrderStatusHistory::new(SupplierOrderStatusHistoryId::new("history-1"), sample_data())
                .unwrap();

        assert_eq!(history.supplier_status_version, "v5");
        assert_eq!(history.external_event_id, "EVT-1001");
        assert_eq!(history.previous_status, FulfillmentStatus::Received);
        assert_eq!(history.new_status, FulfillmentStatus::Submitting);
        assert_eq!(history.source_type, SourceType::SupplierCallback);
    }

    #[test]
    fn new_rejects_equal_previous_and_new_status() {
        let data = SupplierOrderStatusHistoryData {
            new_status: FulfillmentStatus::Received,
            ..sample_data()
        };
        assert!(
            SupplierOrderStatusHistory::new(SupplierOrderStatusHistoryId::new("history-2"), data).is_err()
        );
    }

    #[test]
    fn new_rejects_illegal_transition() {
        let data = SupplierOrderStatusHistoryData {
            previous_status: FulfillmentStatus::Received,
            new_status: FulfillmentStatus::Accepted,
            ..sample_data()
        };
        let error = SupplierOrderStatusHistory::new(SupplierOrderStatusHistoryId::new("history-3"), data)
            .unwrap_err();
        assert!(matches!(error, Error::InvalidStateTransition { .. }));

        let regression = SupplierOrderStatusHistoryData {
            previous_status: FulfillmentStatus::Completed,
            new_status: FulfillmentStatus::Fulfilling,
            ..sample_data()
        };
        assert!(
            SupplierOrderStatusHistory::new(SupplierOrderStatusHistoryId::new("history-4"), regression)
                .is_err(),
            "完成后的重复回调不得使状态倒退"
        );
    }

    #[test]
    fn new_rejects_received_before_occurred() {
        let data = SupplierOrderStatusHistoryData {
            received_at: Instant::from_unix_secs(1_699_999_900),
            ..sample_data()
        };
        assert!(
            SupplierOrderStatusHistory::new(SupplierOrderStatusHistoryId::new("history-5"), data).is_err()
        );
    }

    #[test]
    fn new_rejects_empty_or_overlong_fields() {
        let empty_version = SupplierOrderStatusHistoryData {
            supplier_status_version: "  ".to_string(),
            ..sample_data()
        };
        assert!(SupplierOrderStatusHistory::new(
            SupplierOrderStatusHistoryId::new("history-6"),
            empty_version
        )
        .is_err());

        let overlong_event_id = SupplierOrderStatusHistoryData {
            external_event_id: "e".repeat(129),
            ..sample_data()
        };
        assert!(SupplierOrderStatusHistory::new(
            SupplierOrderStatusHistoryId::new("history-7"),
            overlong_event_id
        )
        .is_err());
    }
}
