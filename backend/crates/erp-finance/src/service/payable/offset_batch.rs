//! 退款/冲正冲减块的批量分录与账户索引（SALES-R07）。
//!
//! 先收集去重分录 ID，批量读取分录后再批量读取账户。乱序结果按 ID 建索引；
//! 缺任一分录或账户失败关闭。逐账户原子 `revert_settlement`、任务同步和事务
//! 生命周期仍由 Service 编排。

use erp_core::ids::{PayableAccountId, PayableEntryId};
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::payable::{PayableAccount, PayableEntry};
use crate::repository::PayableExt;
pub use crate::service::receivable::receipt_reversal::OffsetFacts;
use crate::service::receivable::receipt_reversal::{
    index_required_by_id, unique_account_ids_for_entries, unique_ids_in_first_seen_order,
};
use crate::{Error, Result};

/// 批量读取应付分录及其账户，缺任一项失败关闭。
///
/// # 参数
/// * `db` - 数据库
/// * `entry_ids` - 冲减块引用的增加分录 ID；可含重复
/// * `executor` - 调用方执行器，须与写入共用事务快照
///
/// # 返回
/// 返回按 ID 索引的分录与账户。
///
/// # 错误
/// 缺任一分录或账户时返回 `NotFound`；仓储失败时传播基础设施错误。
///
/// # 约束
/// 固定两次批量读取，次数不随冲减块数量增长；不执行条件更新。
pub async fn load_payable_offset_facts(
    db: &Database,
    entry_ids: impl IntoIterator<Item = PayableEntryId>,
    executor: &mut dyn Executor,
) -> Result<OffsetFacts<PayableEntry, PayableAccount>> {
    let unique_entry_ids = unique_ids_in_first_seen_order(entry_ids.into_iter().map(|id| id.to_string()));
    let typed_entry_ids = unique_entry_ids.iter().cloned().map(PayableEntryId::new).collect::<Vec<_>>();
    let entries = db.payable_entries().find_entries_by_ids(&typed_entry_ids, executor).await?;
    let entries = index_required_by_id(
        entries,
        &unique_entry_ids,
        |entry| entry.base.id.clone(),
        |_| Error::NotFound("应付分录不存在".to_string()),
    )?;
    let unique_account_ids = unique_account_ids_for_entries(&entries, &unique_entry_ids, |entry| {
        entry.payable_account_id.to_string()
    });
    let typed_account_ids = unique_account_ids.iter().cloned().map(PayableAccountId::new).collect::<Vec<_>>();
    let accounts = db.payable_accounts().find_accounts_by_ids(&typed_account_ids, executor).await?;
    let accounts = index_required_by_id(
        accounts,
        &unique_account_ids,
        |account| account.base.id.clone(),
        |_| Error::NotFound("应付往来子账不存在".to_string()),
    )?;
    Ok(OffsetFacts { entries, accounts })
}

