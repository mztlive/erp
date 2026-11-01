//! 旧范围迁移的纯规则；不能等价解耦角色时明确阻断。
use super::person_scope::{PersonScopeExpression, PersonScopeTerm};
use super::personal_grant::PersonalBusinessGrant;
use super::{DataScope, DataScopeSubjectType};
use crate::{Error, Result};

/// 迁移一个人员资源动作，保留分支维度和动态所属部门。
/// # 参数
/// 当前完整动作合格角色、旧规则、个人扩展及是否允许合法历史读取。
/// # 返回
/// 无角色引用的单份表达式。
/// # 错误
/// 未迁移旧规则、角色相关动态范围或不同角色责任范围无法等价时拒绝。
pub fn expression(
    user: &str,
    roles: &[String],
    resource: &str,
    action: &str,
    rules: &[DataScope],
    grants: &[PersonalBusinessGrant],
    allows_history: bool,
) -> Result<PersonScopeExpression> {
    let mut source = rules.to_vec();
    for grant in grants.iter().filter(|g| g.applies(user, roles, resource, action)) {
        source.push(grant.as_role_scope()?);
    }
    let relevant = source
        .iter()
        .filter(|r| !r.base.is_deleted() && r.binding.applies(resource, action))
        .collect::<Vec<_>>();
    let mut alternatives = Vec::new();
    for role in roles {
        let group = terms(
            relevant
                .iter()
                .copied()
                .filter(|r| r.subject_type == DataScopeSubjectType::Role && r.subject_id == *role),
        )?;
        alternatives.push(group);
    }
    let keys = alternatives.iter().map(|terms| canonical(terms)).collect::<Result<Vec<_>>>()?;
    if keys.windows(2).any(|pair| pair[0] != pair[1]) {
        return Err(Error::ValidationError(
            "多个角色的范围不同，合并可能扩大复合操作或审批节点范围；请人工设置人员范围".into(),
        ));
    }
    let condition = terms(
        relevant
            .iter()
            .copied()
            .filter(|r| r.subject_type == DataScopeSubjectType::User && r.subject_id == user),
    )?;
    Ok(PersonScopeExpression {
        additive: false,
        history_read: allows_history,
        alternatives,
        condition: (!condition.is_empty()).then_some(condition),
    })
}

/// 同维度项顺序不影响语义，用规范排序比较各角色分支。
fn canonical(terms: &[PersonScopeTerm]) -> Result<Vec<String>> {
    let mut values = terms
        .iter()
        .map(|term| serde_json::to_string(term).map_err(|e| Error::Internal(e.to_string())))
        .collect::<Result<Vec<_>>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}

/// 每条动态规则保留语义，拒绝角色管理部门。
fn terms<'a>(rules: impl Iterator<Item = &'a DataScope>) -> Result<Vec<PersonScopeTerm>> {
    rules.map(PersonScopeTerm::from_legacy).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_control::{DataScopeType, ScopeDimension, ScopeTargetMode};
    fn rule(role: &str, scope: DataScopeType) -> DataScope {
        PersonScopeTerm {
            scope_type: scope,
            target_dimension: ScopeDimension::InternalOrg,
            target_mode: None,
            include_descendants: None,
            scope_targets: vec![],
        }
        .rule("sales_order", "detail", role, false)
        .unwrap()
    }
    #[test]
    fn differing_roles_block_but_equal_roles_preserve_history() {
        let roles = vec!["root".into(), "sales".into()];
        assert!(
            expression(
                "alice",
                &roles,
                "sales_order",
                "detail",
                &[rule("root", DataScopeType::Company), rule("sales", DataScopeType::SelfOwned)],
                &[],
                true
            )
            .is_err()
        );
        let result = expression(
            "alice",
            &roles,
            "sales_order",
            "detail",
            &[rule("root", DataScopeType::SelfOwned), rule("sales", DataScopeType::SelfOwned)],
            &[],
            true,
        )
        .unwrap();
        assert!(result.history_read);
        assert_eq!(result.alternatives.len(), 2);
    }
    #[test]
    fn role_managed_departments_cannot_be_silently_frozen() {
        let mut managed = rule("sales", DataScopeType::SelfOwned);
        managed.scope_type = DataScopeType::Organization;
        managed.binding.target_mode = Some(ScopeTargetMode::ManagedOrgs);
        assert!(PersonScopeTerm::from_legacy(&managed).is_err());
    }
}
