//! 历史候选独立快照：只接受期间和客户上下文，名称来自首次生效归属。
use std::collections::{BTreeMap, BTreeSet};

use application_core::FilterOption;
use erp_sales::entity::sales_order::SalesAttribution;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 客户经营质量历史目录查询；禁止混入结果筛选和页码。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalDirectoryQuery {
    pub from: String,
    pub to: String,
    pub customer_id: Option<String>,
    pub scope_version: Option<String>,
}

/// 实际盈亏历史目录只额外接受已实现的期间口径。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfitLossDirectoryQuery {
    pub from: String,
    pub to: String,
    pub period_basis: String,
    pub customer_id: Option<String>,
    pub scope_version: Option<String>,
}

/// 完整且有界的冻结人员与组织目录，不包含报表结果或金额。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricalDirectoryView {
    pub attribution_user_options: Vec<FilterOption>,
    pub attribution_org_options: Vec<FilterOption>,
    pub scope_version: String,
    pub empty_reason: Option<String>,
}
impl HistoricalDirectoryView {
    /// 聚合冻结身份、并列历史名称，组织包含直接归属及当时祖先路径。
    /// # 参数
    /// `rows` 是完整授权期间订单的归属；`scope_version` 绑定该来源快照。
    /// # 返回
    /// 稳定身份排序的完整候选，空授权单独标识。
    /// # 错误
    /// 任一候选维度超过 10000 项整体拒绝，不截断。
    pub(crate) fn from_attributions<'a>(
        rows: impl Iterator<Item = &'a SalesAttribution>,
        scope_version: String,
        no_scope: bool,
    ) -> Result<Self> {
        let mut users = BTreeMap::<String, BTreeSet<String>>::new();
        let mut orgs = BTreeMap::<String, BTreeSet<String>>::new();
        for row in rows {
            remember(&mut users, &row.attribution_user_id, &row.attribution_user_name);
            remember(&mut orgs, &row.attribution_org_unit_id, &row.attribution_org_unit_name);
            for node in &row.org_path {
                remember(&mut orgs, &node.id, &node.name);
            }
        }
        if users.len() > 10_000 || orgs.len() > 10_000 {
            return Err(Error::ValidationError("历史候选超过 10000 项，请缩小期间或指定客户".into()));
        }
        let empty_reason = if no_scope {
            Some("no_scope".into())
        } else if users.is_empty() && orgs.is_empty() {
            Some("no_data".into())
        } else {
            None
        };
        Ok(Self {
            attribution_user_options: labels(users),
            attribution_org_options: labels(orgs),
            scope_version,
            empty_reason,
        })
    }
}
/// 冻结身份不因当前账号删除、改名或组织移动而变化。
fn remember(values: &mut BTreeMap<String, BTreeSet<String>>, id: &str, name: &str) {
    if id.trim().is_empty() {
        return;
    }
    let names = values.entry(id.to_owned()).or_default();
    if !name.trim().is_empty() {
        names.insert(name.to_owned());
    }
}
/// 同名不同身份不合并；一个身份的多个历史名称稳定并列。
fn labels(values: BTreeMap<String, BTreeSet<String>>) -> Vec<FilterOption> {
    values
        .into_iter()
        .map(|(value, names)| {
            let shown = names.into_iter().collect::<Vec<_>>().join("／");
            let label = if shown.is_empty() { value.clone() } else { format!("{shown} · {value}") };
            FilterOption { value, label }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use erp_sales::entity::sales_order::AttributionOrgNode;

    use super::*;
    #[test]
    fn directory_preserves_frozen_names_and_paths_with_stable_identities() {
        let first = SalesAttribution {
            attribution_user_id: "former-a".into(),
            attribution_user_name: "同名".into(),
            attribution_org_unit_id: "old-team".into(),
            attribution_org_unit_name: "原团队".into(),
            org_path: vec![AttributionOrgNode { id: "old-parent".into(), name: "原部门".into() }],
            attributed_at: erp_core::common::time::Instant::from_unix_secs(1),
            attribution_version: 1,
            organization_version: 1,
        };
        let mut second = first.clone();
        second.attribution_user_id = "former-b".into();
        let rows = [first, second];
        let view = HistoricalDirectoryView::from_attributions(rows.iter(), "v1".into(), false).unwrap();
        assert_eq!(view.attribution_user_options.len(), 2);
        assert_ne!(view.attribution_user_options[0].label, view.attribution_user_options[1].label);
        assert!(
            view.attribution_org_options
                .iter()
                .any(|option| option.value == "old-parent" && option.label.contains("原部门"))
        );
        let reversed =
            HistoricalDirectoryView::from_attributions(rows.iter().rev(), "v1".into(), false).unwrap();
        assert_eq!(serde_json::to_value(view).unwrap(), serde_json::to_value(reversed).unwrap());
    }
    #[test]
    fn candidate_request_rejects_report_filters_and_empty_scope_remains_empty() {
        for extra in ["page", "q", "attribution_user_ids", "org_unit_ids", "coverage"] {
            let mut value = serde_json::json!({"from":"2026-09-01", "to":"2026-09-23"});
            value[extra] = serde_json::json!("narrowed");
            assert!(serde_json::from_value::<HistoricalDirectoryQuery>(value).is_err());
        }
        let empty = HistoricalDirectoryView::from_attributions([].iter(), "v2".into(), true).unwrap();
        assert_eq!(empty.empty_reason.as_deref(), Some("no_scope"));
        assert!(empty.attribution_user_options.is_empty());
    }
}
