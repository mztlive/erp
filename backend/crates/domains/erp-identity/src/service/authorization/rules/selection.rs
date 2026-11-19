//! 选品入口的客户可见性与独立读取前提；所有业务入口共用固定组合。
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::slice;

use erp_permissions::{customer, sales_selection as selection};

use super::ScopeLookup;
use crate::entity::access_control::ResolvedScope;
use crate::entity::authorization::{AccessFact, AccessRule, FactKind, RelationKey, RelationMatch};
use crate::{Error, Permission, QualifiedPermissions, Result};

/// 为选品预取客户或选型册的附加读取资格，各独立入口可由不同启用角色授予。
pub(crate) fn qualifications(permission: &Permission) -> Vec<Permission> {
    let mut requested = Vec::new();
    if is_list(permission) {
        requested.extend([customer::LIST, customer::ALL_AUTHORIZED_LIST]);
    } else if is_selection(permission) {
        requested.push(customer::DETAIL);
        if requires_read(permission) {
            requested.push(selection::ADMIN_IMAGE);
        }
    }
    requested
}

/// 按选品用途请求客户列表、客户详情或选型册详情的各自范围，不借用选品主入口的角色。
pub(crate) fn scope_requests(permission: &Permission) -> Vec<(Permission, Permission)> {
    if is_list(permission) {
        return vec![(customer::LIST, customer::LIST)];
    }
    let mut requested = Vec::new();
    if is_selection(permission) {
        requested.push((customer::DETAIL, customer::DETAIL));
        if requires_read(permission) {
            requested.push((selection::ADMIN_IMAGE, selection::ADMIN_IMAGE));
        }
    }
    requested
}

// 构造启动和运行时共用的选品规则：列表按客户范围及可选归属过滤，其他选品动作还须读真实客户，指定动作再检查选型册详情范围。
pub(super) fn condition(
    permission: &Permission,
    qualified: Option<&QualifiedPermissions>,
    scopes: &ScopeLookup<'_>,
    base: AccessRule,
) -> Result<AccessRule> {
    if is_list(permission) {
        require_qualification(qualified, &customer::LIST)?;
        let scope = find_scope(scopes, &customer::LIST)?;
        let ownership =
            if qualified.is_some_and(|qualified| qualified.same_role(&[customer::ALL_AUTHORIZED_LIST])) {
                AccessRule::Allow
            } else {
                AccessRule::Fact(AccessFact::Owned)
            };
        return Ok(AccessRule::all(vec![base, customer_condition(&scope, ownership)?]));
    }

    if !is_selection(permission) {
        return Ok(base);
    }
    require_qualification(qualified, &customer::DETAIL)?;
    let customer_scope = find_scope(scopes, &customer::DETAIL)?;
    let mut conditions = vec![base, customer_detail_condition(&customer_scope)?];
    if requires_read(permission) {
        require_qualification(qualified, &selection::ADMIN_IMAGE)?;
        let scope = find_scope(scopes, &selection::ADMIN_IMAGE)?;
        conditions.push(read_condition(&scope));
    }
    Ok(AccessRule::all(conditions))
}

// 识别以选型册或方案为根的选品权限，其他资源不叠加本规则。
fn is_selection(permission: &Permission) -> bool {
    matches!(permission.resource(), "sales_selection_booklet" | "sales_selection_proposal")
}
// 识别选型册或方案的列表动作，以采用独立客户列表范围。
fn is_list(permission: &Permission) -> bool {
    is_selection(permission) && permission.action() == "list"
}

// 判断选型册动作是否另需独立详情读取，列表、详情和创建不重复要求。
fn requires_read(permission: &Permission) -> bool {
    permission.resource() == "sales_selection_booklet"
        && !matches!(permission.action(), "list" | "get" | "create")
}

// 运行时核对独立读取入口是否有启用角色授予权限；启动能力校验不检查账号。
fn require_qualification(qualified: Option<&QualifiedPermissions>, permission: &Permission) -> Result<()> {
    if qualified.is_none_or(|qualified| qualified.same_role(slice::from_ref(permission))) {
        Ok(())
    } else {
        tracing::debug!(%permission, "缺少选品入口所需的独立读取资格");
        Err(Error::Forbidden("当前账号没有查看选品所需资料的权限，请联系权限管理员".into()))
    }
}

// 按独立入口及其自身范围权限查找范围，缺失时返回选品配置错误。
fn find_scope<'a>(scopes: &ScopeLookup<'a>, permission: &Permission) -> Result<Cow<'a, ResolvedScope>> {
    scopes.get(permission, permission, || {
        tracing::debug!(%permission, "选品缺少独立范围");
        Error::ValidationError("当前账号尚未配置选品所需的数据范围，请联系权限管理员".into())
    })
}

