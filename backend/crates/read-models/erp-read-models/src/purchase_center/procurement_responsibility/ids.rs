/// 对强类型目录 ID 去重并按字典序稳定排序，供 Entity 单一规则源复用。
///
/// # 参数
/// * `values` - 待去重的强类型 ID 迭代器
///
/// # 返回
/// 返回按字符串字典序稳定排序的唯一值集合，满足确定性；与 Entity 值对象共用同一排序实现。
///
/// # 错误
/// 无。
///
/// # 约束
/// 去重后按字符串字典序排序，避免 HashMap 随机迭代导致查询批次不稳定；直接复用 Entity 的 `dedup_sorted_ids`。
pub(super) fn unique_ids<T>(values: impl Iterator<Item = T>) -> Vec<T>
where
    T: PartialEq + ToString,
{
    erp_procurement::entity::procurement_responsibility::dedup_sorted_ids(values)
}
