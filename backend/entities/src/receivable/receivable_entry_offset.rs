//! `receivable_entry_offset` 应收分录抵销（数据模型 §6.8）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{ReceivableEntryId, ReceivableEntryOffsetId};
use crate::money::Amount;

/// 应收分录抵销创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceivableEntryOffsetData {
    /// 同一往来子账内的减少分录。
    pub decrease_entry_id: ReceivableEntryId,
    /// 同一往来子账内被冲减的增加分录。
    pub increase_entry_id: ReceivableEntryId,
    /// 减少分录内序号（从 1 递增）。
    pub offset_sequence: u32,
    /// 正数冲减金额。
    pub offset_amount: Amount,
}

/// 应收分录抵销实体（正式事实，数据模型 §6.8）。
///
/// 显式表达一笔减少分录对一笔增加分录的冲减；`(decrease_entry_id,
/// offset_sequence)` 唯一。减少分录分配合计等于其金额、每笔增加分录累计净冲减
/// 不超过原增加金额是跨分录约束，由 P3 过账事务校验（§8.3）；实体层只保证
/// 金额正数与序列号合法性。过账后不可更新或删除。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ReceivableEntryOffset {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 减少分录。
    pub decrease_entry_id: ReceivableEntryId,
    /// 被冲减的增加分录。
    pub increase_entry_id: ReceivableEntryId,
    /// 减少分录内序号。
    pub offset_sequence: u32,
    /// 正数冲减金额。
    pub offset_amount: Amount,
}

impl ReceivableEntryOffset {
    /// 创建应收分录抵销。
    ///
    /// 完成金额正数、序号从 1 起与「减少分录不得冲减自身」校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ReceivableEntryOffsetId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的抵销实体。
    ///
    /// # 错误
    /// 当冲减金额非正、序号为 0 或两端为同一分录时返回错误。
    pub fn new(id: ReceivableEntryOffsetId, data: ReceivableEntryOffsetData) -> Result<Self> {
        if data.offset_amount.to_decimal().is_sign_negative() || data.offset_amount.to_decimal().is_zero() {
            return Err(Error::from("冲减金额必须为正数"));
        }
        if data.offset_sequence == 0 {
            return Err(Error::from("抵销序号必须从 1 开始"));
        }
        if data.decrease_entry_id == data.increase_entry_id {
            return Err(Error::from("减少分录不能冲减自身"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            decrease_entry_id: data.decrease_entry_id,
            increase_entry_id: data.increase_entry_id,
            offset_sequence: data.offset_sequence,
            offset_amount: data.offset_amount,
        })
    }

    /// 更新应收分录抵销。
    ///
    /// 正式事实过账后不可更新或删除（数据模型 §6.9 末尾），任何修改都被拒绝。
    ///
    /// # 参数
    /// * `update` - 更新数据（恒被拒绝）
    /// * `updated_by` - 更新执行人（恒被拒绝）
    ///
    /// # 返回
    /// 恒返回错误。
    pub fn update(&mut self, update: ReceivableEntryOffsetData, updated_by: impl Into<String>) -> Result<()> {
        let _ = (update, updated_by);
        Err(Error::from("正式事实过账后不可更新或删除"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn data() -> ReceivableEntryOffsetData {
        ReceivableEntryOffsetData {
            decrease_entry_id: ReceivableEntryId::new("re-2"),
            increase_entry_id: ReceivableEntryId::new("re-1"),
            offset_sequence: 1,
            offset_amount: Amount::from_str("100.00").unwrap(),
        }
    }

    #[test]
    fn new_accepts_valid_offset() {
        let offset = ReceivableEntryOffset::new(ReceivableEntryOffsetId::new("oe-1"), data()).unwrap();
        assert_eq!(offset.decrease_entry_id, ReceivableEntryId::new("re-2"));
        assert_eq!(offset.offset_amount, Amount::from_str("100.00").unwrap());
    }

    #[test]
    fn new_rejects_non_positive_and_self_offset() {
        let non_positive = ReceivableEntryOffsetData {
            offset_amount: Amount::from_str("-1.00").unwrap(),
            ..data()
        };
        assert!(ReceivableEntryOffset::new(ReceivableEntryOffsetId::new("oe-2"), non_positive).is_err());

        let zero_seq = ReceivableEntryOffsetData {
            offset_sequence: 0,
            ..data()
        };
        assert!(ReceivableEntryOffset::new(ReceivableEntryOffsetId::new("oe-3"), zero_seq).is_err());

        let self_offset = ReceivableEntryOffsetData {
            increase_entry_id: ReceivableEntryId::new("re-2"),
            ..data()
        };
        assert!(ReceivableEntryOffset::new(ReceivableEntryOffsetId::new("oe-4"), self_offset).is_err());
    }

    #[test]
    fn formal_fact_rejects_update() {
        let mut offset = ReceivableEntryOffset::new(ReceivableEntryOffsetId::new("oe-1"), data()).unwrap();
        assert!(offset.update(data(), "admin-2").is_err());
    }
}
