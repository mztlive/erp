//! 账户明细合并近期记录和必须办理的历史记录，保留稳定身份及确定排序。

use std::collections::BTreeMap;

use entity_core::HasBaseModel;

use crate::{Error, Result};

/// 为同一授权账户合并近期与全部待处理记录，不按近期窗口截断历史事项。
/// # 参数
/// `recent` 是最近一百项，`required` 是拥有仓储完整读取的最多一千项待处理记录。
/// # 返回
/// 返回按创建时间和稳定身份倒序、相同身份仅保留一次的完整列表。
/// # 错误
/// 输入超限或同一身份读取到冲突版本时拒绝，不能交付不完整资金明细。
pub(super) fn merge_recent<T: HasBaseModel + PartialEq>(recent: Vec<T>, required: Vec<T>) -> Result<Vec<T>> {
    if recent.len() > 100 || required.len() > 1000 {
        return Err(Error::BusinessLogicError("账户待处理资金记录超过读取上限，请先分批核实原记录".into()));
    }
    let mut records = BTreeMap::new();
    for value in recent.into_iter().chain(required) {
        let id = value.base().id.clone();
        if records.get(&id).is_some_and(|previous| previous != &value) {
            return Err(Error::ConflictError("资金明细在读取期间发生变化，请刷新账户后重试".into()));
        }
        records.insert(id, value);
    }
    let mut records = records.into_values().collect::<Vec<_>>();
    records.sort_by(|left, right| {
        right
            .base()
            .created_at
            .cmp(&left.base().created_at)
            .then_with(|| right.base().id.cmp(&left.base().id))
    });
    Ok(records)
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Record(BaseModel);

    impl HasBaseModel for Record {
        fn base(&self) -> &BaseModel {
            &self.0
        }

        fn base_mut(&mut self) -> &mut BaseModel {
            &mut self.0
        }
    }

    fn record(id: &str, created_at: u64) -> Record {
        Record(BaseModel { id: id.into(), created_at, ..BaseModel::fake() })
    }

    #[test]
    fn historical_required_records_survive_recent_window_and_duplicates_merge_once() {
        let recent = (0..100).map(|id| record(&format!("recent-{id:03}"), 100)).collect::<Vec<_>>();
        let duplicate = recent[0].clone();
        let records = merge_recent(recent, vec![record("old-unresolved", 1), duplicate]).unwrap();
        assert_eq!(records.len(), 101);
        assert_eq!(records.last().unwrap().base().id, "old-unresolved");
        assert_eq!(records.iter().filter(|value| value.base().id == "recent-000").count(), 1);
        assert_eq!(records.first().unwrap().base().id, "recent-099");
    }

    #[test]
    fn complete_required_set_is_not_truncated_and_over_limit_is_explicit() {
        let required = (0..1000).map(|id| record(&format!("pending-{id:04}"), 1)).collect::<Vec<_>>();
        let records = merge_recent(vec![record("latest", 2)], required.clone()).unwrap();
        assert_eq!(records.len(), 1001);
        let mut too_many = required;
        too_many.push(record("one-more", 1));
        assert!(matches!(merge_recent(vec![], too_many), Err(Error::BusinessLogicError(_))));
    }

    #[test]
    fn conflicting_version_is_rejected_and_empty_account_stays_empty() {
        let original = record("original", 1);
        let mut changed = original.clone();
        changed.0.version += 1;
        assert!(matches!(merge_recent(vec![original], vec![changed]), Err(Error::ConflictError(_))));
        assert!(merge_recent::<Record>(vec![], vec![]).unwrap().is_empty());
    }
}