// 列表只纳入实际客户非空且全部通过独立客户列表范围和归属条件的选品对象。
fn customer_condition(scope: &ResolvedScope, ownership: AccessRule) -> Result<AccessRule> {
    let facts = BTreeSet::from([FactKind::Owner, FactKind::Organization]);
    let rule = AccessRule::all(vec![AccessRule::scope(scope, false), ownership]).requiring(&facts);
    Ok(AccessRule::Independent {
        permission: customer::LIST,
        rule: Box::new(AccessRule::Related {
            relation: RelationKey::parse("customer")?,
            resource: "customer".into(),
            mode: RelationMatch::AllNonEmpty,
            rule: Box::new(rule),
        }),
    })
}

// 选品详情或写入须沿独立客户详情入口检查唯一真实客户，沿用详情自身的范围语义。
fn customer_detail_condition(scope: &ResolvedScope) -> Result<AccessRule> {
    Ok(AccessRule::Independent {
        permission: customer::DETAIL,
        rule: Box::new(AccessRule::Related {
            relation: RelationKey::parse("customer")?,
            resource: "customer".into(),
            mode: RelationMatch::ExactlyOne,
            rule: Box::new(super::data_condition(&customer::DETAIL, Some(scope))?),
        }),
    })
}

// 按独立选型册详情入口构造当前读取范围，要求负责人及组织事实，不借历史参与扩大范围。
fn read_condition(scope: &ResolvedScope) -> AccessRule {
    AccessRule::Independent {
        permission: selection::ADMIN_IMAGE,
        rule: Box::new(
            AccessRule::scope(scope, false)
                .requiring(&BTreeSet::from([FactKind::Owner, FactKind::Organization])),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use super::*;
    use crate::entity::access_control::ScopeClause;
    use crate::entity::authorization::{FactValue, ObjectFacts, RelatedObjects};

    // 构造公司或本人两种选品范围样本，不添加个人上限。
    fn scope(company: bool) -> ResolvedScope {
        ResolvedScope {
            role_clauses: vec![ScopeClause { company, self_owned: !company, ..Default::default() }],
            user_limit: None,
        }
    }

    // 构造归属已知的选型册事实，供修改负责人或添加客户关联。
    fn object(owner: &str) -> ObjectFacts {
        ObjectFacts {
            resource: "sales_selection_booklet".into(),
            id: "book".into(),
            version: "1".into(),
            owner_id: FactValue::Known(Some(owner.into())),
            org_unit_id: FactValue::Known(None),
            history_participants: FactValue::Known(BTreeSet::new()),
            ..Default::default()
        }
    }

    // 构造带单一实际客户关系的选型册样本，供独立客户范围及归属检查。
    fn list_object(owner: &str) -> ObjectFacts {
        let mut root = object("actor");
        let mut child = object(owner);
        child.resource = "customer".into();
        child.id = "customer".into();
        root.relations.insert(
            RelationKey::parse("customer").unwrap(),
            RelatedObjects { expected_ids: [child.id.clone()].into(), objects: vec![child] },
        );
        root
    }

    // 验证发布须独立具备选型册详情资格及其范围，并满足客户详情条件。
    #[test]
    fn write_requires_independent_get_qualification_and_get_scope() {
        let permission = selection::BOOKLET_PUBLISH;
        let qualified = QualifiedPermissions::test_grants(&[("write", slice::from_ref(&permission))]);
        assert!(matches!(
            condition(&permission, Some(&qualified), &ScopeLookup::Resolved(&[]), AccessRule::Allow),
            Err(Error::Forbidden(_))
        ));
        let qualified = QualifiedPermissions::test_grants(&[
            ("write", slice::from_ref(&permission)),
            ("read", &[selection::ADMIN_IMAGE]),
            ("customer", &[customer::DETAIL]),
        ]);
        let scopes = [
            (selection::ADMIN_IMAGE, selection::ADMIN_IMAGE, scope(false)),
            (customer::DETAIL, customer::DETAIL, scope(true)),
        ];
        let rule =
            condition(&permission, Some(&qualified), &ScopeLookup::Resolved(&scopes), AccessRule::Allow)
                .unwrap();
        let mut root = list_object("actor");
        assert!(rule.allows_object("actor", &root, &BTreeSet::new()).unwrap());
        root.owner_id = FactValue::Known(Some("other".into()));
        assert!(!rule.allows_object("actor", &root, &BTreeSet::new()).unwrap());
        assert!(
            condition(&permission, Some(&qualified), &ScopeLookup::Resolved(&[]), AccessRule::Allow).is_err()
        );
    }

    // 验证在能力范围且无额外全量查看资格时，选品启动条件与运行时结构一致。
    #[test]
    fn binding_matches_runtime_with_capability_scopes_and_no_optional_qualification() {
        let qualified = QualifiedPermissions::test_grants(&[
            ("customer-list", &[customer::LIST]),
            ("customer-detail", &[customer::DETAIL]),
            ("booklet-read", &[selection::ADMIN_IMAGE]),
        ]);
        for permission in [selection::BOOKLET_LIST, selection::BOOKLET_CREATE, selection::BOOKLET_PUBLISH] {
            let scopes: Vec<_> = scope_requests(&permission)
                .into_iter()
                .map(|(entry, source)| {
                    let scope = super::super::capability_scope(&source).unwrap();
                    (entry, source, scope)
                })
                .collect();
            let base = AccessRule::Fact(AccessFact::Owned);
            assert_eq!(
                condition(&permission, None, &ScopeLookup::Capability, base.clone()).unwrap(),
                condition(&permission, Some(&qualified), &ScopeLookup::Resolved(&scopes), base).unwrap(),
                "{permission}"
            );
        }
    }

    // 验证选品列表总须真实客户及独立范围，可选资格只去掉本人归属限制。
    #[test]
    fn customer_list_requires_scope_and_owned_customer_unless_optional_qualification_exists() {
        let permission = selection::BOOKLET_LIST;
        let scopes = [(customer::LIST, customer::LIST, scope(true))];
        let ordinary = QualifiedPermissions::test_grants(&[("read", &[customer::LIST])]);
        let rule =
            condition(&permission, Some(&ordinary), &ScopeLookup::Resolved(&scopes), AccessRule::Allow)
                .unwrap();
        assert!(rule.allows_object("actor", &list_object("actor"), &BTreeSet::new()).unwrap());
        assert!(!rule.allows_object("actor", &list_object("other"), &BTreeSet::new()).unwrap());

        let all = QualifiedPermissions::test_grants(&[
            ("read", &[customer::LIST]),
            ("all", &[customer::ALL_AUTHORIZED_LIST]),
        ]);
        let rule =
            condition(&permission, Some(&all), &ScopeLookup::Resolved(&scopes), AccessRule::Allow).unwrap();
        assert!(rule.allows_object("actor", &list_object("other"), &BTreeSet::new()).unwrap());
        assert!(rule.allows_object("actor", &object("actor"), &BTreeSet::new()).is_err());

        let limited = [(customer::LIST, customer::LIST, scope(false))];
        let rule =
            condition(&permission, Some(&all), &ScopeLookup::Resolved(&limited), AccessRule::Allow).unwrap();
        assert!(!rule.allows_object("actor", &list_object("other"), &BTreeSet::new()).unwrap());
    }

    // 验证创建和详情要求客户详情资格，但不另加选型册详情资格。
    #[test]
    fn create_and_detail_require_customer_read_without_an_additional_booklet_get() {
        let qualified = QualifiedPermissions::test_grants(&[("customer", &[customer::DETAIL])]);
        let scopes = [(customer::DETAIL, customer::DETAIL, scope(true))];
        for permission in [selection::BOOKLET_CREATE, selection::ADMIN_IMAGE, selection::PROPOSAL_DETAIL] {
            let rule =
                condition(&permission, Some(&qualified), &ScopeLookup::Resolved(&scopes), AccessRule::Allow)
                    .unwrap();
            assert!(rule.allows_object("actor", &list_object("other"), &BTreeSet::new()).unwrap());
            assert_eq!(qualifications(&permission), vec![customer::DETAIL]);
            let missing = QualifiedPermissions::test_grants(&[]);
            assert!(matches!(
                condition(&permission, Some(&missing), &ScopeLookup::Resolved(&scopes), AccessRule::Allow),
                Err(Error::Forbidden(_))
            ));
        }
    }

    // 验证真实客户详情允许历史参与，再与个人上限取交集。
    #[test]
    fn customer_detail_preserves_history_before_the_personal_limit() {
        let qualified = QualifiedPermissions::test_grants(&[("customer", &[customer::DETAIL])]);
        let mut root = list_object("other");
        root.relations.get_mut(&RelationKey::parse("customer").unwrap()).unwrap().objects[0]
            .history_participants = FactValue::Known(["actor".into()].into());
        let mut customer_scope = scope(false);
        let scopes = [(customer::DETAIL, customer::DETAIL, customer_scope.clone())];
        let rule = condition(
            &selection::ADMIN_IMAGE,
            Some(&qualified),
            &ScopeLookup::Resolved(&scopes),
            AccessRule::Allow,
        )
        .unwrap();
        assert!(rule.allows_object("actor", &root, &BTreeSet::new()).unwrap());

        customer_scope.user_limit = Some(ScopeClause::default());
        let scopes = [(customer::DETAIL, customer::DETAIL, customer_scope)];
        let rule = condition(
            &selection::ADMIN_IMAGE,
            Some(&qualified),
            &ScopeLookup::Resolved(&scopes),
            AccessRule::Allow,
        )
        .unwrap();
        assert!(!rule.allows_object("actor", &root, &BTreeSet::new()).unwrap());
    }
}
