//! `mall_balance_snapshot`：商城卡实例余额快照（数据模型 §6.17）。
//!
//! 余额快照是不可变事实（§4.5），只提供 `new()`。`(mall_card_instance_id, snapshot_at)`
//! 是业务唯一键，`source_event_id` 另作消息层去重，两者均由 P2 唯一索引落实。
//! 快照实体只保存卡实例 ID 与余额，不含卡号、卡密或绑定手机号（§4.5.6）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::MallBalanceSnapshotId;
use crate::ids::MallCardInstanceId;
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 来源事件 ID 最大长度。
const EVENT_ID_MAX_LEN: usize = 256;
/// 来源快照版本最大长度。
const SNAPSHOT_VERSION_MAX_LEN: usize = 64;

/// 余额快照创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallBalanceSnapshotData {
    /// 卡实例。
    pub mall_card_instance_id: MallCardInstanceId,
    /// 快照时间。
    pub snapshot_at: Instant,
    /// 商城当时有效余额。
    pub balance: Amount,
    /// 商城余额快照版本，可空。
    pub source_snapshot_version: Option<String>,
    /// 必填来源消息事件 ID。
    pub source_event_id: String,
}

/// 余额快照实体（数据模型 §6.17）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallBalanceSnapshot {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 卡实例。
    pub mall_card_instance_id: MallCardInstanceId,
    /// 快照时间。
    pub snapshot_at: Instant,
    /// 商城当时有效余额。
    pub balance: Amount,
    /// 商城余额快照版本，可空。
    pub source_snapshot_version: Option<String>,
    /// 来源消息事件 ID。
    pub source_event_id: String,
}

impl MallBalanceSnapshot {
    /// 创建余额快照。
    ///
    /// 完成文本字段校验与规范化；`balance` 必须非负。快照是不可变事实，
    /// 只提供 `new()`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallBalanceSnapshotId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的余额快照实体。
    ///
    /// # 错误
    /// 当事件 ID 为空/超长，或余额为负时返回错误。
    pub fn new(id: MallBalanceSnapshotId, data: MallBalanceSnapshotData) -> Result<Self> {
        let source_event_id = normalize_required_text(
            data.source_event_id,
            "来源事件ID不能为空",
            EVENT_ID_MAX_LEN,
            "来源事件ID过长",
        )?;
        let source_snapshot_version = normalize_optional_text(
            data.source_snapshot_version,
            "来源快照版本",
            SNAPSHOT_VERSION_MAX_LEN,
        )?;
        if data.balance.to_decimal().is_sign_negative() {
            return Err(Error::from("余额不能为负"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_card_instance_id: data.mall_card_instance_id,
            snapshot_at: data.snapshot_at,
            balance: data.balance,
            source_snapshot_version,
            source_event_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{MallBalanceSnapshot, MallBalanceSnapshotData};
    use crate::common::time::Instant;
    use crate::ids::{MallBalanceSnapshotId, MallCardInstanceId};
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> MallBalanceSnapshotData {
        MallBalanceSnapshotData {
            mall_card_instance_id: MallCardInstanceId::new("card-1"),
            snapshot_at: Instant::from_unix_secs(1_700_000_000),
            balance: Amount::from_str("88.00").unwrap(),
            source_snapshot_version: Some(" v7 ".to_string()),
            source_event_id: " evt-001 ".to_string(),
        }
    }

    /// happy path：字段规范化，余额与来源身份落库。
    #[test]
    fn new_trims_fields_and_keeps_balance() {
        let snapshot = MallBalanceSnapshot::new(MallBalanceSnapshotId::new("snap-1"), data()).unwrap();

        assert_eq!(snapshot.source_event_id, "evt-001");
        assert_eq!(snapshot.source_snapshot_version.as_deref(), Some("v7"));
        assert_eq!(snapshot.balance, Amount::from_str("88.00").unwrap());
        assert_eq!(snapshot.mall_card_instance_id, MallCardInstanceId::new("card-1"));
        assert_eq!(snapshot.snapshot_at, Instant::from_unix_secs(1_700_000_000));
    }

    /// 失败路径：必填空、超长、负余额。
    #[test]
    fn new_rejects_empty_overlong_and_negative_balance() {
        let empty = MallBalanceSnapshotData {
            source_event_id: "  ".to_string(),
            ..data()
        };
        assert!(MallBalanceSnapshot::new(MallBalanceSnapshotId::new("s2"), empty).is_err());

        let overlong = MallBalanceSnapshotData {
            source_event_id: "e".repeat(257),
            ..data()
        };
        assert!(MallBalanceSnapshot::new(MallBalanceSnapshotId::new("s3"), overlong).is_err());

        let negative = MallBalanceSnapshotData {
            balance: Amount::from_str("-1.00").unwrap(),
            ..data()
        };
        assert!(MallBalanceSnapshot::new(MallBalanceSnapshotId::new("s4"), negative).is_err());
    }

    /// 敏感字段（§4.5.6）：快照字段清单不含卡号、卡密、手机号。
    #[test]
    fn entity_does_not_hold_forbidden_card_fields() {
        let snapshot = MallBalanceSnapshot::new(MallBalanceSnapshotId::new("snap-1"), data()).unwrap();
        let value = serde_json::to_value(&snapshot).unwrap();
        let keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        let forbidden = [
            "card_no",
            "card_number",
            "card_secret",
            "card_password",
            "phone",
            "mobile",
            "bound_phone",
        ];
        for key in forbidden {
            assert!(!keys.contains(&key), "余额快照不得包含字段 {key}");
        }
    }
}
