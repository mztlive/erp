//! `receivable_entry` 应收分录（数据模型 §6.8）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::{ReceivableAccountId, ReceivableEntryId};
use crate::money::Amount;
use crate::validation::normalize_required_text;

/// 来源事实类型最大长度。
const FACT_TYPE_MAX_LEN: usize = 64;
/// 来源单据 ID 最大长度。
const DOCUMENT_ID_MAX_LEN: usize = 128;
/// 来源修订 ID 最大长度。
const REVISION_ID_MAX_LEN: usize = 128;

/// 应收分录类型（数据模型 §6.8：原始应收、销售变更差额、作废冲减、退款、冲正）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceivableEntryType {
    /// 原始应收。
    Original,
    /// 销售变更差额。
    SalesChangeDelta,
    /// 作废冲减。
    VoidReduction,
    /// 退款。
    Refund,
    /// 冲正。
    Reversal,
}

impl ReceivableEntryType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Original => "原始应收",
            Self::SalesChangeDelta => "销售变更差额",
            Self::VoidReduction => "作废冲减",
            Self::Refund => "退款",
            Self::Reversal => "冲正",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::SalesChangeDelta => "sales_change_delta",
            Self::VoidReduction => "void_reduction",
            Self::Refund => "refund",
            Self::Reversal => "reversal",
        }
    }
}

/// 分录方向（数据模型 §6.8：增加或减少）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryDirection {
    /// 增加（正向应收）。
    Increase,
    /// 减少（冲减应收）。
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

/// 应收分录创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceivableEntryData {
    /// 应收往来子账。
    pub receivable_account_id: ReceivableAccountId,
    /// 分录类型。
    pub entry_type: ReceivableEntryType,
    /// 分录方向。
    pub direction: EntryDirection,
    /// 正数含税金额。
    pub amount: Amount,
    /// 到期日。
    pub due_date: BusinessDate,
    /// 唯一业务来源：来源事实类型。
    pub source_fact_type: String,
    /// 唯一业务来源：来源单据 ID。
    pub source_document_id: String,
    /// 唯一业务来源：来源修订 ID。
    pub source_revision_id: String,
    /// 唯一业务来源：来源内序号。
    pub source_sequence: u32,
    /// 入账时间。
    pub posted_at: Instant,
}

/// 应收分录实体（正式事实，数据模型 §6.8）。
///
/// 金额一律为正数含税金额，方向由 `direction` 表达；
/// `(receivable_account_id, source_fact_type, source_document_id, source_revision_id,
/// entry_type, source_sequence)` 业务幂等唯一由唯一索引保证。
/// 正式事实过账后不可更新或删除（§4.5），纠错追加反向分录。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ReceivableEntry {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 应收往来子账。
    pub receivable_account_id: ReceivableAccountId,
    /// 分录类型。
    pub entry_type: ReceivableEntryType,
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

impl ReceivableEntry {
    /// 创建应收分录。
    ///
    /// 完成来源文本的 trim/非空/长度校验、金额正数校验与「类型 ↔ 方向」一致性校验
    /// （原始应收必须是增加，冲减类分录必须是减少，销售变更差额双向均可）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ReceivableEntryId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分录实体。
    ///
    /// # 错误
    /// 当来源字段为空/超长、金额非正、来源序号为 0 或类型与方向矛盾时返回错误。
    pub fn new(id: ReceivableEntryId, data: ReceivableEntryData) -> Result<Self> {
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
            return Err(Error::from("应收分录金额必须为正数"));
        }
        if data.source_sequence == 0 {
            return Err(Error::from("来源内序号必须从 1 开始"));
        }
        validate_direction_consistency(data.entry_type, data.direction)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            receivable_account_id: data.receivable_account_id,
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

    /// 更新应收分录。
    ///
    /// 正式事实过账后不可更新（数据模型 §4.5、§6.8「不修改原应收」），
    /// 任何字段的修改都被拒绝，纠错必须追加反向分录。
    ///
    /// # 参数
    /// * `update` - 更新数据（恒被拒绝）
    /// * `updated_by` - 更新执行人（恒被拒绝）
    ///
    /// # 返回
    /// 恒返回错误。
    pub fn update(&mut self, update: ReceivableEntryData, updated_by: impl Into<String>) -> Result<()> {
        let _ = (update, updated_by);
        Err(Error::from("正式事实过账后不可更新，纠错请追加反向分录"))
    }
}

