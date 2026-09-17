//! 应收与应付共用的批量分录/账户索引 helper（FIN-SHARED）。
//!
//! 原定义寄宿于应收 `receipt_reversal`，应付 `offset_batch` 反向复用；现下沉
//! 到 service 直属共享模块，应收与应付各自只依赖本层。行为与读取次数不变。

use std::collections::{HashMap, HashSet};

/// 按标识索引的冲减分录与账户事实；读取方解释具体业务缺项。
#[derive(Debug, Clone)]
pub struct OffsetFacts<Entry, Account> {
    /// 分录主键索引。
    pub entries: HashMap<String, Entry>,
    /// 账户主键索引。
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
pub fn index_required_by_id<T, E>(
    items: Vec<T>,
    required_ids: &[String],
    id_of: impl Fn(&T) -> String,
    missing: impl Fn(&str) -> E,
) -> std::result::Result<HashMap<String, T>, E> {
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
    unique_ids_in_first_seen_order(entry_ids.iter().filter_map(|id| entries.get(id).map(&account_id_of)))
}
