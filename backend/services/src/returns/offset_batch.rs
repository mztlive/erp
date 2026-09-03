//! 退款/冲正冲减块的批量分录与账户索引（SALES-R07）。
//!
//! 先收集去重分录 ID，批量读取分录后再批量读取账户。乱序结果按 ID 建索引；
//! 缺任一分录或账户失败关闭。逐账户原子 `revert_settlement`、任务同步和事务
//! 生命周期仍由 Service 编排。

use std::collections::{HashMap, HashSet};

use database::{Executor, PayableExt, ReceivableExt};
use entities::ids::{PayableAccountId, PayableEntryId, ReceivableEntryId};
use entities::payable::{PayableAccount, PayableEntry};
use entities::receivable::{ReceivableAccount, ReceivableEntry};
use mongodb::Database;

use crate::errors::{Error, Result};

/// 冲减路径批量装载的分录与账户索引。
#[derive(Debug, Clone)]
pub struct OffsetFacts<Entry, Account> {
    /// 按分录主键索引。
    pub entries: HashMap<String, Entry>,
    /// 按账户主键索引。
    pub accounts: HashMap<String, Account>,
}

/// 去重 ID 并保留首次出现顺序。
///
/// # 参数
/// * `ids` - 可能含重复的 ID 序列
///
/// # 返回
/// 返回去重后的 ID 列表；空输入返回空向量。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 不去重业务结论；调用方负责解释缺项。
pub fn unique_ids_in_first_seen_order(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            unique.push(id);
        }
    }
    unique
}

/// 按主键将批量读取结果建索引，并确认每个请求 ID 都存在。
///
/// # 参数
/// * `items` - 仓储返回的乱序结果
/// * `required_ids` - 去重后的请求 ID
/// * `id_of` - 从结果取主键
/// * `missing` - 缺项时构造失败关闭错误
///
/// # 返回
/// 返回按主键索引的结果；重复结果保留首次。
///
/// # 错误
/// 任一请求 ID 缺失时返回 `missing` 给出的错误。
///
/// # 约束
/// 不解释业务规则，不写库。
pub fn index_required_by_id<T>(
    items: Vec<T>,
    required_ids: &[String],
    id_of: impl Fn(&T) -> String,
    missing: impl Fn(&str) -> Error,
) -> Result<HashMap<String, T>> {
    let mut index = HashMap::with_capacity(items.len());
    for item in items {
        index.entry(id_of(&item)).or_insert(item);
    }
    for id in required_ids {
        if !index.contains_key(id) {
            return Err(missing(id));
        }
    }
    Ok(index)
}

/// 按已确认分录 ID 收集去重账户 ID，保留分录首次出现顺序。
///
/// # 参数
/// * `entries` - 已通过缺项校验的分录索引
/// * `entry_ids` - 去重后的请求分录 ID
/// * `account_id_of` - 从分录取账户主键
///
/// # 返回
/// 返回去重账户 ID；空输入返回空向量。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 只解释已索引分录，不读取额外仓储结果。
pub fn unique_account_ids_for_entries<T>(
    entries: &HashMap<String, T>,
    entry_ids: &[String],
    account_id_of: impl Fn(&T) -> String,
) -> Vec<String> {
    unique_ids_in_first_seen_order(
        entry_ids
            .iter()
            .filter_map(|id| entries.get(id).map(&account_id_of)),
    )
}