/// 校验分录类型与方向的一致性。
///
/// 规则（数据模型 §6.8）：原始应收形成正向增加；作废冲减、退款、冲正是冲减类，
/// 必须是减少；销售变更差额根据变更方向可增可减。
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
fn validate_direction_consistency(entry_type: ReceivableEntryType, direction: EntryDirection) -> Result<()> {
    let fixed = match entry_type {
        ReceivableEntryType::Original => Some(EntryDirection::Increase),
        ReceivableEntryType::VoidReduction | ReceivableEntryType::Refund | ReceivableEntryType::Reversal => {
            Some(EntryDirection::Decrease)
        }
        ReceivableEntryType::SalesChangeDelta => None,
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

    fn data() -> ReceivableEntryData {
        ReceivableEntryData {
            receivable_account_id: ReceivableAccountId::new("ra-1"),
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: Amount::from_str("1000.00").unwrap(),
            due_date: BusinessDate::from_ymd(2026, 9, 30).unwrap(),
            source_fact_type: " SALES_ORDER ".to_string(),
            source_document_id: " SO-1 ".to_string(),
            source_revision_id: " SO-1-R1 ".to_string(),
            source_sequence: 1,
            posted_at: Instant::from_unix_secs(1_700_000_000),
        }
    }

    #[test]
    fn new_trims_and_normalizes_source_fields() {
        let entry = ReceivableEntry::new(ReceivableEntryId::new("re-1"), data()).unwrap();

        assert_eq!(entry.source_fact_type, "SALES_ORDER");
        assert_eq!(entry.source_document_id, "SO-1");
        assert_eq!(entry.source_revision_id, "SO-1-R1");
        assert_eq!(entry.direction, EntryDirection::Increase);
        assert_eq!(entry.amount, Amount::from_str("1000.00").unwrap());
    }

    #[test]
    fn new_rejects_blank_overlong_and_negative() {
        let blank_doc = ReceivableEntryData {
            source_document_id: "   ".to_string(),
            ..data()
        };
        assert!(ReceivableEntry::new(ReceivableEntryId::new("re-2"), blank_doc).is_err());

        let overlong = ReceivableEntryData {
            source_revision_id: "r".repeat(129),
            ..data()
        };
        assert!(ReceivableEntry::new(ReceivableEntryId::new("re-3"), overlong).is_err());

        let non_positive = ReceivableEntryData {
            amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(ReceivableEntry::new(ReceivableEntryId::new("re-4"), non_positive).is_err());

        let zero_seq = ReceivableEntryData {
            source_sequence: 0,
            ..data()
        };
        assert!(ReceivableEntry::new(ReceivableEntryId::new("re-5"), zero_seq).is_err());
    }

    #[test]
    fn new_rejects_direction_type_mismatch() {
        let refund_increase = ReceivableEntryData {
            entry_type: ReceivableEntryType::Refund,
            direction: EntryDirection::Increase,
            ..data()
        };
        assert!(ReceivableEntry::new(ReceivableEntryId::new("re-6"), refund_increase).is_err());

        let original_decrease = ReceivableEntryData {
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Decrease,
            ..data()
        };
        assert!(ReceivableEntry::new(ReceivableEntryId::new("re-7"), original_decrease).is_err());

        let delta_both = ReceivableEntryData {
            entry_type: ReceivableEntryType::SalesChangeDelta,
            direction: EntryDirection::Decrease,
            ..data()
        };
        assert!(ReceivableEntry::new(ReceivableEntryId::new("re-8"), delta_both).is_ok());
    }

    #[test]
    fn formal_fact_rejects_update() {
        let mut entry = ReceivableEntry::new(ReceivableEntryId::new("re-1"), data()).unwrap();
        assert!(entry.update(data(), "admin-2").is_err());
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&ReceivableEntryType::VoidReduction).unwrap(),
            "\"void_reduction\""
        );
        assert_eq!(
            serde_json::to_string(&EntryDirection::Increase).unwrap(),
            "\"increase\""
        );
        assert_eq!(ReceivableEntryType::SalesChangeDelta.label(), "销售变更差额");
        assert_eq!(EntryDirection::Decrease.label(), "减少");
        assert_eq!(ReceivableEntryType::Reversal.as_str(), "reversal");
    }
}
