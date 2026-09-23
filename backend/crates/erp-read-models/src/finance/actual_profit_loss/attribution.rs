//! 历史身份筛选和候选复用完整授权订单集合，不从当前页推断候选。

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