/// 批量读取应收分录及其账户，缺任一项失败关闭。
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
pub async fn load_receivable_offset_facts(
    db: &Database,
    entry_ids: impl IntoIterator<Item = ReceivableEntryId>,
    executor: &mut dyn Executor,
) -> Result<OffsetFacts<ReceivableEntry, ReceivableAccount>> {
    let unique_entry_ids = unique_ids_in_first_seen_order(entry_ids.into_iter().map(|id| id.to_string()));
    let typed_entry_ids = unique_entry_ids
        .iter()
        .cloned()
        .map(ReceivableEntryId::new)
        .collect::<Vec<_>>();
    let entries = db
        .receivable_entries()
        .find_entries_by_ids(&typed_entry_ids, executor)
        .await?;
    let entries = index_required_by_id(
        entries,
        &unique_entry_ids,
        |entry| entry.base.id.clone(),
        |_| Error::NotFound("应收分录不存在".to_string()),
    )?;
    let unique_account_ids = unique_account_ids_for_entries(&entries, &unique_entry_ids, |entry| {
        entry.receivable_account_id.to_string()
    });
    let accounts = db
        .receivable_accounts()
        .find_accounts_by_ids(&unique_account_ids, executor)
        .await?;
    let accounts = index_required_by_id(
        accounts,
        &unique_account_ids,
        |account| account.base.id.clone(),
        |_| Error::NotFound("应收往来子账不存在".to_string()),
    )?;
    Ok(OffsetFacts { entries, accounts })
}

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
    let typed_entry_ids = unique_entry_ids
        .iter()
        .cloned()
        .map(PayableEntryId::new)
        .collect::<Vec<_>>();
    let entries = db
        .payable_entries()
        .find_entries_by_ids(&typed_entry_ids, executor)
        .await?;
    let entries = index_required_by_id(
        entries,
        &unique_entry_ids,
        |entry| entry.base.id.clone(),
        |_| Error::NotFound("应付分录不存在".to_string()),
    )?;
    let unique_account_ids = unique_account_ids_for_entries(&entries, &unique_entry_ids, |entry| {
        entry.payable_account_id.to_string()
    });
    let typed_account_ids = unique_account_ids
        .iter()
        .cloned()
        .map(PayableAccountId::new)
        .collect::<Vec<_>>();
    let accounts = db
        .payable_accounts()
        .find_accounts_by_ids(&typed_account_ids, executor)
        .await?;
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
    use crate::errors::Error;

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
        let chunks = [
            Chunk {
                entry_id: "e1".into(),
            },
            Chunk {
                entry_id: "e2".into(),
            },
        ];
        let facts = assemble(
            &chunks,
            vec![
                Entry {
                    id: "e2".into(),
                    account_id: "a2".into(),
                },
                Entry {
                    id: "e1".into(),
                    account_id: "a1".into(),
                },
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
        let chunks = [
            Chunk {
                entry_id: "e1".into(),
            },
            Chunk {
                entry_id: "e1".into(),
            },
        ];
        let facts = assemble(
            &chunks,
            vec![Entry {
                id: "e1".into(),
                account_id: "a1".into(),
            }],
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
        let chunks = [
            Chunk {
                entry_id: "e1".into(),
            },
            Chunk {
                entry_id: "e2".into(),
            },
        ];
        let facts = assemble(
            &chunks,
            vec![
                Entry {
                    id: "e1".into(),
                    account_id: "a1".into(),
                },
                Entry {
                    id: "e2".into(),
                    account_id: "a1".into(),
                },
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
        let chunks = [
            Chunk {
                entry_id: "e1".into(),
            },
            Chunk {
                entry_id: "e2".into(),
            },
        ];
        let facts = assemble(
            &chunks,
            vec![
                Entry {
                    id: "e1".into(),
                    account_id: "a1".into(),
                },
                Entry {
                    id: "e2".into(),
                    account_id: "a2".into(),
                },
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
        let chunks = [Chunk {
            entry_id: "e1".into(),
        }];
        let facts = assemble(
            &chunks,
            vec![
                Entry {
                    id: "e-extra".into(),
                    account_id: "a-extra".into(),
                },
                Entry {
                    id: "e1".into(),
                    account_id: "a1".into(),
                },
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
        let chunks = [Chunk {
            entry_id: "e1".into(),
        }];
        let missing_entry = assemble(&chunks, Vec::new(), vec![Account { id: "a1".into() }]);
        assert!(matches!(missing_entry, Err(Error::NotFound(_))));

        let missing_account = assemble(
            &chunks,
            vec![Entry {
                id: "e1".into(),
                account_id: "a1".into(),
            }],
            Vec::new(),
        );
        assert!(matches!(missing_account, Err(Error::NotFound(_))));
    }
}
