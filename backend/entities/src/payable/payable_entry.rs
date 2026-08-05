//! `payable_entry` 应付分录（数据模型 §6.9）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::{PayableAccountId, PayableEntryId};
use crate::money::Amount;
use crate::validation::normalize_required_text;

/// 来源事实类型最大长度。
const FACT_TYPE_MAX_LEN: usize = 64;
/// 来源单据 ID 最大长度。
const DOCUMENT_ID_MAX_LEN: usize = 128;
/// 来源修订 ID 最大长度。
const REVISION_ID_MAX_LEN: usize = 128;

/// 应付分录类型（数据模型 §6.9：原始应付、变更差额、供应商退款、冲正、结算差额）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PayableEntryType {
    /// 原始应付。
    Original,
    /// 变更差额。
    ChangeDelta,
    /// 供应商退款。
    SupplierRefund,
    /// 冲正。
    Reversal,
    /// 结算差额。
    SettlementDelta,
}

impl PayableEntryType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Original => "原始应付",
            Self::ChangeDelta => "变更差额",
            Self::SupplierRefund => "供应商退款",
            Self::Reversal => "冲正",
            Self::SettlementDelta => "结算差额",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::ChangeDelta => "change_delta",
            Self::SupplierRefund => "supplier_refund",
            Self::Reversal => "reversal",
            Self::SettlementDelta => "settlement_delta",
        }
    }
}

/// 分录方向（数据模型 §6.9：增加或减少）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryDirection {
    /// 增加（正向应付）。
    Increase,
    /// 减少（冲减应付）。
    Decrease,
}

impl EntryDirection {
    /// 返回方向的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Increase => "增加",
            Self::Decrease => "减少",
        }
    }

    /// 返回方向的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Increase => "increase",
            Self::Decrease => "decrease",
        }
    }
}

/// 应付分录创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayableEntryData {
    /// 应付往来子账。
    pub payable_account_id: PayableAccountId,
    /// 分录类型。
    pub entry_type: PayableEntryType,
    /// 分录方向。
    pub direction: EntryDirection,
    /// 正数含税金额。
    pub amount: Amount,
    /// 到期日。
    pub due_date: BusinessDate,
    /// 来源事实类型。
    pub source_fact_type: String,
    /// 来源单据 ID（子账来源类型为采购单时是采购单，否则是结算单）。
    pub source_document_id: String,
    /// 来源修订 ID。
    pub source_revision_id: String,
    /// 来源内序号。
    pub source_sequence: u32,
    /// 入账时间。
    pub posted_at: Instant,
}

/// 应付分录实体（正式事实，数据模型 §6.9）。
///
/// 金额一律为正数含税金额，方向由 `direction` 表达；
/// `(payable_account_id, source_fact_type, source_document_id, source_revision_id,
/// entry_type, source_sequence)` 业务幂等唯一由唯一索引保证。
/// 正式事实过账后不可更新或删除（§4.5），纠错追加反向分录。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PayableEntry {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 应付往来子账。
    pub payable_account_id: PayableAccountId,
    /// 分录类型。
    pub entry_type: PayableEntryType,
    /// 分录方向。
    pub direction: EntryDirection,
    /// 正数含税金额。
    pub amount: Amount,
    /// 到期日。
    pub due_date: BusinessDate,
    /// 来源事实类型。
    pub source_fact_type: String,
    /// 来源单据 ID。
    pub source_document_id: String,
    /// 来源修订 ID。
    pub source_revision_id: String,
    /// 来源内序号。
    pub source_sequence: u32,
    /// 入账时间。
    pub posted_at: Instant,
}

impl PayableEntry {
    /// 创建应付分录。
    ///
    /// 完成来源文本的 trim/非空/长度校验、金额正数校验与「类型 ↔ 方向」一致性
    /// 校验（原始应付必须是增加，供应商退款/冲正是减少，差额类双向均可）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PayableEntryId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分录实体。
    ///
    /// # 错误
    /// 当来源字段为空/超长、金额非正、来源序号为 0 或类型与方向矛盾时返回错误。
    #[allow(clippy::too_many_arguments)]
    pub fn new(id: PayableEntryId, data: PayableEntryData) -> Result<Self> {
        let source_fact_type = normalize_required_text(
            data.source_fact_type,
            "来源事实类型不能为空",
            FACT_TYPE_MAX_LEN,
            "来源事实类型过长",
        )?;
        let source_document_id = normalize_required_text(
            data.source_document_id,
            "来源单据ID不能为空",
            DOCUMENT_ID_MAX_LEN,
            "来源单据ID过长",
        )?;
        let source_revision_id = normalize_required_text(
            data.source_revision_id,
            "来源修订ID不能为空",
            REVISION_ID_MAX_LEN,
            "来源修订ID过长",
        )?;
        if data.amount.to_decimal().is_sign_negative() || data.amount.to_decimal().is_zero() {
            return Err(Error::from("应付分录金额必须为正数"));
        }
        if data.source_sequence == 0 {
            return Err(Error::from("来源内序号必须从 1 开始"));
        }
        validate_direction_consistency(data.entry_type, data.direction)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            payable_account_id: data.payable_account_id,
            entry_type: data.entry_type,
            direction: data.direction,
            amount: data.amount,
            due_date: data.due_date,
            source_fact_type,
            source_document_id,
            source_revision_id,
            source_sequence: data.source_sequence,
            posted_at: data.posted_at,
        })
    }

    /// 更新应付分录。
    ///
    /// 正式事实过账后不可更新（数据模型 §4.5、§6.9「不改写已确认应付」），
    /// 任何字段的修改都被拒绝，纠错必须追加反向分录。
    ///
    /// # 参数
    /// * `update` - 更新数据（恒被拒绝）
    /// * `updated_by` - 更新执行人（恒被拒绝）
    ///
    /// # 返回
    /// 恒返回错误。
    pub fn update(&mut self, update: PayableEntryData, updated_by: impl Into<String>) -> Result<()> {
        let _ = (update, updated_by);
        Err(Error::from("正式事实过账后不可更新，纠错请追加反向分录"))
    }
}