#[cfg(test)]
mod tests {
    use super::{index_required_by_id, unique_account_ids_for_entries, unique_ids_in_first_seen_order};
    use crate::Error;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Entry {
        id: String,
        account_id: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Account {
        id: String,
    }

    #[derive(Debug, Clone)]
    struct Chunk {
        entry_id: String,
    }

    fn assemble(
        chunks: &[Chunk],
        entries: Vec<Entry>,
        accounts: Vec<Account>,
    ) -> Result<super::OffsetFacts<Entry, Account>, Error> {
        let entry_ids = unique_ids_in_first_seen_order(chunks.iter().map(|chunk| chunk.entry_id.clone()));
        let entries = index_required_by_id(
            entries,
            &entry_ids,
            |entry| entry.id.clone(),
            |_| Error::NotFound("应收分录不存在".to_string()),
        )?;
        let account_ids =
            unique_account_ids_for_entries(&entries, &entry_ids, |entry| entry.account_id.clone());
        let accounts = index_required_by_id(
            accounts,
            &account_ids,
            |account| account.id.clone(),
            |_| Error::NotFound("应收往来子账不存在".to_string()),
        )?;
        Ok(super::OffsetFacts { entries, accounts })
    }

    #[test]
    fn unique_ids_drop_duplicates_and_preserve_first_seen_order() {
        assert_eq!(
            unique_ids_in_first_seen_order(["e2".into(), "e1".into(), "e2".into(), "e1".into()]),
            vec!["e2".to_string(), "e1".to_string()]
        );
        assert!(unique_ids_in_first_seen_order(Vec::<String>::new()).is_empty());
    }

    #[test]
    fn unordered_results_are_indexed_by_id() {
        let chunks = [Chunk { entry_id: "e1".into() }, Chunk { entry_id: "e2".into() }];
        let facts = assemble(
            &chunks,
            vec![
                Entry { id: "e2".into(), account_id: "a2".into() },
                Entry { id: "e1".into(), account_id: "a1".into() },
            ],
            vec![Account { id: "a2".into() }, Account { id: "a1".into() }],
        )
        .expect("乱序结果必须可索引");
        assert_eq!(facts.entries["e1"].account_id, "a1");
        assert_eq!(facts.entries["e2"].account_id, "a2");
        assert!(facts.accounts.contains_key("a1"));
        assert!(facts.accounts.contains_key("a2"));
    }

    #[test]
    fn duplicate_chunk_ids_load_entry_once_and_same_account_once() {
        let chunks = [Chunk { entry_id: "e1".into() }, Chunk { entry_id: "e1".into() }];
        let facts = assemble(
            &chunks,
            vec![Entry { id: "e1".into(), account_id: "a1".into() }],
            vec![Account { id: "a1".into() }],
        )
        .expect("重复分录 ID 必须成功");
        assert_eq!(facts.entries.len(), 1);
        assert_eq!(facts.accounts.len(), 1);
        assert_eq!(
            unique_ids_in_first_seen_order(chunks.iter().map(|chunk| chunk.entry_id.clone())).len(),
            1
        );
    }

    #[test]
    fn same_account_multiple_chunks_share_one_account() {
        let chunks = [Chunk { entry_id: "e1".into() }, Chunk { entry_id: "e2".into() }];
        let facts = assemble(
            &chunks,
            vec![
                Entry { id: "e1".into(), account_id: "a1".into() },
                Entry { id: "e2".into(), account_id: "a1".into() },
            ],
            vec![Account { id: "a1".into() }],
        )
        .expect("同账户多块必须成功");
        assert_eq!(facts.entries.len(), 2);
        assert_eq!(facts.accounts.len(), 1);
        assert_eq!(facts.entries["e1"].account_id, facts.entries["e2"].account_id);
    }

    #[test]
    fn cross_account_multiple_chunks_require_each_account() {
        let chunks = [Chunk { entry_id: "e1".into() }, Chunk { entry_id: "e2".into() }];
        let facts = assemble(
            &chunks,
            vec![
                Entry { id: "e1".into(), account_id: "a1".into() },
                Entry { id: "e2".into(), account_id: "a2".into() },
            ],
            vec![Account { id: "a1".into() }, Account { id: "a2".into() }],
        )
        .expect("跨账户多块必须成功");
        assert_eq!(facts.entries.len(), 2);
        assert_eq!(facts.accounts.len(), 2);
        assert_ne!(facts.entries["e1"].account_id, facts.entries["e2"].account_id);
    }

    #[test]
    fn extra_unordered_entries_do_not_expand_required_accounts() {
        let chunks = [Chunk { entry_id: "e1".into() }];
        let facts = assemble(
            &chunks,
            vec![
                Entry { id: "e-extra".into(), account_id: "a-extra".into() },
                Entry { id: "e1".into(), account_id: "a1".into() },
            ],
            vec![Account { id: "a1".into() }],
        )
        .expect("额外乱序分录不得导致缺账户失败");
        assert_eq!(facts.entries["e1"].account_id, "a1");
        assert_eq!(facts.accounts.len(), 1);
        assert!(facts.accounts.contains_key("a1"));
        assert!(!facts.accounts.contains_key("a-extra"));
    }

    #[test]
    fn missing_entry_or_account_fails_closed() {
        let chunks = [Chunk { entry_id: "e1".into() }];
        let missing_entry = assemble(&chunks, Vec::new(), vec![Account { id: "a1".into() }]);
        assert!(matches!(missing_entry, Err(Error::NotFound(_))));

        let missing_account =
            assemble(&chunks, vec![Entry { id: "e1".into(), account_id: "a1".into() }], Vec::new());
        assert!(matches!(missing_account, Err(Error::NotFound(_))));
    }
}
