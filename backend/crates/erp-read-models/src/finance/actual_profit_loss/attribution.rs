//! 历史身份筛选和候选复用完整授权订单集合，不从当前页推断候选。

use std::collections::{BTreeMap, BTreeSet};

use application_core::FilterOption;

use super::calculation::OrderResult;
use super::dto::ProfitLossQuery;

/// 同字段按 OR、人员与历史组织路径按 AND；组织移动不改变冻结路径。
pub(super) fn matches(order: &OrderResult, query: &ProfitLossQuery) -> bool {
    matches_group(order, query.attribution_group.as_deref())
        && query.attribution_user_ids.as_ref().is_none_or(|ids| {
            order.row.attribution_user_id.as_ref().is_some_and(|id| ids.as_slice().contains(id))
        })
        && query
            .attribution_org_unit_ids
            .as_ref()
            .is_none_or(|ids| order.attribution_path.iter().any(|node| ids.as_slice().contains(&node.id)))
}

/// 分组下钻匹配冻结的直接归属，不能用祖先路径条件扩大原分组；空身份仅匹配未知归属。
fn matches_group(order: &OrderResult, group: Option<&str>) -> bool {
    let Some(group) = group else {
        return true;
    };
    let Some((dimension, id)) = group.split_once(':') else {
        return false;
    };
    let actual = match dimension {
        "attribution_user" => order.row.attribution_user_id.as_deref(),
        "attribution_org" => order.row.attribution_org_unit_id.as_deref(),
        _ => return false,
    };
    actual.unwrap_or_default() == id
}

/// 候选覆盖本次期间和业务条件内的授权数据；历史名称保留，身份相同的旧名称合并。
pub(super) fn options(orders: &[OrderResult]) -> (Vec<FilterOption>, Vec<FilterOption>) {
    let mut users = BTreeMap::<String, BTreeSet<String>>::new();
    let mut organizations = BTreeMap::<String, BTreeSet<String>>::new();
    for order in orders {
        if let (Some(id), Some(name)) = (&order.row.attribution_user_id, &order.row.attribution_user_name) {
            users.entry(id.clone()).or_default().insert(name.clone());
        }
        for node in &order.attribution_path {
            organizations.entry(node.id.clone()).or_default().insert(node.name.clone());
        }
    }
    (labels(users), labels(organizations))
}

/// 同名候选以稳定身份区分；候选没有责任分派资格语义。
fn labels(values: BTreeMap<String, BTreeSet<String>>) -> Vec<FilterOption> {
    values
        .into_iter()
        .map(|(value, names)| FilterOption {
            label: format!("{} · {}", names.into_iter().collect::<Vec<_>>().join("／"), value),
            value,
        })
        .collect()
}