/// 校验分录类型与方向的一致性。
///
/// 规则（数据模型 §6.9）：原始应付形成正向增加；供应商退款、冲正是冲减类，
/// 必须是减少；变更差额与结算差额根据方向可增可减。
///
/// # 参数
/// * `entry_type` - 分录类型
/// * `direction` - 分录方向
///
/// # 返回
/// 一致返回 `Ok(())`。
///
/// # 错误
/// 类型与方向矛盾时返回错误。
fn validate_direction_consistency(entry_type: PayableEntryType, direction: EntryDirection) -> Result<()> {
    let fixed = match entry_type {
        PayableEntryType::Original => Some(EntryDirection::Increase),
        PayableEntryType::SupplierRefund | PayableEntryType::Reversal => Some(EntryDirection::Decrease),
        PayableEntryType::ChangeDelta | PayableEntryType::SettlementDelta => None,
    };
    if let Some(fixed) = fixed {
        if direction != fixed {
            return Err(Error::from(format!(
                "{} 分录方向必须为 {}",
                entry_type.label(),
                fixed.label()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn data() -> PayableEntryData {
        PayableEntryData {
            payable_account_id: PayableAccountId::new("pa-1"),
            entry_type: PayableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: Amount::from_str("1000.00").unwrap(),
            due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
            source_fact_type: " PURCHASE_ORDER ".to_string(),
            source_document_id: " PO-1 ".to_string(),
            source_revision_id: " PO-1-R1 ".to_string(),
            source_sequence: 1,
            posted_at: Instant::from_unix_secs(1_700_000_000),
        }
    }

    #[test]
    fn new_trims_and_normalizes_source_fields() {
        let entry = PayableEntry::new(PayableEntryId::new("pe-1"), data()).unwrap();

        assert_eq!(entry.source_fact_type, "PURCHASE_ORDER");
        assert_eq!(entry.source_document_id, "PO-1");
        assert_eq!(entry.source_revision_id, "PO-1-R1");
        assert_eq!(entry.direction, EntryDirection::Increase);
    }

    #[test]
    fn new_rejects_blank_overlong_and_negative() {
        let blank = PayableEntryData {
            source_document_id: "   ".to_string(),
            ..data()
        };
        assert!(PayableEntry::new(PayableEntryId::new("pe-2"), blank).is_err());

        let overlong = PayableEntryData {
            source_revision_id: "r".repeat(129),
            ..data()
        };
        assert!(PayableEntry::new(PayableEntryId::new("pe-3"), overlong).is_err());

        let non_positive = PayableEntryData {
            amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(PayableEntry::new(PayableEntryId::new("pe-4"), non_positive).is_err());

        let zero_seq = PayableEntryData {
            source_sequence: 0,
            ..data()
        };
        assert!(PayableEntry::new(PayableEntryId::new("pe-5"), zero_seq).is_err());
    }

    #[test]
    fn new_rejects_direction_type_mismatch() {
        let refund_increase = PayableEntryData {
            entry_type: PayableEntryType::SupplierRefund,
            direction: EntryDirection::Increase,
            ..data()
        };
        assert!(PayableEntry::new(PayableEntryId::new("pe-6"), refund_increase).is_err());

        let original_decrease = PayableEntryData {
            entry_type: PayableEntryType::Original,
            direction: EntryDirection::Decrease,
            ..data()
        };
        assert!(PayableEntry::new(PayableEntryId::new("pe-7"), original_decrease).is_err());

        let delta_both = PayableEntryData {
            entry_type: PayableEntryType::SettlementDelta,
            direction: EntryDirection::Decrease,
            ..data()
        };
        assert!(PayableEntry::new(PayableEntryId::new("pe-8"), delta_both).is_ok());
    }

    #[test]
    fn formal_fact_rejects_update() {
        let mut entry = PayableEntry::new(PayableEntryId::new("pe-1"), data()).unwrap();
        assert!(entry.update(data(), "admin-2").is_err());
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&PayableEntryType::SettlementDelta).unwrap(),
            "\"settlement_delta\""
        );
        assert_eq!(
            serde_json::to_string(&EntryDirection::Decrease).unwrap(),
            "\"decrease\""
        );
        assert_eq!(PayableEntryType::SupplierRefund.label(), "供应商退款");
        assert_eq!(PayableEntryType::Reversal.as_str(), "reversal");
        assert_eq!(EntryDirection::Increase.label(), "增加");
    }
}
