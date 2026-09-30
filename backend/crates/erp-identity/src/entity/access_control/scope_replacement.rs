//! 逐动作替换岗位范围；未选中的动作与其他岗位、业务保持原样。

use super::{DataScope, DataScopeSubjectType, DataScopeType};
use crate::error::{Error, Result};

/// 一条旧规则的替换结果；空动作表示整条移除。
pub(crate) struct ScopeReplacement {
    pub original: DataScope,
    pub remaining_actions: Vec<String>,
}

/// 校验替换目标并计算需要移除或缩减的旧规则。
///
/// # 参数
/// * `replacement` - 已规范化的新范围。
/// * `existing` - 当前主体的完整范围快照。
/// # 返回
/// 返回所有命中所选动作的旧规则及其保留动作。
/// # 错误
/// 非岗位主体、停用规则或协作规则不得作为简化编辑的替换目标。
pub(crate) fn plan_replacement(
    replacement: &DataScope,
    existing: Vec<DataScope>,
) -> Result<Vec<ScopeReplacement>> {
    if replacement.subject_type != DataScopeSubjectType::Role
        || !replacement.binding.enabled
        || replacement.scope_type == DataScopeType::Collaborative
    {
        return Err(Error::ValidationError("请选择岗位的有效数据范围".into()));
    }
    Ok(existing
        .into_iter()
        .filter(|row| {
            row.subject_type == replacement.subject_type
                && row.subject_id == replacement.subject_id
                && row.binding.resource == replacement.binding.resource
                && row.binding.actions.iter().any(|action| replacement.binding.actions.contains(action))
        })
        .map(|original| {
            let remaining_actions = original
                .binding
                .actions
                .iter()
                .filter(|action| !replacement.binding.actions.contains(action))
                .cloned()
                .collect();
            ScopeReplacement { original, remaining_actions }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_control::{DataScopeData, DataScopeId, ScopeBinding, ScopeDimension};

    fn scope(id: &str, actions: &[&str], kind: DataScopeType) -> DataScope {
        DataScope::new(
            DataScopeId::new(id),
            DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: "sales".into(),
                scope_type: kind,
                scope_targets: vec![],
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: "sales_order".into(),
                    actions: actions.iter().map(|value| (*value).into()).collect(),
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: None,
                    include_descendants: None,
                    enabled: true,
                },
            },
        )
        .unwrap()
    }

    #[test]
    fn narrowing_removes_company_and_inert_rules_but_preserves_other_actions() {
        let replacement = scope("new", &["update"], DataScopeType::SelfOwned);
        let company = scope("wide", &["list", "update"], DataScopeType::Company);
        let inert = scope("inert", &["update"], DataScopeType::Collaborative);
        let plan = plan_replacement(&replacement, vec![company, inert]).unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].remaining_actions, vec!["list"]);
        assert!(plan[1].remaining_actions.is_empty());
    }

    #[test]
    fn other_subjects_resources_and_unselected_actions_are_untouched() {
        let replacement = scope("new", &["update"], DataScopeType::SelfOwned);
        let mut other_role = replacement.clone();
        other_role.subject_id = "manager".into();
        let mut other_resource = replacement.clone();
        other_resource.binding.resource = "purchase_order".into();
        let read = scope("read", &["list"], DataScopeType::Company);
        assert!(plan_replacement(&replacement, vec![other_role, other_resource, read]).unwrap().is_empty());
    }

    #[test]
    fn disabled_old_rules_are_removed_for_selected_actions() {
        let replacement = scope("new", &["list"], DataScopeType::SelfOwned);
        let mut old = scope("old", &["list"], DataScopeType::Company);
        old.binding.enabled = false;
        assert_eq!(plan_replacement(&replacement, vec![old]).unwrap().len(), 1);
    }

    #[test]
    fn invalid_replacement_is_rejected_even_for_empty_existing_rules() {
        let mut replacement = scope("new", &["list"], DataScopeType::Collaborative);
        assert!(plan_replacement(&replacement, vec![]).is_err());
        replacement.scope_type = DataScopeType::SelfOwned;
        replacement.subject_type = DataScopeSubjectType::User;
        assert!(plan_replacement(&replacement, vec![]).is_err());
        replacement.subject_type = DataScopeSubjectType::Role;
        replacement.binding.enabled = false;
        assert!(plan_replacement(&replacement, vec![]).is_err());
    }
}
